#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
import sys
import re
import json
import argparse
from datetime import datetime

# Import shared parser
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from audit_case_metrics import parse_showcase_rs_cases

def ensure_dir(path):
    if not os.path.exists(path):
        os.makedirs(path, exist_ok=True)

def _resolve_project_root() -> str:
    """Find the TokenSlim repo root.

    Default paths are relative to the repo root, so running this script
    from scripts/ or elsewhere must still work. Probe up to 3 levels.
    """
    try:
        cwd = os.getcwd()
    except OSError:
        return ""
    candidates = [
        cwd,
        os.path.dirname(cwd),
        os.path.dirname(os.path.dirname(cwd)),
    ]
    for c in candidates:
        if not c:
            continue
        if (
            os.path.isdir(os.path.join(c, "config"))
            and os.path.isdir(os.path.join(c, "src"))
            and os.path.isdir(os.path.join(c, "samples"))
        ):
            return c
    return cwd

def read_json_file(path):
    if not os.path.exists(path):
        return None
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None

def count_case_files(plugin_name, samples_dir):
    dirs = [
        os.path.join(samples_dir, f"{plugin_name}_plugin"),
        os.path.join(samples_dir, plugin_name)
    ]
    for d in dirs:
        if os.path.exists(d):
            try:
                files = [f for f in os.listdir(d) if os.path.isfile(os.path.join(d, f)) and re.match(r'^case_.*\.[^.]+$', f)]
                return len(files)
            except Exception:
                pass
    return 0

def get_coverage_status(plugin_name, samples, showcase, audit, frozen, auditing, has_config, has_source, enabled):
    if not enabled: return "disabled_config"
    if plugin_name == "vcs": return "orchestrator"
    if plugin_name == "explain": return "meta"
    if audit > 0 and frozen >= audit and auditing == 0: return "frozen"
    if audit > 0: return "auditing"
    if samples > 0 or showcase > 0: return "missing_audit"
    if has_config and not has_source: return "config_only"
    if has_source: return "source_only"
    return "unknown"

def count_showcase_cases(plugin_name, source_dir):
    cases = parse_showcase_rs_cases(plugin_name, source_dir=source_dir)
    if cases is None:
        return 0
    if len(cases) == 0:
        raise ValueError(f"showcase.rs exists for {plugin_name} but failed to parse any cases. Please check the parser regex.")
    return len(cases)

def get_showcase_text(plugin_name, source_dir):
    showcase = os.path.join(source_dir, f"{plugin_name}_plugin", "showcase.rs")
    if not os.path.exists(showcase):
        return ""
    try:
        with open(showcase, "r", encoding="utf-8", errors="ignore") as f:
            return f.read()
    except Exception:
        return ""

def get_audit_summary(plugin_name, audit_dir):
    d = os.path.join(audit_dir, plugin_name)
    latest = os.path.join(d, f"{plugin_name}.latest.json")
    state = os.path.join(d, "audit_state.json")
    frozen = os.path.join(d, "frozen_cases.json")

    latest_obj = read_json_file(latest)
    state_obj = read_json_file(state)
    frozen_obj = read_json_file(frozen)

    cases = 0
    if latest_obj and "cases" in latest_obj:
        cases = len(latest_obj["cases"])

    frozen_count = 0
    if frozen_obj:
        if isinstance(frozen_obj, list):
            frozen_count = len(frozen_obj)
        elif "cases" in frozen_obj:
            frozen_count = len(frozen_obj["cases"])
        elif "frozen_cases" in frozen_obj:
            frozen_count = len(frozen_obj["frozen_cases"])

    auditing_count = 0
    if state_obj and "cases" in state_obj:
        for c in state_obj["cases"]:
            status = c.get("status", c.get("state", ""))
            if status == "auditing":
                auditing_count += 1

    return {
        "latest_json": latest,
        "audit_state": state,
        "frozen_cases": frozen,
        "audit_cases": cases,
        "frozen_cases_count": frozen_count,
        "auditing_cases_count": auditing_count
    }

