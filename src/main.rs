use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use memmap2::{MmapMut, MmapOptions};
use std::path::PathBuf;

mod create;
mod sort;
mod verify;

pub const RECORD_SIZE: usize = 128;
pub const ALIGNMENT: usize = 16384;

/// Page-aligned buffer suitable for O_DIRECT and io_uring.
pub struct AlignedBuf {
    mmap: MmapMut,
    size: usize,
    capacity: usize,
}

fn align_up(v: usize, align: usize) -> usize {
    (v + align - 1) & !(align - 1)
}

impl AlignedBuf {
    pub fn new(size: usize) -> Self {
        let aligned_size = align_up(size, ALIGNMENT);

        // This should give us page aligned memory, eg. 4k or 16k etc.
        let mut mmap = MmapOptions::new()
            .len(aligned_size)
            .populate()
            .map_anon()
            .unwrap();

        unsafe {
            libc::madvise(
                mmap.as_mut_ptr() as *mut _,
                aligned_size,
                libc::MADV_HUGEPAGE,
            );

            libc::madvise(
                mmap.as_mut_ptr() as *mut _,
                aligned_size,
                libc::MADV_DONTDUMP,
            );
        }

        Self {
            mmap,
            size,
            capacity: aligned_size,
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.mmap[..self.size]
    }

    pub fn as_slice(&mut self) -> &[u8] {
        &self.mmap[..self.size]
    }

    pub fn as_slice_range(&mut self, size: usize) -> &[u8] {
        &self.mmap[..size]
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap.as_mut_ptr()
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.mmap.as_ptr()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[derive(Parser)]
#[command(
    name = "rustybucket",
    about = "High-performance external sort for 128-byte fixed-size records.\nKey: first 16 bytes, interpreted as u128 little-endian."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a test file filled with randomly-keyed 128-byte records.
    Create {
        /// Output file path.
        file: PathBuf,
        /// File size. Supports suffixes: K, M, G, T (e.g. 10G).
        size: String,
    },
    /// Verify that a file of 128-byte records is sorted low-to-high by their u128 key.
    Verify {
        /// File to check.
        file: PathBuf,
    },
    /// Sort a file of 128-byte records by their 16-byte u128 key.
    Sort {
        /// Input file path.
        input: PathBuf,
        /// Output file path (will be created/overwritten).
        output: PathBuf,
        /// One or more scratch directories for temporary bin files.
        /// Spread across separate NVMe devices for best performance.
        #[arg(required = true, num_args = 1..)]
        scratch: Vec<PathBuf>,
        /// Total memory budget (e.g. 16G, 512M).
        #[arg(short, long, default_value = "4G")]
        memory: String,
        /// Number of worker threads. 0 = use all available cores.
        #[arg(short, long, default_value = "0")]
        threads: usize,
    },
}

pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num_str, mult) = if let Some(n) = s
        .strip_suffix("TiB")
        .or(s.strip_suffix("tib"))
        .or(s.strip_suffix(['T', 't']))
    {
        (n, 1u64 << 40)
    } else if let Some(n) = s
        .strip_suffix("GiB")
        .or(s.strip_suffix("gib"))
        .or(s.strip_suffix(['G', 'g']))
    {
        (n, 1u64 << 30)
    } else if let Some(n) = s
        .strip_suffix("MiB")
        .or(s.strip_suffix("mib"))
        .or(s.strip_suffix(['M', 'm']))
    {
        (n, 1u64 << 20)
    } else if let Some(n) = s
        .strip_suffix("KiB")
        .or(s.strip_suffix("kib"))
        .or(s.strip_suffix(['K', 'k']))
    {
        (n, 1u64 << 10)
    } else {
        (s, 1u64)
    };
    let num: f64 = num_str.trim().parse().context("invalid size number")?;
    Ok((num * mult as f64) as u64)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Verify { file } => {
            std::process::exit(verify::verify_file(&file)? as i32);
        }
        Commands::Create { file, size } => {
            let bytes = parse_size(&size)?;
            create::create_file(&file, bytes)?;
        }
        Commands::Sort {
            input,
            output,
            scratch,
            memory,
            threads,
        } => {
            let memory_bytes = parse_size(&memory)?;
            let num_threads = if threads == 0 {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            } else {
                threads
            };
            rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build_global()
                .ok();
            sort::sort_file(&input, &output, &scratch, memory_bytes)?;
        }
    }
    Ok(())
}
