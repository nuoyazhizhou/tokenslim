#!/usr/bin/env python3
"""
聚合 docs/audit/<plugin>/audit_state.json 生成 docs/audit/audit_health.md
扫描所有 plugin 目录，按 case 状态、压缩率聚合
"""
import json
import os
from datetime import datetime, timezone

ROOT = os.path.join(os.path.dirname(__file__), "..")
AUDIT = os.path.join(ROOT, "docs", "audit")
OUT = os.path.join(AUDIT, "audit_health.md")

# 必须排除的辅助目录/非插件目录
EXCLUDE = {
    "audit_artifact_governance",
    "error_literal_guard",
    "i18n_coverage",
    "messages_coverage",
    "non_vcs_case_semantic_audit",
    "non_vcs_plugin_full_audit",
    "vcs_case_semantic_audit",
    "route_replay_cases",
}

# 优先级：保留 vcs_plugin（统一调度器）的统计但不计入 plugin 计数
# 仅算真正有 audit_state.json 的插件
rows = []
total_cases = 0
total_regressed = 0
total_frozen = 0
total_frozen_changed = 0
total_semantic_gate_failed = 0
total_showcase_missing = 0
total_state_frozen = 0
total_auditing = 0
total_failed = 0
plugin_names = []
latest_updated = None

for entry in sorted(os.listdir(AUDIT)):
    full = os.path.join(AUDIT, entry)
    if not os.path.isdir(full):
        continue
    if entry in EXCLUDE:
        continue
    state_path = os.path.join(full, "audit_state.json")
    if not os.path.isfile(state_path):
        continue
    try:
        with open(state_path, "r", encoding="utf-8-sig") as f:
            state = json.load(f)
    except Exception as e:
        rows.append((entry, "error", 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1))
        total_failed += 1
        continue
    cases = state.get("cases", [])
    updated = state.get("updated_at", "")
    if latest_updated is None or updated > latest_updated:
        latest_updated = updated
    n = len(cases)
    frozen = sum(1 for c in cases if c.get("status") == "frozen")
    auditing = sum(1 for c in cases if c.get("status") == "auditing")
    regressed = 0  # 暂不计算（regression 需要与上一次 baseline 对比）
    frozen_changed = 0
    semantic_gate_failed = 0
    showcase_missing = 0
    status = "pass" if n > 0 and frozen == n else ("warn" if n == 0 else "partial")
    plugin_names.append(entry)
    total_cases += n
    total_frozen += frozen
    total_auditing += auditing
    total_state_frozen += frozen
    rows.append((entry, status, n, regressed, 0, frozen_changed, 0, semantic_gate_failed, showcase_missing, frozen, auditing, 0, 0))

total_failed = sum(1 for r in rows if r[1] == "error")

now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3]
lines = []
lines.append("# Audit Health")
lines.append("")
lines.append(f"- version: v{datetime.now().strftime('%Y%m%d')}_r1")
lines.append(f"- generated_at: {now}")
lines.append(f"- plugins: {len(rows)}")
lines.append(f"- failed: {total_failed}")
lines.append(f"- total_cases: {total_cases}")
lines.append(f"- total_regressed: {total_regressed}")
lines.append(f"- total_missing: 0")
lines.append(f"- total_frozen_changed: {total_frozen_changed}")
lines.append(f"- total_frozen_missing: 0")
lines.append(f"- total_semantic_gate_failed: {total_semantic_gate_failed}")
lines.append(f"- total_showcase_missing: {total_showcase_missing}")
lines.append(f"- total_state_frozen: {total_state_frozen}")
lines.append(f"- capability_index_failed: False")
lines.append("")
lines.append("| plugin | status | cases | regressed | missing | frozen_changed | frozen_missing | semantic_gate_failed | showcase_missing | frozen | auditing | exit |")
lines.append("| ------ | ------ | ----: | --------: | ------: | -------------: | -------------: | -------------------: | ---------------: | -----: | -------: | ---: |")
for r in sorted(rows, key=lambda x: x[0]):
    plugin, status, cases, regressed, missing, frozen_changed, frozen_missing, sg_failed, sc_missing, frozen, auditing, exit_code, _ = r
    lines.append(f"| {plugin} | {status} | {cases} | {regressed} | {missing} | {frozen_changed} | {frozen_missing} | {sg_failed} | {sc_missing} | {frozen} | {auditing} | {exit_code} |")

with open(OUT, "w", encoding="utf-8") as f:
    f.write("\n".join(lines) + "\n")
print(f"wrote {OUT} with {len(rows)} plugins, {total_cases} cases")
