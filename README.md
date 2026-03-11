<img width="512" height="512" alt="rust_bucket" src="https://github.com/user-attachments/assets/46a3d3a0-0751-4c08-a20e-a513b5573ff4" />

A simple Rust implementation of the classic **large-scale sorting benchmarks** that once pushed
entire storage systems to their limits. The goal is to revisit those benchmarks on **modern hardware**
and see how much storage performance has improved.

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
Early days of getting things working.  I'll post some results when things look good.
