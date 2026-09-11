#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
generate_comment_checklist.py
=============================

静态扫描 src/ 目录下所有 .rs 文件，提取需要注释的代码结构，
生成待注释清单（JSON 格式）。

**本脚本只做机械提取，不生成任何注释内容。**
提取范围包括：
  - 模块级注释（检查是否已有 //!）
  - pub fn / pub(crate) fn / fn
  - pub struct / pub(crate) struct / struct
  - pub enum / pub(crate) enum / enum
  - pub trait / pub(crate) trait / trait
  - pub const / pub(crate) const / const
  - pub static / pub(crate) static / static
  - macro_rules!
  - impl 块（标记 impl Trait for Type 类型）
  - #[test] 函数

输出产物：
  docs/plans/comment_checklist.json  —— 全量清单（结构化数据）
  docs/plans/comment_checklist.md    —— 可读版总览

用法：
    python scripts/generate_comment_checklist.py
    python scripts/generate_comment_checklist.py --summary   # 只打印统计
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from typing import Any, Dict, List, Optional, Tuple

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, ".."))
SRC_DIR = os.path.join(PROJECT_ROOT, "src")
OUTPUT_JSON = os.path.join(PROJECT_ROOT, "docs", "plans", "comment_checklist.json")
OUTPUT_MD = os.path.join(PROJECT_ROOT, "docs", "plans", "comment_checklist.md")


# ============================================================================
# Rust 源码解析（基于正则的轻量提取，不依赖 tree-sitter）
# ============================================================================

# 可见性前缀
VISIBILITY_RE = r"(?:pub\s*\(\s*crate\s*\)\s*|pub\s+)?"

# 函数定义
FN_PATTERN = re.compile(
    rf"^(\s*){VISIBILITY_RE}(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*[<(]",
    re.MULTILINE,
)

# 结构体定义
STRUCT_PATTERN = re.compile(
    rf"^(\s*){VISIBILITY_RE}struct\s+([A-Za-z_][A-Za-z0-9_]*)\s*[<{{(]",
    re.MULTILINE,
)

# 枚举定义
ENUM_PATTERN = re.compile(
    rf"^(\s*){VISIBILITY_RE}enum\s+([A-Za-z_][A-Za-z0-9_]*)\s*[<{{]",
    re.MULTILINE,
)

# trait 定义
TRAIT_PATTERN = re.compile(
    rf"^(\s*){VISIBILITY_RE}trait\s+([A-Za-z_][A-Za-z0-9_]*)\s*[<{{:]",
    re.MULTILINE,
)

# const 定义
CONST_PATTERN = re.compile(
    rf"^(\s*){VISIBILITY_RE}const\s+([A-Z_][A-Z0-9_]*)\s*:",
    re.MULTILINE,
)

# static 定义
STATIC_PATTERN = re.compile(
    rf"^(\s*){VISIBILITY_RE}static\s+(?:ref\s+)?([A-Z_][A-Z0-9_]*)\s*:",
    re.MULTILINE,
)

# macro_rules! 定义
MACRO_PATTERN = re.compile(
    r"^(\s*)macro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{",
    re.MULTILINE,
)

# impl 块
IMPL_PATTERN = re.compile(
    r"^(\s*)impl\s+(?:<[^>]+>\s+)?(?:[A-Za-z_][A-Za-z0-9_:<>]*\s+for\s+)?([A-Za-z_][A-Za-z0-9_:<>]*)\s*\{",
    re.MULTILINE,
)

# #[test] 属性
TEST_ATTR_PATTERN = re.compile(
    r"#\[test\]\s*\n\s*(?:pub\s+|pub\s*\(\s*crate\s*\)\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
)

# 模块级注释
MODULE_COMMENT_PATTERN = re.compile(r"^//!", re.MULTILINE)


def _line_of(source: str, pos: int) -> int:
    """计算字符位置对应的行号（1-based）"""
    return source.count("\n", 0, pos) + 1


def _collect_matches(
    source: str, pattern: re.Pattern, kind: str
) -> List[Dict[str, Any]]:
    """收集匹配项，返回结构化列表"""
    results = []
    for m in pattern.finditer(source):
        name = m.group(2) if len(m.groups()) >= 2 else m.group(1)
        line = _line_of(source, m.start())
        full_match = m.group(0).strip()
        visibility = "pub" if "pub " in full_match else (
            "pub(crate)" if "pub(" in full_match else "private"
        )
        # 跳过注释内的匹配（简单检查：前面有没有 // 或 /*）
        # 这里不做严格语法分析，接受少量误报
        results.append({
            "name": name,
            "line": line,
            "visibility": visibility,
            "kind": kind,
        })
    return results


def _collect_test_fns(source: str) -> List[Dict[str, Any]]:
    """收集 #[test] 函数"""
    results = []
    for m in TEST_ATTR_PATTERN.finditer(source):
        name = m.group(1)
        line = _line_of(source, m.start())
        results.append({
            "name": name,
            "line": line,
            "visibility": "test",
            "kind": "test_fn",
        })
    return results


