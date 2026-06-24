#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
import sys
import re
import json
import argparse
import subprocess
from datetime import datetime

def ensure_dir(path):
    if not os.path.exists(path):
        os.makedirs(path, exist_ok=True)

def parse_key_value_output(lines):
    kv = {}
    for line in lines:
        match = re.match(r'^([A-Za-z0-9_]+)=(.*)$', line.strip())
        if match:
            kv[match.group(1)] = match.group(2)
    return kv

def get_report_plugins(reports_dir="target"):
    samples_dir = "samples"
    if not os.path.exists(samples_dir):
        raise FileNotFoundError("samples directory not found")
        
    plugins = []
    for entry in os.listdir(samples_dir):
        full_path = os.path.join(samples_dir, entry)
        if os.path.isdir(full_path) and entry.endswith("_plugin"):
            # 保留完整 plugin 名（含 _plugin 后缀），与 samples/ 和 docs/audit/ 目录名一致
            report_path = os.path.join(reports_dir, f"{entry}_compact_showcase_report.txt")
            if not os.path.exists(report_path):
                # 兼容旧版 report 文件名（无 _plugin 后缀）
                bare = entry[:-7]
                report_path = os.path.join(reports_dir, f"{bare}_compact_showcase_report.txt")
            if os.path.exists(report_path):
                plugins.append(entry)
            
    return sorted(list(set(plugins)))

def resolve_tokenslim_command():
    candidates = [
        os.path.join("target", "debug", "tokenslim.exe"),
        os.path.join("target", "debug", "tokenslim")
    ]
    for c in candidates:
        if os.path.exists(c):
            return c
            
    # Try finding in system path
    import shutil
    cmd = shutil.which("tokenslim")
    if cmd:
        return cmd
    return ""

def invoke_explain_plugin(tokenslim_exe, cli_args):
    if not tokenslim_exe:
        return {
            "lines": ["explain_skipped=tokenslim_binary_not_found"],
            "parsed": False,
            "json": None,
            "selected_plugin": "",
            "fallback_decision": "",
            "retry_plugin": "",
            "recommendation_primary": "",
            "recommendation_confidence": "",
            "recommendation_action": "",
            "recommendation_reason": ""
        }
        
    cmd = [tokenslim_exe] + cli_args
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, errors="replace", check=False)
        output = res.stdout + "\n" + res.stderr
    except Exception as e:
        return {
            "lines": [f"explain_failed={str(e)}"],
            "parsed": False,
            "json": None,
            "selected_plugin": "",
            "fallback_decision": "",
            "retry_plugin": "",
            "recommendation_primary": "",
            "recommendation_confidence": "",
            "recommendation_action": "",
            "recommendation_reason": ""
        }
        
    lines = output.splitlines()
    raw = "\n".join(lines)
    parsed = False
    json_data = None
    selected_plugin = ""
    fallback_decision = ""
    retry_plugin = ""
    rec_primary = ""
    rec_confidence = ""
    rec_action = ""
    rec_reason = ""
    
    if raw.strip():
        start = raw.find("{")
        end = raw.rfind("}")
        if start >= 0 and end > start:
            candidate = raw[start:end+1].strip()
            try:
                json_data = json.loads(candidate)
                parsed = True
                selected_plugin = str(json_data.get("selected_plugin", ""))
                fallback_decision = str(json_data.get("fallback_decision", ""))
                retry_plugin = str(json_data.get("retry_plugin", ""))
                rec = json_data.get("recommendation", {})
                if rec:
                    rec_primary = str(rec.get("primary", ""))
                    rec_confidence = str(rec.get("confidence", ""))
                    rec_action = str(rec.get("action", ""))
                    rec_reason = str(rec.get("reason", ""))
            except Exception:
                parsed = False
                json_data = None
                
    return {
        "lines": lines,
        "parsed": parsed,
        "json": json_data,
        "selected_plugin": selected_plugin,
        "fallback_decision": fallback_decision,
        "retry_plugin": retry_plugin,
        "recommendation_primary": rec_primary,
        "recommendation_confidence": rec_confidence,
        "recommendation_action": rec_action,
        "recommendation_reason": rec_reason
    }

