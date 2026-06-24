# Pipeline Benchmark Report (C2)

Generated at: 2026-06-24 11:47:25 +0800

- Input file: `benchmarks/input_20mb.txt`
- Input size: 20.01 MB
- Iterations per scenario: 3
- Original tokens (cl100k_base): 8211144
- Scenario filter: `all`
- Skip tokenization: `false`

> **注**：本报告用 20MB input（之前 baseline 是 100MB / `benchmarks/input_100mb.txt`，
> 留作历史对照，未参与本次重测）。serial 路径在 20MB 上吞吐明显下降，
> 是因为 serial 主要面向 < 256KB 小文件，20MB 时派发开销主导耗时。
> 实际生产场景请关注 `non_mmap+parallel`（~176 MB/s）这一行。

| scenario | avg ms | min ms | max ms | throughput MB/s | avg output MB (json) | avg token ratio |
|---|---:|---:|---:|---:|---:|---:|
| mmap+parallel | 189.93 | 100.80 | 270.19 | 105.37 | 5.85 | 0.0909 |
| mmap+serial | 4328.44 | 2733.99 | 5739.51 | 4.62 | 17.60 | 0.7966 |
| non_mmap+parallel | 113.70 | 96.08 | 126.38 | 176.01 | 5.86 | 0.0906 |
| non_mmap+serial | 2220.36 | 1690.94 | 2582.68 | 9.01 | 17.60 | 0.7966 |

## Threshold Recommendation

- `parallel_threshold`: `1048576` (prefer parallel from ~1MB)
- `stream_mmap_threshold`: `18446744073709551615` (prefer non-mmap for this profile)
