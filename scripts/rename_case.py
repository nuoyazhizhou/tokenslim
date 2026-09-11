# scripts/rename_case.py
# 重命名 case 物理文件（log + scenario.yaml 等同 stem 的所有扩展名），
# 并在 showcase.rs / test.rs 里同步改 case_id 字符串。
#
# 用法：
#   python scripts/rename_case.py --plugin cloud_log_plugin --old case_050_aliyun_csv_multiline --new case_052_aliyun_csv_multiline
import argparse
import re
import shutil
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--plugin", required=True)
    ap.add_argument("--old", required=True, help="原 case id（文件名 stem 的一部分）")
    ap.add_argument("--new", required=True, help="新 case id")
    args = ap.parse_args()

    samples_dir = Path("samples") / args.plugin
    if not samples_dir.is_dir():
        print(f"[error] {samples_dir} 不存在")
        return 2

    moved = []
    for fp in sorted(samples_dir.glob(f"{args.old}.*")):
        target = fp.with_name(fp.name.replace(args.old, args.new, 1))
        if target.exists():
            print(f"[skip] {target} 已存在")
            continue
        shutil.move(str(fp), str(target))
        moved.append((fp.name, target.name))
        print(f"moved: {fp.name}  ->  {target.name}")

    # showcase.rs 改 case_id 字符串
    showcase = Path("src/plugins") / args.plugin / "showcase.rs"
    if showcase.is_file():
        text = showcase.read_text(encoding="utf-8")
        new_text = re.sub(rf'"{re.escape(args.old)}"', f'"{args.new}"', text)
        if new_text != text:
            showcase.write_text(new_text, encoding="utf-8")
            print(f"updated: {showcase}")
        else:
            print(f"[skip] {showcase} 中未找到 {args.old!r}")

    # test.rs 同理
    test_rs = Path("src/plugins") / args.plugin / "test.rs"
    if test_rs.is_file():
        text = test_rs.read_text(encoding="utf-8")
        if args.old in text:
            new_text = text.replace(args.old, args.new)
            test_rs.write_text(new_text, encoding="utf-8")
            print(f"updated: {test_rs}")
        else:
            print(f"[skip] {test_rs} 中未找到 {args.old!r}")

    print(f"\n总移动: {len(moved)} 个文件")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
