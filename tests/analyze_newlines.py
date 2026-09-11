#!/usr/bin/env python3
import json
from collections import Counter

with open('tests/output/gcc_build_success_fixed.json', 'r', encoding='utf-8') as f:
    data = json.load(f)

tokens = data.get('tokens', [])
print(f"Total tokens: {len(tokens)}")

# 统计 token 类型
text_counter = Counter()
for t in tokens:
    if isinstance(t, dict) and 'Text' in t:
        text = t['Text']
        if text == '\n':
            text_counter['newline_only'] += 1
        elif text.startswith('\\n'):
            text_counter['escaped_newline'] += 1
        elif '\n' in text:
            text_counter['has_newline'] += 1
        else:
            text_counter['no_newline'] += 1

print("\nToken 分布:")
for k, v in text_counter.most_common():
    print(f"  {k}: {v} ({v/len(tokens)*100:.1f}%)")

# 单独换行符 token
newline_count = sum(1 for t in tokens if isinstance(t, dict) and t.get('Text') == '\n')
print(f"\n单独的换行符 token: {newline_count} ({newline_count/len(tokens)*100:.1f}%)")

# 检查 dictionary
d = data.get('dictionary', {})
print(f"\nDictionary:")
print(f"  - paths: {len(d.get('paths', {}))}")
print(f"  - macros: {len(d.get('macros', {}))}")
print(f"  - packages: {len(d.get('packages', {}))}")

# 找出使用最多的 token
token_usage = Counter()
for t in tokens:
    if isinstance(t, dict) and 'Text' in t:
        text = t['Text']
        # 查找 $P, $M 等 token
        import re
        matches = re.findall(r'\$(P|M|F)\d+', text)
        for m in matches:
            token_usage[m] += 1

print(f"\nToken 使用频率 (Top 10):")
for k, v in token_usage.most_common(10):
    print(f"  ${k}: {v}")
