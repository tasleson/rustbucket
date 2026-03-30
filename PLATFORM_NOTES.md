# Cross-Platform Implementation Notes

## Overview

rustbucket has been refactored to support multiple platforms while maintaining optimal performance on Linux.

## Architecture

### Conditional Compilation Strategy

The project uses Rust's `#[cfg(target_os = "...")]` attributes to compile platform-specific code:

- **Linux**: High-performance io_uring-based implementation
- **Other platforms** (macOS, Windows, BSD, etc.): Standard file I/O fallback

### Platform-Specific Dependencies

Dependencies are now conditionally compiled:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
io-uring = "0.7"
libc = "0.2"
```

This means:
- On Linux: Full dependencies including io_uring and libc
- On other platforms: Only core dependencies (clap, rayon, anyhow, etc.)

### Code Structure

#### `src/io_backend/` module
- `linux.rs` - io_uring-based implementation with O_DIRECT support
- `portable.rs` - Standard file I/O for cross-platform compatibility

#### Platform-specific implementations

Each major operation has two implementations:

1. **scatter phase** (distributing records into bins):
   - Linux: io_uring double-buffered async reads
   - Portable: Synchronous buffered reads

2. **gather phase** (reading, sorting, writing bins):
   - Linux: io_uring async I/O with triple-buffered pipeline
   - Portable: Standard file I/O with same pipeline architecture

3. **create** (generating test data):
   - Linux: io_uring double-buffered async writes
   - Portable: Synchronous buffered writes

4. **verify** (checking sort order):
   - Linux: io_uring double-buffered async reads
   - Portable: Synchronous buffered reads

#### Memory management

- **Linux**: Uses `madvise()` with `MADV_HUGEPAGE`, `MADV_DONTDUMP`, `MADV_WILLNEED` for performance
- **Other platforms**: Uses basic mmap without platform-specific hints

## Performance Characteristics

### Linux (optimal)
- Zero-copy I/O via io_uring
- O_DIRECT bypasses page cache
- SQPOLL mode eliminates submission syscalls
- Huge pages reduce TLB misses
- Measured: ~3 GiB/s scatter, ~2.9 GiB/s gather on NVMe

### Portable (compatible)
- Standard buffered I/O
- Still benefits from:
  - Multi-threaded sorting (rayon)
  - Triple-buffered pipeline
  - Memory-aligned buffers
- Expected: 50-70% of Linux performance on same hardware

## Building

### Linux
```bash
cargo build --release
```
Automatically includes io_uring support.

### macOS/Windows
```bash
cargo build --release
```
Automatically uses portable implementation.

### Cross-compilation
To cross-compile for other platforms:
```bash
# For macOS target
cargo build --release --target x86_64-apple-darwin

# For Windows target
cargo build --release --target x86_64-pc-windows-msvc
```

## Testing

The same command-line interface works across all platforms:

```bash
# Create test file
rustbucket create test.dat 1G

# Sort it
rustbucket sort test.dat sorted.dat /tmp --memory 4G

# Verify
rustbucket verify sorted.dat
```

## Limitations on Non-Linux Platforms

1. **No O_DIRECT**: Portable version uses OS page cache
2. **No io_uring**: Falls back to synchronous I/O
3. **No memory hints**: Basic mmap without optimization hints
4. **Lower throughput**: Expect 50-70% of Linux performance

## Future Enhancements

Potential platform-specific optimizations:

- **Windows**: Use I/O Completion Ports (IOCP) for async I/O
- **macOS**: Use kqueue or aio for better async performance
- **All platforms**: Investigate using tokio for unified async runtime

## Development Notes

When adding new I/O operations:

1. Implement Linux version in `src/io_backend/linux.rs` using io_uring
2. Implement portable version in `src/io_backend/portable.rs` using std::fs
3. Add `#[cfg(target_os = "linux")]` and `#[cfg(not(target_os = "linux"))]` attributes
4. Test on both Linux and at least one other platform

## Compatibility

- **Tested on**: Linux (Fedora 43), expected to work on macOS and Windows
- **Rust version**: 1.70+ (uses stabilized features like `as_chunks_mut`)
- **Architecture**: x86_64, aarch64 (platform-independent algorithm)