def get_active_case_ids(plugin, out_dir):
    state_path = os.path.join(out_dir, plugin, "audit_state.json")
    if not os.path.exists(state_path):
        return []
        
    try:
        with open(state_path, "r", encoding="utf-8-sig") as f:
            state = json.load(f)
        ids = []
        for case in state.get("cases", []):
            if case.get("status") in ("auditing", "todo"):
                ids.append(str(case.get("case_id", "")))
        return ids
    except Exception:
        return []

def add_explain_block(md, title, explain_result):
    md.append(f"### {title}\n")
    if explain_result:
        md.append(f"- explain_json_parsed: {explain_result['parsed']}")
        
    if explain_result and explain_result["parsed"]:
        md.append(f"- selected_plugin: {explain_result['selected_plugin']}")
        md.append(f"- fallback_decision: {explain_result['fallback_decision']}")
        md.append(f"- retry_plugin: {explain_result['retry_plugin']}")
        md.append(f"- recommendation_primary: {explain_result['recommendation_primary']}")
        md.append(f"- recommendation_confidence: {explain_result['recommendation_confidence']}")
        md.append(f"- recommendation_action: {explain_result['recommendation_action']}")
        if explain_result["recommendation_reason"]:
            md.append(f"- recommendation_reason: {explain_result['recommendation_reason']}")
        md.append("")
        
        summary = {
            "selected_plugin": explain_result["selected_plugin"],
            "fallback_decision": explain_result["fallback_decision"],
            "retry_plugin": explain_result["retry_plugin"],
            "recommendation": {
                "primary": explain_result["recommendation_primary"],
                "confidence": explain_result["recommendation_confidence"],
                "action": explain_result["recommendation_action"],
                "reason": explain_result["recommendation_reason"]
            }
        }
        md.append("#### machine_readable_summary\n")
        md.append("```json")
        md.append(json.dumps(summary))
        md.append("```\n")
        
        if explain_result["json"]:
            md.append("#### explain_output_json\n")
            md.append("```json")
            md.append(json.dumps(explain_result["json"], indent=4))
            md.append("```\n")
            return
            
    md.append("```text")
    for line in explain_result["lines"][:80]:
        md.append(line)
    md.append("```\n")

