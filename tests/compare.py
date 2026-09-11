#!/usr/bin/env python3
import json

# 原始文件大小
original_size = 29950986  # bytes

# 读取压缩后的 JSON
with open('tests/output/gcc_build_success_v2.json', 'r', encoding='utf-8') as f:
    data = json.load(f)

# 计算压缩后的文本大小
compressed_text = ""
for t in data.get('tokens', []):
    if isinstance(t, dict):
        if 'Text' in t:
            compressed_text += t['Text']
        if 'Path' in t:
            compressed_text += t['Path']

compressed_size = len(compressed_text)
print(f"原始大小: {original_size:,} bytes")
print(f"压缩后大小: {compressed_size:,} bytes")
print(f"字节压缩比: {compressed_size/original_size:.2%}")

# 估算 LLM token 数量
original_tokens = original_size // 4
compressed_tokens = compressed_size // 4
print(f"\n估算 LLM Tokens:")
print(f"原始: {original_tokens:,}")
print(f"压缩后: {compressed_tokens:,}")
print(f"Token 压缩比: {compressed_tokens/original_tokens:.2%}")

# Dictionary 信息
d = data.get('dictionary', {})
print(f"\n=== Dictionary ===")
print(f"paths: {len(d.get('paths', {}))}")
print(f"macros: {len(d.get('macros', {}))}")

# 统计 token 数量
print(f"\n=== Token 统计 ===")
print(f"总 token 数: {len(data.get('tokens', []))}")