def parse_rust_file(file_path: str) -> Dict[str, Any]:
    """解析单个 Rust 文件，返回提取的结构信息"""
    with open(file_path, "r", encoding="utf-8", errors="replace") as f:
        source = f.read()

    rel_path = os.path.relpath(file_path, PROJECT_ROOT)

    # 检查模块级注释
    has_module_comment = bool(MODULE_COMMENT_PATTERN.search(source[:500]))  # 只看文件头

    items = []
    items.extend(_collect_matches(source, FN_PATTERN, "fn"))
    items.extend(_collect_matches(source, STRUCT_PATTERN, "struct"))
    items.extend(_collect_matches(source, ENUM_PATTERN, "enum"))
    items.extend(_collect_matches(source, TRAIT_PATTERN, "trait"))
    items.extend(_collect_matches(source, CONST_PATTERN, "const"))
    items.extend(_collect_matches(source, STATIC_PATTERN, "static"))
    items.extend(_collect_matches(source, MACRO_PATTERN, "macro_rules"))
    items.extend(_collect_matches(source, IMPL_PATTERN, "impl"))
    items.extend(_collect_test_fns(source))

    # 按行号排序
    items.sort(key=lambda x: x["line"])

    # 统计
    stats = {}
    for item in items:
        kind = item["kind"]
        stats[kind] = stats.get(kind, 0) + 1

    return {
        "file": rel_path,
        "total_lines": source.count("\n") + 1,
        "has_module_comment": has_module_comment,
        "item_count": len(items),
        "items": items,
        "stats": stats,
        "commented": False,  # 待后续更新
    }


# ============================================================================
# 文件扫描
# ============================================================================

def scan_src_dir() -> List[Dict[str, Any]]:
    """扫描 src/ 下所有 .rs 文件"""
    results = []
    for root, dirs, files in os.walk(SRC_DIR):
        # 跳过 target 等目录（src 下应该没有，保险起见）
        dirs[:] = [d for d in dirs if d not in ("target", ".git")]
        for fname in sorted(files):
            if fname.endswith(".rs"):
                fpath = os.path.join(root, fname)
                results.append(parse_rust_file(fpath))
    return results


# ============================================================================
# 统计与输出
# ============================================================================

def build_summary(file_results: List[Dict[str, Any]]) -> Dict[str, Any]:
    """构建总览统计"""
    total_files = len(file_results)
    total_lines = sum(f["total_lines"] for f in file_results)
    total_items = sum(f["item_count"] for f in file_results)
    files_with_module_comment = sum(1 for f in file_results if f["has_module_comment"])

    # 按类型统计
    kind_stats: Dict[str, int] = {}
    for f in file_results:
        for kind, count in f["stats"].items():
            kind_stats[kind] = kind_stats.get(kind, 0) + count

    # 按层级统计
    layer_stats = {
        "utils": {"files": 0, "items": 0, "lines": 0},
        "cli": {"files": 0, "items": 0, "lines": 0},
        "core": {"files": 0, "items": 0, "lines": 0},
        "plugins": {"files": 0, "items": 0, "lines": 0},
        "bin": {"files": 0, "items": 0, "lines": 0},
        "top_level": {"files": 0, "items": 0, "lines": 0},
    }

    for f in file_results:
        layer = _file_layer(f["file"])

        layer_stats[layer]["files"] += 1
        layer_stats[layer]["items"] += f["item_count"]
        layer_stats[layer]["lines"] += f["total_lines"]

    return {
        "total_files": total_files,
        "total_lines": total_lines,
        "total_items": total_items,
        "files_with_module_comment": files_with_module_comment,
        "kind_stats": kind_stats,
        "layer_stats": layer_stats,
    }


def write_json(file_results: List[Dict[str, Any]], summary: Dict[str, Any]):
    """写入 JSON 清单"""
    output = {
        "version": "1.0",
        "generated_by": "scripts/generate_comment_checklist.py",
        "summary": summary,
        "files": file_results,
    }
    os.makedirs(os.path.dirname(OUTPUT_JSON), exist_ok=True)
    with open(OUTPUT_JSON, "w", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2)


