use anyhow::{Context, Result};
use crossbeam_channel::bounded;
use io_uring::{opcode, types, IoUring};
use rayon::prelude::*;
use std::fs::{self, remove_file, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{AlignedBuf, RECORD_SIZE};

// ── tunables ──────────────────────────────────────────────────────────────────

/// Read buffer for the scatter phase (per buffer; two are allocated).
const SCATTER_READ_BUF: usize = 64 * 1024 * 1024; // 64 MiB

/// BufWriter capacity per bin file during scatter.
const BIN_WRITE_BUF_MAX: usize = 8 * 1024 * 1024; // 4 MiB ceiling per bin

/// Chunk size for io_uring writes during the gather phase.
const GATHER_WRITE_CHUNK: usize = 64 * 1024 * 1024; // 64 MiB

/// Chunk size per io_uring read op when loading a bin file.
const BIN_READ_CHUNK: usize = 4 * 1024 * 1024; // 4 MiB

/// io_uring queue depth for bin reads.
const BIN_QUEUE_DEPTH: u32 = 64;

// ── helpers ───────────────────────────────────────────────────────────────────

#[inline]
fn round_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

// ── record key helpers ────────────────────────────────────────────────────────

//#[inline]
//fn record_key(rec: &[u8; RECORD_SIZE]) -> u128 {
//    u128::from_le_bytes(rec[..16].try_into().unwrap())
//}

#[inline]
fn bin_for_key(key: u128, num_bins: usize) -> usize {
    let shift = 128 - num_bins.ilog2();
    (key >> shift) as usize
}

// ── bin count / size calculation ──────────────────────────────────────────────

fn compute_num_bins(file_size: u64, memory_limit: u64) -> usize {
    let usable = ((memory_limit as f64) * 0.75) as u64;
    let usable = usable.max(RECORD_SIZE as u64 * 2);
    let min_bins = file_size.div_ceil(usable).max(2) as usize;
    min_bins.next_power_of_two()
}

fn bin_write_buf_size(num_bins: usize, memory_limit: u64) -> usize {
    let total = (memory_limit / 4) as usize;
    let per = total / num_bins;
    per.clamp(RECORD_SIZE * 64, BIN_WRITE_BUF_MAX)
}

// ── io_uring ring helper ──────────────────────────────────────────────────────

/// Build an io_uring ring, preferring SQPOLL to eliminate submission syscalls.
/// Falls back to a plain ring if SQPOLL is unavailable (needs CAP_SYS_NICE on
/// kernels < 5.12).
fn build_ring(depth: u32) -> IoUring {
    IoUring::builder()
        .setup_sqpoll(2000)
        .build(depth)
        .unwrap_or_else(|_| IoUring::new(depth).expect("failed to create io_uring"))
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

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(input)
        .or_else(|_| {
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
    // SQPOLL eliminates submission syscalls during the sequential double-buffer loop.
    let mut ring = build_ring(4);

    let buf_size = SCATTER_READ_BUF - (SCATTER_READ_BUF % RECORD_SIZE);
    // AlignedBuf::new now pre-faults pages so DMA doesn't trigger inline faults.
    let mut bufs = [AlignedBuf::new(buf_size), AlignedBuf::new(buf_size)];

    let mut bin_writers: Vec<BinWriter> = (0..num_bins)
        .map(|i| {
            let dir = &scratch_dirs[i % scratch_dirs.len()];
            let path = dir.join(format!("nsort_{}_{:06}.tmp", pid, i));
            BinWriter::new(path, bin_buf)
        })
        .collect::<Result<_>>()?;

    let read_size = |off: u64| -> u32 { (buf_size as u64).min(file_size - off) as u32 };

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

        let buf = bufs[current].as_slice_range(bytes_read);
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

// ── bin read ──────────────────────────────────────────────────────────────────

/// Read the entire file at `path` into `buf` using io_uring with O_DIRECT.
///
/// Accepts a caller-supplied `AlignedBuf` so the same pre-faulted memory can
/// be reused across multiple bin reads.  The first time a buf is used its pages
/// are already hot (pre-faulted in `AlignedBuf::new`); subsequent calls into
/// the same buf pay zero page-fault overhead.
///
/// Returns the number of valid bytes written (== file size).
fn fill_buf_uring_direct(path: &Path, buf: &mut AlignedBuf) -> Result<usize> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;

    let file_size = file.metadata()?.len() as usize;
    let padded_size = round_up(file_size, 4096);

    anyhow::ensure!(
        padded_size <= buf.capacity(),
        "bin file ({} B) exceeds buffer capacity ({} B); increase memory budget",
        file_size,
        buf.capacity()
    );

    let mut ring = build_ring(BIN_QUEUE_DEPTH);
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
            while in_flight < BIN_QUEUE_DEPTH && submit_offset < file_size {
                let raw_size = BIN_READ_CHUNK.min(file_size - submit_offset);
                // O_DIRECT: transfer size must be 4096-aligned.  The extra bytes
                // for the last chunk land in the padding region (capacity >
                // file_size) and are never exposed to callers.
                let io_size = round_up(raw_size, 4096);

                // SAFETY: submit_offset is a multiple of BIN_READ_CHUNK (itself
                // 4096-aligned), satisfying O_DIRECT alignment.
                // submit_offset + io_size <= padded_size <= buf.capacity().
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

        ring.submit_and_wait(1)
            .context("fill_buf: submit_and_wait")?;

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

// ── sort + gather phase ───────────────────────────────────────────────────────

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

#[inline(always)]
fn record_cmp(a: &[u8; 128], b: &[u8; 128]) -> std::cmp::Ordering {
    let ka = unsafe { std::ptr::read_unaligned(a.as_ptr() as *const u128) };
    let kb = unsafe { std::ptr::read_unaligned(b.as_ptr() as *const u128) };
    ka.cmp(&kb)
}

fn sort_and_gather(bin_writers: &[BinWriter], output: &Path, _memory_limit: u64) -> Result<u64> {
    let output_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output)
        .context("create output file")?;

    let output_fd = types::Fd(output_file.as_raw_fd());
    let mut ring = build_ring(4);
    let mut output_offset = 0u64;

    // Size pooled buffers for the largest ACTUAL bin, computed from the scatter
    // phase record counts.  Using memory_limit * 0.75 is wrong: it is an average
    // bound, but random key distribution means individual bins can exceed it.
    // We already know exact sizes — use them instead of estimating.
    let max_bin_bytes = round_up(
        bin_writers
            .iter()
            .map(|bw| bw.records as usize * RECORD_SIZE)
            .max()
            .unwrap_or(RECORD_SIZE),
        4096,
    );

    // Three AlignedBufs for triple-buffering: read one, sort one, write one.
    //   - reader thread: reads bin N+1 from disk into buf_a
    //   - sorter thread: sorts bin N in buf_b
    //   - writer thread: writes bin N-1 from buf_c
    //   All three operations happen concurrently.
    //
    // AlignedBuf::new pre-faults all pages once.  Every subsequent read into
    // these bufs pays zero page-fault overhead.
    let (read_tx, read_rx) = bounded::<(AlignedBuf, usize, usize)>(1); // reader → sorter: (buf, bytes, idx)
    let (sorted_tx, sorted_rx) = bounded::<(AlignedBuf, usize, usize)>(1); // sorter → writer: (buf, bytes, idx)
    let (free_tx, free_rx) = bounded::<AlignedBuf>(3); // writer → reader (pool)

    free_tx
        .send(AlignedBuf::new(max_bin_bytes))
        .expect("send buf 0");
    free_tx
        .send(AlignedBuf::new(max_bin_bytes))
        .expect("send buf 1");
    free_tx
        .send(AlignedBuf::new(max_bin_bytes))
        .expect("send buf 2");

    let bin_paths: Vec<PathBuf> = bin_writers.iter().map(|b| b.path.clone()).collect();

    let bin_record_counts: Vec<u64> = bin_writers.iter().map(|b| b.records).collect();

    // Reader thread: read bins from disk
    let reader_paths = bin_paths.clone();
    let reader_counts = bin_record_counts.clone();
    let reader = std::thread::spawn(move || -> Result<()> {
        let mut total_read = std::time::Duration::ZERO;
        let mut total_wait_pool = std::time::Duration::ZERO;
        let mut total_wait_send = std::time::Duration::ZERO;

        for (idx, path) in reader_paths.iter().enumerate() {
            let count = reader_counts[idx];
            if count == 0 {
                continue;
            }

            let t_wait = Instant::now();
            let mut buf = free_rx.recv().context("buffer pool closed")?;
            let wait_pool = t_wait.elapsed();
            total_wait_pool += wait_pool;

            let t_read = Instant::now();
            let bytes_read = fill_buf_uring_direct(path, &mut buf)?;
            let read_elapsed = t_read.elapsed();
            total_read += read_elapsed;

            let t_send = Instant::now();
            read_tx.send((buf, bytes_read, idx)).ok();
            let wait_send = t_send.elapsed();
            total_wait_send += wait_send;

            eprintln!(
                "  [reader] bin {:>4}: {:.1} MiB  read {:.3}s  wait-pool {:.3}s  wait-sorter {:.3}s",
                idx,
                bytes_read as f64 / (1u64 << 20) as f64,
                read_elapsed.as_secs_f64(),
                wait_pool.as_secs_f64(),
                wait_send.as_secs_f64()
            );
        }

        eprintln!(
            "  [reader] totals: read {:.3}s  wait-pool {:.3}s  wait-sorter {:.3}s",
            total_read.as_secs_f64(),
            total_wait_pool.as_secs_f64(),
            total_wait_send.as_secs_f64()
        );
        Ok(())
    });

    // Sorter thread: sort bins
    let sorter = std::thread::spawn(move || -> Result<()> {
        let mut total_sort = std::time::Duration::ZERO;
        let mut total_wait_reader = std::time::Duration::ZERO;
        let mut total_wait_writer = std::time::Duration::ZERO;

        loop {
            let t_wait = Instant::now();
            let (mut buf, bytes_read, idx) = match read_rx.recv() {
                Ok(v) => v,
                Err(_) => break, // reader finished
            };
            let wait_reader = t_wait.elapsed();
            total_wait_reader += wait_reader;

            let t_sort = Instant::now();
            {
                let data = &mut buf.as_mut_slice()[..bytes_read];
                let (records, remainder) = data.as_chunks_mut::<RECORD_SIZE>();
                assert!(remainder.is_empty());
                //records.par_sort_unstable_by_key(record_key);
                records.par_sort_unstable_by(record_cmp);
            }
            let sort_elapsed = t_sort.elapsed();
            total_sort += sort_elapsed;

            let t_send = Instant::now();
            sorted_tx.send((buf, bytes_read, idx)).ok();
            let wait_writer = t_send.elapsed();
            total_wait_writer += wait_writer;

            eprintln!(
                "  [sorter] bin {:>4}: {:.1} MiB  sort {:.3}s  wait-reader {:.3}s  wait-writer {:.3}s",
                idx,
                bytes_read as f64 / (1u64 << 20) as f64,
                sort_elapsed.as_secs_f64(),
                wait_reader.as_secs_f64(),
                wait_writer.as_secs_f64()
            );
        }

        eprintln!(
            "  [sorter] totals: sort {:.3}s  wait-reader {:.3}s  wait-writer {:.3}s",
            total_sort.as_secs_f64(),
            total_wait_reader.as_secs_f64(),
            total_wait_writer.as_secs_f64()
        );
        Ok(())
    });

    let start = Instant::now();
    let total_records: u64 = bin_writers.iter().map(|b| b.records).sum();
    let mut written_records = 0u64;
    let mut total_wait_sorter = std::time::Duration::ZERO;
    let mut total_write_time = std::time::Duration::ZERO;

    eprintln!("Gather: writing {} sorted records to output", total_records);

    for bw in bin_writers {
        if bw.records == 0 {
            continue;
        }

        let t_recv = Instant::now();
        let (mut buf, bytes, idx) = match sorted_rx.recv() {
            Ok(v) => v,
            Err(_) => {
                // Channel closed early — a thread hit an error. Join them to surface it.
                let _ = reader.join();
                return match sorter.join() {
                    Ok(Err(e)) => Err(e.context("sorter thread failed")),
                    Ok(Ok(())) => anyhow::bail!("sorter finished early without sending all bins"),
                    Err(_) => anyhow::bail!("sorter thread panicked"),
                };
            }
        };
        let wait_sorter = t_recv.elapsed();
        total_wait_sorter += wait_sorter;

        let t_write = Instant::now();
        uring_write_all(
            &mut ring,
            output_fd,
            buf.as_slice_range(bytes),
            &mut output_offset,
        )?;
        let write_elapsed = t_write.elapsed();
        total_write_time += write_elapsed;

        written_records += bytes as u64 / RECORD_SIZE as u64;

        // Return the buffer to the pool so the reader can reuse it for the next bin.
        free_tx.send(buf).ok();

        if let Err(e) = fs::remove_file(&bw.path) {
            eprintln!("  Warning: could not remove {}: {}", bw.path.display(), e);
        }

        let secs = start.elapsed().as_secs_f64();
        eprintln!(
            "  [writer] bin {:>4}: {:.1}%  {:.0} MiB/s  (wait-sorter {:.3}s  write {:.3}s)",
            idx,
            100.0 * written_records as f64 / total_records as f64,
            output_offset as f64 / secs / (1u64 << 20) as f64,
            wait_sorter.as_secs_f64(),
            write_elapsed.as_secs_f64(),
        );
    }

    eprintln!(
        "  [writer] totals: wait-sorter {:.3}s  write {:.3}s",
        total_wait_sorter.as_secs_f64(),
        total_write_time.as_secs_f64()
    );

    reader.join().expect("reader thread panicked")?;
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
    remove_input: bool,
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

    // Remove input file if requested (after scatter, before sort/gather).
    if remove_input {
        remove_file(input).with_context(|| format!("remove input file {}", input.display()))?;
        eprintln!("Input file removed: {}", input.display());
    }

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
    eprintln!(
        "Total: {:.1}s  effective throughput {:.0} MiB/s (based on input size)",
        total_secs,
        file_size as f64 / total_secs / (1u64 << 20) as f64
    );

    Ok(())
}