def get_capability_tags(config):
    tags = set()
    text = ""
    plugin_name = config.get("name", "")
    if plugin_name == "ci_log": return ["ci_cd"]
    if plugin_name == "cloud_log": return ["cloud_log"]
    if plugin_name == "web_log": return ["web_log"]
    if plugin_name.startswith("vcs_") or plugin_name == "git_diff": return ["vcs"]

    if "description" in config: text += " " + str(config["description"])
    if "name" in config: text += " " + str(config["name"])
    lower = text.lower()

    tag_rules = {
        "vcs": ["svn", "mercurial", "perforce", "version control", "bitbucket", "gerrit"],
        "ci_cd": ["ci/cd", "pipeline", "github actions", "jenkins", "travis", "teamcity", "buildkite", "circleci"],
        "cloud_log": ["aws", "gcp", "azure", "cloud", "oci", "cloudflare", "aliyun", "tencent", "huawei"],
        "web_log": ["nginx", "apache", "http", "access log", "web"],
        "build_log": ["build", "compile", "maven", "gradle", "gcc", "cmake", "ninja", "bazel"],
        "test_log": ["pytest", "junit", "test", "coverage"],
        "data_format": ["json", "yaml", "xml", "html", "ndjson", "csv"],
        "database": ["database", "sql", "postgres", "mongodb", "redis", "mysql"],
        "infra": ["terraform", "ansible", "helm", "cloudformation", "pulumi", "kubernetes", "docker"],
        "stack_trace": ["traceback", "stack", "exception", "java", "python", "node", "rust", "go"]
    }

    for tag, needles in tag_rules.items():
        for needle in needles:
            if needle in lower:
                tags.add(tag)
                break

    if not tags: tags.add("general")
    return sorted(list(tags))

def read_route_configs(config_dir):
    routes = {}
    if not os.path.exists(config_dir): return routes
    for f in os.listdir(config_dir):
        if f.endswith(".route.json"):
            path = os.path.join(config_dir, f)
            route = read_json_file(path)
            if route and "name" in route:
                routes[str(route["name"])] = route
    return routes

def get_route_arg_prefix_strings(route_config):
    if not route_config or "route" not in route_config: return []
    route = route_config["route"]
    if not route or "command_arg_prefixes" not in route: return []
    res = []
    for p in route["command_arg_prefixes"]:
        command = p.get("command", "")
        args = p.get("args", [])
        if not isinstance(args, list): args = []
        val = (str(command) + " " + " ".join([str(a) for a in args])).strip()
        if val: res.append(val)
    return res

def get_route_keywords(route_config):
    if not route_config or "route" not in route_config: return []
    route = route_config["route"]
    if not route or "command_keywords" not in route: return []
    kw = route["command_keywords"]
    if isinstance(kw, list): return [str(k) for k in kw]
    return []

def get_route_group(route_config):
    if not route_config or "route" not in route_config: return ""
    route = route_config["route"]
    if not route or "route_group" not in route: return ""
    return str(route["route_group"])

def get_claim_rules():
    return [
        {"claim": "aws", "needles": ["aws", "cloudwatch", "ecs", "alb", "cloudformation"]},
        {"claim": "gcp", "needles": ["gcp", "google", "cloud_run", "httprequest"]},
        {"claim": "azure", "needles": ["azure", "az "]},
        {"claim": "oci", "needles": ["oci", "oracle"]},
        {"claim": "cloudflare", "needles": ["cloudflare"]},
        {"claim": "alb", "needles": ["alb", "load balancer", "loadbalancer"]},
        {"claim": "nginx", "needles": ["nginx", "ingress"]},
        {"claim": "apache", "needles": ["apache"]},
        {"claim": "uvicorn", "needles": ["uvicorn", "fastapi"]},
        {"claim": "csv", "needles": ["csv"]},
        {"claim": "json", "needles": ["json", "jsonl", "ndjson"]},
        {"claim": "table", "needles": ["table"]},
        {"claim": "sarif", "needles": ["sarif"]},
        {"claim": "junit", "needles": ["junit"]},
        {"claim": "github_actions", "needles": ["github actions", "github_actions"]},
        {"claim": "gitlab", "needles": ["gitlab"]},
        {"claim": "jenkins", "needles": ["jenkins"]},
        {"claim": "teamcity", "needles": ["teamcity"]},
        {"claim": "travis", "needles": ["travis"]},
        {"claim": "buildkite", "needles": ["buildkite"]},
        {"claim": "circleci", "needles": ["circleci"]},
        {"claim": "postgresql", "needles": ["postgresql", "postgres", "psql"]},
        {"claim": "mongodb", "needles": ["mongodb", "mongo", "mongod", "mongosh"]},
        {"claim": "redis", "needles": ["redis"]},
        {"claim": "mysql", "needles": ["mysql"]},
        {"claim": "cmake", "needles": ["cmake"]},
        {"claim": "ninja", "needles": ["ninja"]},
        {"claim": "pytest", "needles": ["pytest"]},
        {"claim": "terraform", "needles": ["terraform"]},
        {"claim": "ansible", "needles": ["ansible"]},
        {"claim": "helm", "needles": ["helm"]},
        {"claim": "pulumi", "needles": ["pulumi"]},
        {"claim": "cloudformation", "needles": ["cloudformation"]}
    ]

