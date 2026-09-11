# scripts/diagnose_orphan_cases.py
# 对单个 plugin：列 showcase.rs 注册的 case 与 samples/ 物理 case 的差集（孤儿）
# 用法：
#   python scripts/diagnose_orphan_cases.py --plugin cloud_log_plugin
import argparse
import re
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--plugin", required=True)
    args = ap.parse_args()

    showcase = Path("src/plugins") / args.plugin / "showcase.rs"
    text = showcase.read_text(encoding="utf-8")
    # 抓所有 "case_XXX_..." 字符串
    registered = sorted(set(re.findall(r'"(case_\d+_[A-Za-z0-9_]+)"', text)))
    samples = sorted(
        p.stem for p in (Path("samples") / args.plugin).glob("case_*.log")
    )

    orphans = [s for s in samples if s not in registered]
    missing = [r for r in registered if r not in samples]

    print(f"plugin       : {args.plugin}")
    print(f"showcase.rs  : {showcase}")
    print(f"samples dir  : samples/{args.plugin}")
    print(f"physical     : {len(samples)}")
    print(f"registered   : {len(registered)}")
    print(f"orphans      : {len(orphans)}")
    print(f"missing_reg  : {len(missing)}")

    if orphans:
        print("\n--- orphan case files (in samples/ but not in showcase.rs) ---")
        for s in orphans:
            print(s)

    if missing:
        print("\n--- missing samples (in showcase.rs but not in samples/) ---")
        for r in missing:
            print(r)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
