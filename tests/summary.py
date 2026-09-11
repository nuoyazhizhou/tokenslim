#!/usr/bin/env python3
import json

print("=" * 60)
print("GCC Build Success 压缩效率对比")
print("=" * 60)

# 从日志中提取的数据
versions = [
    ("v1 (修复换行符前)", {"compressed_tokens": 7406971, "ratio": 0.70}),
    ("v2 (打包宏 - 排序)", {"compressed_tokens": 5887873, "ratio": 0.56}),
    ("v4 (打包宏 - 保持顺序)", {"compressed_tokens": 4328318, "ratio": 0.41}),
]

original_tokens = 10585062

print(f"\n原始 Token 数: {original_tokens:,}")
print("-" * 60)
print(f"{'版本':<25} {'压缩后 Token':<15} {'压缩率':<10} {'节省'}")
print("-" * 60)

for name, data in versions:
    saved = original_tokens - data["compressed_tokens"]
    print(f"{name:<25} {data['compressed_tokens']:>12,}  {data['ratio']:>7%}   {saved:>10,} (-{saved/data['compressed_tokens']*100:.1f}%)")

print("-" * 60)
print(f"\n最佳版本: v4 (打包宏 - 保持顺序)")
print(f"Token 压缩率: 41% (压缩了 59%)")
print(f"节省 Token: {original_tokens - 4328318:,}")
