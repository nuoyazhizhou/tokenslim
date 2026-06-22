#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import hashlib
import re
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def plugin_dirs(base: Path) -> list[str]:
    if not base.exists():
        return []
    names = []
    for path in base.iterdir():
        if not path.is_dir():
            continue
        name = path.name
        if not name.endswith("_plugin"):
            continue
        if name == "vcs_plugin" or not name.startswith("vcs_"):
            names.append(name)
    return sorted(set(names))


def quoted_items(text: str) -> list[str]:
    return sorted(set(re.findall(r'"([^"]+)"', text)))


def parse_config_lists(config_path: Path) -> dict[str, list[str]]:
    content = config_path.read_text(encoding="utf-8-sig", errors="replace")

    static_match = re.search(r"static_plugins\s*=\s*\[(.*?)\]", content, re.DOTALL)
    dynamic_match = re.search(r"dynamic_plugins\s*=\s*\[(.*?)\]", content, re.DOTALL)
    enabled_match = re.search(r"enabled\s*=\s*\[(.*?)\]", content, re.DOTALL)

    static_raw = static_match.group(1) if static_match else ""
    dynamic_raw = dynamic_match.group(1) if dynamic_match else ""
    enabled_raw = enabled_match.group(1) if enabled_match else ""

    return {
        "static_names": quoted_items(static_raw),
        "dynamic_names": sorted(set(re.findall(r'name\s*=\s*"([^"]+)"', dynamic_raw))),
        "enabled_names": quoted_items(enabled_raw),
    }


def sha256_or_na(path: Path) -> str:
    if not path.exists():
        return "N/A"
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def rel(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def write_bullets(lines: list[str], items: list[str], indent: str = "") -> None:
    if not items:
        lines.append(f"{indent}- None")
        return
    for item in items:
        lines.append(f"{indent}- {item}")


def generate_report(output_path: Path) -> None:
    dynamic_dirs = plugin_dirs(ROOT / "plugins")
    static_dirs = plugin_dirs(ROOT / "src" / "plugins")

    dynamic_set = set(dynamic_dirs)
    static_set = set(static_dirs)
    common_dirs = sorted(dynamic_set & static_set)
    dynamic_only_dirs = sorted(dynamic_set - static_set)
    static_only_dirs = sorted(static_set - dynamic_set)

    config = parse_config_lists(ROOT / "config" / "plugins.toml")
    expected_static_dirs = sorted(f"{name}_plugin" for name in config["static_names"])
    expected_dynamic_dirs = sorted(f"{name}_plugin" for name in config["dynamic_names"])

    expected_static_set = set(expected_static_dirs)
    expected_dynamic_set = set(expected_dynamic_dirs)
    missing_static_by_config = sorted(expected_static_set - static_set)
    extra_static_by_config = sorted(static_set - expected_static_set)
    missing_dynamic_by_config = sorted(expected_dynamic_set - dynamic_set)
    extra_dynamic_by_config = sorted(dynamic_set - expected_dynamic_set)

    comparison_rows = []
    for name in common_dirs:
        dynamic_file = ROOT / "plugins" / name / "src" / "lib.rs"
        static_file = ROOT / "src" / "plugins" / name / "methods.rs"
        dynamic_hash = sha256_or_na(dynamic_file)
        static_hash = sha256_or_na(static_file)
        comparison_rows.append({
            "plugin": name,
            "dynamic_file": rel(dynamic_file),
            "static_file": rel(static_file),
            "dynamic_hash": dynamic_hash,
            "static_hash": static_hash,
            "same_hash": dynamic_hash != "N/A" and dynamic_hash == static_hash,
        })

    generated_at = datetime.now(timezone.utc).astimezone().strftime("%Y-%m-%d %H:%M:%S %z")
    lines: list[str] = []
    lines.append("# Plugin Alignment Report")
    lines.append("")
    lines.append(f"Generated at: {generated_at}")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(f"- plugins/ dynamic dirs: {len(dynamic_dirs)}")
    lines.append(f"- src/plugins/ static dirs: {len(static_dirs)}")
    lines.append(f"- Same-name dirs in both trees: {len(common_dirs)}")
    lines.append("")

    lines.append("## Dynamic Only (plugins/)")
    lines.append("")
    write_bullets(lines, dynamic_only_dirs)
    lines.append("")

    lines.append("## Static Only (src/plugins/)")
    lines.append("")
    write_bullets(lines, static_only_dirs)
    lines.append("")

    lines.append("## Same-Name Plugin Hash Comparison")
    lines.append("")
    if not comparison_rows:
        lines.append("No same-name plugin dirs exist in both trees.")
    else:
        lines.append("| plugin | dynamic file | static file | same hash |")
        lines.append("|---|---|---|---|")
        for row in comparison_rows:
            same = "yes" if row["same_hash"] else "no"
            lines.append(f"| {row['plugin']} | {row['dynamic_file']} | {row['static_file']} | {same} |")
    lines.append("")

    lines.append("## Config Alignment (config/plugins.toml)")
    lines.append("")
    lines.append(f"- static_plugins entries: {len(config['static_names'])}")
    lines.append(f"- dynamic_plugins entries: {len(config['dynamic_names'])}")
    lines.append(f"- enabled entries: {len(config['enabled_names'])}")
    lines.append("")

    lines.append("### Static Config vs src/plugins/")
    lines.append("")
    lines.append("- Missing dirs from config:")
    write_bullets(lines, missing_static_by_config, indent="  ")
    lines.append("- Extra dirs not declared in static_plugins:")
    write_bullets(lines, extra_static_by_config, indent="  ")
    lines.append("")

    lines.append("### Dynamic Config vs plugins/")
    lines.append("")
    lines.append("- Missing dirs from config:")
    write_bullets(lines, missing_dynamic_by_config, indent="  ")
    lines.append("- Extra dirs not declared in dynamic_plugins:")
    write_bullets(lines, extra_dynamic_by_config, indent="  ")
    lines.append("")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Report generated: {rel(output_path)}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate plugin alignment report.")
    parser.add_argument("--output-path", default="docs/reports/plugin_alignment_report.md")
    parser.add_argument("--OutputPath", dest="output_path")
    args = parser.parse_args()

    output_path = Path(args.output_path)
    if not output_path.is_absolute():
        output_path = ROOT / output_path

    generate_report(output_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
