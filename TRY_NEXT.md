# Performance Optimization Ideas

## Current Bottleneck

Sorter thread: 349.292s (critical path)
- Writer waits 222s for sorter
- Reader waits 59s for sorter
- I/O is already fast (161s read, 132s write)

**Focus: Reduce the 349s sorting time**

## Optimization Paths (Ordered by Impact vs. Effort)

### 1. **More Bins** (EASIEST, HIGH IMPACT)

Since sorting is O(n log n), doubling the number of bins does MORE than halve the sort time per bin:
- 2 bins of 50 GiB each: 2 × (50 log 50)
- 4 bins of 25 GiB each: 4 × (25 log 25) ≈ **40% less total work**

Try multiplying your bin count by 4x or 8x:

```rust
fn compute_num_bins(file_size: u64, memory_limit: u64) -> usize {
    let usable = ((memory_limit as f64) * 0.75) as u64;
    let usable = usable.max(RECORD_SIZE as u64 * 2);
    let min_bins = file_size.div_ceil(usable).max(2) as usize;
    let base = min_bins.next_power_of_two();
    base * 8  // <-- Try 4x or 8x more bins
}
```

**Trade-off**: More scatter I/O overhead, but your I/O is already much faster than sorting (161s vs 349s).

### 2. **Sort Keys Only, Then Permute** (MEDIUM EFFORT, HIGH IMPACT)

Sort small (key, index) pairs instead of moving 128-byte records:

```rust
// Extract keys
let mut key_index: Vec<(u128, u32)> = records
    .iter()
    .enumerate()
    .map(|(i, rec)| {
        let key = u128::from_le_bytes(rec[..16].try_into().unwrap());
        (key, i as u32)
    })
    .collect();

// Sort 24-byte pairs (not 128-byte records)
key_index.par_sort_unstable_by_key(|&(k, _)| k);

// Permute records in-place based on sorted indices
permute_records(&mut records, &key_index);
```

**Why faster**: Moving 24 bytes during sort vs 128 bytes = ~5x less memory bandwidth, better cache utilization.

### 3. **Enable Native CPU Features** (FREE)

Make sure you're compiling with:
```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

This enables AVX2/AVX-512/BMI2 etc. for your specific CPU.

### 4. **Profile What's Actually Slow**

Quick profile to see what's eating the 349s:
```bash
perf record -g ./rustbucket sort ...
perf report
```

Or use `cargo flamegraph`:
```bash
cargo install flamegraph
cargo flamegraph --bin rustbucket -- sort ...
```

This will show if the time is in:
- Comparisons
- Memory movement
- Cache misses
- Something unexpected

## Recommended Order

1. **4x-8x more bins** (5 minute change, could cut sort time in half)
2. **RUSTFLAGS="-C target-cpu=native"** (free performance)
3. **Key-only sort** if the above aren't enough (1-2 hours of work)

The bin count is likely too conservative. You have fast NVMe - lean into more bins, smaller sorts.

## Previous Attempts

- **Radix sort**: Tried, was slower (didn't investigate why)
  - Likely issues: 16 passes, poor cache locality, random scatter/gather patterns
