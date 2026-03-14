use anyhow::{Context, Result};
use io_uring::{opcode, types, IoUring};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::time::Instant;

use crate::{AlignedBuf, RECORD_SIZE};

const READ_BUF_SIZE: usize = 64 * 1024 * 1024; // 64 MiB

#[inline]
fn key_of(rec: &[u8]) -> u128 {
    u128::from_le_bytes(rec[..16].try_into().unwrap())
}

/// Returns `false` (exit code 1) if the file is not sorted, `true` (exit code 0) if it is.
pub fn verify_file(path: &Path) -> Result<bool> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
        .or_else(|_| OpenOptions::new().read(true).open(path))
        .context("open file")?;

    let file_size = file.metadata()?.len();

    anyhow::ensure!(
        file_size % RECORD_SIZE as u64 == 0,
        "file size ({}) is not a multiple of RECORD_SIZE ({})",
        file_size,
        RECORD_SIZE
    );

    let total_records = file_size / RECORD_SIZE as u64;
    eprintln!(
        "Verifying {} records ({:.3} GiB) in {}",
        total_records,
        file_size as f64 / (1u64 << 30) as f64,
        path.display()
    );

    if total_records == 0 {
        eprintln!("OK: file is empty");
        return Ok(true);
    }

    let fd = types::Fd(file.as_raw_fd());
    let mut ring = IoUring::builder()
        .setup_sqpoll(2000)
        .build(4)
        .unwrap_or_else(|_| IoUring::new(4).expect("io_uring"));

    let buf_size = READ_BUF_SIZE - (READ_BUF_SIZE % RECORD_SIZE);
    let mut bufs = [AlignedBuf::new(buf_size), AlignedBuf::new(buf_size)];

    let read_size = |off: u64| -> u32 { (buf_size as u64).min(file_size - off) as u32 };

    // Submit first read.
    {
        let rs = read_size(0);
        let sqe = opcode::Read::new(fd, bufs[0].as_mut_ptr(), rs)
            .offset(0)
            .build();
        unsafe { ring.submission().push(&sqe).expect("sq full") };
        ring.submit().context("initial submit")?;
    }

    let mut inflight_offset = read_size(0) as u64;
    let mut current = 0usize;
    let mut have_inflight = true;

    let mut prev_key: u128 = 0;
    let mut checked: u64 = 0;
    // Track whether `prev_key` has been set from at least one record.
    let mut first = true;

    let start = Instant::now();
    let report_step = (total_records / 20).max(1);
    let mut next_report = report_step;

    loop {
        if !have_inflight {
            break;
        }

        ring.submit_and_wait(1).context("wait read")?;
        let cqe = ring.completion().next().expect("expected cqe");
        if cqe.result() < 0 {
            anyhow::bail!(
                "read failed: {}",
                std::io::Error::from_raw_os_error(-cqe.result())
            );
        }
        let bytes_read = cqe.result() as usize;
        have_inflight = false;

        // Submit next read into the other buffer before processing this one.
        let next = 1 - current;
        if inflight_offset < file_size {
            let rs = read_size(inflight_offset);
            let sqe = opcode::Read::new(fd, bufs[next].as_mut_ptr(), rs)
                .offset(inflight_offset)
                .build();
            unsafe { ring.submission().push(&sqe).expect("sq full") };
            ring.submit().context("submit read")?;
            inflight_offset += rs as u64;
            have_inflight = true;
        }

        // Check each record in this buffer.
        let buf = bufs[current].as_slice_range(bytes_read);
        let num_recs = bytes_read / RECORD_SIZE;
        for i in 0..num_recs {
            let key = key_of(&buf[i * RECORD_SIZE..]);
            if !first && key < prev_key {
                let record_index = checked + i as u64;
                eprintln!(
                    "FAIL: record {} is out of order\n  key {:#034x}\n  prev {:#034x}",
                    record_index, key, prev_key
                );
                return Ok(false);
            }
            prev_key = key;
            first = false;
        }

        checked += num_recs as u64;
        current = next;

        if checked >= next_report {
            let secs = start.elapsed().as_secs_f64();
            eprintln!(
                "  {:.1}%  {:.0} MiB/s",
                100.0 * checked as f64 / total_records as f64,
                (checked * RECORD_SIZE as u64) as f64 / secs / (1u64 << 20) as f64
            );
            next_report += report_step;
        }
    }

    let secs = start.elapsed().as_secs_f64();
    eprintln!(
        "OK: {} records verified in {:.1}s ({:.0} MiB/s)",
        checked,
        secs,
        file_size as f64 / secs / (1u64 << 20) as f64
    );
    Ok(true)
}
