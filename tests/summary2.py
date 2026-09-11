#!/usr/bin/env python3
print("=" * 70)
print("GCC Build Success 压缩效率对比")
print("=" * 70)

versions = [
    ("v1 (修复换行符前)", {"compressed_tokens": 7406971, "ratio": 0.70, "paths": 84363, "macros": 0}),
    ("v4 (打包宏-保持顺序)", {"compressed_tokens": 4328318, "ratio": 0.41, "paths": 84363, "macros": 2997}),
    ("v5 (单独宏+公共前缀)", {"compressed_tokens": 6178841, "ratio": 0.58, "paths": 2266, "macros": 3106}),
]

original_tokens = 10585062
print(f"\n原始 Token 数: {original_tokens:,}")
print("-" * 70)
print(f"{'版本':<28} {'压缩后Token':<12} {'压缩率':<8} {'Paths':<8} {'Macros':<8}")
print("-" * 70)

for name, data in versions:
    print(f"{name:<28} {data['compressed_tokens']:>10,}  {data['ratio']:>6%}   {data['paths']:>6,}   {data['macros']:>6,}")

print("-" * 70)
print("""
分析：
- v5 字典大幅减少：84K → 2.2K (减少 97%)
- v5 压缩率 58%，比 v4 的 41% 差，但字典更合理
- v5 输出 JSON 大小会更小，因为字典数据量少
""")
