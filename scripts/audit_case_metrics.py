#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
import sys
import re
import json
import csv
import hashlib
import argparse
import time
import urllib.request
import urllib.error
import pathlib
import tempfile
from datetime import datetime

# audit_llm_common：LLM 调用 + 提示词加载 公共模块（fix #29）
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import audit_llm_common as _alc  # noqa: E402

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

def parse_env(env_path=".env"):
    env = dict(os.environ)
    if os.path.exists(env_path):
        with open(env_path, "r", encoding="utf-8", errors="ignore") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                if "=" in line:
                    k, v = line.split("=", 1)
                    value = v.strip()
                    if (value.startswith('"') and value.endswith('"')) or (value.startswith("'") and value.endswith("'")):
                        value = value[1:-1]
                    key = k.strip()
                    if key not in os.environ:
                        env[key] = value
    return env

def sha256_hex(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()

def ensure_dir(path):
    if not os.path.exists(path):
        os.makedirs(path, exist_ok=True)

# TOKENSLIM_AUDIT_SYSTEM_PREFIX 已抽到 scripts/prompts/audit/case_metrics/_base.md（fix #29）
# 加载走 audit_llm_common.build_case_metrics_prompt，build_llm_system_prompt 是薄转发。

SEMANTIC_PROFILE_PATH = os.path.join("docs", "prompts", "semantic_audit_profiles.md")

PROFILE_GROUPS = {
    "vcs-dvcs": {
        "vcs_git", "vcs_hg", "vcs_bzr", "vcs_darcs", "vcs_fossil",
    },
    "vcs-centralized": {
        "vcs_svn", "vcs_p4", "vcs_cvs",
    },
    "vcs-cloud-cli": {
        "vcs_gh", "vcs_glab", "vcs_az", "vcs_bitbucket", "vcs_gerrit", "vcs_repo",
    },
    "build-compiler": {
        "android_gradle", "ansible", "bazel", "cloudformation", "dotnet",
        "gcc_log", "helm", "maven", "pulumi", "rust_go",
        "terraform", "unity_unreal", "webpack_vite", "xcode_log",
    },
    "runtime-trace": {
        "java_stack", "node_error", "php_ruby", "pytest", "python_traceback", "smart_code",
    },
    "structured-log": {
        "ci_log", "cloud_log", "db_log", "kubernetes_docker", "nodejs",
        "spring_boot", "syslog", "web_log",
    },
    "data-format": {
        "artifact_summary", "git_diff", "json", "markdown", "ndjson", "protobuf", "sql",
        "xml_html", "yaml",
    },
    "utility-text": {
        "ansi_cleaner", "generic_text", "noise_filter", "smart_path",
        "static_rule", "template_driven",
    },
    "shell-command": {
        "shell_command", "shell", "cmd", "powershell", "bash", "zsh", "fish", "shell_session",
    },
}

def build_llm_system_prompt(tactical_rules=None, plugin_type="default", plugin=""):
    """fix #29: 转发到 audit_llm_common.build_case_metrics_prompt。

    加载顺序：scripts/prompts/audit/case_metrics/_base.md（含 9 条 audit constitution
    + JSON schema）+ {plugin_type}.md（type-specific 规则段）+ _footer.md。
    plugin_type / plugin 入参向后兼容 — 老调用方传 (tactical_rules) 也能工作。
    """
    return _alc.build_case_metrics_prompt(
        plugin_type=plugin_type,
        tactical_rules=tactical_rules or "",
        kb=None,
    )

def atomic_write_json(path, obj):
    target_dir = os.path.dirname(path) or "."
    ensure_dir(target_dir)
    fd, tmp_path = tempfile.mkstemp(
        prefix=f".{os.path.basename(path)}.",
        suffix=".tmp",
        dir=target_dir,
        text=True
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            json.dump(obj, f, indent=4)
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp_path, path)
    except Exception:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass
        raise

def parse_showcase_rs_cases(plugin, source_dir=None):
    if not source_dir:
        source_dir = os.path.join("src", "plugins")
        
    plugin_normalized = plugin.replace("_plugin", "")
    candidates = [
        os.path.join(source_dir, f"{plugin_normalized}_plugin", "showcase.rs"),
        os.path.join(source_dir, plugin_normalized, "showcase.rs")
    ]
    
    showcase_path = None
    for path in candidates:
        if os.path.exists(path):
            showcase_path = path
            break
            
    if not showcase_path:
        return None  # None indicates file does not exist
        
    with open(showcase_path, "r", encoding="utf-8", errors="ignore") as f:
        content = f.read()
        
    # Match cases blocks such as:
    #   let cases = [ ... ];
    #   let cases: &[&str] = &[ ... ];
    #   let cases: Vec<...> = vec![ ... ];
    cases_match = re.search(r'(?:let|const|static)\s+\w+\s*(?::\s*[^=]+)?\s*=\s*&?\s*(?:vec!\s*)?\[(.*?)\]\s*;', content, re.DOTALL)
    registered = []

    def tuple_case_record(case_id_or_file, file_base, title):
        if "." in case_id_or_file:
            filename = case_id_or_file
        elif 'format!("{}_{}.log"' in content or file_base:
            filename = f"{case_id_or_file}_{file_base}.log"
        else:
            filename = f"{case_id_or_file}.log"

        return {
            "case_id": pathlib.Path(filename).stem,
            "filename": filename,
            "title": title
        }
    
    if cases_match:
        cases_block = cases_match.group(1)
        
        # 1. Try parsing struct ShowcaseCase
        struct_blocks = re.findall(r'ShowcaseCase\s*\{([^}]+)\}', cases_block)
        for block in struct_blocks:
            file_match = re.search(r'file_name:\s*"([^"]+)"', block)
            title_match = re.search(r'title:\s*"([^"]+)"', block)
            if file_match and title_match:
                filename = file_match.group(1)
                title = title_match.group(1)
                case_id = pathlib.Path(filename).stem if "." in filename else filename
                registered.append({
                    "case_id": case_id,
                    "filename": filename,
                    "title": title
                })
            
        if not registered:
            # 2. Try parsing tuples. VCS plugins often use
            # ("case_23", "git_status", "status") and load
            # samples as case_23_git_status.log.
            tuple_matches = re.findall(r'\((.*?)\)', cases_block, re.DOTALL)
            for t in tuple_matches:
                strings = re.findall(r'"([^"]*)"', t)
                if len(strings) >= 2:
                    case_id_or_file = strings[0]
                    if "." in case_id_or_file:
                        filename = case_id_or_file
                        registered.append({
                            "case_id": pathlib.Path(filename).stem,
                            "filename": filename,
                            "title": strings[-1] if len(strings) >= 2 else ""
                        })
                    elif 'format!("{}_{}.log"' in content or 'format!("{}_{}.log"' in cases_block or len(strings) >= 3:
                        registered.append(tuple_case_record(
                            case_id_or_file,
                            strings[1],
                            strings[-1] if len(strings) >= 2 else ""
                        ))
                    else:
                        case_id = case_id_or_file
                        registered.append({
                            "case_id": case_id,
                            "filename": f"{case_id}.log",
                            "title": strings[-1] if len(strings) >= 2 else ""
                        })

        if not registered:
            # 3. Try parsing string-only arrays like:
            # let cases: &[&str] = &["case_96_az_repos_show", ...];
            string_matches = re.findall(r'"(case_[^"]+)"', cases_block)
            for case_id_or_file in string_matches:
                if "." in case_id_or_file:
                    filename = case_id_or_file
                    case_id = pathlib.Path(filename).stem
                else:
                    case_id = case_id_or_file
                    filename = f"{case_id}.log"
                registered.append({
                    "case_id": case_id,
                    "filename": filename,
                    "title": case_id
                })
    
    if not registered:
        # Fallback regex scanner
        tuple_matches = re.findall(r'\(\s*"case_([^"]+)"\s*,\s*"([^"]+)"\s*(?:,\s*"([^"]+)"\s*)?\)', content)
        for m in tuple_matches:
            case_id_raw = "case_" + m[0]
            if m[2]:
                registered.append(tuple_case_record(case_id_raw, m[1], m[2]))
            elif "." in case_id_raw:
                filename = case_id_raw
                registered.append({
                    "case_id": pathlib.Path(filename).stem,
                    "filename": filename,
                    "title": m[1]
                })
            else:
                registered.append({
                    "case_id": case_id_raw,
                    "filename": f"{case_id_raw}.log",
                    "title": m[1]
                })
            
    # Check for duplicates
    seen = set()
    duplicates = set()
    for r in registered:
        if r["case_id"] in seen:
            duplicates.add(r["case_id"])
        seen.add(r["case_id"])
        
    if duplicates:
        raise ValueError(f"Duplicate case_id(s) registered in showcase.rs: {', '.join(duplicates)}")
        
    return registered

def get_physical_samples(plugin):
    plugin_normalized = plugin.replace("_plugin", "")
    candidates = [
        os.path.join("samples", f"{plugin_normalized}_plugin"),
        os.path.join("samples", plugin_normalized)
    ]

    samples_path = None
    for path in candidates:
        if os.path.exists(path):
            samples_path = path
            break

    if not samples_path:
        return {}

    # fix: case 后缀有 6 种（.log/.json/.hex/.md/.xml/.txt）— 用显式白名单 glob，
    # 不能用 "case_*.*"（会误吞 .bak/.tar.gz 等备份/元数据文件）。
    import glob
    ALLOWED_CASE_EXTS = ("log", "json", "hex", "md", "xml", "txt")
    matched: Dict[str, str] = {}
    for ext in ALLOWED_CASE_EXTS:
        for f in glob.glob(os.path.join(samples_path, f"case_*.{ext}")):
            matched[os.path.basename(f)] = f
    return matched

def get_first_non_empty_line(text):
    for line in text.splitlines():
        trimmed = line.strip()
        if trimmed:
            return trimmed
    return ""

def get_command_anchor_token(line):
    if not line:
        return ""
    trimmed = line.strip()
    had_prompt = False
    
    match1 = re.match(r'^([$|>])\s+(.+)$', trimmed)
    match2 = re.match(r'^PS\s+[^>]+>\s+(.+)$', trimmed)
    if match1:
        trimmed = match1.group(2).strip()
        had_prompt = True
    elif match2:
        trimmed = match2.group(1).strip()
        had_prompt = True
        
    parts = trimmed.split()
    if not parts:
        return ""
    token = parts[0]
    
    normalized = re.sub(r'^[./\\]+', '', token)
    normalized = re.sub(r'\.(exe|cmd|bat)$', '', normalized, flags=re.IGNORECASE).lower()
    
    known = r'^(git|svn|hg|p4|cvs|bzr|fossil|darcs|gh|glab|az|bb|repo|gerrit|cargo|go|mvn|maven|npm|yarn|pnpm|node|pytest|python|terraform|ansible|pulumi|aws|helm|bazel|protoc|buf|cmake|ctest|ninja|gradle|gradlew|kubectl|docker|docker-compose|act|circleci|buildkite|buildkite-agent|jenkins|dotnet|xcodebuild|gcc|g\+\+|clang|clang\+\+|make|redis-cli|redis-server|mongod|mongosh|mongo|psql|postgres)$'
    if not re.match(known, normalized):
        return ""
        
    if not had_prompt and len(parts) > 1:
        second = parts[1].strip(':').lower()
        log_words = r'^(warn|warning|error|errors|info|debug|trace|fatal|deprecated|added|found|downloaded|downloading|compiling|compiled)$'
        if re.match(log_words, second):
            return ""
            
    return token

def test_regex_gates(case):
    failures = []
    
    if case["compression_pct"] < 0:
        failures.append("G1_ROI")
        
    if case["original_text"] and not case["compact_text"]:
        failures.append("G2_NONEMPTY")
        
    signal_pattern = r'(?i)\b(error|errors|fatal|panic|exception|failed|failure|traceback)\b'
    compact_signal_pattern = r'(?i)(\b(error|errors|fatal|panic|exception|failed|failure|traceback)\b|\b[A-Za-z_]*(Error|Exception)\b|\$PY\|TB|\$PY\|EX|(^|\s)!)'
    if re.search(signal_pattern, case["original_text"]) and not re.search(compact_signal_pattern, case["compact_text"]):
        failures.append("G3_SIGNAL")
        
    anchor = get_first_non_empty_line(case["original_text"])
    anchor_token = get_command_anchor_token(anchor)
    if anchor_token:
        compact_first = get_first_non_empty_line(case["compact_text"])
        compact_token = get_command_anchor_token(compact_first)
        if anchor_token != compact_token:
            failures.append("G4_ANCHOR")
            
    return failures

def get_tactical_prompt(plugin):
    normalized = plugin.replace("_plugin", "")
    profiles = load_semantic_audit_profiles()
    selected = ["default"]
    if normalized.startswith("vcs_"):
        selected.append("vcs-common")
    for group, plugins in PROFILE_GROUPS.items():
        if normalized in plugins:
            selected.append(group)
            break

    blocks = []
    missing = []
    for name in selected:
        block = profiles.get(name)
        if block:
            blocks.append(f"## {name}\n{block}")
        else:
            missing.append(name)

    if missing:
        raise ValueError("Missing semantic audit profile(s): " + ", ".join(missing))

    return "\n\n".join(blocks)

def load_semantic_audit_profiles(path=SEMANTIC_PROFILE_PATH):
    if not os.path.exists(path):
        raise FileNotFoundError(f"Semantic audit profile file not found: {path}")
    with open(path, "r", encoding="utf-8-sig", errors="ignore") as f:
        content = f.read()
    profiles = {}
    current = None
    buf = []
    for line in content.splitlines():
        match = re.match(r"^##\s+([A-Za-z0-9_-]+)\s*$", line.strip())
        if match:
            if current:
                profiles[current] = "\n".join(buf).strip()
            current = match.group(1)
            buf = []
        elif current:
            buf.append(line)
    if current:
        profiles[current] = "\n".join(buf).strip()
    return profiles

def call_llm(env, prompt, tactical_rules=None):
    """fix #29: 转发到 audit_llm_common.call_llm_chat。

    保留旧签名 `(env, prompt, tactical_rules=None)` 以兼容 test_llm_gate。
    - env 是 dict
    - tactical_rules 不传 None 时会拼到 system prompt 头部（SEMANTIC-AUDIT-PROFILES 段）
    """
    system_prompt = build_llm_system_prompt(tactical_rules or "")
    cfg = _alc.LLMConfig.from_env(
        env={
            "OPENAI_API_KEY": env.get("OPENAI_API_KEY", ""),
            "OPENAI_BASE_URL": env.get("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            "OPENAI_MODEL": env.get("LLM_MODEL", "deepseek-v4-pro"),
            "OPENAI_MAX_TOKENS": env.get("LLM_MAX_TOKENS", "4096"),
            "OPENAI_TIMEOUT": env.get("LLM_HTTP_TIMEOUT", "900"),
            "OPENAI_RETRIES": env.get("LLM_RETRIES", "5"),
            "OPENAI_RETRY_SLEEP": env.get("LLM_RETRY_SLEEP", "5"),
            "OPENAI_JSON_MODE": "1" if env.get("LLM_JSON_MODE", "true").lower() == "true" else "0",
            "OPENAI_REASONING_EFFORT": env.get("LLM_MERGE_REASONING_EFFORT", ""),
        },
        audit_kind="case_metrics",
    )
    return _alc.call_llm_chat(cfg, prompt, system_prompt)

def test_llm_gate(env, plugin, case, tactical_rules):
    prompt = f"""Task: audit one TokenSlim compression case.

Case metadata:
- plugin: {plugin}
- case_id: {case["case_id"]}
- original_bytes: {case["original_bytes"]}
- compact_bytes: {case["compact_bytes"]}
- compression_pct: {case["compression_pct"]}

Compare Original and Compact. Return JSON only.

[Original Log]
{case["original_text"]}

[Compressed (Compact) Log]
{case["compact_text"]}
"""
    try:
        res = call_llm(env, prompt, tactical_rules)
        return res.get("pass", False), res.get("failures", []), res.get("explanation", "")
    except Exception as e:
        print(f"Error executing LLM audit for case {case['case_id']}: {e}")
        return False, [f"LLM_AUDIT_ERROR: {str(e)}"], "API call failed"

def parse_report(path):
    if not os.path.exists(path):
        raise FileNotFoundError(f"Report not found: {path}")
        
    with open(path, "r", encoding="utf-8-sig", errors="replace") as f:
        content = f.read()
        
    lines = content.splitlines()
    cases = []
    
    i = 0
    while i < len(lines):
        line = lines[i]
        match = re.match(r'^Case\s+(case_[A-Za-z0-9_]+)\s+-\s+.+\s+\(([^)]+)\)', line)
        if not match:
            i += 1
            continue
            
        header_case_id = match.group(1)
        case_file = match.group(2)
        case_id = pathlib.Path(case_file).stem if pathlib.Path(case_file).stem.startswith("case_") else header_case_id
        
        metric_line = None
        j = i + 1
        while j < len(lines):
            metric_match = re.match(
                r'^Original:\s+(\d+)\s+lines,\s+(\d+)\s+bytes\s*\|\s*Compact:\s+(\d+)\s+lines,\s+(\d+)\s+bytes\s*\|\s*Compression:\s+(-?\d+(?:\.\d+)?)%',
                lines[j]
            )
            if metric_match:
                metric_line = lines[j]
                break
            if re.match(r'^Case\s+case_[A-Za-z0-9_]+\s+-', lines[j]):
                break
            j += 1
            
        if not metric_line:
            i += 1
            continue
            
        metric_match = re.match(
            r'^Original:\s+(\d+)\s+lines,\s+(\d+)\s+bytes\s*\|\s*Compact:\s+(\d+)\s+lines,\s+(\d+)\s+bytes\s*\|\s*Compression:\s+(-?\d+(?:\.\d+)?)%',
            metric_line
        )
        orig_lines = int(metric_match.group(1))
        orig_bytes = int(metric_match.group(2))
        compact_lines = int(metric_match.group(3))
        compact_bytes = int(metric_match.group(4))
        ratio = float(metric_match.group(5))
        
        k = j
        while k < len(lines) and '-- Case text --' not in lines[k]:
            k += 1
            
        if k >= len(lines):
            i = j + 1
            continue
            
        k += 1
        if k < len(lines) and re.match(r'^-{20,}$', lines[k]):
            k += 1
            
        orig_start = k
        while k < len(lines) and '-- Compact Output (full) --' not in lines[k]:
            k += 1
            
        if k >= len(lines):
            i = j + 1
            continue
            
        orig_end = k - 1
        
        k += 1
        if k < len(lines) and re.match(r'^-{20,}$', lines[k]):
            k += 1
            
        compact_start = k
        while k < len(lines):
            if re.match(r'^Case\s+case_[A-Za-z0-9_]+\s+-', lines[k]):
                break
            if re.match(r'^-{20,}$', lines[k]) and (k + 1) < len(lines) and re.match(r'^Case\s+case_[A-Za-z0-9_]+\s+-', lines[k+1]):
                break
            k += 1
            
        compact_end = k - 1
        
        while orig_end >= orig_start and re.match(r'^-{20,}$', lines[orig_end]):
            orig_end -= 1
        while compact_end >= compact_start and re.match(r'^-{20,}$', lines[compact_end]):
            compact_end -= 1
            
        orig_text = "\n".join(lines[orig_start:orig_end+1]) if orig_end >= orig_start else ""
        compact_slice = lines[compact_start:compact_end+1] if compact_end >= compact_start else []
        compact_slice = [
            line for line in compact_slice
            if not (line.startswith("[SKIP] ") and "file not found or empty:" in line)
        ]
        compact_text = "\n".join(compact_slice)
        
        cases.append({
            "case_id": case_id,
            "case_file": case_file,
            "original_lines": orig_lines,
            "original_bytes": orig_bytes,
            "compact_lines": compact_lines,
            "compact_bytes": compact_bytes,
            "compression_pct": ratio,
            "original_text": orig_text,
            "compact_text": compact_text,
            "compact_hash": sha256_hex(compact_text)
        })
        i = k
        
    def get_case_num(c):
        id_str = c["case_id"].replace("case_", "")
        match = re.match(r'^(\d+)', id_str)
        if match:
            return int(match.group(1))
        return 999999
        
    cases.sort(key=get_case_num)
    return cases

def check_three_way_alignment(plugin_name, rows, emit_warnings=True):
    showcase_missing_list = []
    unregistered_physical = []
    registered_missing_sample = []
    registered_missing_report = []
    stale_report_case = []
    duplicate_report_case = []

    try:
        registered_cases = parse_showcase_rs_cases(plugin_name)
    except ValueError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(7)

    physical_samples = get_physical_samples(plugin_name)
    report_ids = [r["case_id"] for r in rows]
    report_id_set = set(report_ids)
    seen_report = set()
    duplicate_report_case = []
    for case_id in report_ids:
        if case_id in seen_report and case_id not in duplicate_report_case:
            duplicate_report_case.append(case_id)
        seen_report.add(case_id)

    if registered_cases is not None:
        if len(registered_cases) == 0:
            print("ERROR: showcase.rs exists but failed to parse any cases. Please check the parser regex.", file=sys.stderr)
            sys.exit(6)

        registered_ids = {r["case_id"]: r for r in registered_cases}
        registered_files = {r["filename"]: r for r in registered_cases}

        for pf in physical_samples:
            if pf not in registered_files:
                case_id_guess = pathlib.Path(pf).stem
                unregistered_physical.append(case_id_guess)
                showcase_missing_list.append(case_id_guess)
                if emit_warnings:
                    print(f"WARN: Case file {pf} exists in samples/ but is not registered in showcase.rs.")

        for r in registered_cases:
            if r["filename"] not in physical_samples:
                registered_missing_sample.append(r["case_id"])
                showcase_missing_list.append(r["case_id"])
                if emit_warnings:
                    print(f"WARN: Case {r['case_id']} is registered in showcase.rs but the log file {r['filename']} is missing from samples/.")

        for r in registered_cases:
            if r["case_id"] not in report_id_set:
                registered_missing_report.append(r["case_id"])
                showcase_missing_list.append(r["case_id"])
                if emit_warnings:
                    print(f"WARN: Registered case {r['case_id']} is missing from the generated report.")

        for r in rows:
            if r["case_id"] not in registered_ids:
                stale_report_case.append(r["case_id"])
                showcase_missing_list.append(r["case_id"])
                if emit_warnings:
                    print(f"WARN: Report contains case {r['case_id']} which is NOT registered in showcase.rs.")
    else:
        audited_set = {c["case_id"] for c in rows}
        for f_name in physical_samples:
            case_id = pathlib.Path(f_name).stem if "." in f_name else f_name
            if case_id not in audited_set:
                unregistered_physical.append(case_id)
                showcase_missing_list.append(case_id)

    for case_id in duplicate_report_case:
        showcase_missing_list.append(case_id)
        if emit_warnings:
            print(f"WARN: Report contains duplicate case {case_id}; regenerate the showcase report from the current showcase.rs.")

    return {
        "showcase_missing_list": sorted(set(showcase_missing_list)),
        "unregistered_physical": sorted(set(unregistered_physical)),
        "registered_missing_sample": sorted(set(registered_missing_sample)),
        "registered_missing_report": sorted(set(registered_missing_report)),
        "stale_report_case": sorted(set(stale_report_case)),
        "duplicate_report_case": sorted(set(duplicate_report_case)),
    }

def add_missing_empty_sample_rows(plugin_name, rows):
    registered_cases = parse_showcase_rs_cases(plugin_name)
    if registered_cases is None:
        return rows

    physical_samples = get_physical_samples(plugin_name)
    existing_ids = {r["case_id"] for r in rows}
    added = []
    for registered in registered_cases:
        if registered["case_id"] in existing_ids:
            continue
        sample_path = physical_samples.get(registered["filename"])
        if not sample_path or os.path.getsize(sample_path) != 0:
            continue
        added.append({
            "case_id": registered["case_id"],
            "case_file": registered["filename"],
            "original_lines": 0,
            "original_bytes": 0,
            "compact_lines": 0,
            "compact_bytes": 0,
            "compression_pct": 0.0,
            "original_text": "",
            "compact_text": "",
            "compact_hash": sha256_hex("")
        })

    if not added:
        return rows

    print(f"empty_sample_rows_added={len(added)}")
    for row in added:
        print(f"empty_sample_case={row['case_id']}")
    return sorted(rows + added, key=lambda c: c["case_id"])

def export_case(c, base_dir):
    case_dir = os.path.join(base_dir, c["case_id"])
    ensure_dir(case_dir)
    with open(os.path.join(case_dir, "original.txt"), "w", encoding="utf-8") as f:
        f.write(c["original_text"])
    with open(os.path.join(case_dir, "compact.txt"), "w", encoding="utf-8") as f:
        f.write(c["compact_text"])
        
    summary = {
        "case_id": c["case_id"],
        "case_file": c["case_file"],
        "original_lines": c["original_lines"],
        "original_bytes": c["original_bytes"],
        "compact_lines": c["compact_lines"],
        "compact_bytes": c["compact_bytes"],
        "compression_pct": c["compression_pct"],
        "compact_hash": c["compact_hash"],
        "semantic_gate_pass": c.get("semantic_gate_pass", True),
        "semantic_gate_failures": c.get("semantic_gate_failures", "")
    }
    with open(os.path.join(case_dir, "summary.json"), "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=4)

def save_state(state_file, track, state_map):
    state_list = sorted(list(state_map.values()), key=lambda x: x["case_id"])
    state_out = {
        "track": track,
        "updated_at": datetime.now().isoformat(),
        "cases": state_list
    }
    atomic_write_json(state_file, state_out)

def save_frozen(freeze_file, track, freeze_map):
    frozen_list = sorted(list(freeze_map.values()), key=lambda x: x["case_id"])
    # 向后兼容: 旧 frozen 条目无 frozen_by 标记, 统一补 "hash"
    for f in frozen_list:
        if not f.get("frozen_by"):
            f["frozen_by"] = "hash"
    frozen_out = {
        "track": track,
        "updated_at": datetime.now().isoformat(),
        "cases": frozen_list
    }
    atomic_write_json(freeze_file, frozen_out)

def remap_case_map_to_current(case_map, rows):
    """Migrate legacy short case IDs (case_15) to current full stems (case_15_p4_opened)."""
    current_ids = {c["case_id"] for c in rows}
    prefix_to_current = {}
    for case_id in current_ids:
        m = re.match(r"^(case_\d+)(?:_|$)", case_id)
        if not m:
            continue
        prefix_to_current.setdefault(m.group(1), []).append(case_id)

    alias = {
        prefix: ids[0]
        for prefix, ids in prefix_to_current.items()
        if len(ids) == 1
    }

    remapped = {}
    for case_id, record in case_map.items():
        target_id = case_id if case_id in current_ids else alias.get(case_id)
        if not target_id:
            continue

        migrated = dict(record)
        migrated["case_id"] = target_id
        # Prefer an exact current-id record over a migrated legacy alias.
        if target_id not in remapped or case_id == target_id:
            remapped[target_id] = migrated

    return remapped

def main():
    parser = argparse.ArgumentParser(description="Audit case metrics for TokenSlim plugin.")
    parser.add_argument("--plugin", default="")
    parser.add_argument("--report-path", default="")
    parser.add_argument("--out-dir", default="")
    parser.add_argument("--version", default="")
    parser.add_argument("--track", default="")
    parser.add_argument("--fail-on-regression", action="store_true")
    parser.add_argument("--fail-on-frozen-change", action="store_true")
    parser.add_argument("--export-cases", action="store_true")
    parser.add_argument("--case-id", default="")
    parser.add_argument("--case-out-dir", default="")
    parser.add_argument("--freeze-file", default="")
    parser.add_argument("--freeze-case", default="")
    parser.add_argument("--freeze-unchanged", action="store_true")
    parser.add_argument("--list-frozen", action="store_true")
    parser.add_argument("--require-semantic-gate", action="store_true")
    parser.add_argument("--state-file", default="")
    parser.add_argument("--llm-force-audit", action="store_true")
    
    # Capitalized aliases to support PowerShell calls
    parser.add_argument("--Plugin", dest="plugin")
    parser.add_argument("--ReportPath", dest="report_path")
    parser.add_argument("--OutDir", dest="out_dir")
    parser.add_argument("--Version", dest="version")
    parser.add_argument("--Track", dest="track")
    parser.add_argument("--FailOnRegression", dest="fail_on_regression", action="store_true")
    parser.add_argument("--FailOnFrozenChange", dest="fail_on_frozen_change", action="store_true")
    parser.add_argument("--ExportCases", dest="export_cases", action="store_true")
    parser.add_argument("--CaseId", dest="case_id")
    parser.add_argument("--CaseOutDir", dest="case_out_dir")
    parser.add_argument("--FreezeFile", dest="freeze_file")
    parser.add_argument("--FreezeCase", dest="freeze_case")
    parser.add_argument("--FreezeUnchanged", dest="freeze_unchanged", action="store_true")
    parser.add_argument("--ListFrozen", dest="list_frozen", action="store_true")
    parser.add_argument("--RequireSemanticGate", dest="require_semantic_gate", action="store_true")
    parser.add_argument("--StateFile", dest="state_file")
    parser.add_argument("--LlmForceAudit", dest="llm_force_audit", action="store_true")

    args = parser.parse_args()
    
    plugin = args.plugin or ""
    report_path = args.report_path or ""
    out_dir = args.out_dir or ""
    track = args.track or ""
    
    if plugin:
        # Validate name
        if not re.match(r'^[a-z0-9_]+$', plugin):
            raise ValueError(f"Invalid plugin name: '{plugin}'. Must be lowercase, digits, underscores.")
        if not report_path:
            report_path = f"target/{plugin}_compact_showcase_report.txt"
            # 兼容 showcase test 输出命名: 有些插件去掉 _plugin 后缀
            if not os.path.exists(report_path):
                alt_path = f"target/{plugin.removesuffix('_plugin')}_compact_showcase_report.txt"
                if os.path.exists(alt_path):
                    report_path = alt_path
        if not out_dir:
            out_dir = f"docs/audit/{plugin}"
        if not track:
            track = plugin
        print(f"Using plugin: {plugin}")
        print(f"ReportPath: {report_path}")
        print(f"OutDir: {out_dir}")
        print(f"Track: {track}")
    else:
        report_path = report_path or "target/vcs_git_compact_showcase_report.txt"
        out_dir = out_dir or "docs/audit/vcs_git"
        track = track or "vcs_git"
        
    version = args.version or ""
    if not version:
        version = datetime.now().strftime("%Y%m%d-%H%M%S")
        
    ensure_dir(out_dir)
    case_out_dir = args.case_out_dir or os.path.join(out_dir, "cases")
    freeze_file = args.freeze_file or os.path.join(out_dir, "frozen_cases.json")
    state_file = args.state_file or os.path.join(out_dir, "audit_state.json")
    
    # Parse report
    rows = parse_report(report_path)
    if not rows:
        raise ValueError(f"No cases parsed from report: {report_path}")
    rows = add_missing_empty_sample_rows(plugin or track, rows)

    alignment = check_three_way_alignment(plugin or track, rows)
    if alignment["showcase_missing_list"]:
        print(f"track={track}")
        print(f"version={version}")
        print(f"cases={len(rows)}")
        print(f"showcase_missing={len(alignment['showcase_missing_list'])}")
        for m in alignment["showcase_missing_list"]:
            print(f"showcase_missing_case={m}")
        print(f"ERROR: Showcase missing failed: {len(alignment['showcase_missing_list'])} case(s) are orphaned/unaligned.", file=sys.stderr)
        sys.exit(5)

    # Read environment
    env = parse_env()
    has_llm = bool(env.get("OPENAI_API_KEY"))
    if args.require_semantic_gate and not has_llm:
        print("ERROR: --require-semantic-gate was specified but OPENAI_API_KEY is not configured.", file=sys.stderr)
        sys.exit(1)
    
    # Load state & frozen cases
    state = {"track": track, "updated_at": datetime.now().isoformat(), "cases": []}
    if os.path.exists(state_file):
        with open(state_file, "r", encoding="utf-8-sig") as f:
            state = json.load(f)
            
    state_map = {c["case_id"]: c for c in state.get("cases", [])}
    
    frozen = {"track": track, "updated_at": datetime.now().isoformat(), "cases": []}
    if os.path.exists(freeze_file):
        with open(freeze_file, "r", encoding="utf-8-sig") as f:
            frozen = json.load(f)

    if isinstance(frozen, list):
        frozen_cases = frozen
    else:
        frozen_cases = frozen.get("cases", frozen.get("frozen_cases", []))
    freeze_map = {c["case_id"]: c for c in frozen_cases}
    state_map = remap_case_map_to_current(state_map, rows)
    freeze_map = remap_case_map_to_current(freeze_map, rows)
    
    latest_path = os.path.join(out_dir, f"{track}.latest.json")
    prev = None
    if os.path.exists(latest_path):
        with open(latest_path, "r", encoding="utf-8-sig") as f:
            prev = json.load(f)
            
    # Load tactical rules for LLM
    tactical_rules = get_tactical_prompt(track) if args.require_semantic_gate and has_llm else ""
    
    # Three-way alignment and semantic gate validation
    semantic_gate_failed = []
    for c in rows:
        original_hash = sha256_hex(c["original_text"])
        is_frozen = False
        if c["case_id"] in freeze_map:
            stored = freeze_map[c["case_id"]]
            if stored.get("compact_hash") == c["compact_hash"] and stored.get("original_hash") == original_hash:
                is_frozen = True

        # Regex semantic gates
        failures = test_regex_gates(c)
        is_empty_case = (
            c["original_bytes"] == 0
            and c["compact_bytes"] == 0
            and not c["original_text"]
            and not c["compact_text"]
        )

        # LLM semantic gate (only if credentials exist, require-semantic-gate is active, and case is not frozen/changed)
        llm_audited = False
        if args.require_semantic_gate and has_llm and len(failures) == 0 and not is_empty_case:
            state_status = state_map.get(c["case_id"], {}).get("status", "")

            # Audit if not frozen, or if hash changed, or if state is todo/auditing, or if forced
            if not is_frozen or state_status in ("todo", "auditing") or args.llm_force_audit:
                print(f"Running LLM semantic audit for case: {c['case_id']}...")
                llm_pass, llm_fails, explanation = test_llm_gate(env, track, c, tactical_rules)
                llm_audited = True
                if not llm_pass:
                    failures.append("G5_LLM_SEMANTIC")
                    print(f"  [FAIL] LLM audit failed for {c['case_id']}: {', '.join(llm_fails)}")
                    print(f"  Explanation: {explanation}")
                else:
                    print(f"  [PASS] LLM audit passed for {c['case_id']}")
                    
        c["semantic_gate_pass"] = (len(failures) == 0)
        c["semantic_gate_failures"] = ",".join(failures)
        c["llm_audited"] = llm_audited
        
        # Incremental state & frozen cases saving
        if c["semantic_gate_pass"]:
            should_freeze = (args.freeze_case == c["case_id"])
            if args.require_semantic_gate and not is_frozen:
                should_freeze = True

            if args.freeze_unchanged and prev:
                prev_map = {pc["case_id"]: float(pc["compression_pct"]) for pc in prev.get("cases", [])}
                if c["case_id"] in prev_map and float(c["compression_pct"]) == prev_map[c["case_id"]]:
                    should_freeze = True
            
            if should_freeze:
                frozen_by = freeze_map.get(c["case_id"], {}).get("frozen_by", "")
                if not frozen_by:
                    frozen_by = "llm" if c.get("llm_audited") else "hash"
                freeze_map[c["case_id"]] = {
                    "case_id": c["case_id"],
                    "version": version,
                    "compression_pct": c["compression_pct"],
                    "compact_hash": c["compact_hash"],
                    "original_hash": original_hash,
                    "original_bytes": c["original_bytes"],
                    "compact_bytes": c["compact_bytes"],
                    "frozen_by": frozen_by,
                    "frozen_at": datetime.now().isoformat()
                }
                state_map[c["case_id"]] = {
                    "case_id": c["case_id"],
                    "status": "frozen",
                    "last_version": version,
                    "compression_pct": c["compression_pct"],
                    "compact_hash": c["compact_hash"],
                    "note": "",
                    "updated_at": datetime.now().isoformat()
                }
                save_frozen(freeze_file, track, freeze_map)
                save_state(state_file, track, state_map)
                is_frozen = True
            else:
                current_status = "frozen" if is_frozen else state_map.get(c["case_id"], {}).get("status", "todo")
                state_map[c["case_id"]] = {
                    "case_id": c["case_id"],
                    "status": current_status,
                    "last_version": version,
                    "compression_pct": c["compression_pct"],
                    "compact_hash": c["compact_hash"],
                    "note": "",
                    "updated_at": datetime.now().isoformat()
                }
                save_state(state_file, track, state_map)
        else:
            state_map[c["case_id"]] = {
                "case_id": c["case_id"],
                "status": "auditing",
                "last_version": version,
                "compression_pct": c["compression_pct"],
                "compact_hash": c["compact_hash"],
                "note": f"Semantic gate failed: {c['semantic_gate_failures']}",
                "updated_at": datetime.now().isoformat()
            }
            save_state(state_file, track, state_map)
            
            semantic_gate_failed.append({
                "case_id": c["case_id"],
                "failures": c["semantic_gate_failures"]
            })
            
    # Snapshots
    metrics = []
    for r in rows:
        metrics.append({
            "case_id": r["case_id"],
            "original_lines": r["original_lines"],
            "original_bytes": r["original_bytes"],
            "compact_lines": r["compact_lines"],
            "compact_bytes": r["compact_bytes"],
            "compression_pct": r["compression_pct"],
            "semantic_gate_pass": r["semantic_gate_pass"],
            "semantic_gate_failures": r["semantic_gate_failures"]
        })
        
    snapshot = {
        "track": track,
        "version": version,
        "generated_at": datetime.now().isoformat(),
        "report_path": report_path,
        "case_count": len(metrics),
        "cases": metrics
    }
    
    json_path = os.path.join(out_dir, f"{track}.{version}.json")
    csv_path = os.path.join(out_dir, f"{track}.{version}.csv")
    diff_md_path = os.path.join(out_dir, f"{track}.{version}.diff.md")
    
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(snapshot, f, indent=4)
        
    with open(csv_path, "w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=metrics[0].keys())
        writer.writeheader()
        writer.writerows(metrics)
            
    improved = []
    regressed = []
    unchanged = []
    new_cases = []
    missing_cases = []
    
    if prev:
        prev_map = {c["case_id"]: float(c["compression_pct"]) for c in prev.get("cases", [])}
        curr_map = {c["case_id"]: float(c["compression_pct"]) for c in metrics}
        
        for k, curr_val in curr_map.items():
            if k not in prev_map:
                new_cases.append({"case_id": k, "curr": curr_val})
                continue
            prev_val = prev_map[k]
            delta = round(curr_val - prev_val, 1)
            if delta > 0:
                improved.append({"case_id": k, "prev": prev_val, "curr": curr_val, "delta": delta})
            elif delta < 0:
                regressed.append({"case_id": k, "prev": prev_val, "curr": curr_val, "delta": delta})
            else:
                unchanged.append({"case_id": k, "curr": curr_val})
                
        for k, prev_val in prev_map.items():
            if k not in curr_map:
                missing_cases.append({"case_id": k, "prev": prev_val})
                
        # Write diff md
        md = [
            f"# Audit Diff: {track} {version}\n",
            f"- prev: {prev.get('version', 'unknown')}",
            f"- curr: {version}",
            f"- improved: {len(improved)}",
            f"- regressed: {len(regressed)}",
            f"- unchanged: {len(unchanged)}",
            f"- new: {len(new_cases)}",
            f"- missing: {len(missing_cases)}\n",
            "## Regressed Cases\n"
        ]
        if not regressed:
            md.append("None\n")
        else:
            md.append("| case | prev | curr | delta |")
            md.append("| --- | ---: | ---: | ---: |")
            for r in sorted(regressed, key=lambda x: x["case_id"]):
                md.append(f"| {r['case_id']} | {r['prev']:.1f}% | {r['curr']:.1f}% | {r['delta']:.1f}% |")
            md.append("")
            
        md.append("## Improved Cases\n")
        if not improved:
            md.append("None\n")
        else:
            md.append("| case | prev | curr | delta |")
            md.append("| --- | ---: | ---: | ---: |")
            for r in sorted(improved, key=lambda x: x["case_id"]):
                md.append(f"| {r['case_id']} | {r['prev']:.1f}% | {r['curr']:.1f}% | +{r['delta']:.1f}% |")
            md.append("")
            
        with open(diff_md_path, "w", encoding="utf-8") as f:
            f.write("\n".join(md))
            
    # Export cases if requested
    if args.export_cases:
        for c in rows:
            export_case(c, case_out_dir)
            
    if args.case_id:
        target = next((c for c in rows if c["case_id"] == args.case_id), None)
        if not target:
            raise ValueError(f"Case not found: {args.case_id}")
        export_case(target, case_out_dir)
        print(f"case_exported={args.case_id}")
        print(f"case_dir={os.path.join(case_out_dir, args.case_id)}")
        
    # Freezing case logic
    if args.freeze_case:
        target = next((c for c in rows if c["case_id"] == args.freeze_case), None)
        if not target:
            raise ValueError(f"Case not found for freeze: {args.freeze_case}")
        if args.require_semantic_gate and not target["semantic_gate_pass"]:
            raise ValueError(f"Case failed semantic gate and cannot be frozen: {args.freeze_case} ({target['semantic_gate_failures']})")
        freeze_map[args.freeze_case] = {
            "case_id": target["case_id"],
            "version": version,
            "compression_pct": target["compression_pct"],
            "compact_hash": target["compact_hash"],
            "original_hash": sha256_hex(target["original_text"]),
            "original_bytes": target["original_bytes"],
            "compact_bytes": target["compact_bytes"],
            "frozen_by": "llm" if target.get("llm_audited") else "hash",
            "frozen_at": datetime.now().isoformat()
        }
        
    if args.freeze_unchanged and prev:
        for u in unchanged:
            target = next((c for c in rows if c["case_id"] == u["case_id"]), None)
            if target:
                freeze_map[u["case_id"]] = {
                    "case_id": target["case_id"],
                    "version": version,
                    "compression_pct": target["compression_pct"],
                    "compact_hash": target["compact_hash"],
                    "original_hash": sha256_hex(target["original_text"]),
                    "original_bytes": target["original_bytes"],
                    "compact_bytes": target["compact_bytes"],
                    "frozen_by": "llm" if target.get("llm_audited") else "hash",
                    "frozen_at": datetime.now().isoformat()
                }
                
    frozen_list = sorted(list(freeze_map.values()), key=lambda x: x["case_id"])
    # 向后兼容: 旧 frozen 条目无 frozen_by 标记, 统一补 "hash"
    for f in frozen_list:
        if not f.get("frozen_by"):
            f["frozen_by"] = "hash"
    frozen_out = {
        "track": track,
        "updated_at": datetime.now().isoformat(),
        "cases": frozen_list
    }
    atomic_write_json(freeze_file, frozen_out)
        
    # Check frozen changed and missing
    frozen_changed = []
    frozen_missing = []
    curr_case_map = {c["case_id"]: c for c in rows}
    
    for f in frozen_list:
        if f["case_id"] in curr_case_map:
            if curr_case_map[f["case_id"]]["compact_hash"] != f["compact_hash"] or sha256_hex(curr_case_map[f["case_id"]]["original_text"]) != f.get("original_hash"):
                frozen_changed.append({
                    "case_id": f["case_id"],
                    "frozen_hash": f["compact_hash"],
                    "current_hash": curr_case_map[f["case_id"]]["compact_hash"]
                })
        else:
            frozen_missing.append({
                "case_id": f["case_id"],
                "frozen_hash": f["compact_hash"]
            })
            
    # Sync audit state
    for c in rows:
        if c["case_id"] not in state_map:
            state_map[c["case_id"]] = {
                "case_id": c["case_id"],
                "status": "todo",
                "last_version": version,
                "compression_pct": c["compression_pct"],
                "compact_hash": c["compact_hash"],
                "note": "",
                "updated_at": datetime.now().isoformat()
            }
        else:
            s = state_map[c["case_id"]]
            s["last_version"] = version
            s["compression_pct"] = c["compression_pct"]
            s["compact_hash"] = c["compact_hash"]
            s["updated_at"] = datetime.now().isoformat()
            
    for f in frozen_list:
        if f["case_id"] in state_map:
            state_map[f["case_id"]]["status"] = "frozen"
            state_map[f["case_id"]]["last_version"] = version
            state_map[f["case_id"]]["updated_at"] = datetime.now().isoformat()
            
    for x in frozen_changed:
        if x["case_id"] in state_map:
            state_map[x["case_id"]]["status"] = "auditing"
            state_map[x["case_id"]]["note"] = "Frozen case changed; re-audit required."
            state_map[x["case_id"]]["updated_at"] = datetime.now().isoformat()
            
    for x in frozen_missing:
        if x["case_id"] in state_map:
            state_map[x["case_id"]]["status"] = "auditing"
            state_map[x["case_id"]]["note"] = "Frozen case missing from current report; re-audit required."
            state_map[x["case_id"]]["updated_at"] = datetime.now().isoformat()
            
    state_list = sorted(list(state_map.values()), key=lambda x: x["case_id"])
    state_out = {
        "track": track,
        "updated_at": datetime.now().isoformat(),
        "cases": state_list
    }
    atomic_write_json(state_file, state_out)
        
    state_todo = len([s for s in state_list if s["status"] == "todo"])
    state_auditing = len([s for s in state_list if s["status"] == "auditing"])
    state_frozen = len([s for s in state_list if s["status"] == "frozen"])
    state_waived = len([s for s in state_list if s["status"] == "waived"])
    
    if args.list_frozen:
        print(f"frozen_total={len(frozen_list)}")
        for f in frozen_list:
            print(f"frozen_case={f['case_id']} ratio={f['compression_pct']}% version={f['version']}")
            
    # Three-way alignment was already validated before LLM work to avoid
    # spending tokens or updating latest snapshots for stale reports.
    showcase_missing_list = alignment["showcase_missing_list"]
    unregistered_physical = alignment["unregistered_physical"]
    registered_missing_sample = alignment["registered_missing_sample"]
    registered_missing_report = alignment["registered_missing_report"]
    stale_report_case = alignment["stale_report_case"]
                
    # Copy json to latest
    import shutil
    shutil.copyfile(json_path, latest_path)
    
    showcase_missing_list = sorted(list(set(showcase_missing_list)))
    
    # Print metrics outputs
    print(f"track={track}")
    print(f"version={version}")
    print(f"cases={len(metrics)}")
    print(f"snapshot_json={json_path}")
    print(f"snapshot_csv={csv_path}")
    if prev:
        print(f"diff_md={diff_md_path}")
        print(f"improved={len(improved)}")
        print(f"regressed={len(regressed)}")
        print(f"unchanged={len(unchanged)}")
    print(f"new={len(new_cases)}")
    print(f"missing={len(missing_cases)}")
    print(f"freeze_file={freeze_file}")
    print(f"frozen_total={len(frozen_list)}")
    print(f"frozen_changed={len(frozen_changed)}")
    print(f"frozen_missing={len(frozen_missing)}")
    print(f"semantic_gate_failed={len(semantic_gate_failed)}")
    print(f"semantic_gate_passed={len(metrics) - len(semantic_gate_failed)}")
    for g in sorted(semantic_gate_failed, key=lambda x: x["case_id"]):
        print(f"semantic_gate_failed_case={g['case_id']} failures={g['failures']}")
    print(f"showcase_missing={len(showcase_missing_list)}")
    for m in showcase_missing_list:
        print(f"showcase_missing_case={m}")
    print(f"state_file={state_file}")
    print(f"state_todo={state_todo}")
    print(f"state_auditing={state_auditing}")
    print(f"state_frozen={state_frozen}")
    print(f"state_waived={state_waived}")
    
    if frozen_changed:
        print("WARN: frozen case content changed; please re-audit these case(s).")
        for x in frozen_changed:
            print(f"frozen_changed_case={x['case_id']} frozen_hash={x['frozen_hash']} current_hash={x['current_hash']}")
    if frozen_missing:
        print("WARN: frozen case missing from current report; please re-audit these case(s).")
        for x in frozen_missing:
            print(f"frozen_missing_case={x['case_id']}")
            
    if showcase_missing_list:
        print("WARN: showcase_missing detected; the following cases exist in samples/ but are not audited in the report.")
        for m in showcase_missing_list:
            print(f"orphaned_case={m}")
            
    # Exit codes
    if args.fail_on_regression and (len(regressed) > 0 or len(missing_cases) > 0):
        print(f"ERROR: Regression detected: regressed={len(regressed)}, missing={len(missing_cases)}.", file=sys.stderr)
        sys.exit(2)
    if args.fail_on_frozen_change and (len(frozen_changed) > 0 or len(frozen_missing) > 0):
        print(f"ERROR: Frozen case changed or missing: changed={len(frozen_changed)}, missing={len(frozen_missing)}.", file=sys.stderr)
        sys.exit(3)
    if args.require_semantic_gate and len(semantic_gate_failed) > 0:
        print(f"ERROR: Semantic gate failed: {len(semantic_gate_failed)} case(s).", file=sys.stderr)
        sys.exit(4)
    if len(showcase_missing_list) > 0:
        print(f"ERROR: Showcase missing failed: {len(showcase_missing_list)} case(s) are orphaned/unaligned.", file=sys.stderr)
        sys.exit(5)

if __name__ == "__main__":
    main()
