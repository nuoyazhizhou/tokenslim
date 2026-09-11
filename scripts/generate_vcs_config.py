#!/usr/bin/env python3
"""
Generate vcs_plugin configuration from VCS command/log samples.

Usage examples:
  python scripts/generate_vcs_config.py --input samples/git_status.log
  python scripts/generate_vcs_config.py --input logs/a.log --input logs/b.log --output config/vcs_plugin.json
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Dict, List

VCS_DEFAULT_COMMANDS: Dict[str, List[str]] = {
    "git": [
        "status",
        "diff",
        "log",
        "show",
        "branch",
        "checkout",
        "switch",
        "merge",
        "rebase",
        "reset",
        "stash",
        "fetch",
        "pull",
        "push",
        "remote",
        "tag",
        "cherry-pick",
        "revert",
        "blame",
        "bisect",
        "restore",
        "clean",
        "submodule",
    ],
    "svn": [
        "status",
        "diff",
        "log",
        "info",
        "add",
        "delete",
        "move",
        "copy",
        "commit",
        "update",
        "checkout",
        "revert",
        "merge",
        "switch",
        "resolve",
        "cleanup",
        "blame",
    ],
    "hg": [
        "status",
        "diff",
        "log",
        "summary",
        "add",
        "remove",
        "rename",
        "commit",
        "update",
        "branch",
        "pull",
        "push",
        "annotate",
        "graft",
        "rebase",
        "shelve",
        "unshelve",
        "revert",
    ],
    "p4": [
        "opened",
        "changes",
        "describe",
        "diff",
        "submit",
        "sync",
        "edit",
        "add",
        "delete",
        "revert",
        "integrate",
        "resolve",
        "reconcile",
        "shelve",
        "unshelve",
        "files",
        "client",
    ],
    "cvs": [
        "status",
        "diff",
        "log",
        "add",
        "remove",
        "commit",
        "update",
        "checkout",
        "tag",
        "annotate",
        "edit",
        "unedit",
        "release",
        "history",
    ],
    "bzr": [
        "status",
        "diff",
        "log",
        "add",
        "remove",
        "commit",
        "update",
        "branch",
        "pull",
        "push",
        "merge",
        "resolve",
        "missing",
        "revert",
    ],
    "fossil": [
        "status",
        "diff",
        "timeline",
        "changes",
        "add",
        "rm",
        "commit",
        "update",
        "sync",
        "checkout",
        "merge",
        "stash",
        "undo",
        "tag",
    ],
    "darcs": [
        "whatsnew",
        "diff",
        "changes",
        "record",
        "pull",
        "push",
        "rebase",
        "add",
        "remove",
        "revert",
        "tag",
        "amend-record",
        "obliterate",
    ],
}

VCS_DEFAULT_SIGNATURES: Dict[str, List[str]] = {
    "git": ["on branch", "not a git repository", "diff --git"],
    "svn": ["svn:", "checked out revision", "subversion"],
    "hg": ["mercurial", "changeset:", "abort: no repository found"],
    "p4": ["perforce", "client error:", "... //"],
    "cvs": ["cvs [", "cvs checkout", "cvs update"],
    "bzr": ["bzr:", "bazaar", "no working tree"],
    "fossil": ["fossil", "project-name:", "checkout:"],
    "darcs": ["darcs", "no repository present", "whatsnew"],
}

VCS_SIGNATURE_HINTS = {
    "git": ["on branch", "changes not staged", "untracked files", "diff --git"],
    "svn": ["svn:", "revision", "checked out"],
    "hg": ["changeset:", "mercurial", "working directory"],
    "p4": ["perforce", "... //", "client error"],
    "cvs": ["cvs", "updating"],
    "bzr": ["bzr:", "bazaar"],
    "fossil": ["fossil", "project-name:", "timeline"],
    "darcs": ["darcs", "whatsnew"],
}

PATH_RE = re.compile(r"(?:[A-Za-z]:\\|[./]|[\w.-]+/)[\w./\\-]+(?:\.[\w-]+)?")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate vcs_plugin.json from logs")
    parser.add_argument("--input", action="append", required=True, help="Input log file path (repeatable)")
    parser.add_argument(
        "--output",
        default="config/vcs_plugin.json",
        help="Output config path (default: config/vcs_plugin.json)",
    )
    parser.add_argument("--replace", action="store_true", help="Set replace flags in output config")
    return parser.parse_args()


def detect_tool(text: str) -> str | None:
    lowered = text.lower()
    best_tool = None
    best_score = 0
    for tool, hints in VCS_SIGNATURE_HINTS.items():
        score = sum(1 for h in hints if h in lowered)
        if score > best_score:
            best_score = score
            best_tool = tool
    return best_tool


def collect_dynamic_signatures(text: str) -> List[str]:
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    selected = []
    for line in lines[:80]:
        if len(line) > 6 and len(line) < 120 and not PATH_RE.search(line):
            selected.append(line.lower())
        if len(selected) >= 6:
            break
    return selected


def collect_commands(text: str, defaults: List[str]) -> List[str]:
    lowered = text.lower()
    discovered = []
    for cmd in defaults:
        if f" {cmd} " in lowered or f"\n{cmd} " in lowered or f" {cmd}\n" in lowered or f"{cmd}:" in lowered:
            discovered.append(cmd)
    if not discovered:
        return list(defaults)
    return sorted(set(defaults + discovered))


def main() -> int:
    args = parse_args()

    merged_signatures = {k: list(v) for k, v in VCS_DEFAULT_SIGNATURES.items()}
    merged_commands = {k: list(v) for k, v in VCS_DEFAULT_COMMANDS.items()}

    dictionaryize_paths = True
    for input_path in args.input:
        path = Path(input_path)
        if not path.exists():
            raise FileNotFoundError(f"Input not found: {path}")

        text = path.read_text(encoding="utf-8", errors="ignore")
        tool = detect_tool(text)
        if not tool:
            continue

        dynamic_sigs = collect_dynamic_signatures(text)
        if dynamic_sigs:
            merged_signatures[tool] = sorted(set(merged_signatures[tool] + dynamic_sigs))[:16]

        merged_commands[tool] = collect_commands(text, VCS_DEFAULT_COMMANDS[tool])

        if PATH_RE.search(text):
            dictionaryize_paths = True

    output = {
        "dictionaryize_paths": dictionaryize_paths,
        "compact_leading_ws": True,
        "collapse_blank_lines": True,
        "max_blank_lines": 1,
        "replace_command_whitelists": bool(args.replace),
        "replace_signatures": bool(args.replace),
        "command_whitelists": merged_commands,
        "signatures": merged_signatures,
    }

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"Generated {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
