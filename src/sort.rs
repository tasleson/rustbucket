use anyhow::{Context, Result};
use crossbeam_channel::bounded;
use io_uring::{opcode, types, IoUring};
use rayon::prelude::*;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{AlignedBuf, RECORD_SIZE};

// ── tunables ──────────────────────────────────────────────────────────────────

/// Read buffer for the scatter phase (per buffer; two are allocated).
/// Must be a multiple of RECORD_SIZE and large enough to amortise syscall overhead.
const SCATTER_READ_BUF: usize = 64 * 1024 * 1024; // 64 MiB

/// BufWriter capacity per bin file during scatter.  Kept small so total
/// bin-buffer memory ≈ num_bins × BIN_WRITE_BUF stays well under the limit.
const BIN_WRITE_BUF_MAX: usize = 4 * 1024 * 1024; // 4 MiB ceiling per bin

/// Chunk size for io_uring writes during the gather phase.
const GATHER_WRITE_CHUNK: usize = 64 * 1024 * 1024; // 64 MiB

// ── record key helpers ────────────────────────────────────────────────────────

#[inline]
fn record_key(rec: &[u8; RECORD_SIZE]) -> u128 {
    u128::from_le_bytes(rec[..16].try_into().unwrap())
}

#[inline]
fn bin_for_key(key: u128, num_bins: usize) -> usize {
    // num_bins is always a power of two ≥ 2.
    let shift = 128 - num_bins.ilog2();
    (key >> shift) as usize
}

// ── bin count / size calculation ──────────────────────────────────────────────

fn compute_num_bins(file_size: u64, memory_limit: u64) -> usize {
    // Reserve 75 % of memory for a single in-memory bin during the sort phase.
    let usable = ((memory_limit as f64) * 0.75) as u64;
    let usable = usable.max(RECORD_SIZE as u64 * 2);
    // Minimum bins so that each bin fits in usable memory.
    let min_bins = file_size.div_ceil(usable).max(2) as usize;
    // Round up to next power of two for simple key-based routing.
    min_bins.next_power_of_two()
}

fn bin_write_buf_size(num_bins: usize, memory_limit: u64) -> usize {
    // Allocate at most 25 % of memory across all bin write buffers.
    let total = (memory_limit / 4) as usize;
    let per = total / num_bins;
    per.clamp(RECORD_SIZE * 64, BIN_WRITE_BUF_MAX)
}

// ── scatter phase ─────────────────────────────────────────────────────────────

struct BinWriter {
    writer: BufWriter<File>,
    path: PathBuf,
    records: u64,
}

impl BinWriter {
    fn new(path: PathBuf, buf_cap: usize) -> Result<Self> {
        let f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .with_context(|| format!("create bin file {}", path.display()))?;
        Ok(BinWriter {
            writer: BufWriter::with_capacity(buf_cap, f),
            path,
            records: 0,
        })
    }

    #[inline]
    fn write_record(&mut self, rec: &[u8]) -> Result<()> {
        self.writer.write_all(rec)?;
        self.records += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.writer.flush().context("flush bin writer")
    }
}

