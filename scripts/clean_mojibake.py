import os

# Mapping of common mojibake strings to their correct Chinese counterparts
REPLACEMENTS = {
    '鏂規硶瀹炵幇': '方法实现',
    '鏂规硶姒傝堪': '方法概述',
    '妯″潡姒傝堪': '模块概述',
    '涓昏涓氫笟閫昏緫': '主要业务逻辑',
    '鍖呭惈鎵€鏈夊叕鍏?API 鐨勫疄鐜帮紝浠ュ強鍐呴儴杈呭姪鍑芥暟': '包含所有公共 API 的实现，以及内部辅助函数',
    '妯″潡瀹炵幇': '模块实现',
    '浼樺厛绾?': '优先级',
    '璁＄畻': '计算',
    '璁板綍': '记录',
    '鍚堝苟': '合并',
    '璇樊': '误差',
    '妯″潡': '模块',
    '瀹炵幇': '实现',
    '姒傝堪': '概述',
    '鏂规硶': '方法',
    '鍐呴儴': '内部',
    '杈呭姪': '辅助',
    '鍑芥暟': '函数',
}

def fix_content(content):
    for wrong, right in REPLACEMENTS.items():
        content = content.replace(wrong, right)
    return content

def process_dir(directory):
    for root, dirs, files in os.walk(directory):
        for file in files:
            if file.endswith('.rs'):
                path = os.path.join(root, file)
                try:
                    with open(path, 'r', encoding='utf-8') as f:
                        content = f.read()
                    
                    new_content = fix_content(content)
                    if new_content != content:
                        print(f"Fixed mojibake in {path}")
                        with open(path, 'w', encoding='utf-8') as f:
                            f.write(new_content)
                except Exception as e:
                    print(f"Error processing {path}: {e}")

if __name__ == "__main__":
    process_dir(r'c:\git_work\TokenSlim\src')
