#!/usr/bin/env python3
"""
TokenSlim 项目 Rust 代码自动注释生成器

此脚本用于为 TokenSlim 项目的所有 Rust 源文件自动生成中文注释模板。
脚本会分析每个文件的结构，并为模块、结构体、函数等生成合适的中文注释。

使用方法：
    python add_comments.py

注意事项：
    1. 脚本会直接修改源文件，建议先提交到版本控制系统
    2. 生成的注释需要人工审查和补充
    3. 主要添加模块级文档注释和文件头注释
"""

import os
import re
from pathlib import Path

# 项目根目录
PROJECT_ROOT = Path("C:/git_work/TokenSlim/src")

# 模块描述映射表
MODULE_DESCRIPTIONS = {
    "mod.rs": {
        "core": "//! TokenSlim 核心模块",
        "cli": "//! 命令行接口模块",
        "plugins": "//! 压缩插件模块集合",
        "utils": "//! 工具函数模块",
        "compression": "//! 压缩基础类型模块",
        "compression_pipeline": "//! 压缩流水线模块",
        "content_analyzer": "//! 内容分析器模块",
        "dedup_engine": "//! 去重引擎模块",
        "dictionary_engine": "//! 字典引擎模块",
        "error_isolation": "//! 错误隔离执行器模块",
        "metrics": "//! 性能指标收集模块",
        "plugin_dispatcher": "//! 插件调度器模块",
        "rehydration_pipeline": "//! 还原流水线模块",
        "stream_reader": "//! 流式读取器模块",
        "text_slicer": "//! 文本切片器模块",
        "sys_env": "//! 系统环境信息模块",
    },
    "types.rs": "//! 类型定义模块\n//!\n//! 本模块定义了该组件的核心数据结构和类型。",
    "methods.rs": "//! 方法实现模块\n//!\n//! 本模块实现了该组件的主要业务逻辑和公共 API。",
    "test.rs": "//! 单元测试模块\n//!\n//! 本模块包含该组件的单元测试和集成测试。",
}

# 文件头注释模板
FILE_HEADER_TEMPLATES = {
    "mod.rs": """//! {module_name} 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 {module_name} 功能。
//!
//! ## 主要功能
//!
//! - 提供核心类型定义和接口
//! - 协调各子组件的工作流程
//! - 对外提供统一的 API 接口

""",
    "types.rs": """//! {module_name} 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 {module_name} 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

""",
    "methods.rs": """//! {module_name} 方法实现
//!
//! # 方法概述
//!
//! 本模块实现了 {module_name} 模块的主要业务逻辑。
//! 包含所有公共 API 的实现，以及内部辅助函数。

""",
    "test.rs": """//! {module_name} 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 {module_name} 模块的单元测试和集成测试。
//! 测试覆盖了主要功能和边界情况。

""",
}

def get_module_name(path: Path) -> str:
    """从文件路径提取模块名称"""
    # 获取父目录名称
    parent = path.parent.name
    return parent.replace("_", " ")

def add_module_comment(content: str, path: Path) -> str:
    """为模块文件添加注释"""
    filename = path.name
    module_name = get_module_name(path)
    
    # 检查是否已有模块级注释
    if content.startswith("//!"):
        return content
    
    # 获取模块描述
    if filename in MODULE_DESCRIPTIONS:
        module_desc = MODULE_DESCRIPTIONS[filename]
        if isinstance(module_desc, dict):
            module_desc = module_desc.get(path.parent.name, f"//! {module_name} 模块")
    else:
        module_desc = f"//! {module_name} 模块"
    
    # 生成文件头注释
    if filename in FILE_HEADER_TEMPLATES:
        header = FILE_HEADER_TEMPLATES[filename].format(module_name=module_name)
    else:
        header = f"//! {module_name} 模块\n//!\n"
    
    return header + "\n" + content

def process_file(file_path: Path) -> bool:
    """处理单个文件，添加注释"""
    try:
        # 读取文件内容
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
        
        # 添加模块注释
        new_content = add_module_comment(content, file_path)
        
        # 如果内容有变化，写入文件
        if new_content != content:
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write(new_content)
            print(f"✓ 已更新：{file_path.relative_to(PROJECT_ROOT.parent)}")
            return True
        else:
            print(f"- 已跳过（已有注释）: {file_path.relative_to(PROJECT_ROOT.parent)}")
            return False
    except Exception as e:
        print(f"✗ 处理失败 {file_path.relative_to(PROJECT_ROOT.parent)}: {e}")
        return False

def find_rust_files(root: Path) -> list:
    """递归查找所有 Rust 源文件"""
    rust_files = []
    for dirpath, _, filenames in os.walk(root):
        # 跳过测试文件
        if 'target' in str(dirpath):
            continue
        
        for filename in filenames:
            if filename.endswith('.rs'):
                rust_files.append(Path(dirpath) / filename)
    
    return sorted(rust_files)

def main():
    """主函数"""
    print("=" * 60)
    print("TokenSlim 项目 Rust 代码自动注释工具")
    print("=" * 60)
    print()
    
    # 查找所有 Rust 文件
    rust_files = find_rust_files(PROJECT_ROOT)
    print(f"找到 {len(rust_files)} 个 Rust 源文件\n")
    
    # 处理每个文件
    updated_count = 0
    for file_path in rust_files:
        if process_file(file_path):
            updated_count += 1
    
    print()
    print("=" * 60)
    print(f"处理完成！更新了 {updated_count}/{len(rust_files)} 个文件")
    print("=" * 60)

if __name__ == "__main__":
    main()
