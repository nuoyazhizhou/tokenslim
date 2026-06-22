# Pipeline Benchmark Report (C2)

Generated at: 2026-03-20 22:34:25 +0800

- Input file: `benchmarks/input_100mb.txt`
- Input size: 100.07 MB
- Iterations per scenario: 3
- Original tokens (cl100k_base): 41155985
- Scenario filter: `all`
- Skip tokenization: `false`

| scenario | avg ms | min ms | max ms | throughput MB/s | avg output MB (json) | avg token ratio |
|---|---:|---:|---:|---:|---:|---:|
| mmap+parallel | 573.11 | 508.80 | 626.87 | 174.60 | 6.46 | 0.0995 |
| mmap+serial | 486.53 | 421.52 | 546.24 | 205.67 | 6.45 | 0.1001 |
| non_mmap+parallel | 448.60 | 398.56 | 475.12 | 223.06 | 6.45 | 0.1002 |
| non_mmap+serial | 469.29 | 415.12 | 506.74 | 213.23 | 6.46 | 0.1002 |

## Threshold Recommendation

- `parallel_threshold`: `1048576` (prefer parallel from ~1MB)
- `stream_mmap_threshold`: `18446744073709551615` (prefer non-mmap for this profile)
