<img width="512" height="512" alt="rust_bucket" src="https://github.com/user-attachments/assets/46a3d3a0-0751-4c08-a20e-a513b5573ff4" />

A simple Rust implementation of the classic **large-scale sorting benchmarks** that once pushed
entire storage systems to their limits. The goal is to revisit those benchmarks on **modern hardware**
and see how much storage performance has improved.

## Platform Support

**rustbucket** is now cross-platform with optimized implementations for different operating systems:

- **Linux**: Uses `io_uring` for high-performance async I/O, O_DIRECT for bypassing page cache, and memory hints (MADV_HUGEPAGE) for optimal performance
- **macOS/Windows**: Uses standard file I/O with the same algorithmic approach, maintaining portability while sacrificing some performance compared to Linux

The Linux version provides the best performance due to io_uring's zero-copy I/O and kernel-level optimizations, but the portable version works across all platforms.

---

### A Bit of History

In the late 1990s and early 2000s, the database research community created the **Sort Benchmark** to measure full-system performance.

Some well-known variants included:

- **Daytona** – production-quality external sort
- **PennySort** – cheapest system capable of sorting 1 TB
- **MinuteSort** – sort as much data as possible in one minute
- **TerabyteSort** – sort 1 TB as fast as possible

These benchmarks stressed the entire system: CPU, memory, disk, and I/O scheduling.

At the time, sorting **1 TB** was a major engineering challenge.

---

### Hardware Then vs Now

Around 2000, high-end benchmark systems often looked like:

- dozens of spinning disks
- < 1 GB RAM
- large SCSI arrays
- ~500 MB/s aggregate I/O

Sorting 1 TB could take **hours**.

Today, a single NVMe drive can deliver **5–7 GB/s**, and a workstation with multiple drives can exceed **20 GB/s**.

The same 1 TB sort that once required racks of disks can now finish in **minutes** on a desktop.

Taken from a white paper about nsort running on a SGI setup.
```
In order to demonstrate Nsort's ability to sort large data sets, we
sorted a terabyte of data (10,000,000,000 100-byte records with random
10-byte keys) in 2.5 hours. The Origin2000 system for this September
1997 result included 32 processors, 8GB of main memory, and 559 4GB
disks:

• 1 system disk
• a 280-disk XLV volume for input and output files
• 278 temporary disks

Nsort read a terabyte input file from the 280-disk file system,
partially sorted the data and wrote it to the temporary disks. The
partially sorted data was then read from the temporary files and
merged to produce a 1-terabyte output file. To save on disk space, the
input file was overwritten to produce the output file.

Note that the 110 MB/sec speed of the terabyte sort was not much lower
than the 127 MB/sec speed of the two-pass MinuteSort, even though more
than two orders of magnitude more data was sorted. Fairly uniform
performance can be expected for two-pass sorts with data sizes between
1 gigabyte and 1 terabyte. For MinuteSort-type records, this is a
consistent sort rate of more than a million records per second.
```

---

### Status

## 1 TiB Sort Benchmark

The first full **1 TiB run** was performed using git commit
`6d5c30130d62694c23ac721b2363dfae100b2263`.

### System

- **CPU:** AMD Threadripper Pro 5945WX
- **Memory:** 128 GiB RAM
- **Storage:** 3× NVMe SSD (gen4)

### Storage Layout

- **1 NVMe:** Input file and final output
- **2 NVMe:** Scratch space used during scatter and bin sorting

### Sort Configuration

- **Dataset size:** 1 TiB
- **Record size:** 128 bytes (16 byte key)
- **# of records:** 8,589,934,592
- **Memory budget:** ~16 GiB

The memory budget has not yet been strictly validated, though earlier testing indicated that **smaller working sets often produced better performance**.

As in the case of the original `nsort` run on SGI hardware, the input file had to be removed before writing the sorted output in order to free enough space.

This run is **not an exact reproduction** of the original benchmark. In this case the dataset is a
 full **1 TiB**, and the records are **128 bytes rather than 100 bytes**. When time permits, I plan to attempt a closer reproduction of the original benchmark configuration.

I also still need to determine the **key size used in the original records**, this one uses 128bit keys.

### Result


full output from run