def write_route_replay_cases(results, out_dir, version, route_replay_path, route_replay_json_path):
    tokenslim_exe = resolve_tokenslim_command()
    md = [
        "# Route Misclassification Replay Cases\n",
        f"- generated_at: {datetime.now().isoformat()}",
        f"- source_audit_version: {version}",
        f"- tokenslim_explain_binary: {tokenslim_exe or 'not_found'}\n",
        "Use this file when a compact/original mirror looks suspicious: replay the input through `tokenslim explain-plugin`, inspect `fallback_decision`, `retry_plugin`, `recommendation_*`, and capability evidence, then decide whether the issue is a real route/detector mismatch or an expected generic fallback.\n"
    ]
    
    json_active = []
    json_smoke = []
    
    active_items = []
    for r in sorted(results, key=lambda x: x["plugin"]):
        if r["status"] != "pass" or r["state_auditing"] > 0:
            ids = get_active_case_ids(r["plugin"], out_dir)
            if not ids:
                active_items.append({"plugin": r["plugin"], "case_id": "", "reason": "plugin_failed_or_needs_manual_case_lookup"})
            else:
                for cid in ids:
                    active_items.append({"plugin": r["plugin"], "case_id": cid, "reason": "audit_state_active"})
                    
    md.append("## Active Audit Replay Templates\n")
    if not active_items:
        md.append("- None. Current audit has no failed or auditing case requiring route replay.\n")
    else:
        # Show first 20 cases in detail
        for item in active_items[:20]:
            plugin = item["plugin"]
            case_id = item["case_id"]
            reason = item["reason"]
            
            if not case_id:
                md.append(f"### {plugin}\n")
                md.append(f"- reason: {reason}")
                md.append(f"- action: inspect `docs/audit/{plugin}/{plugin}.latest.json` and export a focused case with `audit_case_metrics.py` before replay.\n")
                json_active.append({
                    "plugin": plugin,
                    "case_id": "",
                    "reason": reason,
                    "status": "needs_manual_case_lookup"
                })
                continue
                
            case_dir = os.path.join(out_dir, plugin, "cases", case_id)
            original_path = os.path.join(case_dir, "original.txt")
            replay_command = f"tokenslim explain-plugin --format json --input {original_path} --explain-replay-out {case_dir}/route_replay.md"
            
            md.append(f"### {plugin}/{case_id}\n")
            md.append(f"- reason: {reason}")
            md.append(f"- original: `{original_path}`")
            md.append(f"- replay: `{replay_command}`\n")
            
            if os.path.exists(original_path):
                explain = invoke_explain_plugin(tokenslim_exe, ["explain-plugin", "--format", "json", "--input", original_path])
                add_explain_block(md, "explain output", explain)
                json_active.append({
                    "plugin": plugin,
                    "case_id": case_id,
                    "reason": reason,
                    "original_path": original_path,
                    "replay_command": replay_command,
                    "explain_json_parsed": explain["parsed"],
                    "selected_plugin": explain["selected_plugin"],
                    "fallback_decision": explain["fallback_decision"],
                    "retry_plugin": explain["retry_plugin"],
                    "recommendation": {
                        "primary": explain["recommendation_primary"],
                        "confidence": explain["recommendation_confidence"],
                        "action": explain["recommendation_action"],
                        "reason": explain["recommendation_reason"]
                    },
                    "explain_json": explain["json"]
                })
            else:
                md.append("- explain_skipped: original mirror missing\n")
                json_active.append({
                    "plugin": plugin,
                    "case_id": case_id,
                    "reason": reason,
                    "original_path": original_path,
                    "replay_command": replay_command,
                    "explain_json_parsed": False,
                    "status": "original_mirror_missing"
                })
                
    md.append("## Smoke Explainability Baseline\n")
    command_explain = invoke_explain_plugin(tokenslim_exe, ["explain-plugin", "--format", "json", "--explain-command", "az pipelines runs show"])
    add_explain_block(md, "command: az pipelines runs show", command_explain)
    json_smoke.append({
        "kind": "command",
        "input": "az pipelines runs show",
        "explain_json_parsed": command_explain["parsed"],
        "selected_plugin": command_explain["selected_plugin"],
        "fallback_decision": command_explain["fallback_decision"],
        "retry_plugin": command_explain["retry_plugin"],
        "recommendation": {
            "primary": command_explain["recommendation_primary"],
            "confidence": command_explain["recommendation_confidence"],
            "action": command_explain["recommendation_action"],
            "reason": command_explain["recommendation_reason"]
        },
        "explain_json": command_explain["json"]
    })
    
    sample_path = os.path.join("samples", "web_log_plugin", "case_016_nginx_health_aggregate.log")
    if os.path.exists(sample_path):
        log_explain = invoke_explain_plugin(tokenslim_exe, ["explain-plugin", "--format", "json", "--input", sample_path])
        add_explain_block(md, "log sample: web_log case_016", log_explain)
        json_smoke.append({
            "kind": "log",
            "input": sample_path,
            "explain_json_parsed": log_explain["parsed"],
            "selected_plugin": log_explain["selected_plugin"],
            "fallback_decision": log_explain["fallback_decision"],
            "retry_plugin": log_explain["retry_plugin"],
            "recommendation": {
                "primary": log_explain["recommendation_primary"],
                "confidence": log_explain["recommendation_confidence"],
                "action": log_explain["recommendation_action"],
                "reason": log_explain["recommendation_reason"]
            },
            "explain_json": log_explain["json"]
        })
    else:
        md.append(f"- log sample skipped: {sample_path} not found\n")
        json_smoke.append({
            "kind": "log",
            "input": sample_path,
            "explain_json_parsed": False,
            "status": "sample_not_found"
        })
        
    with open(route_replay_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
        
    json_doc = {
        "generated_at": datetime.now().isoformat(),
        "source_audit_version": version,
        "tokenslim_explain_binary": tokenslim_exe or "not_found",
        "active_replay_cases": json_active,
        "smoke_baseline": json_smoke
    }
    with open(route_replay_json_path, "w", encoding="utf-8") as f:
        json.dump(json_doc, f, indent=4)

def write_audit_review_prompt(audit_index, results, failed, path, route_replay_path, route_replay_json_path):
    prompt = [
        "# LLM Audit Review Prompt\n",
        "You are reviewing TokenSlim case audit results. First read CONTRIBUTING.md and the project's compression protocol (docs/development/PLUGIN_DEVELOPMENT.md), then review the generated audit artifacts listed below. Older *_tactical_prompt.md files are historical human handoff notes, not the active LLM semantic-gate contract.\n",
        "## Audit Run\n",
        f"- version: {audit_index['version']}",
        f"- generated_at: {audit_index['generated_at']}",
        f"- plugins: {audit_index['plugin_count']}",
        f"- failed: {audit_index['failed_count']}",
        f"- total_cases: {audit_index['total_cases']}",
        f"- total_regressed: {audit_index['total_regressed']}",
        f"- total_missing: {audit_index['total_missing']}",
        f"- total_frozen_changed: {audit_index['total_frozen_changed']}",
        f"- total_frozen_missing: {audit_index['total_frozen_missing']}",
        f"- total_semantic_gate_failed: {audit_index['total_semantic_gate_failed']}",
        f"- total_state_frozen: {audit_index['total_state_frozen']}\n",
        "## Required Review\n",
        "1. Check docs/audit/audit_health.md and docs/audit/audit_index.json for failed plugins, regressions, missing cases, frozen drift, and semantic gate failures.",
        "2. For every failed or auditing case, compare docs/audit/<plugin>/cases/<case_id>/original.txt with compact.txt and summary.json.",
        "3. Decide each reviewed case immediately: pass and freeze, needs optimization, or waived with reason.",
        "4. If a case needs optimization, update the relevant task board before ending the turn.",
        "5. Do not mark the task complete while any active P0/P1/P2 item remains stale in docs/tasks, docs/plans, or docs/reports.",
        f"6. Review {route_replay_path} and {route_replay_json_path} for route/detector explainability, fallback decisions, retry_plugin suggestions, recommendation fields, and replay templates for suspicious cases.\n",
        "## Failed Plugins\n"
    ]
    
    if not failed:
        prompt.append("- None\n")
    else:
        for r in sorted(failed, key=lambda x: x["plugin"]):
            prompt.append(f"- {r['plugin']}: regressed={r['regressed']}, missing={r['missing']}, frozen_changed={r['frozen_changed']}, frozen_missing={r['frozen_missing']}, semantic_gate_failed={r['semantic_gate_failed']}, auditing={r['state_auditing']}")
        prompt.append("")
        
    prompt.append("## Plugins With Auditing State\n")
    auditing = [r for r in results if r["state_auditing"] > 0]
    if not auditing:
        prompt.append("- None\n")
    else:
        for r in sorted(auditing, key=lambda x: x["plugin"]):
            prompt.append(f"- {r['plugin']}: auditing={r['state_auditing']}, frozen={r['state_frozen']}, cases={r['cases']}")
        prompt.append("")
        
    prompt.append("## Useful Commands\n")
    prompt.append("~~~powershell")
    prompt.append(f"tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version {audit_index['version']} --case-id <case_id> --require-semantic-gate")
    prompt.append(f"tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version {audit_index['version']} --freeze-case <case_id> --require-semantic-gate")
    prompt.append(f"tokenslim run python scripts/audit_all_case_metrics.py --version {audit_index['version']} --require-semantic-gate --fail-on-regression --fail-on-frozen-change --fail-on-any-failure")
    prompt.append("tokenslim explain-plugin --format json --input docs/audit/<plugin>/cases/<case_id>/original.txt --explain-replay-out docs/audit/<plugin>/cases/<case_id>/route_replay.md")
    prompt.append("~~~\n")
    
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(prompt))

