# scripts/sync_messages_fields.py
# 当 zh-CN 加了新 i18n 键、其他语言没跟上时：
#   1) 列出所有缺失字段（只读，不修改）
#   2) 用 --apply 时把缺失字段以 zh-CN 原文作为占位值同步到所有其他语言文件
#
# 这样保证：i18n::t 永不返回 None → 永远不会因为缺翻译而 panic / 渲染空白。
# 真翻译到达时直接覆盖占位即可。
#
# 用法：
#   python scripts/sync_messages_fields.py                  # 干跑，只打印
#   python scripts/sync_messages_fields.py --apply         # 真正写入占位
#   python scripts/sync_messages_fields.py --reference en  # 以 en 为参考
import argparse
import json
import sys
from pathlib import Path


def load(path: Path) -> dict:
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def dump(path: Path, obj: dict) -> None:
    # 保留与既有文件一致的 4 空格缩进 + 末尾换行
    text = json.dumps(obj, ensure_ascii=False, indent=4) + "\n"
    path.write_text(text, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Sync missing i18n fields across messages.*.json")
    parser.add_argument("--dir", default="resources", help="Directory with messages.<lang>.json")
    parser.add_argument("--reference", default="zh-CN", help="Reference language code")
    parser.add_argument("--apply", action="store_true", help="Write placeholder values into target files")
    args = parser.parse_args()

    base = Path(args.dir)
    if not base.is_dir():
        print(f"[error] directory not found: {base}", file=sys.stderr)
        return 2

    ref_file = base / f"messages.{args.reference}.json"
    if not ref_file.is_file():
        print(f"[error] reference file missing: {ref_file}", file=sys.stderr)
        return 2

    ref = load(ref_file)
    files = sorted(base.glob("messages.*.json"))
    if not files:
        print(f"[error] no messages.*.json files under {base}", file=sys.stderr)
        return 2

    print(f"reference: {args.reference}  ({len(ref)} keys)")
    if not args.apply:
        print("DRY-RUN (use --apply to write placeholders)\n")

    total_added = 0
    for fp in files:
        lang = fp.stem.removeprefix("messages.")
        if lang == args.reference:
            continue
        bundle = load(fp)
        missing = [k for k in ref.keys() if k not in bundle]
        if not missing:
            print(f"[{lang}] up-to-date  ({len(bundle)} keys)")
            continue

        print(f"[{lang}] missing {len(missing)} keys:")
        for k in missing:
            placeholder = ref[k]
            print(f"  + {k} = {placeholder!r}")
            bundle[k] = placeholder
            total_added += 1

        if args.apply:
            dump(fp, bundle)
            print(f"  -> wrote {fp}")

    if total_added == 0:
        print("\nno missing keys.")
    else:
        print(f"\n{'written' if args.apply else 'would write'} {total_added} placeholder entries.")
        if not args.apply:
            print("re-run with --apply to commit.")

    return 0


if __name__ == "__main__":
    sys.exit(main())