fn scatter(
    input: &Path,
    scratch_dirs: &[PathBuf],
    num_bins: usize,
    bin_buf: usize,
) -> Result<(Vec<BinWriter>, u64)> {
    let pid = std::process::id();

    // Open input with O_DIRECT to bypass page cache for large sequential reads.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(input)
        .or_else(|_| {
            // Fall back if O_DIRECT is not supported (e.g. on tmpfs).
            eprintln!("  Warning: O_DIRECT unavailable, falling back to buffered reads");
            OpenOptions::new().read(true).open(input)
        })
        .context("open input")?;

    let file_size = file.metadata()?.len();
    anyhow::ensure!(
        file_size % RECORD_SIZE as u64 == 0,
        "input file size ({}) is not a multiple of RECORD_SIZE ({})",
        file_size,
        RECORD_SIZE
    );
    let total_records = file_size / RECORD_SIZE as u64;

    eprintln!(
        "Scatter: {} records across {} bins ({} scratch dir(s))",
        total_records,
        num_bins,
        scratch_dirs.len()
    );

    let fd = types::Fd(file.as_raw_fd());
    let mut ring = IoUring::new(4).context("io_uring for scatter")?;

    // Two aligned read buffers.
    let buf_size = SCATTER_READ_BUF - (SCATTER_READ_BUF % RECORD_SIZE);
    let mut bufs = [AlignedBuf::new(buf_size), AlignedBuf::new(buf_size)];

    // Create all bin writers, distributed round-robin across scratch dirs.
    let mut bin_writers: Vec<BinWriter> = (0..num_bins)
        .map(|i| {
            let dir = &scratch_dirs[i % scratch_dirs.len()];
            let path = dir.join(format!("nsort_{}_{:06}.tmp", pid, i));
            BinWriter::new(path, bin_buf)
        })
        .collect::<Result<_>>()?;

    // ── io_uring double-buffer read loop ───────────────────────────────────
    let read_size = |off: u64| -> u32 { (buf_size as u64).min(file_size - off) as u32 };

    // Submit first read.
    {
        let rs = read_size(0);
        let sqe = opcode::Read::new(fd, bufs[0].as_mut_ptr(), rs)
            .offset(0)
            .build();
        unsafe { ring.submission().push(&sqe).expect("sq full") };
        ring.submit().context("scatter: initial submit")?;
    }

    let mut inflight_offset = read_size(0) as u64;
    let mut current = 0usize;
    let mut have_inflight = true;
    let mut processed = 0u64;

    let start = Instant::now();
    let report_step = (total_records / 20).max(1);
    let mut next_report = report_step;

    loop {
        if !have_inflight {
            break;
        }

        // Wait for the in-flight read to complete.
        ring.submit_and_wait(1).context("scatter: wait read")?;
        let cqe = ring.completion().next().expect("expected cqe");
        if cqe.result() < 0 {
            anyhow::bail!(
                "read failed: {}",
                std::io::Error::from_raw_os_error(-cqe.result())
            );
        }
        let bytes_read = cqe.result() as usize;
        have_inflight = false;

        // Submit the next read into the OTHER buffer before processing,
        // so disk I/O and CPU routing overlap.
        let next = 1 - current;
        if inflight_offset < file_size {
            let rs = read_size(inflight_offset);
            let sqe = opcode::Read::new(fd, bufs[next].as_mut_ptr(), rs)
                .offset(inflight_offset)
                .build();
            unsafe { ring.submission().push(&sqe).expect("sq full") };
            ring.submit().context("scatter: submit read")?;
            inflight_offset += rs as u64;
            have_inflight = true;
        }

        // Route every record in the current buffer to its bin.
        let buf = bufs[current].as_slice(bytes_read);
        let num_recs = bytes_read / RECORD_SIZE;
        for i in 0..num_recs {
            let rec = &buf[i * RECORD_SIZE..(i + 1) * RECORD_SIZE];
            let key = u128::from_le_bytes(rec[..16].try_into().unwrap());
            let bin = bin_for_key(key, num_bins);
            bin_writers[bin].write_record(rec)?;
        }

        processed += num_recs as u64;
        current = next;

        if processed >= next_report {
            let secs = start.elapsed().as_secs_f64();
            let mib = (processed * RECORD_SIZE as u64) as f64 / (1u64 << 20) as f64;
            eprintln!(
                "  scatter {:.1}%  {:.0} MiB/s",
                100.0 * processed as f64 / total_records as f64,
                mib / secs
            );
            next_report += report_step;
        }
    }

    // Flush all bin writers to disk.
    for bw in &mut bin_writers {
        bw.flush()?;
    }

    let secs = start.elapsed().as_secs_f64();
    eprintln!(
        "Scatter done: {:.1}s  {:.0} MiB/s",
        secs,
        file_size as f64 / secs / (1u64 << 20) as f64
    );

    Ok((bin_writers, processed))
}

// ── sort + gather phase ───────────────────────────────────────────────────────

/// Write `data` to `fd` starting at `output_offset` using io_uring, in chunks.
/// Returns the number of bytes written.
fn uring_write_all(
    ring: &mut IoUring,
    fd: types::Fd,
    data: &[u8],
    output_offset: &mut u64,
) -> Result<()> {
    let mut pos = 0usize;
    while pos < data.len() {
        let end = (pos + GATHER_WRITE_CHUNK).min(data.len());
        let chunk = &data[pos..end];

        let sqe = opcode::Write::new(fd, chunk.as_ptr(), chunk.len() as u32)
            .offset(*output_offset)
            .build();
        unsafe { ring.submission().push(&sqe).expect("sq full") };
        ring.submit_and_wait(1).context("gather write")?;
        let cqe = ring.completion().next().expect("expected cqe");
        if cqe.result() < 0 {
            anyhow::bail!(
                "output write failed: {}",
                std::io::Error::from_raw_os_error(-cqe.result())
            );
        }
        let written = cqe.result() as usize;
        *output_offset += written as u64;
        pos += written;
    }
    Ok(())
}