def main():
    parser = argparse.ArgumentParser(description="Aggregated audit run coordinator for all TokenSlim plugins.")
    parser.add_argument("--plugins", default="")
    parser.add_argument("--version", default="")
    parser.add_argument("--reports-dir", default="target")
    parser.add_argument("--out-dir", default="docs/audit")
    parser.add_argument("--matrix-out-dir", default="docs/reports")
    parser.add_argument("--require-semantic-gate", action="store_true")
    parser.add_argument("--fail-on-regression", action="store_true")
    parser.add_argument("--fail-on-frozen-change", action="store_true")
    parser.add_argument("--fail-on-any-failure", action="store_true")
    parser.add_argument("--fail-on-explain-review", action="store_true")
    
    # Capitalized aliases
    parser.add_argument("--Plugins", dest="plugins")
    parser.add_argument("--Version", dest="version")
    parser.add_argument("--ReportsDir", dest="reports_dir")
    parser.add_argument("--OutDir", dest="out_dir")
    parser.add_argument("--MatrixOutDir", dest="matrix_out_dir")
    parser.add_argument("--RequireSemanticGate", dest="require_semantic_gate", action="store_true")
    parser.add_argument("--FailOnRegression", dest="fail_on_regression", action="store_true")
    parser.add_argument("--FailOnFrozenChange", dest="fail_on_frozen_change", action="store_true")
    parser.add_argument("--FailOnAnyFailure", dest="fail_on_any_failure", action="store_true")
    parser.add_argument("--FailOnExplainReview", dest="fail_on_explain_review", action="store_true")

    args = parser.parse_args()
    
    version = args.version or ""
    if not version:
        version = "all_" + datetime.now().strftime("%Y%m%d-%H%M%S")
        
    reports_dir = args.reports_dir or "target"
    out_dir = args.out_dir or "docs/audit"
    matrix_out_dir = args.matrix_out_dir or "docs/reports"
    ensure_dir(out_dir)
    ensure_dir(matrix_out_dir)
    
    # Resolve plugin list
    plugin_names = []
    if args.plugins:
        for p in args.plugins.split(","):
            p_trimmed = p.strip()
            if p_trimmed:
                plugin_names.append(p_trimmed)
    else:
        plugin_names = get_report_plugins(reports_dir)
        
    results = []
    for plugin in sorted(list(set(plugin_names))):
        print(f"audit_plugin={plugin}")
        
        # 构造 report path, 兼容两种命名: target/<plugin>_compact_showcase_report.txt 或 target/<bare>_compact_showcase_report.txt
        report_path = os.path.join(reports_dir, f"{plugin}_compact_showcase_report.txt")
        if not os.path.exists(report_path):
            alt_path = os.path.join(reports_dir, f"{plugin.removesuffix('_plugin')}_compact_showcase_report.txt")
            if os.path.exists(alt_path):
                report_path = alt_path

        # Build command args
        cmd = [
            sys.executable,
            "scripts/audit_case_metrics.py",
            "--plugin", plugin,
            "--report-path", report_path,
            "--out-dir", os.path.join(out_dir, plugin),
            "--version", version,
            "--export-cases"
        ]
        if args.require_semantic_gate:
            cmd.append("--require-semantic-gate")
        if args.fail_on_regression:
            cmd.append("--fail-on-regression")
        if args.fail_on_frozen_change:
            cmd.append("--fail-on-frozen-change")
            
        res = subprocess.run(cmd, capture_output=True, text=True, errors="replace", check=False)
        output_lines = res.stdout.splitlines()
        kv = parse_key_value_output(output_lines)
        
        cases = int(kv.get("cases", 0))
        regressed = int(kv.get("regressed", 0))
        missing = int(kv.get("missing", 0))
        frozen_changed = int(kv.get("frozen_changed", 0))
        frozen_missing = int(kv.get("frozen_missing", 0))
        semantic_gate_failed = int(kv.get("semantic_gate_failed", 0))
        showcase_missing = int(kv.get("showcase_missing", 0))
        state_frozen = int(kv.get("state_frozen", 0))
        state_auditing = int(kv.get("state_auditing", 0))
        
        status = "pass"
        if res.returncode != 0 or regressed > 0 or missing > 0 or frozen_changed > 0 or frozen_missing > 0 or semantic_gate_failed > 0 or showcase_missing > 0:
            status = "fail"
            
        results.append({
            "plugin": plugin,
            "version": version,
            "status": status,
            "exit_code": res.returncode,
            "cases": cases,
            "regressed": regressed,
            "missing": missing,
            "frozen_changed": frozen_changed,
            "frozen_missing": frozen_missing,
            "semantic_gate_failed": semantic_gate_failed,
            "showcase_missing": showcase_missing,
            "state_frozen": state_frozen,
            "state_auditing": state_auditing,
            "snapshot_json": kv.get("snapshot_json", ""),
            "latest_json": os.path.join(out_dir, plugin, f"{plugin}.latest.json"),
            "output": output_lines + res.stderr.splitlines()
        })
        
    # Aggregate stats
    failed = [r for r in results if r["status"] != "pass"]
    total_cases = sum(r["cases"] for r in results)
    total_regressed = sum(r["regressed"] for r in results)
    total_missing = sum(r["missing"] for r in results)
    total_frozen_changed = sum(r["frozen_changed"] for r in results)
    total_frozen_missing = sum(r["frozen_missing"] for r in results)
    total_gate_failed = sum(r["semantic_gate_failed"] for r in results)
    total_showcase_missing = sum(r["showcase_missing"] for r in results)
    total_frozen = sum(r["state_frozen"] for r in results)
    
    # Subprocess run generate_plugin_capability_index.py
    capability_script = os.path.join("scripts", "generate_plugin_capability_index.py")
    capability_output = []
    capability_failed = False
    if os.path.exists(capability_script):
        cmd = [
            sys.executable, capability_script,
            "--audit-dir", out_dir,
            "--json-out", os.path.join(out_dir, "plugin_capability_index.json"),
            "--markdown-out", os.path.join(matrix_out_dir, "plugin_capability_matrix.md")
        ]
        try:
            res_cap = subprocess.run(cmd, capture_output=True, text=True, errors="replace", check=False)
            capability_output = res_cap.stdout.splitlines()
            if res_cap.returncode != 0:
                print(f"ERROR: generate_plugin_capability_index.py failed with code {res_cap.returncode}: {res_cap.stderr}", file=sys.stderr)
                capability_failed = True
        except Exception as e:
            print(f"ERROR: unable to run generate_plugin_capability_index.py: {e}", file=sys.stderr)
            capability_failed = True
    
    audit_index = {
        "version": version,
        "generated_at": datetime.now().isoformat(),
        "plugin_count": len(results),
        "failed_count": len(failed),
        "total_cases": total_cases,
        "total_regressed": total_regressed,
        "total_missing": total_missing,
        "total_frozen_changed": total_frozen_changed,
        "total_frozen_missing": total_frozen_missing,
        "total_semantic_gate_failed": total_gate_failed,
        "total_showcase_missing": total_showcase_missing,
        "total_state_frozen": total_frozen,
        "require_semantic_gate": args.require_semantic_gate,
        "capability_index_failed": capability_failed,
        "plugins": results
    }
    
    index_path = os.path.join(out_dir, "audit_index.json")
    health_path = os.path.join(out_dir, "audit_health.md")
    review_prompt_path = os.path.join(out_dir, "audit_review_prompt.md")
    route_replay_path = os.path.join(out_dir, "route_replay_cases.md")
    route_replay_json_path = os.path.join(out_dir, "route_replay_cases.json")
    
    with open(index_path, "w", encoding="utf-8") as f:
        json.dump(audit_index, f, indent=4)
        
    # Write health md
    md = [
        "# Audit Health\n",
        f"- version: {version}",
        f"- generated_at: {audit_index['generated_at']}",
        f"- plugins: {len(results)}",
        f"- failed: {len(failed)}",
        f"- total_cases: {total_cases}",
        f"- total_regressed: {total_regressed}",
        f"- total_missing: {total_missing}",
        f"- total_frozen_changed: {total_frozen_changed}",
        f"- total_frozen_missing: {total_frozen_missing}",
        f"- total_semantic_gate_failed: {total_gate_failed}",
        f"- total_showcase_missing: {total_showcase_missing}",
        f"- total_state_frozen: {total_frozen}",
        f"- capability_index_failed: {capability_failed}\n",
        "| plugin | status | cases | regressed | missing | frozen_changed | frozen_missing | semantic_gate_failed | showcase_missing | frozen | auditing | exit |",
        "| ------ | ------ | ----: | --------: | ------: | -------------: | -------------: | -------------------: | ---------------: | -----: | -------: | ---: |"
    ]
    for r in sorted(results, key=lambda x: x["plugin"]):
        md.append(f"| {r['plugin']} | {r['status']} | {r['cases']} | {r['regressed']} | {r['missing']} | {r['frozen_changed']} | {r['frozen_missing']} | {r['semantic_gate_failed']} | {r['showcase_missing']} | {r['state_frozen']} | {r['state_auditing']} | {r['exit_code']} |")
        
    if failed:
        md.append("\n## Failed Plugins\n")
        for r in sorted(failed, key=lambda x: x["plugin"]):
            md.append(f"### {r['plugin']}\n")
            md.append("```text")
            for line in r["output"][-20:]:
                md.append(line)
            md.append("```\n")
            
    with open(health_path, "w", encoding="utf-8") as f:
        f.write("\n".join(md))
        
    # Explain review calculations
    write_route_replay_cases(results, out_dir, version, route_replay_path, route_replay_json_path)
    write_audit_review_prompt(audit_index, results, failed, review_prompt_path, route_replay_path, route_replay_json_path)
    
    explain_review_flagged = 0
    explain_review_active_cases = 0
    explain_review_smoke_cases = 0
    
    if os.path.exists(route_replay_json_path):
        try:
            with open(route_replay_json_path, "r", encoding="utf-8-sig") as f:
                replay_json = json.load(f)
            active_cases = replay_json.get("active_replay_cases", [])
            explain_review_active_cases = len(active_cases)
            for entry in active_cases:
                action = entry.get("recommendation", {}).get("action", "") if entry.get("recommendation") else ""
                decision = entry.get("fallback_decision", "")
                if action == "review_and_retry" or decision == "review_recommended":
                    explain_review_flagged += 1
                    
            smoke_cases = replay_json.get("smoke_baseline", [])
            explain_review_smoke_cases = len(smoke_cases)
            for entry in smoke_cases:
                action = entry.get("recommendation", {}).get("action", "") if entry.get("recommendation") else ""
                decision = entry.get("fallback_decision", "")
                if action == "review_and_retry" or decision == "review_recommended":
                    explain_review_flagged += 1
        except Exception as e:
            print(f"WARN: unable to parse route replay json for explain-review gate: {e}")
            
    # Outputs
    print(f"audit_index={index_path}")
    print(f"audit_health={health_path}")
    print(f"audit_review_prompt={review_prompt_path}")
    print(f"route_replay_cases={route_replay_path}")
    print(f"route_replay_cases_json={route_replay_json_path}")
    for line in capability_output:
        print(line)
    print(f"plugins={len(results)}")
    print(f"failed={len(failed)}")
    if capability_failed:
        print("capability_index_failed=true")
    print(f"total_cases={total_cases}")
    print(f"total_regressed={total_regressed}")
    print(f"total_missing={total_missing}")
    print(f"total_frozen_changed={total_frozen_changed}")
    print(f"total_frozen_missing={total_frozen_missing}")
    print(f"total_semantic_gate_failed={total_gate_failed}")
    print(f"total_showcase_missing={total_showcase_missing}")
    print(f"total_state_frozen={total_frozen}")
    print(f"explain_review_path={route_replay_json_path}")
    print(f"explain_review_active_cases={explain_review_active_cases}")
    print(f"explain_review_smoke_cases={explain_review_smoke_cases}")
    print(f"explain_review_flagged={explain_review_flagged}")
    
    if args.fail_on_any_failure and (len(failed) > 0 or capability_failed):
        print(f"ERROR: Audit failed: {len(failed)} plugin(s) failed, capability_failed={capability_failed}.", file=sys.stderr)
        sys.exit(1)
    
    if args.fail_on_explain_review and explain_review_flagged > 0:
        print(f"ERROR: Explain review gate failed: flagged={explain_review_flagged}.", file=sys.stderr)
        sys.exit(11)

if __name__ == "__main__":
    main()