```bash
$ rustbucket sort 1tib_input.dat  1tib_sorted.dat /mnt/scratch2/tony /mnt/scratch3/tony --memory 16GiB --remove-input
Sort: 1024.000 GiB input, 128 bins, 8.0 MiB per bin write-buffer, 16.0 GiB memory budget
Scatter: 8589934592 records across 128 bins (2 scratch dir(s))
  scatter 5.0%  4067 MiB/s
  scatter 10.0%  4109 MiB/s
  scatter 15.0%  4119 MiB/s
  scatter 20.0%  4124 MiB/s
  scatter 25.0%  4122 MiB/s
  scatter 30.0%  4123 MiB/s
  scatter 35.0%  4124 MiB/s
  scatter 40.0%  4107 MiB/s
  scatter 45.0%  3943 MiB/s
  scatter 50.0%  3815 MiB/s
  scatter 55.0%  3715 MiB/s
  scatter 60.0%  3638 MiB/s
  scatter 65.0%  3573 MiB/s
  scatter 70.0%  3522 MiB/s
  scatter 75.0%  3477 MiB/s
  scatter 80.0%  3438 MiB/s
  scatter 85.0%  3406 MiB/s
  scatter 90.0%  3378 MiB/s
  scatter 95.0%  3352 MiB/s
  scatter 100.0%  3331 MiB/s
Scatter done: 315.0s  3329 MiB/s
Phase 1 done: 8589934592 records scattered
Input file removed: 1tib_input.dat
Gather: writing 8589934592 sorted records to output
  [reader] bin    0: 8193.2 MiB  read 4.205s  wait-pool 0.000s  wait-sorter 0.000s
  [reader] bin    1: 8193.0 MiB  read 2.497s  wait-pool 0.000s  wait-sorter 0.000s
  [sorter] bin    0: 8193.2 MiB  sort 2.504s  wait-reader 4.205s  wait-writer 0.000s
  [writer] bin    0: 0.8%  1042 MiB/s  (wait-sorter 6.708s  write 1.154s)
  [reader] bin    2: 8193.1 MiB  read 1.216s  wait-pool 0.000s  wait-sorter 0.000s
  [sorter] bin    1: 8193.0 MiB  sort 2.937s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin    3: 8193.7 MiB  read 1.665s  wait-pool 0.000s  wait-sorter 0.063s
  [writer] bin    1: 1.6%  1530 MiB/s  (wait-sorter 1.783s  write 1.063s)
  [sorter] bin    2: 8193.1 MiB  sort 2.702s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin    4: 8191.1 MiB  read 1.907s  wait-pool 1.062s  wait-sorter 0.000s
  [writer] bin    2: 2.3%  1844 MiB/s  (wait-sorter 1.639s  write 0.981s)
  [sorter] bin    3: 8193.7 MiB  sort 2.676s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin    5: 8191.7 MiB  read 1.176s  wait-pool 0.714s  wait-sorter 0.519s
  [writer] bin    3: 3.1%  2033 MiB/s  (wait-sorter 1.695s  write 1.093s)
  [sorter] bin    4: 8191.1 MiB  sort 2.682s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin    6: 8194.0 MiB  read 1.227s  wait-pool 1.093s  wait-sorter 0.361s
  [writer] bin    4: 3.9%  2190 MiB/s  (wait-sorter 1.588s  write 1.002s)
  [sorter] bin    5: 8191.7 MiB  sort 2.700s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin    7: 8191.4 MiB  read 1.172s  wait-pool 1.002s  wait-sorter 0.526s
  [writer] bin    5: 4.7%  2289 MiB/s  (wait-sorter 1.698s  write 1.073s)
  [sorter] bin    6: 8194.0 MiB  sort 2.716s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin    8: 8190.6 MiB  read 1.186s  wait-pool 1.073s  wait-sorter 0.457s
  [writer] bin    6: 5.5%  2372 MiB/s  (wait-sorter 1.642s  write 1.057s)
  [sorter] bin    7: 8191.4 MiB  sort 2.863s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin    9: 8191.1 MiB  read 1.182s  wait-pool 1.057s  wait-sorter 0.624s
  [writer] bin    7: 6.3%  2427 MiB/s  (wait-sorter 1.806s  write 1.017s)
  [sorter] bin    8: 8190.6 MiB  sort 2.752s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   10: 8192.4 MiB  read 1.183s  wait-pool 1.018s  wait-sorter 0.552s
  [writer] bin    8: 7.0%  2476 MiB/s  (wait-sorter 1.735s  write 1.040s)
  [sorter] bin    9: 8191.1 MiB  sort 2.788s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   11: 8192.2 MiB  read 1.187s  wait-pool 1.040s  wait-sorter 0.560s
  [writer] bin    9: 7.8%  2518 MiB/s  (wait-sorter 1.747s  write 1.011s)
  [sorter] bin   10: 8192.4 MiB  sort 2.666s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   12: 8191.8 MiB  read 1.166s  wait-pool 1.011s  wait-sorter 0.490s
  [writer] bin   10: 8.6%  2561 MiB/s  (wait-sorter 1.655s  write 1.000s)
  [sorter] bin   11: 8192.2 MiB  sort 2.639s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   13: 8190.1 MiB  read 1.182s  wait-pool 1.000s  wait-sorter 0.457s
  [writer] bin   11: 9.4%  2600 MiB/s  (wait-sorter 1.639s  write 0.978s)
  [sorter] bin   12: 8191.8 MiB  sort 2.822s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   14: 8189.8 MiB  read 1.174s  wait-pool 0.977s  wait-sorter 0.670s
  [writer] bin   12: 10.2%  2617 MiB/s  (wait-sorter 1.844s  write 1.047s)
  [sorter] bin   13: 8190.1 MiB  sort 2.651s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   15: 8189.8 MiB  read 1.168s  wait-pool 1.047s  wait-sorter 0.436s
  [writer] bin   13: 10.9%  2643 MiB/s  (wait-sorter 1.604s  write 1.097s)
  [sorter] bin   14: 8189.8 MiB  sort 2.741s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   16: 8192.1 MiB  read 1.168s  wait-pool 1.097s  wait-sorter 0.477s
  [writer] bin   14: 11.7%  2666 MiB/s  (wait-sorter 1.644s  write 1.051s)
  [sorter] bin   15: 8189.8 MiB  sort 2.675s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   17: 8191.4 MiB  read 1.177s  wait-pool 1.051s  wait-sorter 0.447s
  [writer] bin   15: 12.5%  2687 MiB/s  (wait-sorter 1.624s  write 1.052s)
  [sorter] bin   16: 8192.1 MiB  sort 2.862s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   18: 8193.0 MiB  read 1.184s  wait-pool 1.052s  wait-sorter 0.626s
  [writer] bin   16: 13.3%  2700 MiB/s  (wait-sorter 1.811s  write 0.990s)
  [sorter] bin   17: 8191.4 MiB  sort 2.768s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   19: 8191.3 MiB  read 1.183s  wait-pool 0.990s  wait-sorter 0.596s
  [writer] bin   17: 14.1%  2714 MiB/s  (wait-sorter 1.778s  write 0.972s)
  [sorter] bin   18: 8193.0 MiB  sort 2.653s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   20: 8190.9 MiB  read 1.193s  wait-pool 0.972s  wait-sorter 0.488s
  [writer] bin   18: 14.8%  2730 MiB/s  (wait-sorter 1.681s  write 1.021s)
  [sorter] bin   19: 8191.3 MiB  sort 2.682s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   21: 8192.8 MiB  read 1.180s  wait-pool 1.020s  wait-sorter 0.481s
  [writer] bin   19: 15.6%  2745 MiB/s  (wait-sorter 1.661s  write 1.005s)
  [sorter] bin   20: 8190.9 MiB  sort 2.658s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   22: 8193.9 MiB  read 1.180s  wait-pool 1.005s  wait-sorter 0.472s
  [writer] bin   20: 16.4%  2756 MiB/s  (wait-sorter 1.652s  write 1.071s)
  [sorter] bin   21: 8192.8 MiB  sort 2.647s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   23: 8193.0 MiB  read 1.188s  wait-pool 1.070s  wait-sorter 0.388s
  [writer] bin   21: 17.2%  2774 MiB/s  (wait-sorter 1.576s  write 0.977s)
  [sorter] bin   22: 8193.9 MiB  sort 2.704s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   24: 8193.7 MiB  read 1.177s  wait-pool 0.977s  wait-sorter 0.550s
  [writer] bin   22: 18.0%  2783 MiB/s  (wait-sorter 1.727s  write 1.011s)
  [sorter] bin   23: 8193.0 MiB  sort 2.717s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   25: 8191.7 MiB  read 1.197s  wait-pool 1.011s  wait-sorter 0.509s
  [writer] bin   23: 18.7%  2793 MiB/s  (wait-sorter 1.706s  write 0.989s)
  [sorter] bin   24: 8193.7 MiB  sort 2.619s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   26: 8193.4 MiB  read 1.174s  wait-pool 0.989s  wait-sorter 0.457s
  [writer] bin   24: 19.5%  2805 MiB/s  (wait-sorter 1.631s  write 0.983s)
  [sorter] bin   25: 8191.7 MiB  sort 2.640s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   27: 8192.1 MiB  read 1.184s  wait-pool 0.982s  wait-sorter 0.474s
  [writer] bin   25: 20.3%  2814 MiB/s  (wait-sorter 1.657s  write 1.019s)
  [sorter] bin   26: 8193.4 MiB  sort 2.727s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   28: 8192.4 MiB  read 1.185s  wait-pool 1.019s  wait-sorter 0.524s
  [writer] bin   26: 21.1%  2820 MiB/s  (wait-sorter 1.709s  write 1.048s)
  [sorter] bin   27: 8192.1 MiB  sort 2.725s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   29: 8191.7 MiB  read 1.174s  wait-pool 1.048s  wait-sorter 0.502s
  [writer] bin   27: 21.9%  2822 MiB/s  (wait-sorter 1.676s  write 1.151s)
  [sorter] bin   28: 8192.4 MiB  sort 2.776s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   30: 8192.2 MiB  read 1.187s  wait-pool 1.151s  wait-sorter 0.438s
  [writer] bin   28: 22.7%  2829 MiB/s  (wait-sorter 1.625s  write 1.064s)
  [sorter] bin   29: 8191.7 MiB  sort 2.714s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   31: 8192.4 MiB  read 1.187s  wait-pool 1.064s  wait-sorter 0.463s
  [writer] bin   29: 23.4%  2837 MiB/s  (wait-sorter 1.650s  write 1.016s)
  [sorter] bin   30: 8192.2 MiB  sort 2.645s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   32: 8192.0 MiB  read 1.172s  wait-pool 1.016s  wait-sorter 0.456s
  [writer] bin   30: 24.2%  2844 MiB/s  (wait-sorter 1.628s  write 1.032s)
  [sorter] bin   31: 8192.4 MiB  sort 2.717s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   33: 8192.2 MiB  read 1.201s  wait-pool 1.032s  wait-sorter 0.485s
  [writer] bin   31: 25.0%  2849 MiB/s  (wait-sorter 1.686s  write 1.035s)
  [sorter] bin   32: 8192.0 MiB  sort 2.664s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   34: 8192.0 MiB  read 1.181s  wait-pool 1.035s  wait-sorter 0.449s
  [writer] bin   32: 25.8%  2856 MiB/s  (wait-sorter 1.628s  write 1.006s)
  [sorter] bin   33: 8192.2 MiB  sort 2.663s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   35: 8192.0 MiB  read 1.194s  wait-pool 1.005s  wait-sorter 0.463s
  [writer] bin   33: 26.6%  2861 MiB/s  (wait-sorter 1.657s  write 1.048s)
  [sorter] bin   34: 8192.0 MiB  sort 2.774s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   36: 8193.0 MiB  read 1.217s  wait-pool 1.048s  wait-sorter 0.510s
  [writer] bin   34: 27.3%  2865 MiB/s  (wait-sorter 1.726s  write 1.014s)
  [sorter] bin   35: 8192.0 MiB  sort 2.766s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   37: 8191.2 MiB  read 1.189s  wait-pool 1.014s  wait-sorter 0.563s
  [writer] bin   35: 28.1%  2867 MiB/s  (wait-sorter 1.752s  write 1.020s)
  [sorter] bin   36: 8193.0 MiB  sort 2.741s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   38: 8190.7 MiB  read 1.255s  wait-pool 1.020s  wait-sorter 0.466s
  [writer] bin   36: 28.9%  2872 MiB/s  (wait-sorter 1.722s  write 0.950s)
  [sorter] bin   37: 8191.2 MiB  sort 2.650s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   39: 8192.3 MiB  read 1.237s  wait-pool 0.950s  wait-sorter 0.464s
  [writer] bin   37: 29.7%  2877 MiB/s  (wait-sorter 1.701s  write 0.977s)
  [sorter] bin   38: 8190.7 MiB  sort 2.616s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   40: 8191.5 MiB  read 1.228s  wait-pool 0.977s  wait-sorter 0.411s
  [writer] bin   38: 30.5%  2883 MiB/s  (wait-sorter 1.639s  write 0.986s)
  [sorter] bin   39: 8192.3 MiB  sort 2.738s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   41: 8192.3 MiB  read 1.242s  wait-pool 0.986s  wait-sorter 0.510s
  [writer] bin   39: 31.3%  2884 MiB/s  (wait-sorter 1.753s  write 1.039s)
  [sorter] bin   40: 8191.5 MiB  sort 2.757s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   42: 8191.8 MiB  read 1.254s  wait-pool 1.039s  wait-sorter 0.465s
  [writer] bin   40: 32.0%  2885 MiB/s  (wait-sorter 1.719s  write 1.090s)
  [sorter] bin   41: 8192.3 MiB  sort 2.925s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   43: 8191.5 MiB  read 1.262s  wait-pool 1.090s  wait-sorter 0.573s
  [writer] bin   41: 32.8%  2880 MiB/s  (wait-sorter 1.836s  write 1.177s)
  [sorter] bin   42: 8191.8 MiB  sort 3.143s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   44: 8192.7 MiB  read 1.215s  wait-pool 1.177s  wait-sorter 0.750s
  [writer] bin   42: 33.6%  2877 MiB/s  (wait-sorter 1.965s  write 1.027s)
  [sorter] bin   43: 8191.5 MiB  sort 2.694s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   45: 8192.6 MiB  read 1.223s  wait-pool 1.026s  wait-sorter 0.445s
  [writer] bin   43: 34.4%  2879 MiB/s  (wait-sorter 1.668s  write 1.109s)
  [sorter] bin   44: 8192.7 MiB  sort 2.774s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   46: 8190.6 MiB  read 1.177s  wait-pool 1.109s  wait-sorter 0.488s
  [writer] bin   44: 35.2%  2883 MiB/s  (wait-sorter 1.665s  write 0.990s)
  [sorter] bin   45: 8192.6 MiB  sort 2.709s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   47: 8191.0 MiB  read 1.240s  wait-pool 0.990s  wait-sorter 0.480s
  [writer] bin   45: 35.9%  2885 MiB/s  (wait-sorter 1.720s  write 1.041s)
  [sorter] bin   46: 8190.6 MiB  sort 2.701s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   48: 8191.6 MiB  read 1.240s  wait-pool 1.041s  wait-sorter 0.421s
  [writer] bin   46: 36.7%  2888 MiB/s  (wait-sorter 1.661s  write 1.009s)
  [sorter] bin   47: 8191.0 MiB  sort 2.695s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   49: 8190.3 MiB  read 1.225s  wait-pool 1.009s  wait-sorter 0.460s
  [writer] bin   47: 37.5%  2890 MiB/s  (wait-sorter 1.686s  write 1.057s)
  [sorter] bin   48: 8191.6 MiB  sort 2.652s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   50: 8192.4 MiB  read 1.244s  wait-pool 1.057s  wait-sorter 0.352s
  [writer] bin   48: 38.3%  2895 MiB/s  (wait-sorter 1.595s  write 1.013s)
  [sorter] bin   49: 8190.3 MiB  sort 2.679s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   51: 8192.7 MiB  read 1.221s  wait-pool 1.013s  wait-sorter 0.445s
  [writer] bin   49: 39.1%  2897 MiB/s  (wait-sorter 1.666s  write 1.059s)
  [sorter] bin   50: 8192.4 MiB  sort 2.636s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   52: 8192.5 MiB  read 1.217s  wait-pool 1.058s  wait-sorter 0.360s
  [writer] bin   50: 39.8%  2902 MiB/s  (wait-sorter 1.577s  write 1.018s)
  [sorter] bin   51: 8192.7 MiB  sort 2.651s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   53: 8192.8 MiB  read 1.210s  wait-pool 1.018s  wait-sorter 0.423s
  [writer] bin   51: 40.6%  2905 MiB/s  (wait-sorter 1.633s  write 1.026s)
  [sorter] bin   52: 8192.5 MiB  sort 2.616s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   54: 8193.4 MiB  read 1.175s  wait-pool 1.026s  wait-sorter 0.415s
  [writer] bin   52: 41.4%  2910 MiB/s  (wait-sorter 1.590s  write 0.993s)
  [sorter] bin   53: 8192.8 MiB  sort 2.705s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   55: 8191.8 MiB  read 1.233s  wait-pool 0.993s  wait-sorter 0.479s
  [writer] bin   53: 42.2%  2911 MiB/s  (wait-sorter 1.713s  write 1.030s)
  [sorter] bin   54: 8193.4 MiB  sort 2.618s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   56: 8191.6 MiB  read 1.238s  wait-pool 1.031s  wait-sorter 0.350s
  [writer] bin   54: 43.0%  2912 MiB/s  (wait-sorter 1.587s  write 1.156s)
  [sorter] bin   55: 8191.8 MiB  sort 3.188s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   57: 8192.6 MiB  read 1.238s  wait-pool 1.156s  wait-sorter 0.795s
  [writer] bin   55: 43.8%  2907 MiB/s  (wait-sorter 2.032s  write 1.060s)
  [sorter] bin   56: 8191.6 MiB  sort 2.797s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   58: 8191.9 MiB  read 1.217s  wait-pool 1.060s  wait-sorter 0.519s
  [writer] bin   56: 44.5%  2907 MiB/s  (wait-sorter 1.737s  write 1.090s)
  [sorter] bin   57: 8192.6 MiB  sort 2.869s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   59: 8191.9 MiB  read 1.240s  wait-pool 1.090s  wait-sorter 0.539s
  [writer] bin   57: 45.3%  2908 MiB/s  (wait-sorter 1.779s  write 1.007s)
  [sorter] bin   58: 8191.9 MiB  sort 2.666s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   60: 8192.5 MiB  read 1.214s  wait-pool 1.007s  wait-sorter 0.446s
  [writer] bin   58: 46.1%  2910 MiB/s  (wait-sorter 1.659s  write 1.048s)
  [sorter] bin   59: 8191.9 MiB  sort 2.656s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   61: 8192.6 MiB  read 1.213s  wait-pool 1.048s  wait-sorter 0.396s
  [writer] bin   59: 46.9%  2913 MiB/s  (wait-sorter 1.608s  write 0.989s)
  [sorter] bin   60: 8192.5 MiB  sort 2.759s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   62: 8191.0 MiB  read 1.265s  wait-pool 0.989s  wait-sorter 0.505s
  [writer] bin   60: 47.7%  2914 MiB/s  (wait-sorter 1.770s  write 1.014s)
  [sorter] bin   61: 8192.6 MiB  sort 2.698s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   63: 8191.1 MiB  read 1.223s  wait-pool 1.013s  wait-sorter 0.461s
  [writer] bin   61: 48.4%  2915 MiB/s  (wait-sorter 1.684s  write 1.071s)
  [sorter] bin   62: 8191.0 MiB  sort 2.811s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   64: 8193.1 MiB  read 1.220s  wait-pool 1.071s  wait-sorter 0.520s
  [writer] bin   62: 49.2%  2916 MiB/s  (wait-sorter 1.740s  write 0.998s)
  [sorter] bin   63: 8191.1 MiB  sort 2.679s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   65: 8192.6 MiB  read 1.241s  wait-pool 0.998s  wait-sorter 0.440s
  [writer] bin   63: 50.0%  2917 MiB/s  (wait-sorter 1.681s  write 1.096s)
  [sorter] bin   64: 8193.1 MiB  sort 2.758s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   66: 8191.5 MiB  read 1.214s  wait-pool 1.099s  wait-sorter 0.445s
  [writer] bin   64: 50.8%  2919 MiB/s  (wait-sorter 1.662s  write 1.014s)
  [sorter] bin   65: 8192.6 MiB  sort 2.691s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   67: 8191.4 MiB  read 1.234s  wait-pool 1.014s  wait-sorter 0.444s
  [writer] bin   65: 51.6%  2921 MiB/s  (wait-sorter 1.678s  write 0.990s)
  [sorter] bin   66: 8191.5 MiB  sort 2.637s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   68: 8191.3 MiB  read 1.228s  wait-pool 0.990s  wait-sorter 0.420s
  [writer] bin   66: 52.3%  2923 MiB/s  (wait-sorter 1.647s  write 1.037s)
  [sorter] bin   67: 8191.4 MiB  sort 2.747s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   69: 8192.4 MiB  read 1.224s  wait-pool 1.037s  wait-sorter 0.486s
  [writer] bin   67: 53.1%  2924 MiB/s  (wait-sorter 1.710s  write 1.019s)
  [sorter] bin   68: 8191.3 MiB  sort 2.748s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   70: 8191.3 MiB  read 1.219s  wait-pool 1.019s  wait-sorter 0.510s
  [writer] bin   68: 53.9%  2925 MiB/s  (wait-sorter 1.729s  write 0.975s)
  [sorter] bin   69: 8192.4 MiB  sort 2.703s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   71: 8192.0 MiB  read 1.229s  wait-pool 0.975s  wait-sorter 0.499s
  [writer] bin   69: 54.7%  2926 MiB/s  (wait-sorter 1.728s  write 1.039s)
  [sorter] bin   70: 8191.3 MiB  sort 2.688s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   72: 8194.5 MiB  read 1.250s  wait-pool 1.039s  wait-sorter 0.399s
  [writer] bin   70: 55.5%  2928 MiB/s  (wait-sorter 1.649s  write 0.972s)
  [sorter] bin   71: 8192.0 MiB  sort 2.639s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   73: 8192.5 MiB  read 1.223s  wait-pool 0.972s  wait-sorter 0.443s
  [writer] bin   71: 56.3%  2930 MiB/s  (wait-sorter 1.666s  write 1.039s)
  [sorter] bin   72: 8194.5 MiB  sort 2.671s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   74: 8192.1 MiB  read 1.215s  wait-pool 1.039s  wait-sorter 0.418s
  [writer] bin   72: 57.0%  2932 MiB/s  (wait-sorter 1.632s  write 1.003s)
  [sorter] bin   73: 8192.5 MiB  sort 2.863s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   75: 8194.0 MiB  read 1.248s  wait-pool 1.003s  wait-sorter 0.611s
  [writer] bin   73: 57.8%  2931 MiB/s  (wait-sorter 1.859s  write 1.046s)
  [sorter] bin   74: 8192.1 MiB  sort 2.812s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   76: 8192.2 MiB  read 1.185s  wait-pool 1.046s  wait-sorter 0.581s
  [writer] bin   74: 58.6%  2931 MiB/s  (wait-sorter 1.766s  write 0.988s)
  [sorter] bin   75: 8194.0 MiB  sort 2.821s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   77: 8192.8 MiB  read 1.224s  wait-pool 0.988s  wait-sorter 0.608s
  [writer] bin   75: 59.4%  2929 MiB/s  (wait-sorter 1.832s  write 1.080s)
  [sorter] bin   76: 8192.2 MiB  sort 2.819s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   78: 8194.0 MiB  read 1.241s  wait-pool 1.080s  wait-sorter 0.498s
  [writer] bin   76: 60.2%  2929 MiB/s  (wait-sorter 1.739s  write 1.056s)
  [sorter] bin   77: 8192.8 MiB  sort 2.652s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   79: 8193.2 MiB  read 1.176s  wait-pool 1.056s  wait-sorter 0.420s
  [writer] bin   77: 60.9%  2932 MiB/s  (wait-sorter 1.596s  write 1.030s)
  [sorter] bin   78: 8194.0 MiB  sort 2.771s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   80: 8192.0 MiB  read 1.220s  wait-pool 1.030s  wait-sorter 0.521s
  [writer] bin   78: 61.7%  2932 MiB/s  (wait-sorter 1.740s  write 1.027s)
  [sorter] bin   79: 8193.2 MiB  sort 2.678s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   81: 8191.6 MiB  read 1.214s  wait-pool 1.027s  wait-sorter 0.438s
  [writer] bin   79: 62.5%  2934 MiB/s  (wait-sorter 1.651s  write 1.014s)
  [sorter] bin   80: 8192.0 MiB  sort 2.662s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   82: 8192.6 MiB  read 1.237s  wait-pool 1.014s  wait-sorter 0.412s
  [writer] bin   80: 63.3%  2936 MiB/s  (wait-sorter 1.648s  write 1.012s)
  [sorter] bin   81: 8191.6 MiB  sort 2.766s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   83: 8190.3 MiB  read 1.248s  wait-pool 1.012s  wait-sorter 0.506s
  [writer] bin   81: 64.1%  2936 MiB/s  (wait-sorter 1.754s  write 1.002s)
  [sorter] bin   82: 8192.6 MiB  sort 2.721s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   84: 8191.8 MiB  read 1.212s  wait-pool 1.004s  wait-sorter 0.505s
  [writer] bin   82: 64.8%  2935 MiB/s  (wait-sorter 1.719s  write 1.133s)
  [sorter] bin   83: 8190.3 MiB  sort 2.742s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   85: 8190.6 MiB  read 1.242s  wait-pool 1.132s  wait-sorter 0.367s
  [writer] bin   83: 65.6%  2937 MiB/s  (wait-sorter 1.609s  write 1.014s)
  [sorter] bin   84: 8191.8 MiB  sort 2.659s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   86: 8193.0 MiB  read 1.184s  wait-pool 1.015s  wait-sorter 0.461s
  [writer] bin   84: 66.4%  2938 MiB/s  (wait-sorter 1.645s  write 1.073s)
  [sorter] bin   85: 8190.6 MiB  sort 2.827s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   87: 8191.2 MiB  read 1.238s  wait-pool 1.073s  wait-sorter 0.516s
  [writer] bin   85: 67.2%  2938 MiB/s  (wait-sorter 1.754s  write 1.037s)
  [sorter] bin   86: 8193.0 MiB  sort 2.768s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   88: 8192.2 MiB  read 1.222s  wait-pool 1.037s  wait-sorter 0.510s
  [writer] bin   86: 68.0%  2938 MiB/s  (wait-sorter 1.731s  write 1.031s)
  [sorter] bin   87: 8191.2 MiB  sort 2.732s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   89: 8191.3 MiB  read 1.218s  wait-pool 1.031s  wait-sorter 0.482s
  [writer] bin   87: 68.8%  2939 MiB/s  (wait-sorter 1.701s  write 1.050s)
  [sorter] bin   88: 8192.2 MiB  sort 2.673s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   90: 8191.4 MiB  read 1.221s  wait-pool 1.050s  wait-sorter 0.401s
  [writer] bin   88: 69.5%  2940 MiB/s  (wait-sorter 1.623s  write 1.075s)
  [sorter] bin   89: 8191.3 MiB  sort 2.801s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   91: 8192.1 MiB  read 1.258s  wait-pool 1.075s  wait-sorter 0.469s
  [writer] bin   89: 70.3%  2939 MiB/s  (wait-sorter 1.725s  write 1.127s)
  [sorter] bin   90: 8191.4 MiB  sort 2.807s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   92: 8191.3 MiB  read 1.217s  wait-pool 1.127s  wait-sorter 0.462s
  [writer] bin   90: 71.1%  2940 MiB/s  (wait-sorter 1.680s  write 1.062s)
  [sorter] bin   91: 8192.1 MiB  sort 2.839s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   93: 8190.7 MiB  read 1.262s  wait-pool 1.062s  wait-sorter 0.515s
  [writer] bin   91: 71.9%  2939 MiB/s  (wait-sorter 1.777s  write 1.040s)
  [sorter] bin   92: 8191.3 MiB  sort 2.643s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   94: 8190.8 MiB  read 1.246s  wait-pool 1.040s  wait-sorter 0.357s
  [writer] bin   92: 72.7%  2942 MiB/s  (wait-sorter 1.604s  write 0.937s)
  [sorter] bin   93: 8190.7 MiB  sort 2.786s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   95: 8192.4 MiB  read 1.252s  wait-pool 0.938s  wait-sorter 0.596s
  [writer] bin   93: 73.4%  2940 MiB/s  (wait-sorter 1.848s  write 1.108s)
  [sorter] bin   94: 8190.8 MiB  sort 2.839s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   96: 8190.8 MiB  read 1.233s  wait-pool 1.108s  wait-sorter 0.498s
  [writer] bin   94: 74.2%  2940 MiB/s  (wait-sorter 1.732s  write 1.072s)
  [sorter] bin   95: 8192.4 MiB  sort 2.673s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   97: 8192.2 MiB  read 1.220s  wait-pool 1.074s  wait-sorter 0.379s
  [writer] bin   95: 75.0%  2941 MiB/s  (wait-sorter 1.599s  write 1.058s)
  [sorter] bin   96: 8190.8 MiB  sort 2.711s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   98: 8192.4 MiB  read 1.212s  wait-pool 1.058s  wait-sorter 0.441s
  [writer] bin   96: 75.8%  2943 MiB/s  (wait-sorter 1.653s  write 1.023s)
  [sorter] bin   97: 8192.2 MiB  sort 2.624s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin   99: 8191.1 MiB  read 1.243s  wait-pool 1.023s  wait-sorter 0.358s
  [writer] bin   97: 76.6%  2944 MiB/s  (wait-sorter 1.601s  write 1.049s)
  [sorter] bin   98: 8192.4 MiB  sort 2.639s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  100: 8192.2 MiB  read 1.213s  wait-pool 1.049s  wait-sorter 0.378s
  [writer] bin   98: 77.3%  2946 MiB/s  (wait-sorter 1.590s  write 1.057s)
  [sorter] bin   99: 8191.1 MiB  sort 2.789s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  101: 8190.8 MiB  read 1.255s  wait-pool 1.057s  wait-sorter 0.477s
  [writer] bin   99: 78.1%  2946 MiB/s  (wait-sorter 1.732s  write 0.982s)
  [sorter] bin  100: 8192.2 MiB  sort 2.681s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  102: 8191.3 MiB  read 1.254s  wait-pool 0.982s  wait-sorter 0.445s
  [writer] bin  100: 78.9%  2947 MiB/s  (wait-sorter 1.699s  write 1.008s)
  [sorter] bin  101: 8190.8 MiB  sort 2.663s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  103: 8191.6 MiB  read 1.219s  wait-pool 1.008s  wait-sorter 0.436s
  [writer] bin  101: 79.7%  2948 MiB/s  (wait-sorter 1.655s  write 1.065s)
  [sorter] bin  102: 8191.3 MiB  sort 2.682s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  104: 8192.7 MiB  read 1.248s  wait-pool 1.065s  wait-sorter 0.369s
  [writer] bin  102: 80.5%  2947 MiB/s  (wait-sorter 1.617s  write 1.207s)
  [sorter] bin  103: 8191.6 MiB  sort 2.848s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  105: 8191.2 MiB  read 1.258s  wait-pool 1.207s  wait-sorter 0.384s
  [writer] bin  103: 81.2%  2948 MiB/s  (wait-sorter 1.641s  write 1.039s)
  [sorter] bin  104: 8192.7 MiB  sort 2.785s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  106: 8191.1 MiB  read 1.216s  wait-pool 1.039s  wait-sorter 0.530s
  [writer] bin  104: 82.0%  2948 MiB/s  (wait-sorter 1.746s  write 1.052s)
  [sorter] bin  105: 8191.2 MiB  sort 2.763s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  107: 8193.9 MiB  read 1.225s  wait-pool 1.052s  wait-sorter 0.486s
  [writer] bin  105: 82.8%  2949 MiB/s  (wait-sorter 1.711s  write 0.991s)
  [sorter] bin  106: 8191.1 MiB  sort 2.727s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  108: 8193.0 MiB  read 1.256s  wait-pool 0.991s  wait-sorter 0.480s
  [writer] bin  106: 83.6%  2949 MiB/s  (wait-sorter 1.736s  write 1.011s)
  [sorter] bin  107: 8193.9 MiB  sort 2.755s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  109: 8192.3 MiB  read 1.238s  wait-pool 1.011s  wait-sorter 0.506s
  [writer] bin  107: 84.4%  2949 MiB/s  (wait-sorter 1.744s  write 0.998s)
  [sorter] bin  108: 8193.0 MiB  sort 2.759s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  110: 8191.2 MiB  read 1.225s  wait-pool 0.998s  wait-sorter 0.536s
  [writer] bin  108: 85.2%  2950 MiB/s  (wait-sorter 1.761s  write 0.983s)
  [sorter] bin  109: 8192.3 MiB  sort 2.676s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  111: 8191.6 MiB  read 1.212s  wait-pool 0.983s  wait-sorter 0.481s
  [writer] bin  109: 85.9%  2950 MiB/s  (wait-sorter 1.693s  write 1.045s)
  [sorter] bin  110: 8191.2 MiB  sort 2.777s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  112: 8190.7 MiB  read 1.223s  wait-pool 1.045s  wait-sorter 0.508s
  [writer] bin  110: 86.7%  2950 MiB/s  (wait-sorter 1.731s  write 1.034s)
  [sorter] bin  111: 8191.6 MiB  sort 2.707s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  113: 8192.4 MiB  read 1.245s  wait-pool 1.034s  wait-sorter 0.428s
  [writer] bin  111: 87.5%  2951 MiB/s  (wait-sorter 1.673s  write 1.044s)
  [sorter] bin  112: 8190.7 MiB  sort 2.937s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  114: 8190.4 MiB  read 1.236s  wait-pool 1.044s  wait-sorter 0.657s
  [writer] bin  112: 88.3%  2950 MiB/s  (wait-sorter 1.893s  write 1.006s)
  [sorter] bin  113: 8192.4 MiB  sort 2.676s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  115: 8193.6 MiB  read 1.227s  wait-pool 1.006s  wait-sorter 0.443s
  [writer] bin  113: 89.1%  2950 MiB/s  (wait-sorter 1.670s  write 1.029s)
  [sorter] bin  114: 8190.4 MiB  sort 2.682s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  116: 8190.9 MiB  read 1.214s  wait-pool 1.029s  wait-sorter 0.439s
  [writer] bin  114: 89.8%  2951 MiB/s  (wait-sorter 1.653s  write 1.063s)
  [sorter] bin  115: 8193.6 MiB  sort 2.768s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  117: 8192.6 MiB  read 1.222s  wait-pool 1.063s  wait-sorter 0.482s
  [writer] bin  115: 90.6%  2951 MiB/s  (wait-sorter 1.704s  write 1.047s)
  [sorter] bin  116: 8190.9 MiB  sort 2.769s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  118: 8192.6 MiB  read 1.217s  wait-pool 1.047s  wait-sorter 0.506s
  [writer] bin  116: 91.4%  2952 MiB/s  (wait-sorter 1.722s  write 0.981s)
  [sorter] bin  117: 8192.6 MiB  sort 2.681s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  119: 8193.4 MiB  read 1.222s  wait-pool 0.981s  wait-sorter 0.478s
  [writer] bin  117: 92.2%  2952 MiB/s  (wait-sorter 1.700s  write 1.040s)
  [sorter] bin  118: 8192.6 MiB  sort 2.634s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  120: 8191.6 MiB  read 1.216s  wait-pool 1.040s  wait-sorter 0.378s
  [writer] bin  118: 93.0%  2954 MiB/s  (wait-sorter 1.594s  write 0.978s)
  [sorter] bin  119: 8193.4 MiB  sort 2.619s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  121: 8193.3 MiB  read 1.257s  wait-pool 0.978s  wait-sorter 0.384s
  [writer] bin  119: 93.8%  2955 MiB/s  (wait-sorter 1.641s  write 0.960s)
  [sorter] bin  120: 8191.6 MiB  sort 2.854s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  122: 8192.0 MiB  read 1.229s  wait-pool 0.959s  wait-sorter 0.665s
  [writer] bin  120: 94.5%  2954 MiB/s  (wait-sorter 1.894s  write 1.026s)
  [sorter] bin  121: 8193.3 MiB  sort 2.727s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  123: 8192.9 MiB  read 1.231s  wait-pool 1.026s  wait-sorter 0.471s
  [writer] bin  121: 95.3%  2955 MiB/s  (wait-sorter 1.700s  write 1.016s)
  [sorter] bin  122: 8192.0 MiB  sort 2.661s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  124: 8190.7 MiB  read 1.254s  wait-pool 1.015s  wait-sorter 0.390s
  [writer] bin  122: 96.1%  2956 MiB/s  (wait-sorter 1.644s  write 1.021s)
  [sorter] bin  123: 8192.9 MiB  sort 2.770s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  125: 8191.4 MiB  read 1.215s  wait-pool 1.021s  wait-sorter 0.533s
  [writer] bin  123: 96.9%  2956 MiB/s  (wait-sorter 1.749s  write 1.008s)
  [sorter] bin  124: 8190.7 MiB  sort 2.642s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  126: 8192.1 MiB  read 1.249s  wait-pool 1.008s  wait-sorter 0.385s
  [writer] bin  124: 97.7%  2957 MiB/s  (wait-sorter 1.634s  write 1.006s)
  [sorter] bin  125: 8191.4 MiB  sort 2.801s  wait-reader 0.000s  wait-writer 0.000s
  [reader] bin  127: 8190.9 MiB  read 1.221s  wait-pool 1.006s  wait-sorter 0.574s
  [reader] totals: read 161.184s  wait-pool 127.809s  wait-sorter 59.116s
  [writer] bin  125: 98.4%  2956 MiB/s  (wait-sorter 1.795s  write 1.089s)
  [sorter] bin  126: 8192.1 MiB  sort 2.798s  wait-reader 0.000s  wait-writer 0.000s
  [writer] bin  126: 99.2%  2956 MiB/s  (wait-sorter 1.675s  write 0.984s)
  [sorter] bin  127: 8190.9 MiB  sort 2.592s  wait-reader 0.000s  wait-writer 0.000s
  [sorter] totals: sort 349.292s  wait-reader 4.205s  wait-writer 0.001s
  [writer] bin  127: 100.0%  2960 MiB/s  (wait-sorter 1.578s  write 0.679s)
  [writer] totals: wait-sorter 222.118s  write 131.981s
Gather done: 354.2s  2960 MiB/s
Phase 2 done: 8589934592 records written
Total: 669.3s  effective throughput 1567 MiB/s (based on input size)
```