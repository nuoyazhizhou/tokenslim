#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""步骤 1（sample_case_quality）全量重跑驱动脚本。

用途
----
按 ``docs/audit/<plugin>/`` 逐个调用 ``audit_sample_case_quality.py``，产出全量
证据。之所以需要本驱动而不是直接 ``for`` 循环，是因为 ``docs/audit/`` 下混有
**非插件目录**，直接遍历会误纳入并以 ``No physical samples found`` / 被 gitignore
的本地 scratch 产物硬失败，污染「步骤 1 是否跑通」的判定。

已知非插件目录（均在 ``NON_PLUGIN_DIRS`` 中排除）
------------------------------------------------
- ``vcs_git``：被 ``.gitignore:147`` 显式忽略，是 ``audit_case_metrics.py:924``
  的本地 scratch 输出目录；真实插件 id 为 ``vcs_git_plugin``。
- ``backfill``：目录下只有 ``SAP-*.md`` 审计文档与残留的 ``sample_quality``，
  ``samples/`` 下无对应物理样本。

用法
----
    python scripts/run_audit_step1_all.py                 # 全量（默认 --no-cache）
    python scripts/run_audit_step1_all.py --use-cache     # 允许增量缓存
    python scripts/run_audit_step1_all.py --only xxx_plugin

退出码
------
0 = 全部插件成功；1 = 至少一个插件失败（失败清单会打印到 stdout 末尾）。
注意：插件自身因审计结论（如 ``needs_fix`` / ``routing_boundary_unclear``
硬阻断）返回非 0 也会计入失败——这是**审计结论**而非脚本崩溃，需人工判断是否
属于既有状态（对照 HEAD 版本报告）。
"""

import argparse
import os
import subprocess
import sys
import time

# docs/audit/ 下的非插件目录：误纳入会以 "No physical samples" 硬失败
NON_PLUGIN_DIRS = {
    "vcs_git",   # gitignore 的 scratch 输出目录（真实插件 id 为 vcs_git_plugin）
    "backfill",  # 仅 SAP-*.md 审计文档，samples/ 下无物理样本
}

AUDIT_ROOT = os.path.join("docs", "audit")
SCRIPT = os.path.join("scripts", "audit_sample_case_quality.py")


def discover_plugins(audit_root):
    """列出 docs/audit/ 下的插件目录（排除非插件目录）。"""
    if not os.path.isdir(audit_root):
        raise SystemExit(f"审计根目录不存在：{audit_root}")
    return sorted(
        d for d in os.listdir(audit_root)
        if os.path.isdir(os.path.join(audit_root, d)) and d not in NON_PLUGIN_DIRS
    )


def run_one(plugin, use_cache):
    """对单个插件执行步骤 1，返回 (returncode, 输出末段)。"""
    out_dir = os.path.join(AUDIT_ROOT, plugin, "sample_quality")
    cmd = [sys.executable, SCRIPT, "--plugin", plugin, "--out-dir", out_dir]
    if not use_cache:
        cmd.append("--no-cache")
    r = subprocess.run(cmd, capture_output=True, text=True)
    tail = (r.stderr or r.stdout or "")[-600:]
    return r.returncode, tail


def main():
    ap = argparse.ArgumentParser(description="步骤 1 全量重跑驱动")
    ap.add_argument("--use-cache", action="store_true",
                    help="允许增量缓存（默认 --no-cache 全量重算，产出干净基线）")
    ap.add_argument("--only", default="",
                    help="只跑指定插件（逗号分隔），用于局部验证")
    args = ap.parse_args()

    plugins = discover_plugins(AUDIT_ROOT)
    if args.only:
        wanted = {s.strip() for s in args.only.split(",") if s.strip()}
        plugins = [p for p in plugins if p in wanted]

    mode = "cache" if args.use_cache else "no-cache"
    print(f"步骤 1 全量重跑：{len(plugins)} 个插件，模式={mode}", flush=True)

    failed = []
    t0 = time.time()
    for i, p in enumerate(plugins, 1):
        rc, tail = run_one(p, args.use_cache)
        mark = "OK" if rc == 0 else f"FAIL({rc})"
        print(f"[{i}/{len(plugins)}] {p}: {mark} ({time.time() - t0:.0f}s)", flush=True)
        if rc != 0:
            failed.append((p, rc, tail))

    print("=" * 64)
    print(f"耗时 {time.time() - t0:.0f}s｜总计 {len(plugins)}｜失败 {len(failed)}")
    for p, rc, tail in failed:
        print(f"--- FAIL {p} rc={rc}\n{tail}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