def get_evidence_text(plugin_name, samples_dir, source_dir):
    parts = []
    for d in [os.path.join(samples_dir, f"{plugin_name}_plugin"), os.path.join(samples_dir, plugin_name)]:
        if os.path.exists(d):
            try:
                for f in os.listdir(d):
                    if os.path.isfile(os.path.join(d, f)):
                        parts.append(f)
            except Exception:
                pass
                
    showcase_text = get_showcase_text(plugin_name, source_dir)
    if showcase_text:
        parts.append(showcase_text)
        
    return re.sub(r'\s+', ' ', " ".join(parts)).lower()

def get_coverage_claims(config, dimensions, evidence_text):
    source = ""
    if "description" in config: source += " " + str(config["description"])
    if "name" in config: source += " " + str(config["name"])
    if dimensions: source += " " + " ".join(dimensions)
    source = source.lower()

    claims = []
    for rule in get_claim_rules():
        declared = False
        covered = False
        for needle in rule["needles"]:
            if needle in source: declared = True
            if needle in evidence_text: covered = True
            
        if declared:
            claims.append({
                "name": rule["claim"],
                "covered": covered
            })
    return claims

def main():
    parser = argparse.ArgumentParser(description="Generate plugin capability index and matrix")
    parser.add_argument("--config-dir", default="config/plugins")
    parser.add_argument("--samples-dir", default="samples")
    parser.add_argument("--source-dir", default="src/plugins")
    parser.add_argument("--audit-dir", default="docs/audit")
    parser.add_argument("--json-out", default="docs/audit/plugin_capability_index.json")
    parser.add_argument("--markdown-out", default="docs/reports/plugin_capability_matrix.md")
    args = parser.parse_args()

    # Probe project root so default paths resolve whether we're run from
    # TokenSlim/ or scripts/. Only apply when the default is being used.
    project_root = _resolve_project_root()
    if project_root and os.path.abspath(os.getcwd()) != os.path.abspath(project_root):
        # user did not pass an absolute path and cwd is not the root
        for attr in ("config_dir", "samples_dir", "source_dir", "audit_dir", "json_out", "markdown_out"):
            v = getattr(args, attr, "")
            if v and not os.path.isabs(v) and not os.path.exists(v):
                setattr(args, attr, os.path.join(project_root, v.replace("/", os.sep)))

    ensure_dir(os.path.dirname(args.json_out))
    ensure_dir(os.path.dirname(args.markdown_out))

    route_by_name = read_route_configs(args.config_dir)

    config_by_name = {}
    if os.path.exists(args.config_dir):
        for f in os.listdir(args.config_dir):
            if f.endswith(".json") and not f.endswith(".route.json"):
                path = os.path.join(args.config_dir, f)
                config = read_json_file(path)
                if config and "name" in config:
                    config_by_name[str(config["name"])] = {
                        "config": config,
                        "path": os.path.relpath(path, os.getcwd())
                    }

    name_set = set(config_by_name.keys())
    
    if os.path.exists(args.source_dir):
        for d in os.listdir(args.source_dir):
            if os.path.isdir(os.path.join(args.source_dir, d)) and d.endswith("_plugin"):
                name_set.add(d[:-7])
                
    if os.path.exists(args.samples_dir):
        for d in os.listdir(args.samples_dir):
            if os.path.isdir(os.path.join(args.samples_dir, d)) and d.endswith("_plugin"):
                name_set.add(d[:-7])
                
    if os.path.exists(args.audit_dir):
        for d in os.listdir(args.audit_dir):
            if os.path.isdir(os.path.join(args.audit_dir, d)):
                name_set.add(d.replace("_plugin", ""))

    plugins = []
    for name in sorted(list(name_set)):
        config = config_by_name[name]["config"] if name in config_by_name else {"name": name}
        config_path = config_by_name[name]["path"] if name in config_by_name else ""
        
        audit = get_audit_summary(name, args.audit_dir)
        
        detect_rules = []
        if "detect" in config and config["detect"] and "rules" in config["detect"]:
            for r in config["detect"]["rules"]:
                if "pattern" in r and r["pattern"]:
                    detect_rules.append(str(r["pattern"]))
                    
        dimensions = []
        if "compress" in config and config["compress"] and "semantic_aggregate" in config["compress"]:
            agg = config["compress"]["semantic_aggregate"]
            if agg and "dimensions" in agg:
                dimensions = [str(d) for d in agg["dimensions"]]

        source_path = os.path.join(args.source_dir, f"{name}_plugin")
        sample_cases = count_case_files(name, args.samples_dir)
        showcase_cases = count_showcase_cases(name, args.source_dir)
        
        route_config = route_by_name.get(name)
        route_keywords = get_route_keywords(route_config)
        route_arg_prefixes = get_route_arg_prefix_strings(route_config)
        route_group = get_route_group(route_config)
        
        evidence_text = get_evidence_text(name, args.samples_dir, args.source_dir)
        coverage_claims = get_coverage_claims(config, dimensions, evidence_text)
        coverage_warnings = [f"declared_without_case_evidence:{c['name']}" for c in coverage_claims if not c["covered"]]
        
        has_config = bool(config_path)
        has_source = os.path.exists(source_path)
        enabled = bool(config.get("enabled", True))
        
        coverage_status = get_coverage_status(
            name, sample_cases, showcase_cases, 
            audit["audit_cases"], audit["frozen_cases_count"], audit["auditing_cases_count"], 
            has_config, has_source, enabled
        )
        
        plugins.append({
            "name": name,
            "config_path": config_path,
            "source_path": source_path,
            "has_config": has_config,
            "has_source": has_source,
            "enabled": enabled,
            "priority": int(config.get("priority", 0)),
            "description": str(config.get("description", "")),
            "capability_tags": get_capability_tags(config),
            "route_group": route_group,
            "route_keywords": route_keywords,
            "route_arg_prefixes": route_arg_prefixes,
            "detect_patterns": detect_rules,
            "semantic_dimensions": dimensions,
            "coverage_claims": coverage_claims,
            "coverage_warnings": coverage_warnings,
            "sample_cases": sample_cases,
            "showcase_cases": showcase_cases,
            "audit_cases": audit["audit_cases"],
            "frozen_cases": audit["frozen_cases_count"],
            "auditing_cases": audit["auditing_cases_count"],
            "coverage_status": coverage_status,
            "latest_json": audit["latest_json"]
        })

    audited_plugin_count = len([p for p in plugins if p["audit_cases"] > 0])
    frozen_plugin_count = len([p for p in plugins if p["audit_cases"] > 0 and p["frozen_cases"] >= p["audit_cases"] and p["auditing_cases"] == 0])
    coverage_gap_count = len([p for p in plugins if p["coverage_status"] in ["missing_audit", "config_only", "source_only"]])

    index = {
        "generated_at": datetime.now().isoformat(),
        "plugin_count": len(plugins),
        "audited_plugin_count": audited_plugin_count,
        "frozen_plugin_count": frozen_plugin_count,
        "coverage_gap_count": coverage_gap_count,
        "source": {
            "config_dir": args.config_dir,
            "samples_dir": args.samples_dir,
            "source_dir": args.source_dir,
            "audit_dir": args.audit_dir
        },
        "plugins": sorted(plugins, key=lambda x: x["name"])
    }

    with open(args.json_out, "w", encoding="utf-8") as f:
        json.dump(index, f, indent=4)

    md = [
        "# Plugin Capability Matrix\n",
        f"- generated_at: {index['generated_at']}",
        f"- plugins_total: {index['plugin_count']}",
        f"- audited_plugins: {index['audited_plugin_count']}",
        f"- frozen_plugins: {index['frozen_plugin_count']}",
        f"- coverage_gaps: {index['coverage_gap_count']}",
        "- authoritative sources: config/plugins/*.json, samples/*, src/plugins/*/showcase.rs, docs/audit/*\n",
        "| plugin | status | tags | route | samples | showcase | audit | frozen | auditing | warnings | description |",
        "| ------ | ------ | ---- | ----- | ------: | -------: | ----: | -----: | -------: | -------- | ----------- |"
    ]
    
    for p in sorted(plugins, key=lambda x: x["name"]):
        tags = ",".join(p["capability_tags"])
        desc = p["description"].replace("|", "/")
        route = p["route_group"] if p["route_group"] else "-"
        warnings = "-" if not p["coverage_warnings"] else "<br>".join(p["coverage_warnings"][:3])
        warnings = warnings.replace("|", "/")
        md.append(f"| {p['name']} | {p['coverage_status']} | {tags} | {route} | {p['sample_cases']} | {p['showcase_cases']} | {p['audit_cases']} | {p['frozen_cases']} | {p['auditing_cases']} | {warnings} | {desc} |")
        
    with open(args.markdown_out, "w", encoding="utf-8") as f:
        f.write("\n".join(md))

    print(f"plugin_capability_index={args.json_out}")
    print(f"plugin_capability_matrix={args.markdown_out}")
    print(f"plugins_total={len(plugins)}")
    print(f"audited_plugins={audited_plugin_count}")
    print(f"frozen_plugins={frozen_plugin_count}")
    print(f"coverage_gaps={coverage_gap_count}")

if __name__ == "__main__":
    main()
