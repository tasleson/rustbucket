// Portable I/O implementation using standard filesystem APIs

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::AlignedBuf;

/// Simple wrapper since io_uring is not available
pub struct DummyRing;

pub fn build_ring(_depth: u32) -> DummyRing {
    DummyRing
}

/// Open a file for reading
pub fn open_read_direct(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .open(path)
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

/// Read entire file into buffer using standard I/O
pub fn read_file_direct(path: &Path, buf: &mut AlignedBuf, chunk_size: usize, _queue_depth: u32) -> Result<usize> {
    let mut file = open_read_direct(path)?;
    let file_size = file.metadata()?.len() as usize;

    anyhow::ensure!(
        file_size <= buf.capacity(),
        "file ({} B) exceeds buffer capacity ({} B)",
        file_size,
        buf.capacity()
    );

    let buf_slice = buf.as_mut_slice();
    let mut total_read = 0;

    while total_read < file_size {
        let to_read = chunk_size.min(file_size - total_read);
        let n = file.read(&mut buf_slice[total_read..total_read + to_read])
            .context("read failed")?;
        if n == 0 {
            break;
        }
        total_read += n;
    }

    Ok(total_read)
}

/// Write buffer to file using standard I/O
pub fn write_file_uring(_ring: &mut DummyRing, mut file: File, data: &[u8], _offset: &mut u64, chunk_size: usize) -> Result<()> {
    let mut pos = 0;
    while pos < data.len() {
        let end = (pos + chunk_size).min(data.len());
        let written = file.write(&data[pos..end])
            .context("write failed")?;
        pos += written;
    }
    file.flush().context("flush failed")?;
    Ok(())
}

/// Portable file wrapper for standard I/O
pub struct PortableFile {
    file: File,
    size: u64,
}

impl PortableFile {
    pub fn open_read(path: &Path) -> Result<Self> {
        let file = open_read_direct(path)?;
        let size = file.metadata()?.len();
        Ok(Self { file, size })
    }

    pub fn open_write(path: &Path) -> Result<Self> {
        let file = open_write(path)?;
        Ok(Self { file, size: 0 })
    }

    pub fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read(buf).context("read_at failed")
    }

    pub fn write_at(&mut self, buf: &[u8], offset: u64) -> Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write(buf).context("write_at failed")
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn as_file(&self) -> &File {
        &self.file
    }

    pub fn as_file_mut(&mut self) -> &mut File {
        &mut self.file
    }
}
