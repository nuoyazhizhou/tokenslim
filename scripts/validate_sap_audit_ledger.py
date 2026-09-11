#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Read-only integrity gate for TokenSlim SAP audit ledgers."""
from __future__ import annotations

import argparse
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

HEADER_RE = re.compile(r"^#{1,6}\s+SAP-(\d{4})(?:[：:\s].*)?$", re.MULTILINE)
STATUS_RE = re.compile(
    r"(?:^|\n)\s*(?:status\s*:?\s*(P0|P1|pass)|状态\s*[：:]\s*(P0|P1|通过|条件通过|失败))",
    re.IGNORECASE,
)
SEVERITY_RE = re.compile(
    r"(?:\*\*(P0|P1)\*\*|(?:根因归类|严重级别|缺陷等级|severity)[^\n]{0,120}?\b(P0|P1)\b)",
    re.IGNORECASE,
)
ATTRIBUTION_RE = re.compile(r"调用链\s*/\s*归因|调用链|归因|attribution", re.IGNORECASE)
FUNCTION_RE = re.compile(r"`?[A-Za-z_]\w*::[A-Za-z_]\w*(?:::[A-Za-z_]\w*)?`?")
OUTCOME_RE = re.compile(r"pass|fail|partial|通过|不通过|条件通过|失败", re.IGNORECASE)
STATUS_MAP = {
    "p0": "P0",
    "p1": "P1",
    "pass": "pass",
    "通过": "pass",
    "条件通过": "P1",
    "失败": "P0",
}


def parse_entries(text: str):
    matches = list(HEADER_RE.finditer(text))
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        yield int(match.group(1)), text.count("\n", 0, match.start()) + 1, text[match.start():end]


def status_for(body: str):
    values = []
    for match in STATUS_RE.finditer(body):
        raw = next(value for value in match.groups() if value is not None).lower()
        values.append(STATUS_MAP[raw])
    if len(set(values)) == 1 and values:
        return values[0], values
    return None, values


def has_r_conclusion(body: str, number: int) -> bool:
    return any(
        OUTCOME_RE.search(body[match.start():match.start() + 220])
        for match in re.finditer(fr"\bR{number}\b", body, re.IGNORECASE)
    )


def has_function_attribution(body: str) -> bool:
    lines = body.splitlines()
    for index, line in enumerate(lines):
        if not ATTRIBUTION_RE.search(line):
            continue
        if FUNCTION_RE.search(line):
            return True
        if index + 1 < len(lines) and FUNCTION_RE.search(lines[index + 1]):
            return True
    return False


def render_ids(values: list[int]) -> str:
    rendered = ", ".join(f"SAP-{value:04d}" for value in values[:30])
    return rendered if len(values) <= 30 else f"{rendered}, … (+{len(values) - 30})"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate a read-only SAP audit ledger.")
    parser.add_argument("--ledger", type=Path, default=Path("docs/audit/semantic_preservation_audit.md"))
    parser.add_argument("--cases-root", type=Path, default=Path("docs/audit"))
    parser.add_argument("--expected-count", type=int, help="Override the summary.json-derived full case count.")
    parser.add_argument("--range-start", type=int, help="First SAP number for shard-only validation.")
    parser.add_argument("--range-end", type=int, help="Last SAP number for shard-only validation.")
    parser.add_argument("--max-errors", type=int, default=80)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.ledger.is_file() or not args.cases_root.is_dir():
        print("ERROR[INPUT] ledger or cases-root does not exist", file=sys.stderr)
        return 2
    total_cases = args.expected_count or sum(1 for _ in args.cases_root.rglob("summary.json"))
    if total_cases <= 0:
        print("ERROR[INPUT] expected case count is zero", file=sys.stderr)
        return 2
    if (args.range_start is None) != (args.range_end is None):
        print("ERROR[INPUT] --range-start and --range-end must be supplied together", file=sys.stderr)
        return 2
    if args.range_start is not None and (
        args.range_start < 1 or args.range_end < args.range_start or args.range_end > total_cases
    ):
        print(f"ERROR[INPUT] shard range must be within 1..{total_cases}", file=sys.stderr)
        return 2

    expected_ids = (
        set(range(args.range_start, args.range_end + 1))
        if args.range_start is not None
        else set(range(1, total_cases + 1))
    )
    scope = (
        f"SAP-{args.range_start:04d}..SAP-{args.range_end:04d}"
        if args.range_start is not None
        else f"SAP-0001..SAP-{total_cases:04d}"
    )
    grouped = defaultdict(list)
    parsed = list(parse_entries(args.ledger.read_text(encoding="utf-8-sig")))
    for sap_id, line, body in parsed:
        grouped[sap_id].append((line, body))

    errors = []
    missing = sorted(expected_ids - set(grouped))
    outside = sorted(set(grouped) - expected_ids)
    if missing:
        errors.append(("MISSING_SAP", "ledger", f"Missing {len(missing)} entries: {render_ids(missing)}"))
    if outside:
        errors.append(("UNEXPECTED_SAP", "ledger", f"Outside {scope}: {render_ids(outside)}"))

    for sap_id, records in sorted(grouped.items()):
        if sap_id not in expected_ids:
            continue
        where = f"SAP-{sap_id:04d}"
        if len(records) != 1:
            lines = ", ".join(str(line) for line, _ in records)
            errors.append(("DUPLICATE_SAP", where, f"Individual sections at lines {lines}"))
            continue
        line, body = records[0]
        where = f"{where}:L{line}"
        state, states = status_for(body)
        if state is None:
            errors.append(("INCONSISTENT_STATUS" if states else "MISSING_STATUS", where, ", ".join(states) if states else "No recognized status"))
        for number in range(1, 5):
            if not has_r_conclusion(body, number):
                errors.append((f"MISSING_R{number}", where, f"R{number} lacks an explicit conclusion"))
        if not has_function_attribution(body):
            errors.append(("MISSING_FUNCTION_ATTRIBUTION", where, "Expected labelled Plugin::function attribution"))
        severities = {
            next(value for value in match.groups() if value is not None).upper()
            for match in SEVERITY_RE.finditer(body)
        }
        if len(severities) > 1:
            errors.append(("CONFLICTING_SEVERITY", where, ", ".join(sorted(severities))))
        elif state and severities:
            severity = next(iter(severities))
            if state == "pass" or state != severity:
                errors.append(("STATUS_SEVERITY_MISMATCH", where, f"status={state}, explicit={severity}"))

    print("SAP audit ledger integrity gate")
    print(f"validation_scope={scope}")
    print(f"expected_cases={len(expected_ids)}")
    print(f"individual_sections={len(parsed)}")
    print(f"unique_ids={len(grouped)}")
    print(f"violations={len(errors)}")
    if not errors:
        print("RESULT=PASS")
        return 0
    print("RESULT=FAIL")
    counts = Counter(item[0] for item in errors)
    print("violation_counts=" + ", ".join(f"{code}:{count}" for code, count in sorted(counts.items())))
    for code, where, message in errors[:args.max_errors]:
        print(f"ERROR[{code}] {where}: {message}")
    if len(errors) > args.max_errors:
        print(f"ERROR[TRUNCATED] Printed {args.max_errors} of {len(errors)}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