def write_markdown(summary: Dict[str, Any], file_results: List[Dict[str, Any]]):
    """写入可读版 Markdown 总览"""
    lines = []
    lines.append("# TokenSlim 待注释清单（静态扫描生成）")
    lines.append("")
    lines.append("> **生成方式**: `scripts/generate_comment_checklist.py` 静态扫描")
    lines.append("> **说明**: 本文件只包含清单，不含注释内容。注释由人工逐个分析添加。")
    lines.append("")

    # 总览
    lines.append("## 一、 总览统计")
    lines.append("")
    lines.append(f"| 指标 | 数值 |")
    lines.append(f"|------|------|")
    lines.append(f"| 总文件数 | {summary['total_files']} |")
    lines.append(f"| 总行数 | {summary['total_lines']:,} |")
    lines.append(f"| 总待注释项 | {summary['total_items']:,} |")
    lines.append(f"| 已有模块级注释的文件 | {summary['files_with_module_comment']} / {summary['total_files']} |")
    lines.append("")

    # 按类型统计
    lines.append("## 二、 按结构类型统计")
    lines.append("")
    lines.append("| 类型 | 数量 | 优先级 |")
    lines.append("|------|------|--------|")
    priority_map = {
        "fn": "P0-P1",
        "struct": "P0-P1",
        "enum": "P0-P1",
        "trait": "P0",
        "test_fn": "P2",
        "const": "P3",
        "static": "P3",
        "macro_rules": "P2",
        "impl": "P3",
    }
    for kind, count in sorted(summary["kind_stats"].items(), key=lambda x: -x[1]):
        prio = priority_map.get(kind, "-")
        lines.append(f"| `{kind}` | {count:,} | {prio} |")
    lines.append("")

    # 按层级统计
    lines.append("## 三、 按层级统计（执行顺序）")
    lines.append("")
    lines.append("| 层级 | 文件数 | 待注释项 | 总行数 | 状态 |")
    lines.append("|------|--------|---------|--------|------|")
    for layer in ["utils", "cli", "core", "plugins", "bin", "top_level"]:
        s = summary["layer_stats"][layer]
        lines.append(f"| `{layer}` | {s['files']} | {s['items']:,} | {s['lines']:,} | 待开始 |")
    lines.append("")

    # 文件清单（按层级分组）
    lines.append("## 四、 文件清单")
    lines.append("")
    for layer in ["utils", "cli", "core", "plugins", "bin", "top_level"]:
        layer_files = [f for f in file_results if _file_layer(f["file"]) == layer]
        if not layer_files:
            continue
        lines.append(f"### {layer} 层（{len(layer_files)} 个文件）")
        lines.append("")
        lines.append("| 文件 | 行数 | 待注释项 | 模块注释 | 状态 |")
        lines.append("|------|------|---------|----------|------|")
        for f in sorted(layer_files, key=lambda x: x["file"]):
            mod_cmt = "✅" if f["has_module_comment"] else "❌"
            lines.append(f"| `{f['file']}` | {f['total_lines']} | {f['item_count']} | {mod_cmt} | 待开始 |")
        lines.append("")

    lines.append("---")
    lines.append("")
    lines.append("*本清单由脚本静态扫描生成，确保 100% 文件覆盖率。注释内容由人工逐个分析添加。*")

    os.makedirs(os.path.dirname(OUTPUT_MD), exist_ok=True)
    with open(OUTPUT_MD, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))


def _file_layer(path: str) -> str:
    """根据文件路径判断所属层级"""
    # 统一用正斜杠比较，兼容 Windows
    norm_path = path.replace("\\", "/")
    if norm_path.startswith("src/utils/"):
        return "utils"
    elif norm_path.startswith("src/cli/"):
        return "cli"
    elif norm_path.startswith("src/core/"):
        return "core"
    elif norm_path.startswith("src/plugins/"):
        return "plugins"
    elif norm_path.startswith("src/bin/"):
        return "bin"
    else:
        return "top_level"


def print_summary(summary: Dict[str, Any]):
    """打印摘要到控制台"""
    print("=" * 60)
    print("  TokenSlim 待注释清单 - 统计摘要")
    print("=" * 60)
    print(f"  总文件数:     {summary['total_files']}")
    print(f"  总行数:       {summary['total_lines']:,}")
    print(f"  总待注释项:   {summary['total_items']:,}")
    print(f"  模块注释覆盖率: {summary['files_with_module_comment']}/{summary['total_files']}")
    print()
    print("  按类型统计:")
    for kind, count in sorted(summary["kind_stats"].items(), key=lambda x: -x[1]):
        print(f"    {kind:15s} {count:>6,}")
    print()
    print("  按层级统计:")
    for layer in ["utils", "cli", "core", "plugins", "bin", "top_level"]:
        s = summary["layer_stats"][layer]
        print(f"    {layer:12s} 文件:{s['files']:>3}  项:{s['items']:>5,}  行:{s['lines']:>6,}")
    print("=" * 60)
    print(f"  JSON 输出: {OUTPUT_JSON}")
    print(f"  MD 输出:   {OUTPUT_MD}")
    print("=" * 60)


# ============================================================================
# main
# ============================================================================

def main():
    parser = argparse.ArgumentParser(description="生成 Rust 代码待注释清单")
    parser.add_argument("--summary", action="store_true", help="只打印统计摘要")
    args = parser.parse_args()

    print(f"扫描目录: {SRC_DIR}")
    file_results = scan_src_dir()
    summary = build_summary(file_results)

    if not args.summary:
        write_json(file_results, summary)
        write_markdown(summary, file_results)

    print_summary(summary)


if __name__ == "__main__":
    main()
