# Benchmark History

记录历次性能优化的吞吐量与耗时变化。

---

### [2026-06-24 11:47:25] Pipeline Bench 20MB Refresh (Post-WebUI Merge)

```text
# docs/reports/benchmark_report.md (20MB input, 3 iterations)
scenario        | avg ms  | min ms  | max ms  | throughput MB/s
mmap+parallel   |  189.93 |  100.80 |  270.19 |  105.37
mmap+serial     | 4328.44 | 2733.99 | 5739.51 |    4.62
non_mmap+parallel| 113.70 |   96.08 |  126.38 |  176.01
non_mmap+serial | 2220.36 | 1690.94 | 2582.68 |    9.01
```

说明：本次以 20MB 样本刷新基准（之前的 100MB baseline 留作历史）。
`non_mmap+parallel` 仍是最快的并行路径，~176 MB/s。
serial 路径在 20MB 上吞吐骤降（4–9 MB/s），是预期行为——serial 主要给小文件用。

---


### [2026-03-14 15:13:10] Initial Baseline Setup (without plugins overhead) (Commit: f6f9785)

```text
Gnuplot not found, using plotters backend
StreamReader/iter_lines time:   [692.68 µs 707.03 µs 723.11 µs]
                        thrpt:  [172.86 MiB/s 176.80 MiB/s 180.46 MiB/s]
Found 5 outliers among 100 measurements (5.00%)
  5 (5.00%) high severe

CompressionPipeline/compress_str
                        time:   [5.9189 ms 6.0028 ms 6.1044 ms]
                        thrpt:  [20.477 MiB/s 20.824 MiB/s 21.119 MiB/s]
Found 7 outliers among 100 measurements (7.00%)
  1 (1.00%) low mild
  6 (6.00%) high mild
```

---

### [2026-03-14 15:37:37] Zero-Copy 1: Changed TextSlicer buffer to single String (Commit: f6f9785)

```text
Gnuplot not found, using plotters backend
StreamReader/iter_lines time:   [826.96 µs 846.52 µs 868.54 µs]
                        thrpt:  [143.92 MiB/s 147.66 MiB/s 149.92 MiB/s]
                 change:
                        time:   [+16.326% +19.789% +23.364%] (p = 0.00 < 0.05)
                        thrpt:  [-18.940% -16.519% -14.034%]
                        Performance has regressed.
Found 6 outliers among 100 measurements (6.00%)
  6 (6.00%) high mild

CompressionPipeline/compress_str
                        time:   [5.3134 ms 5.4190 ms 5.5298 ms]
                        thrpt:  [22.605 MiB/s 23.067 MiB/s 23.525 MiB/s]
                 change:
                        time:   [-11.238% -9.5103% -7.7472%] (p = 0.00 < 0.05)
                        thrpt:  [+8.3977% +10.509% +12.661%]
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  4 (4.00%) high mild
```

---

### [2026-03-14 15:47:04] Zero-Copy 2: TextSlicer single String buffer with mem::replace (Commit: f6f9785)

```text
Gnuplot not found, using plotters backend
StreamReader/iter_lines time:   [801.35 µs 825.26 µs 851.34 µs]
                        thrpt:  [146.83 MiB/s 151.47 MiB/s 155.99 MiB/s]
                 change:
                        time:   [-1.3860% +0.7601% +2.8841%] (p = 0.49 > 0.05)
                        thrpt:  [-2.8033% -0.7543% +1.4055%]
                        No change in performance detected.
Found 6 outliers among 100 measurements (6.00%)
  6 (6.00%) high severe

CompressionPipeline/compress_str
                        time:   [5.3908 ms 5.4191 ms 5.4526 ms]
                        thrpt:  [22.925 MiB/s 23.067 MiB/s 23.188 MiB/s]
                 change:
                        time:   [-1.9366% +0.0760% +2.0298%] (p = 0.94 > 0.05)
                        thrpt:  [-1.9894% -0.0759% +1.9749%]
                        No change in performance detected.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild
```

---

### [2026-03-14 16:53:27] StreamReader & TextSlicer Deep Zero-Copy (Commit: a38dbb9)

```text
Gnuplot not found, using plotters backend
Benchmarking StreamReader/iter_lines
Benchmarking StreamReader/iter_lines: Warming up for 3.0000 s
Benchmarking StreamReader/iter_lines: Collecting 100 samples in estimated 8.9993 s (10k iterations)
Benchmarking StreamReader/iter_lines: Analyzing
StreamReader/iter_lines time:   [825.14 µs 844.60 µs 865.85 µs]
                        thrpt:  [144.37 MiB/s 148.00 MiB/s 151.49 MiB/s]
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild

Benchmarking CompressionPipeline/compress_str
Benchmarking CompressionPipeline/compress_str: Warming up for 3.0000 s
Benchmarking CompressionPipeline/compress_str: Collecting 100 samples in estimated 5.6522 s (800 iterations)
Benchmarking CompressionPipeline/compress_str: Analyzing
CompressionPipeline/compress_str
                        time:   [6.4469 ms 6.5752 ms 6.7122 ms]
                        thrpt:  [18.623 MiB/s 19.011 MiB/s 19.389 MiB/s]
Found 1 outliers among 100 measurements (1.00%)
  1 (1.00%) high mild
```

---

### [2026-03-14 17:15:35] Final Zero-Copy baseline using `Cow<'a, str>` (Commit: 9563c78)

```text
Gnuplot not found, using plotters backend
StreamReader/iter_lines time:   [678.81 µs 690.04 µs 703.07 µs]
                        thrpt:  [177.79 MiB/s 181.15 MiB/s 184.15 MiB/s]
                 change:
                        time:   [-21.728% -18.082% -14.603%] (p = 0.00 < 0.05)
                        thrpt:  [+17.100% +22.073% +27.760%]
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high mild

CompressionPipeline/compress_str
                        time:   [5.6637 ms 5.7264 ms 5.7925 ms]
                        thrpt:  [21.580 MiB/s 21.829 MiB/s 22.070 MiB/s]
                 change:
                        time:   [-14.988% -12.910% -10.990%] (p = 0.00 < 0.05)
                        thrpt:  [+12.347% +14.824% +17.631%]
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
```
