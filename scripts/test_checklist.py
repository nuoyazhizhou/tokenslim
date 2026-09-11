#!/usr/bin/env python3
import json
with open('docs/plans/comment_checklist.json', 'r', encoding='utf-8') as f:
    data = json.load(f)

# 检查几个可能有误判的文件
test_files = ['src/cli/app.rs', 'src/main.rs', 'src/lib.rs']
for file_info in data['files']:
    if file_info['file'] in test_files:
        print(f"{file_info['file']}: {file_info['item_count']} items")
        # 打印前10个item看看
        for item in file_info['items'][:10]:
            print(f"  line {item['line']}: {item['kind']} {item['name']} ({item['visibility']})")