use anyhow::{Context, Result};
use io_uring::{opcode, types, IoUring};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::time::Instant;

use crate::{AlignedBuf, RECORD_SIZE};

// 32 MiB per buffer; two buffers for double-buffering.
const BUF_RECORDS: usize = 256 * 1024;
const BUF_SIZE: usize = BUF_RECORDS * RECORD_SIZE; // 32 MiB

fn fill_records(buf: &mut [u8], count: usize, rng: &mut SmallRng) {
    for i in 0..count {
        let off = i * RECORD_SIZE;
        let id: u128 = rng.gen();
        buf[off..off + 16].copy_from_slice(&id.to_le_bytes());
        rng.fill(&mut buf[off + 16..off + RECORD_SIZE]);
    }
}

fn submit_write(
    ring: &mut IoUring,
    fd: types::Fd,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> Result<()> {
    let sqe = opcode::Write::new(fd, ptr, len as u32)
        .offset(offset)
        .build();
    unsafe { ring.submission().push(&sqe).expect("submission queue full") };
    ring.submit().context("io_uring submit")?;
    Ok(())
}

fn wait_write(ring: &mut IoUring) -> Result<()> {
    ring.submit_and_wait(1).context("io_uring wait")?;
    let cqe = ring.completion().next().expect("expected cqe");
    if cqe.result() < 0 {
        anyhow::bail!(
            "write failed: {}",
            std::io::Error::from_raw_os_error(-cqe.result())
        );
    }
    Ok(())
}

pub fn create_file(path: &Path, size_bytes: u64) -> Result<()> {
    let num_records = size_bytes / RECORD_SIZE as u64;
    anyhow::ensure!(
        num_records > 0,
        "size must be at least {} bytes",
        RECORD_SIZE
    );
    let actual_size = num_records * RECORD_SIZE as u64;

    eprintln!(
        "Creating {} records ({:.3} GiB) → {}",
        num_records,
        actual_size as f64 / (1u64 << 30) as f64,
        path.display()
    );

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .context("failed to create output file")?;

    let fd = types::Fd(file.as_raw_fd());
    let mut ring = IoUring::new(4).context("failed to create io_uring")?;

    let mut bufs = [AlignedBuf::new(BUF_SIZE), AlignedBuf::new(BUF_SIZE)];
    let mut rng = SmallRng::from_entropy();

    let mut records_remaining = num_records;
    let mut offset = 0u64;
    // buf[0] is used for the seed write; loop alternates starting from buf[1].
    let mut active = 1usize;

    let start = Instant::now();
    let report_step = (num_records / 20).max(1);
    let mut next_report = report_step;
    let mut records_done = 0u64;

    // --- Seed the pipeline: fill buf[0] and submit first write ---
    let recs0 = (BUF_RECORDS as u64).min(records_remaining) as usize;
    fill_records(bufs[0].as_mut_slice(), recs0, &mut rng);
    let bytes0 = recs0 * RECORD_SIZE;
    submit_write(&mut ring, fd, bufs[0].as_ptr(), bytes0, offset)?;
    offset += bytes0 as u64;
    records_remaining -= recs0 as u64;
    records_done += recs0 as u64;

    // --- Double-buffer loop: fill next while previous write is in flight ---
    while records_remaining > 0 {
        // Fill the idle buffer while the other one is being written.
        let recs = (BUF_RECORDS as u64).min(records_remaining) as usize;
        fill_records(bufs[active].as_mut_slice(), recs, &mut rng);

        // Now wait for the previous write to finish.
        wait_write(&mut ring)?;

        // Submit write for the buffer we just filled.
        let bytes = recs * RECORD_SIZE;
        submit_write(&mut ring, fd, bufs[active].as_ptr(), bytes, offset)?;
        offset += bytes as u64;
        records_remaining -= recs as u64;
        records_done += recs as u64;
        active = 1 - active;

        if records_done >= next_report {
            let secs = start.elapsed().as_secs_f64();
            eprintln!(
                "  {:.1}%  {:.0} MiB/s",
                100.0 * records_done as f64 / num_records as f64,
                offset as f64 / secs / (1u64 << 20) as f64
            );
            next_report += report_step;
        }
    }

    // Wait for the last in-flight write.
    wait_write(&mut ring)?;

    let secs = start.elapsed().as_secs_f64();
    eprintln!(
        "Done: {:.1}s  avg {:.0} MiB/s",
        secs,
        actual_size as f64 / secs / (1u64 << 20) as f64
    );
    Ok(())
}
