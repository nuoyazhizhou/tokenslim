#!/usr/bin/env python3
import json

with open('tests/output/gcc_build_failure-4_final.json', 'r', encoding='utf-8') as f:
    data = json.load(f)

print("Keys in JSON:", list(data.keys()))
if 'dictionary' in data:
    print("\n✅ Dictionary field EXISTS!")
    d = data['dictionary']
    print(f"  - paths: {len(d.get('paths', {}))}")
    print(f"  - macros: {len(d.get('macros', {}))}")
    print(f"  - packages: {len(d.get('packages', {}))}")
    print(f"\nSample paths (first 3):")
    for k, v in list(d.get('paths', {}).items())[:3]:
        print(f"    {k}: {v[:60]}...")
else:
    print("\n❌ Dictionary field NOT found!")

# Show some tokens with replacements
print("\n\nSample tokens with $P replacements:")
for t in data.get('tokens', [])[:10]:
    if isinstance(t, dict) and 'Text' in t:
        if '$P' in t['Text']:
            print(f"  {t['Text'][:100]}")
