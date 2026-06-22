#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
generate_case_sidecars.py
==========================

为 ``samples/<plugin>/`` 下每个 case 文件生成空 sidecar 模板。

sidecar 文件命名：``<case_filename_stem>.scenario.yaml``
sidecar 内容：占位字段，scenario/target_capability/expected_keep/expected_compress
全是空串，由 LLM/人类后续填写。

设计原则：
- **不自动推导 scenario** — 1000+ case 的语义描述靠 LLM 拍脑袋 = 大规模幻觉
- 工具只做"创建空模板"+"补缺不补改"两件事
- 已存在 sidecar 不覆盖（避免丢失人类/LLM 填的内容）
- 报告只追加 warning，不静默失败

用法：
    python scripts/generate_case_sidecars.py --plugin shell_session_plugin
    python scripts/generate_case_sidecars.py --all
    python scripts/generate_case_sidecars.py --plugin shell_session_plugin --force
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import datetime
from typing import List, Tuple
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPT_DIR)

import audit_llm_common as _alc  # noqa: E402

# 侧车 schema — 5 个 scenario 字段 + 5 个 audit 字段（fix: L1 缓存写回 sidecar）
# 注意：LLM 不得自动填 scenario 字段（避免大规模幻觉），但 audit 字段是
# 工具自动回填的（LLM 审完后写 hash + status），人类可以手动 skip: true。
SIDECAR_SCHEMA_COMMENT = """# Case scenario sidecar（fix #29）
#
# === 场景字段（人类或定向 LLM 任务填写，工具不自动推导）===
#   scenario:           一句话描述本 case 的真实场景（如"权限被拒"）
#   target_capability:  这个 case 在测插件的哪个能力（如"错误保留"/"路径字典压缩"）
#   expected_keep:      压缩后期望被保留的行/模式（可空）
#   expected_compress:  压缩后期望被压缩或丢弃的行/模式（可空）
#   source:             本侧车的来源（manual / llm / imported），用于审计
#
# === audit 字段（audit_sample_case_quality.py 自动回填，人类可手动 skip: true）===
#   audit.content_hash:        sha256(case 文件内容)，用于增量审计 L1 缓存
#   audit.final_status:        上次审计 final_status（valid / needs_fix / fabricated / ...）
#   audit.llm_invoked:         上次是否调过 LLM
#   audit.llm_verified_at:     ISO8601 时间戳
#   audit.last_audit_tool:     上次审计工具版本（如 "audit_sample_case_quality@fix#XX"）
#   audit.skip:                true → 永久跳过 lint+LLM（人类干预标志）
#
# 缓存语义：
# - audit.content_hash == sha256(当前 case) 且 audit.final_status == valid
#   且 audit.llm_invoked 与本次 --llm-audit 开关一致 → 跳过 lint+LLM
# - audit.skip: true → 永远跳过（哪怕 hash 变了也不跑）— 用来"封印"有问题的 case
# - audit 字段缺失或 hash 不匹配 → 跑 lint，必要时跑 LLM，结束写回
# - audit 字段是机器写的，人类可以手动删 / 改 skip 标志
#
# 维护规则：
# - LLM 不得自动填 scenario 字段 — 1000+ case 的大规模 LLM 推导 = 幻觉
# - 工具只生成空 scenario 模板；audit 字段由 audit_sample_case_quality.py 自动维护
# - 已存在文件不会被本工具覆盖 scenario 字段，除非 --force（audit 字段永远不覆盖）
"""

SIDECAR_TEMPLATE = """{schema_comment}scenario: ""
target_capability: ""
expected_keep: ""
expected_compress: ""
source: "placeholder"
generated_at: "{ts}"

# === audit 字段（自动维护，请勿手填内容字段）===
audit:
  content_hash: ""
  final_status: ""
  llm_invoked: false
  llm_verified_at: ""
  last_audit_tool: ""
  skip: false
"""


# fix: case 后缀 6 种合法（.log/.json/.hex/.md/.xml/.txt），用白名单 glob
# 与 walk_physical_samples / get_physical_samples 保持一致
_CASE_EXTS = ("log", "json", "hex", "md", "xml", "txt")


def _case_files(samples_dir: str, plugin: str) -> List[str]:
    """列出 samples/<plugin>/ 下所有合法 case 文件名（不含 sidecar 自身）。"""
    full = os.path.join(samples_dir, plugin)
    if not os.path.isdir(full):
        return []
    matched: List[str] = []
    for ext in _CASE_EXTS:
        for f in os.listdir(full):
            if f.endswith(f".{ext}") and f.startswith("case_"):
                matched.append(f)
    return sorted(set(matched))


def _sidecar_path(samples_dir: str, plugin: str, case_filename: str) -> str:
    """case_filename=case_001_xxx.log -> samples/<plugin>/case_001_xxx.scenario.yaml"""
    stem = os.path.splitext(case_filename)[0]
    return os.path.join(samples_dir, plugin, f"{stem}.scenario.yaml")


def generate_for_plugin(
    plugin: str,
    *,
    samples_dir: str = "samples",
    force: bool = False,
    verbose: bool = True,
) -> Tuple[int, int, int]:
    """为一个 plugin 生成缺失 sidecar。

    Returns: (created, skipped_exists, skipped_no_case)
    """
    cases = _case_files(samples_dir, plugin)
    if not cases:
        if verbose:
            print(f"  [skip] {plugin}: no case files in {samples_dir}/{plugin}/")
        return 0, 0, 1

    created = 0
    skipped = 0
    ts = datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%M:%SZ")
    body = SIDECAR_TEMPLATE.format(schema_comment=SIDECAR_SCHEMA_COMMENT, ts=ts)
    for case_fn in cases:
        sc = _sidecar_path(samples_dir, plugin, case_fn)
        if os.path.exists(sc) and not force:
            skipped += 1
            if verbose:
                print(f"  [exist] {sc}")
            continue
        with open(sc, "w", encoding="utf-8") as f:
            f.write(body)
        created += 1
        if verbose:
            print(f"  [new]   {sc}")
    return created, skipped, 0


def list_plugins(samples_dir: str = "samples") -> List[str]:
    if not os.path.isdir(samples_dir):
        return []
    return sorted(
        e for e in os.listdir(samples_dir)
        if os.path.isdir(os.path.join(samples_dir, e))
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="为 samples/<plugin>/ 下每个 case 生成空 scenario sidecar 模板。"
    )
    parser.add_argument("--plugin", help="只处理单个 plugin（如 shell_session_plugin）")
    parser.add_argument("--all", action="store_true", help="处理 samples/ 下所有 plugin")
    parser.add_argument("--samples-dir", default="samples")
    parser.add_argument("--force", action="store_true", help="覆盖已存在的 sidecar（高危）")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args()

    if not args.plugin and not args.all:
        print("ERROR: must specify --plugin <name> or --all", file=sys.stderr)
        return 2

    targets = [args.plugin] if args.plugin else list_plugins(args.samples_dir)
    if not targets:
        print(f"ERROR: no plugins found under {args.samples_dir}/", file=sys.stderr)
        return 2

    total_created = 0
    total_skipped = 0
    for plugin in targets:
        c, s, _ = generate_for_plugin(
            plugin,
            samples_dir=args.samples_dir,
            force=args.force,
            verbose=not args.quiet,
        )
        total_created += c
        total_skipped += s

    print(f"\nDone: created={total_created}, skipped_existing={total_skipped}")
    if total_created and not args.quiet:
        print("提示：scenario/target_capability/expected_keep/expected_compress 字段")
        print("      需人类或定向 LLM 任务填写，本工具不自动推导以避免大规模幻觉。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