fn sort_and_gather(bin_writers: &[BinWriter], output: &Path, memory_limit: u64) -> Result<u64> {
    let output_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output)
        .context("create output file")?;

    let output_fd = types::Fd(output_file.as_raw_fd());
    let mut ring = IoUring::new(4).context("io_uring for gather")?;
    let mut output_offset = 0u64;

    // Pipeline: a background thread reads + sorts the NEXT bin while this
    // thread writes the CURRENT bin.  Channel capacity = 1 so at most one
    // pre-sorted bin is buffered in memory (respects the memory limit).
    let (tx, rx) = bounded::<Vec<u8>>(1);

    let bin_paths: Vec<PathBuf> = bin_writers.iter().map(|b| b.path.clone()).collect();
    let bin_record_counts: Vec<u64> = bin_writers.iter().map(|b| b.records).collect();

    // Sorter thread.
    let sorter = std::thread::spawn(move || -> Result<()> {
        for (idx, path) in bin_paths.iter().enumerate() {
            let count = bin_record_counts[idx];
            if count == 0 {
                tx.send(Vec::new()).ok();
                continue;
            }

            // Read the entire bin into memory.
            let mut data = {
                let f = File::open(path).with_context(|| format!("open bin {}", path.display()))?;
                let sz = f.metadata()?.len() as usize;
                let mut v = vec![0u8; sz];
                let mut f = std::io::BufReader::with_capacity(64 * 1024 * 1024, f);
                std::io::Read::read_exact(&mut f, &mut v)
                    .with_context(|| format!("read bin {}", path.display()))?;
                v
            };

            // Sort in-place as an array of fixed-size records using rayon.
            // SAFETY: data.len() is a multiple of RECORD_SIZE (we only wrote
            // whole records); [u8; RECORD_SIZE] has the same layout as RECORD_SIZE
            // consecutive u8s.
            {
                let records: &mut [[u8; RECORD_SIZE]] = unsafe {
                    std::slice::from_raw_parts_mut(
                        data.as_mut_ptr() as *mut [u8; RECORD_SIZE],
                        data.len() / RECORD_SIZE,
                    )
                };
                records.par_sort_unstable_by_key(record_key);
            }

            tx.send(data).ok();
        }
        Ok(())
    });

    let start = Instant::now();
    let total_records: u64 = bin_writers.iter().map(|b| b.records).sum();
    let mut written_records = 0u64;

    eprintln!("Gather: writing {} sorted records to output", total_records);

    for bw in bin_writers {
        let data = rx.recv().context("sorter thread closed channel early")?;
        if data.is_empty() {
            continue;
        }

        uring_write_all(&mut ring, output_fd, &data, &mut output_offset)?;

        written_records += data.len() as u64 / RECORD_SIZE as u64;

        // Delete the bin file as soon as it has been consumed.
        if let Err(e) = fs::remove_file(&bw.path) {
            eprintln!("  Warning: could not remove {}: {}", bw.path.display(), e);
        }

        let secs = start.elapsed().as_secs_f64();
        eprintln!(
            "  gather {:.1}%  {:.0} MiB/s",
            100.0 * written_records as f64 / total_records as f64,
            output_offset as f64 / secs / (1u64 << 20) as f64
        );
    }

    let _ = memory_limit; // used by caller for bin count; not needed here

    // Propagate any error from the sorter thread.
    sorter.join().expect("sorter thread panicked")?;

    let secs = start.elapsed().as_secs_f64();
    eprintln!(
        "Gather done: {:.1}s  {:.0} MiB/s",
        secs,
        output_offset as f64 / secs / (1u64 << 20) as f64
    );

    Ok(written_records)
}

// ── public entry point ────────────────────────────────────────────────────────

pub fn sort_file(
    input: &Path,
    output: &Path,
    scratch_dirs: &[PathBuf],
    memory_limit: u64,
) -> Result<()> {
    let file_size = input
        .metadata()
        .with_context(|| format!("stat input {}", input.display()))?
        .len();

    anyhow::ensure!(file_size > 0, "input file is empty");
    anyhow::ensure!(
        file_size % RECORD_SIZE as u64 == 0,
        "input file size {} is not a multiple of RECORD_SIZE {}",
        file_size,
        RECORD_SIZE
    );

    let num_bins = compute_num_bins(file_size, memory_limit);
    let bin_buf = bin_write_buf_size(num_bins, memory_limit);

    eprintln!(
        "Sort: {:.3} GiB input, {} bins, {:.1} MiB per bin write-buffer, {:.1} GiB memory budget",
        file_size as f64 / (1u64 << 30) as f64,
        num_bins,
        bin_buf as f64 / (1u64 << 20) as f64,
        memory_limit as f64 / (1u64 << 30) as f64,
    );

    let wall = Instant::now();

    // Phase 1 – Scatter.
    let (bin_writers, scattered) = scatter(input, scratch_dirs, num_bins, bin_buf)?;
    eprintln!("Phase 1 done: {} records scattered", scattered);

    // Phase 2 – Sort + Gather.
    let written = sort_and_gather(&bin_writers, output, memory_limit)?;
    eprintln!("Phase 2 done: {} records written", written);

    anyhow::ensure!(
        scattered == written,
        "record count mismatch: scattered {} but wrote {}",
        scattered,
        written
    );

    let total_secs = wall.elapsed().as_secs_f64();
    let total_bytes = file_size as f64;
    eprintln!(
        "Total: {:.1}s  effective throughput {:.0} MiB/s (based on input size)",
        total_secs,
        total_bytes / total_secs / (1u64 << 20) as f64
    );

    Ok(())
}
