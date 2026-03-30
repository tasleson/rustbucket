// Linux-specific I/O implementation using io_uring and O_DIRECT

use anyhow::{Context, Result};
use io_uring::{opcode, types, IoUring};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use crate::AlignedBuf;

/// Build an io_uring ring, preferring SQPOLL to eliminate submission syscalls.
/// Falls back to a plain ring if SQPOLL is unavailable (needs CAP_SYS_NICE on
/// kernels < 5.12).
pub fn build_ring(depth: u32) -> IoUring {
    IoUring::builder()
        .setup_sqpoll(2000)
        .build(depth)
        .unwrap_or_else(|_| IoUring::new(depth).expect("failed to create io_uring"))
}

/// Open a file for reading with O_DIRECT if possible, falling back to buffered I/O
pub fn open_read_direct(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
        .or_else(|_| {
            eprintln!("  Warning: O_DIRECT unavailable, falling back to buffered reads");
            OpenOptions::new().read(true).open(path)
        })
        .with_context(|| format!("open {}", path.display()))
}

/// Open a file for writing
pub fn open_write(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))
}

/// Read entire file into buffer using io_uring with O_DIRECT
pub fn read_file_direct(path: &Path, buf: &mut AlignedBuf, chunk_size: usize, queue_depth: u32) -> Result<usize> {
    let file = open_read_direct(path)?;
    let file_size = file.metadata()?.len() as usize;
    let padded_size = round_up(file_size, 4096);

    anyhow::ensure!(
        padded_size <= buf.capacity(),
        "file ({} B) exceeds buffer capacity ({} B)",
        file_size,
        buf.capacity()
    );

    let mut ring = build_ring(queue_depth);
    ring.submitter()
        .register_files(&[file.as_raw_fd()])
        .context("register_files")?;
    let fixed_fd = types::Fixed(0);

    let buf_ptr = buf.as_mut_ptr();
    let mut submit_offset = 0usize;
    let mut in_flight: u32 = 0;

    loop {
        {
            let mut sq = ring.submission();
            while in_flight < queue_depth && submit_offset < file_size {
                let raw_size = chunk_size.min(file_size - submit_offset);
                let io_size = round_up(raw_size, 4096);

                let slice_ptr = unsafe { buf_ptr.add(submit_offset) };

                let entry = opcode::Read::new(fixed_fd, slice_ptr, io_size as u32)
                    .offset(submit_offset as u64)
                    .build()
                    .user_data(submit_offset as u64);

                if unsafe { sq.push(&entry).is_err() } {
                    break;
                }
                submit_offset += raw_size;
                in_flight += 1;
            }
        }

        if in_flight == 0 {
            break;
        }

        ring.submit_and_wait(1).context("read: submit_and_wait")?;

        for cqe in ring.completion() {
            if cqe.result() < 0 {
                anyhow::bail!(
                    "read failed at offset {}: {}",
                    cqe.user_data(),
                    std::io::Error::from_raw_os_error(-cqe.result())
                );
            }
            in_flight -= 1;
        }
    }

    Ok(file_size)
}

/// Write buffer to file using io_uring
pub fn write_file_uring(ring: &mut IoUring, fd: types::Fd, data: &[u8], offset: &mut u64, chunk_size: usize) -> Result<()> {
    let mut pos = 0usize;
    while pos < data.len() {
        let end = (pos + chunk_size).min(data.len());
        let chunk = &data[pos..end];

        let sqe = opcode::Write::new(fd, chunk.as_ptr(), chunk.len() as u32)
            .offset(*offset)
            .build();
        unsafe { ring.submission().push(&sqe).expect("sq full") };
        ring.submit_and_wait(1).context("write")?;
        let cqe = ring.completion().next().expect("expected cqe");
        if cqe.result() < 0 {
            anyhow::bail!(
                "write failed: {}",
                std::io::Error::from_raw_os_error(-cqe.result())
            );
        }
        let written = cqe.result() as usize;
        *offset += written as u64;
        pos += written;
    }
    Ok(())
}

#[inline]
fn round_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}
