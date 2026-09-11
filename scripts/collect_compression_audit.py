# -*- coding: utf-8 -*-
r"""compression.jsonl 定期回收与分析工具（P2-89 后续 · 审计数据驱动压缩优化）。

扫描 C:\git_work 下各项目工作区的 `.tokenslim/audit/compression.jsonl`，
按项目聚合压缩质量指标，产出汇总报告（MD + JSON），定位低收益/负收益
压缩记录，为后续压缩器优化提供数据依据。

用法：
    python scripts/collect_compression_audit.py [--root C:\git_work] [--out-dir <dir>]

默认输出目录：`<repo>\.tokenslim\audit\collected\`，文件名带日期，可重复执行（幂等）。
本工具只读源数据，不修改、不删除任何项目的 compression.jsonl。
"""
from __future__ import annotations

import argparse
import io
import json
import os
import sys
from collections import Counter
from datetime import datetime

SKIP_DIR_NAMES = {"node_modules", "target", ".git", "other"}
# 备份快照目录（名称含 -backup- 前缀段）不重复统计
SKIP_DIR_PATTERNS = ("-backup-",)
MAX_DEPTH = 4  # 相对 root 的目录深度：root(ws)/.tokenslim/audit/compression.jsonl 最多 3 层


def find_jsonl_files(root: str) -> list[str]:
    hits = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [
            d for d in dirnames
            if d not in SKIP_DIR_NAMES and not any(p in d for p in SKIP_DIR_PATTERNS)
        ]
        rel = os.path.relpath(dirpath, root)
        depth = 0 if rel == "." else rel.count(os.sep) + 1
        if depth > MAX_DEPTH:
            dirnames[:] = []
            continue
        if "compression.jsonl" in filenames and dirpath.replace("\\", "/").endswith(".tokenslim/audit"):
            hits.append(os.path.join(dirpath, "compression.jsonl"))
    return hits


def analyze_one(path: str, top_k: int = 5) -> dict:
    n = 0
    total_in = total_out = 0
    ratios: list[float] = []
    neg: list[tuple[float, int, str]] = []  # (ratio, in_bytes, first_line)
    poor_big: list[tuple[float, int, str]] = []  # >=20KB 且 ratio>=0.95

    def first_meaningful_line(text: str) -> str:
        for ln in text.splitlines():
            s = ln.strip()
            if s:
                return s[:120]
        return "<empty>"

    with io.open(path, "r", encoding="utf-8", errors="replace") as f:
        for raw in f:
            raw = raw.strip()
            if not raw:
                continue
            try:
                rec = json.loads(raw)
            except json.JSONDecodeError:
                continue
            m = rec.get("compression_metadata") or {}
            try:
                ratio = float(m.get("compression_ratio", 1) or 1)
                ib = int(m.get("original_size", 0) or 0)
                cb = int(m.get("compressed_size", 0) or 0)
            except (TypeError, ValueError):
                continue
            n += 1
            total_in += ib
            total_out += cb
            ratios.append(ratio)
            fl = first_meaningful_line(rec.get("input_redacted", "") or "")
            if ratio > 1.0:
                neg.append((ratio, ib, fl))
            if ib >= 20 * 1024 and ratio >= 0.95:
                poor_big.append((ratio, ib, fl))

    ratios.sort()
    neg.sort(reverse=True)
    poor_big.sort(key=lambda x: -x[1])

    def pct(q: float) -> float:
        return ratios[min(len(ratios) - 1, int(q * (len(ratios) - 1)))] if ratios else 1.0

    return {
        "file": path,
        "records": n,
        "total_in_bytes": total_in,
        "total_out_bytes": total_out,
        "overall_ratio": round(total_out / total_in, 4) if total_in else None,
        "ratio_p50": round(pct(0.5), 4),
        "ratio_p90": round(pct(0.9), 4),
        "neg_count": len(neg),
        "poor_big_count": len(poor_big),
        "worst_neg": [
            {"ratio": round(r, 4), "in_bytes": b, "first_line": fl} for r, b, fl in neg[:top_k]
        ],
        "worst_poor_big": [
            {"ratio": round(r, 4), "in_bytes": b, "first_line": fl} for r, b, fl in poor_big[:top_k]
        ],
    }


def render_markdown(reports: list[dict], root: str) -> str:
    lines = [
        "# compression.jsonl 回收分析报告",
        "",
        f"- 生成时间：{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}",
        f"- 扫描根：`{root}`",
        f"- 覆盖项目数：{len(reports)}",
        "",
        "| 项目 | 记录数 | 输入字节 | 整体 ratio | p50 | p90 | 负收益条数 | 大输入低收益条数 |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for r in reports:
        proj = r["file"].split(os.sep)[: -3]  # 去掉 /.tokenslim/audit/compression.jsonl
        name = os.sep.join(proj[-2:]) if len(proj) >= 2 else r["file"]
        lines.append(
            f"| {name} | {r['records']} | {r['total_in_bytes']:,} | {r['overall_ratio']} "
            f"| {r['ratio_p50']} | {r['ratio_p90']} | {r['neg_count']} | {r['poor_big_count']} |"
        )
    lines += ["", "## 需要关注的记录（负收益 / 大输入低收益）", ""]
    for r in reports:
        if not r["worst_neg"] and not r["worst_poor_big"]:
            continue
        proj = r["file"].split(os.sep)[: -3]
        name = os.sep.join(proj[-2:]) if len(proj) >= 2 else r["file"]
        lines.append(f"### {name}")
        for kind, key in (("负收益", "worst_neg"), ("大输入低收益", "worst_poor_big")):
            for w in r[key]:
                lines.append(
                    f"- [{kind}] ratio={w['ratio']} in={w['in_bytes']:,}B  `{w['first_line']}`"
                )
        lines.append("")
    lines.append(
        "> 优化方向参考：负收益条数>0 → 检查 CLI 负收益守门（P2-89）覆盖；"
        "大输入低收益 → 对照 P3-206~208（cargo test 折叠 / s3 ls 列折叠 / clippy 块折叠）评估新模式。"
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ap = argparse.ArgumentParser(description="compression.jsonl 回收分析")
    ap.add_argument("--root", default=r"C:\git_work")
    ap.add_argument("--out-dir", default=os.path.join(repo, ".tokenslim", "audit", "collected"))
    args = ap.parse_args()

    files = find_jsonl_files(args.root)
    reports = []
    for p in files:
        try:
            reports.append(analyze_one(p))
        except OSError as e:
            print(f"skip {p}: {e}", file=sys.stderr)

    reports.sort(key=lambda r: -(r["total_in_bytes"] or 0))
    os.makedirs(args.out_dir, exist_ok=True)
    stamp = datetime.now().strftime("%Y%m%d")
    md_path = os.path.join(args.out_dir, f"compression-audit-{stamp}.md")
    json_path = os.path.join(args.out_dir, f"compression-audit-{stamp}.json")
    with io.open(md_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(render_markdown(reports, args.root))
    with io.open(json_path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(reports, f, ensure_ascii=False, indent=2)
        f.write("\n")
    print(f"projects={len(reports)} report={md_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
