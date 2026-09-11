# scripts/scan_undeclared_aliases.py
# 全插件 SAP（语义保留审计）扫描器：检测 compact.txt 中的可逆 token 是否在 dict.json 中被声明。
#
# 背景：
#   Compression Protocol V1 使用可逆 token 表达字典化内容：`$PKn/$Pn/$Mn/$Dn/$Cn/$FLn`
#   （PK=包, P=路径, M=宏, D=目录, C=标志, FL=文件），`$S|N` 为自描述空白，无需字典。
#   当 compact.txt 出现某个 token 但对应 case 的 dict.json 缺失该声明时，compact_resolved.txt
#   会残留未解析 token（即"未声明 alias"），下游 AI 无法还原语义——这是最可自动化、低风险的
#   缺陷模式。
#
# 判定规则：
#   - has_dict=False 且 compact 有字典 token  → unverifiable（无法验证，需补导出 dict.json）
#   - has_dict=True                          → 用 dict.json 校验每个 token 声明
#       - 全在字典中 且 resolved 无残留       → pass
#       - 存在 token∈compact 不在 dict      → fail（未声明 alias，resolved 必残留）
#   - compact 无任何字典 token               → ok（无需字典）
#
# 用法：
#   python scripts/scan_undeclared_aliases.py [--audit docs/audit] [--out docs/audit/undeclared_aliases.json]
#   不带 --out 时仅输出控制台摘要；带 --out 时同时落盘 JSON 报告。
import argparse
import json
import re
import sys
from pathlib import Path

# 字典 token 前缀（与 audit_case_metrics.resolve_compact_tokens / dictionary_engine 保持一致）
TOKEN_RE = re.compile(r"\$(?:PK|P|M|D|C|FL)\d+")


def find_cases(audit_dir: Path):
    """遍历 docs/audit/<plugin>_plugin>/cases/<case_id>/ 下的 case 目录。"""
    for plugin_dir in sorted(audit_dir.iterdir()):
        if not plugin_dir.is_dir() or not plugin_dir.name.endswith("_plugin"):
            continue
        cases_dir = plugin_dir / "cases"
        if not cases_dir.is_dir():
            continue
        for case_dir in sorted(cases_dir.iterdir()):
            compact = case_dir / "compact.txt"
            if not compact.is_file():
                continue
            yield plugin_dir.name, case_dir.name, case_dir


def load_dict(case_dir: Path):
    """读取 dict.json；不存在或解析失败时返回 (False, {})。"""
    d = case_dir / "dict.json"
    if not d.is_file():
        return False, {}
    try:
        return True, json.loads(d.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as e:
        return False, {"__parse_error__": str(e)}


def scan_case(case_dir: Path):
    """对单个 case 返回扫描结论 dict。"""
    compact = (case_dir / "compact.txt").read_text(encoding="utf-8", errors="replace")
    resolved_path = case_dir / "compact_resolved.txt"
    has_dict, dictionary = load_dict(case_dir)

    # 收集 compact.txt 中出现的全部字典 token（去重，按出现顺序）
    seen = []
    for m in TOKEN_RE.finditer(compact):
        tok = m.group(0)
        if tok not in seen:
            seen.append(tok)
    tokens = sorted(seen)

    # resolved 中残留的未解析 token
    residual = sorted({m.group(0) for m in TOKEN_RE.finditer(resolved_path.read_text(encoding="utf-8", errors="replace"))}) \
        if resolved_path.is_file() else []

    record = {
        "case_id": case_dir.name,
        "has_dict": has_dict,
        "dict_note": dictionary.get("__parse_error__", ""),
        "token_count": len(tokens),
        "tokens": tokens,
    }

    if not tokens:
        record.update(status="ok", undeclared=[], residual=[])
        return record

    if not has_dict:
        record.update(status="unverifiable", undeclared=[], residual=residual)
        return record

    undeclared = [t for t in tokens if t not in dictionary]
    if has_dict and dictionary.get("__parse_error__"):
        record.update(status="fail", undeclared=tokens, residual=residual)
        return record

    if undeclared:
        record.update(status="fail", undeclared=undeclared, residual=residual)
    else:
        record.update(status="pass", undeclared=[], residual=residual)
    return record


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--audit", default="docs/audit", help="审计根目录")
    ap.add_argument("--out", default="", help="可选：JSON 报告输出路径")
    ap.add_argument(
        "--show-tokens", action="store_true",
        help="控制台明细中列出每个 case 的 token 列表（默认仅列 fail/unverifiable 的）",
    )
    args = ap.parse_args()

    audit_dir = Path(args.audit)
    if not audit_dir.is_dir():
        print(f"错误: 审计目录不存在: {audit_dir}", file=sys.stderr)
        return 2

    results = []
    for plugin, case_id, case_dir in find_cases(audit_dir):
        rec = scan_case(case_dir)
        rec["plugin"] = plugin
        results.append(rec)

    by_status = {}
    for r in results:
        by_status.setdefault(r["status"], []).append(r)

    counts = {s: len(v) for s, v in by_status.items()}
    total = len(results)

    print(f"扫描: {audit_dir}  {total} 个 case")
    print(f"统计: ok={counts.get('ok',0)} pass={counts.get('pass',0)} "
          f"fail(未声明alias)={counts.get('fail',0)} unverifiable(缺字典)={counts.get('unverifiable',0)}")

    fail = by_status.get("fail", [])
    unver = by_status.get("unverifiable", [])
    if fail:
        print(f"\n--- [FAIL] 未声明 alias（{len(fail)}）---")
        for r in sorted(fail, key=lambda x: (x["plugin"], x["case_id"])):
            detail = " [" + ", ".join(r["undeclared"]) + "]" if args.show_tokens else ""
            residual = f" 残留={r['residual']}" if r["residual"] else ""
            note = f" dict解析错误={r['dict_note']}" if r["dict_note"] else ""
            print(f"  {r['plugin']}/{r['case_id']}{detail}{residual}{note}")
    if unver:
        print(f"\n--- [UNVERIFIABLE] compact 有 token 但缺 dict.json（{len(unver)}）---")
        for r in sorted(unver, key=lambda x: (x["plugin"], x["case_id"])):
            detail = " [" + ", ".join(r["tokens"]) + "]" if args.show_tokens else ""
            print(f"  {r['plugin']}/{r['case_id']}{detail}")

    if args.out:
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "audit": str(audit_dir),
            "total_cases": total,
            "counts": counts,
            "results": results,
        }
        out.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
        print(f"\n报告已写入: {out}")
    return 1 if fail else 0


if __name__ == "__main__":
    raise SystemExit(main())