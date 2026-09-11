# 对比 resources/messages.*.json 的字段差异：
# 1. 列出每个语言相对参考语言（默认 zh-CN）的缺失/多余字段
# 2. 列出所有语言共有的字段
# 3. 退出码 0 表示无差异，非 0 表示存在差异（CI 可用）
#
# 用法：
#   python scripts/audit_messages_coverage.py
#   python scripts/audit_messages_coverage.py --reference en --strict
#   python scripts/audit_messages_coverage.py --report docs/audit/messages_coverage.md
import argparse
import json
import sys
from pathlib import Path


def load_messages(path: Path) -> dict:
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit field coverage across i18n message files")
    parser.add_argument(
        "--dir",
        default="resources",
        help="Directory containing messages.<lang>.json files",
    )
    parser.add_argument(
        "--reference",
        default="zh-CN",
        help="Reference language code (default: zh-CN)",
    )
    parser.add_argument(
        "--report",
        default=None,
        help="Optional markdown report output path",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero when any language diverges from reference",
    )
    args = parser.parse_args()

    base = Path(args.dir)
    if not base.is_dir():
        print(f"[error] directory not found: {base}", file=sys.stderr)
        return 2

    files = sorted(base.glob("messages.*.json"))
    if not files:
        print(f"[error] no messages.*.json files under {base}", file=sys.stderr)
        return 2

    ref_file = base / f"messages.{args.reference}.json"
    if not ref_file.is_file():
        print(f"[error] reference file missing: {ref_file}", file=sys.stderr)
        return 2

    bundles: dict[str, dict] = {}
    for fp in files:
        lang = fp.stem.removeprefix("messages.")
        bundles[lang] = load_messages(fp)

    ref_keys = set(bundles[args.reference].keys())
    ref_lang = args.reference
    all_langs = sorted(bundles.keys())

    has_diff = False
    rows: list[tuple[str, int, list[str], list[str]]] = []

    print(f"reference language: {ref_lang}  ({len(ref_keys)} keys)")
    print(f"languages detected: {', '.join(all_langs)}")
    print()

    for lang in all_langs:
        keys = set(bundles[lang].keys())
        missing = sorted(ref_keys - keys)
        extra = sorted(keys - ref_keys)
        rows.append((lang, len(keys), missing, extra))
        if missing or extra:
            has_diff = True
            print(f"[{lang}] keys={len(keys)}  missing={len(missing)}  extra={len(extra)}")
            for k in missing:
                print(f"  - missing: {k}")
            for k in extra:
                print(f"  - extra:   {k}")
        else:
            print(f"[{lang}] OK  ({len(keys)} keys)")

    print()
    common = set.intersection(*(set(b.keys()) for b in bundles.values())) if bundles else set()
    union = set.union(*(set(b.keys()) for b in bundles.values())) if bundles else set()
    print(f"common keys across all languages: {len(common)}")
    print(f"union  keys across all languages: {len(union)}")

    if args.report:
        report_path = Path(args.report)
        report_path.parent.mkdir(parents=True, exist_ok=True)
        with report_path.open("w", encoding="utf-8") as f:
            f.write(f"# i18n Messages Field Coverage\n\n")
            f.write(f"- reference: `{ref_lang}` ({len(ref_keys)} keys)\n")
            f.write(f"- languages: {', '.join(all_langs)}\n\n")
            f.write(f"| language | key count | missing vs ref | extra vs ref |\n")
            f.write(f"| --- | ---: | ---: | ---: |\n")
            for lang, count, missing, extra in rows:
                f.write(f"| {lang} | {count} | {len(missing)} | {len(extra)} |\n")
            f.write(f"\n- common keys: **{len(common)}**\n")
            f.write(f"- union  keys: **{len(union)}**\n")
        print(f"\nreport written to: {report_path}")

    if has_diff and args.strict:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
