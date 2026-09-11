#!/usr/bin/env python3
"""
Project insight extractor for TokenSlim.

Goals:
1) Extract capability/feature points from config-driven evidence rules.
2) Build a project-level call chain map (Rust-focused).
3) Emit stable JSON artifacts for audit/review without changing core audit scripts.

Output files (default: docs/audit):
- project_capability_graph.json
- callchain_map.json
- coverage_matrix.json
- project_insight_summary.md
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Set, Tuple

try:
    import tomllib  # py3.11+
except ModuleNotFoundError as exc:  # pragma: no cover
    raise SystemExit(f"tomllib is required: {exc}")


RUST_FN_RE = re.compile(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
RUST_CALL_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*(?:!|\()")
RUST_METHOD_CALL_RE = re.compile(r"\.\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(")
IDENT_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")

RUST_KEYWORD_OR_BUILTIN = {
    "if",
    "else",
    "match",
    "for",
    "while",
    "loop",
    "return",
    "break",
    "continue",
    "let",
    "fn",
    "mod",
    "impl",
    "pub",
    "use",
    "crate",
    "super",
    "self",
    "Self",
    "Some",
    "None",
    "Ok",
    "Err",
    "String",
    "Vec",
    "format",
    "println",
    "eprintln",
    "dbg",
    "include_str",
    "include_bytes",
}

_TS_INITED = False
_TS_PARSER = None
_TS_PARSER_NAME = None

HOTSPOT_NOISE_NAMES = {
    "new",
    "default",
    "clone",
    "copy",
    "into",
    "from",
    "as_ref",
    "as_mut",
    "as_str",
    "len",
    "is_empty",
    "push",
    "pop",
    "next",
    "contains",
    "starts_with",
    "ends_with",
    "insert",
    "remove",
    "clear",
}

DEFAULT_GOAL_WEIGHTS = {
    "decision_quality": 1.0,
    "compression_roi": 1.0,
    "routing_accuracy": 1.0,
    "audit_regression": 1.0,
    "cross_platform_usability": 1.0,
}

FEATURE_FUNCTION_SAMPLE_LIMIT = 40
FEATURE_FUNCTION_MAX_TRACK = 1200
FEATURE_REVIEW_HOT_OUT = 25
FEATURE_REVIEW_HOT_IN = 20
FEATURE_REVIEW_UNRESOLVED_CALLS = 12
HOTSPOT_EXCLUDE_FILE_GLOBS = [
    "**/test.rs",
    "**/showcase.rs",
]


@dataclass
class FunctionNode:
    function_id: str
    name: str
    file: str
    start_line: int
    end_line: int
    calls: Set[str]
    parser: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Extract TokenSlim capability/callchain insight artifacts")
    parser.add_argument("--config", default="config/project_insight.toml", help="Config path")
    parser.add_argument("--root", default=".", help="Workspace root")
    parser.add_argument("--out-dir", default="docs/audit", help="Output directory")
    parser.add_argument(
        "--opt-plan-out",
        default="docs/tasks/PROJECT_INSIGHT_OPT_PLAN.md",
        help="Optimization plan markdown output path",
    )
    parser.add_argument(
        "--function-task-out",
        default="docs/tasks/PROJECT_INSIGHT_FUNCTION_TASKS.md",
        help="Function-level optimization task markdown output path",
    )
    parser.add_argument("--opt-plan-topn", type=int, default=3, help="Top N prioritized features for optimization plan")
    parser.add_argument("--max-depth", type=int, default=None, help="Override call chain traversal depth")
    parser.add_argument("--strict", action="store_true", help="Fail when config references missing evidence")
    return parser.parse_args()


def now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def load_config(path: Path) -> dict:
    if not path.exists():
        raise FileNotFoundError(f"config not found: {path}")
    with path.open("rb") as f:
        return tomllib.load(f)


def ensure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def relpath(path: Path, root: Path) -> str:
    try:
        return str(path.resolve().relative_to(root.resolve())).replace("\\", "/")
    except Exception:
        return str(path).replace("\\", "/")


def expand_globs(root: Path, includes: Sequence[str], excludes: Sequence[str]) -> List[Path]:
    found: Set[Path] = set()
    for pattern in includes:
        for p in root.glob(pattern):
            if p.is_file():
                found.add(p)
    selected: List[Path] = []
    for p in sorted(found):
        r = relpath(p, root)
        if any(Path(r).match(ex) for ex in excludes):
            continue
        selected.append(p)
    return selected


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="ignore")


def parse_goal_weights(cfg: dict) -> Dict[str, float]:
    raw = cfg.get("goals", {}).get("weights", {}) or {}
    out: Dict[str, float] = {}
    for k, default_v in DEFAULT_GOAL_WEIGHTS.items():
        try:
            out[k] = float(raw.get(k, default_v))
        except Exception:
            out[k] = float(default_v)
    return out


def _find_matching_brace(text: str, open_idx: int) -> int:
    depth = 0
    i = open_idx
    in_string = False
    string_quote = ""
    escaped = False
    while i < len(text):
        ch = text[i]
        if in_string:
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == string_quote:
                in_string = False
        else:
            if ch in ("'", '"'):
                in_string = True
                string_quote = ch
            elif ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return -1


def _extract_calls_from_body(body: str) -> Set[str]:
    out: Set[str] = set()
    for m in RUST_CALL_RE.finditer(body):
        ident = m.group(1)
        if ident in RUST_KEYWORD_OR_BUILTIN:
            continue
        out.add(ident)
    for m in RUST_METHOD_CALL_RE.finditer(body):
        ident = m.group(1)
        if ident in RUST_KEYWORD_OR_BUILTIN:
            continue
        out.add(ident)
    return out


def _extract_functions_regex(path: Path, content: str, root: Path) -> List[FunctionNode]:
    nodes: List[FunctionNode] = []
    rp = relpath(path, root)
    for m in RUST_FN_RE.finditer(content):
        name = m.group(1)
        open_idx = content.find("{", m.end())
        if open_idx < 0:
            continue
        close_idx = _find_matching_brace(content, open_idx)
        if close_idx < 0:
            continue
        body = content[open_idx : close_idx + 1]
        calls = _extract_calls_from_body(body)
        start_line = content.count("\n", 0, m.start()) + 1
        end_line = content.count("\n", 0, close_idx) + 1
        fn_id = f"{rp}:{name}:{start_line}"
        nodes.append(
            FunctionNode(
                function_id=fn_id,
                name=name,
                file=rp,
                start_line=start_line,
                end_line=end_line,
                calls=calls,
                parser="regex",
            )
        )
    return nodes


def _find_node_text(source: bytes, node) -> str:
    return source[node.start_byte : node.end_byte].decode("utf-8", errors="ignore")


def _last_ident(text: str) -> Optional[str]:
    matches = IDENT_RE.findall(text)
    if not matches:
        return None
    return matches[-1]


def _walk_tree_sitter_calls(node, source: bytes, out: Set[str]) -> None:
    ntype = node.type
    if ntype == "call_expression":
        target = node.child_by_field_name("function")
        if target is not None:
            ident = _last_ident(_find_node_text(source, target))
            if ident and ident not in RUST_KEYWORD_OR_BUILTIN:
                out.add(ident)
    elif ntype == "method_call_expression":
        m = node.child_by_field_name("method") or node.child_by_field_name("name")
        if m is not None:
            ident = _last_ident(_find_node_text(source, m))
            if ident and ident not in RUST_KEYWORD_OR_BUILTIN:
                out.add(ident)
    elif ntype == "macro_invocation":
        macro = node.child_by_field_name("macro")
        if macro is not None:
            ident = _last_ident(_find_node_text(source, macro))
            if ident and ident not in RUST_KEYWORD_OR_BUILTIN:
                out.add(ident)
    for child in node.children:
        _walk_tree_sitter_calls(child, source, out)


def _load_tree_sitter_rust_parser():
    global _TS_INITED, _TS_PARSER, _TS_PARSER_NAME
    if _TS_INITED:
        return _TS_PARSER, _TS_PARSER_NAME

    # path 1: tree_sitter_languages (single package with bundled grammars)
    try:
        from tree_sitter_languages import get_parser

        _TS_PARSER = get_parser("rust")
        _TS_PARSER_NAME = "tree_sitter_languages"
        _TS_INITED = True
        return _TS_PARSER, _TS_PARSER_NAME
    except Exception:
        pass

    # path 2: tree_sitter + tree_sitter_rust
    try:
        from tree_sitter import Language, Parser
        import tree_sitter_rust

        parser = Parser()
        lang = None
        if hasattr(tree_sitter_rust, "language"):
            lang = tree_sitter_rust.language()
        elif hasattr(tree_sitter_rust, "LANGUAGE"):
            lang = tree_sitter_rust.LANGUAGE
        if lang is None:
            _TS_INITED = True
            _TS_PARSER = None
            _TS_PARSER_NAME = None
            return None, None
        # For newer python bindings, grammar packages may expose a PyCapsule.
        # Normalize to tree_sitter.Language first.
        if not isinstance(lang, Language):
            lang = Language(lang)
        if hasattr(parser, "set_language"):
            parser.set_language(lang)
        else:
            parser.language = lang
        _TS_PARSER = parser
        _TS_PARSER_NAME = "tree_sitter_rust"
        _TS_INITED = True
        return _TS_PARSER, _TS_PARSER_NAME
    except Exception:
        _TS_INITED = True
        _TS_PARSER = None
        _TS_PARSER_NAME = None
        return None, None


def _extract_functions_tree_sitter(path: Path, content: str, root: Path) -> List[FunctionNode]:
    parser, parser_name = _load_tree_sitter_rust_parser()
    if parser is None:
        return []

    src = content.encode("utf-8", errors="ignore")
    try:
        tree = parser.parse(src)
    except Exception:
        return []

    nodes: List[FunctionNode] = []
    rp = relpath(path, root)

    def _is_in_test_cfg(node) -> bool:
        """判断函数节点是否位于 `#[cfg(test)]` 或 `mod tests` 测试模块内，避免测试函数计入功能点"""
        cur = node.parent
        while cur is not None:
            if cur.type == "mod_item":
                attr = cur.child_by_field_name("attributes")
                if attr is not None and "cfg(test)" in _find_node_text(src, attr):
                    return True
                name_node = cur.child_by_field_name("name")
                if name_node is not None and _find_node_text(src, name_node).strip() == "tests":
                    return True
            cur = cur.parent
        return False

    def visit(node) -> None:
        if node.type == "function_item":
            name_node = node.child_by_field_name("name")
            body_node = node.child_by_field_name("body")
            if name_node is not None and body_node is not None:
                if _is_in_test_cfg(node):
                    return
                name = _find_node_text(src, name_node).strip()
                calls: Set[str] = set()
                _walk_tree_sitter_calls(body_node, src, calls)
                start_line = node.start_point[0] + 1
                end_line = node.end_point[0] + 1
                fn_id = f"{rp}:{name}:{start_line}"
                nodes.append(
                    FunctionNode(
                        function_id=fn_id,
                        name=name,
                        file=rp,
                        start_line=start_line,
                        end_line=end_line,
                        calls=calls,
                        parser=parser_name,
                    )
                )
        for child in node.children:
            visit(child)

    visit(tree.root_node)
    return nodes


def extract_rust_functions(path: Path, content: str, root: Path) -> List[FunctionNode]:
    ts_nodes = _extract_functions_tree_sitter(path, content, root)
    if ts_nodes:
        return ts_nodes
    return _extract_functions_regex(path, content, root)


def _module_prefix(file_path: str) -> str:
    parts = file_path.split("/")
    if len(parts) <= 1:
        return file_path
    return "/".join(parts[:-1])


def _pick_call_targets(caller: FunctionNode, candidates: Sequence[FunctionNode]) -> Tuple[List[FunctionNode], bool]:
    if not candidates:
        return [], False
    if len(candidates) == 1:
        return [candidates[0]], False

    same_file = [c for c in candidates if c.file == caller.file]
    if same_file:
        return [sorted(same_file, key=lambda x: x.start_line)[0]], True

    caller_prefix = _module_prefix(caller.file)
    same_prefix = [c for c in candidates if _module_prefix(c.file) == caller_prefix]
    if same_prefix:
        return [sorted(same_prefix, key=lambda x: x.start_line)[0]], True

    ranked = sorted(candidates, key=lambda x: (x.file, x.start_line))
    return [ranked[0]], True


def build_call_graph(
    functions: Sequence[FunctionNode],
) -> Tuple[List[dict], List[dict], Dict[str, List[str]], int]:
    by_name: Dict[str, List[FunctionNode]] = {}
    for fn in functions:
        by_name.setdefault(fn.name, []).append(fn)

    nodes = []
    edges = []
    unresolved: Dict[str, Set[str]] = {}
    for fn in functions:
        nodes.append(
            {
                "id": fn.function_id,
                "name": fn.name,
                "file": fn.file,
                "start_line": fn.start_line,
                "end_line": fn.end_line,
                "parser": fn.parser,
            }
        )

    ambiguous_counter = 0
    for fn in functions:
        for callee_name in sorted(fn.calls):
            candidates = by_name.get(callee_name, [])
            if not candidates:
                unresolved.setdefault(fn.function_id, set()).add(callee_name)
                continue
            targets, ambiguous = _pick_call_targets(fn, candidates)
            if ambiguous:
                ambiguous_counter += 1
            for callee in targets:
                edges.append(
                    {
                        "from": fn.function_id,
                        "to": callee.function_id,
                        "callee_name": callee_name,
                        "ambiguous": ambiguous,
                    }
                )

    unresolved_json = {k: sorted(v) for k, v in unresolved.items()}
    return nodes, edges, unresolved_json, ambiguous_counter


def resolve_entry_ids(entry_names: Sequence[str], nodes: Sequence[dict]) -> List[str]:
    out = []
    for entry in entry_names:
        matched = [n["id"] for n in nodes if n["name"] == entry]
        out.extend(matched)
    return sorted(set(out))


def bfs_chains(entry_ids: Sequence[str], edge_map: Dict[str, List[str]], max_depth: int) -> List[dict]:
    chains: List[dict] = []
    for eid in entry_ids:
        visited = {eid}
        frontier = [(eid, [eid], 0)]
        while frontier:
            node, path, depth = frontier.pop(0)
            children = edge_map.get(node, [])
            if depth >= max_depth or not children:
                chains.append({"entry": eid, "path": path, "depth": depth})
                continue
            for child in children:
                if child in visited:
                    continue
                visited.add(child)
                frontier.append((child, path + [child], depth + 1))
    return chains


def build_file_function_index(nodes: Sequence[dict]) -> Dict[str, List[dict]]:
    by_file: Dict[str, List[dict]] = {}
    for node in nodes:
        by_file.setdefault(str(node.get("file", "")), []).append(node)
    for file_nodes in by_file.values():
        file_nodes.sort(key=lambda n: int(n.get("start_line", 0)))
    return by_file


def find_enclosing_function_id(file_index: Dict[str, List[dict]], file: str, line: int) -> Optional[str]:
    candidates: List[dict] = []
    for node in file_index.get(file, []):
        start = int(node.get("start_line", 0))
        end = int(node.get("end_line", 0))
        if start <= line <= end:
            candidates.append(node)
    if not candidates:
        return None
    # Prefer the narrowest function span (most specific nesting hit).
    candidates.sort(key=lambda n: (int(n.get("end_line", 0)) - int(n.get("start_line", 0)), int(n.get("start_line", 0))))
    return str(candidates[0].get("id"))


def _feature_rust_path_patterns(feature: dict) -> List[str]:
    out: List[str] = []
    for raw in feature.get("paths", []) or []:
        p = str(raw).replace("\\", "/")
        if p.endswith(".rs") or ".rs" in p:
            out.append(p)
    return out


def _path_matches(path: str, pattern: str) -> bool:
    path_n = path.replace("\\", "/")
    pat_n = pattern.replace("\\", "/")
    return fnmatch.fnmatch(path_n, pat_n)


def _is_excluded_hotspot_file(path: str) -> bool:
    path_n = path.replace("\\", "/")
    return any(fnmatch.fnmatch(path_n, pat) for pat in HOTSPOT_EXCLUDE_FILE_GLOBS)


def _is_rust_pattern(pattern: str) -> bool:
    p = str(pattern).replace("\\", "/")
    return p.endswith(".rs") or ".rs" in p


def collect_feature_involved_function_ids(
    feature: dict,
    entry_ids: Sequence[str],
    chains: Sequence[dict],
    evidences: List[dict],
    nodes: Sequence[dict],
    node_map: Dict[str, dict],
    file_index: Dict[str, List[dict]],
) -> Set[str]:
    involved: Set[str] = set(entry_ids)
    rust_patterns = _feature_rust_path_patterns(feature)

    for chain in chains:
        for fid in chain.get("path", []):
            if len(involved) >= FEATURE_FUNCTION_MAX_TRACK:
                break
            fid_str = str(fid)
            if fid_str not in node_map:
                continue
            if rust_patterns:
                file = str(node_map[fid_str].get("file", ""))
                if not any(_path_matches(file, pat) for pat in rust_patterns):
                    continue
            involved.add(fid_str)
        if len(involved) >= FEATURE_FUNCTION_MAX_TRACK:
            break

    for ev in evidences:
        ev_file = str(ev.get("file", ""))
        if not ev_file.endswith(".rs"):
            continue
        try:
            line = int(ev.get("line", 0))
        except Exception:
            line = 0
        if line <= 0:
            continue
        fid = find_enclosing_function_id(file_index, ev_file, line)
        if not fid:
            continue
        involved.add(fid)
        ev["function_id"] = fid

    # Fallback: when feature declares rust paths but no entry/evidence/callchain function is resolved,
    # include functions from matching rust files so the review still has an actionable function set.
    if not involved:
        if rust_patterns:
            for node in nodes:
                nf = str(node.get("file", ""))
                if any(_path_matches(nf, pat) for pat in rust_patterns):
                    involved.add(str(node.get("id", "")))
                    if len(involved) >= FEATURE_FUNCTION_SAMPLE_LIMIT:
                        break

    return involved


def collect_feature_evidence(
    root: Path, files: Sequence[Path], feature: dict, strict: bool
) -> Tuple[List[dict], List[str]]:
    paths = feature.get("paths", [])
    patterns = feature.get("evidence_patterns", [])
    if not paths:
        target_files = list(files)
    else:
        target_files = []
        for p in paths:
            target_files.extend(root.glob(p))
        target_files = [p for p in target_files if p.is_file()]

    evidences: List[dict] = []
    errors: List[str] = []
    for tf in sorted(set(target_files)):
        txt = read_text(tf)
        rp = relpath(tf, root)
        for pattern in patterns:
            try:
                for m in re.finditer(pattern, txt, flags=re.MULTILINE):
                    line = txt.count("\n", 0, m.start()) + 1
                    evidences.append(
                        {
                            "file": rp,
                            "line": line,
                            "pattern": pattern,
                            "excerpt": txt[m.start() : min(m.start() + 120, len(txt))].splitlines()[0],
                        }
                    )
            except re.error as exc:
                errors.append(f"{feature.get('id', '?')}: invalid regex `{pattern}`: {exc}")
    if strict and not evidences:
        errors.append(f"{feature.get('id', '?')}: no evidence matched")
    return evidences, errors


def build_feature_graph(
    cfg: dict,
    root: Path,
    all_files: Sequence[Path],
    nodes: Sequence[dict],
    edges: Sequence[dict],
    goal_weights: Dict[str, float],
    max_depth_override: Optional[int],
    strict: bool,
) -> Tuple[dict, List[str]]:
    feature_items = cfg.get("features", [])
    call_cfg = cfg.get("callchain", {})
    max_depth = max_depth_override if max_depth_override is not None else int(call_cfg.get("max_depth", 4))
    edge_map: Dict[str, List[str]] = {}
    for e in edges:
        edge_map.setdefault(e["from"], []).append(e["to"])
    node_map: Dict[str, dict] = {str(n["id"]): n for n in nodes}
    file_index = build_file_function_index(nodes)

    feature_graph = []
    errors: List[str] = []
    for feature in feature_items:
        entry_names = feature.get("entry_functions", [])
        entry_ids = resolve_entry_ids(entry_names, nodes)
        chains = bfs_chains(entry_ids, edge_map, max_depth) if entry_ids else []
        evidences, fe = collect_feature_evidence(root, all_files, feature, strict=strict)
        errors.extend(fe)
        involved_ids = collect_feature_involved_function_ids(
            feature=feature,
            entry_ids=entry_ids,
            chains=chains,
            evidences=evidences,
            nodes=nodes,
            node_map=node_map,
            file_index=file_index,
        )
        involved_functions = [
            {
                "id": fid,
                "name": node_map[fid]["name"],
                "file": node_map[fid]["file"],
                "start_line": node_map[fid]["start_line"],
                "end_line": node_map[fid]["end_line"],
            }
            for fid in sorted(
                [fid for fid in involved_ids if fid in node_map],
                key=lambda x: (node_map[x]["file"], int(node_map[x]["start_line"])),
            )
        ]
        for ev in evidences:
            fid = ev.get("function_id")
            if isinstance(fid, str) and fid in node_map:
                ev["function"] = node_map[fid]["name"]

        goal_impact = {
            k: float(v)
            for k, v in (feature.get("goal_impact", {}) or {}).items()
            if k in goal_weights
        }
        weighted_goal_score = 0.0
        for k, w in goal_weights.items():
            weighted_goal_score += float(goal_impact.get(k, 0.0)) * float(w)
        complexity_hint = float(min(len(chains), 200)) / 40.0
        feature_graph.append(
            {
                "id": feature.get("id"),
                "title": feature.get("title", ""),
                "category": feature.get("category", ""),
                "entry_functions": list(entry_names),
                "entry_optional": bool(feature.get("entry_optional", False)),
                "entry_ids": entry_ids,
                "callchain_paths": chains[:200],
                "callchain_path_count": len(chains),
                "evidence_count": len(evidences),
                "evidences": evidences[:200],
                "involved_function_count": len(involved_functions),
                "involved_functions": involved_functions[:400],
                "audit_artifacts": feature.get("audit_artifacts", []),
                "owner": feature.get("owner", ""),
                "priority": feature.get("priority", ""),
                "goal_impact": goal_impact,
                "goal_weighted_score": round(weighted_goal_score, 3),
                "complexity_hint": round(complexity_hint, 3),
            }
        )
    return {"features": feature_graph, "max_depth": max_depth}, errors


def build_feature_optimization_review(
    feature: dict,
    out_degree_stable: Dict[str, int],
    in_degree_stable: Dict[str, int],
    unresolved_calls: Dict[str, List[str]],
) -> dict:
    involved = [f.get("id", "") for f in feature.get("involved_functions", []) if f.get("id")]
    involved = [x for x in involved if isinstance(x, str)]
    max_out = max((out_degree_stable.get(fid, 0) for fid in involved), default=0)
    max_in = max((in_degree_stable.get(fid, 0) for fid in involved), default=0)
    unresolved_total = sum(len(unresolved_calls.get(fid, [])) for fid in involved)

    reasons: List[str] = []
    suggestions: List[str] = []
    entry_optional = bool(feature.get("entry_optional", False))

    if not involved and not entry_optional:
        reasons.append("无法定位该功能点涉及的 Rust 函数集合（entry/callchain/evidence 未命中）")
        suggestions.append("补充 entry_functions 或更精确的 evidence_patterns，使功能点可追溯到函数。")

    if max_out >= FEATURE_REVIEW_HOT_OUT:
        reasons.append(f"存在高扇出函数（stable_out={max_out}）")
        suggestions.append("优先拆分高扇出函数：路由判定、IO 执行、渲染输出分层。")

    if max_in >= FEATURE_REVIEW_HOT_IN:
        reasons.append(f"存在高扇入函数（stable_in={max_in}）")
        suggestions.append("为高扇入函数补充契约测试和回归样本，降低跨模块回归风险。")

    if unresolved_total >= FEATURE_REVIEW_UNRESOLVED_CALLS:
        reasons.append(f"涉及函数 unresolved 调用较多（{unresolved_total}）")
        suggestions.append("补充调用解析规则或白名单映射，减少调用链盲区。")

    needs_optimization = bool(reasons)
    if needs_optimization:
        if max_out >= 35 or unresolved_total >= 30:
            priority = "high"
        elif max_out >= FEATURE_REVIEW_HOT_OUT or unresolved_total >= FEATURE_REVIEW_UNRESOLVED_CALLS:
            priority = "medium"
        else:
            priority = "low"
    else:
        priority = "low"
        suggestions.append("当前该功能点函数图谱较稳定，可优先投入更高 ROI 的功能点。")

    return {
        "needs_optimization": needs_optimization,
        "priority": priority,
        "reasons": reasons,
        "suggestions": suggestions[:4],
        "signals": {
            "involved_function_count": len(involved),
            "max_stable_out_degree": max_out,
            "max_stable_in_degree": max_in,
            "unresolved_call_count": unresolved_total,
        },
    }


def enrich_feature_reviews(feature_graph: dict, edges: Sequence[dict], unresolved_calls: Dict[str, List[str]]) -> None:
    out_degree_stable: Dict[str, int] = {}
    in_degree_stable: Dict[str, int] = {}
    for e in edges:
        if bool(e.get("ambiguous", False)):
            continue
        frm = str(e.get("from", ""))
        to = str(e.get("to", ""))
        out_degree_stable[frm] = out_degree_stable.get(frm, 0) + 1
        in_degree_stable[to] = in_degree_stable.get(to, 0) + 1

    for feature in feature_graph.get("features", []):
        feature["optimization_review"] = build_feature_optimization_review(
            feature=feature,
            out_degree_stable=out_degree_stable,
            in_degree_stable=in_degree_stable,
            unresolved_calls=unresolved_calls,
        )


def build_coverage_matrix(root: Path, feature_graph: dict) -> dict:
    rows = []
    for f in feature_graph["features"]:
        artifacts = f.get("audit_artifacts", [])
        artifact_status = []
        existing = 0
        for pattern in artifacts:
            hits = [relpath(p, root) for p in root.glob(pattern) if p.is_file()]
            if hits:
                existing += 1
            artifact_status.append({"pattern": pattern, "exists": bool(hits), "hits": hits[:20]})
        entry_optional = bool(f.get("entry_optional", False))
        entry_resolved = bool(f.get("entry_ids")) or entry_optional
        coverage = {
            "feature_id": f["id"],
            "title": f.get("title", ""),
            "category": f.get("category", ""),
            "entry_resolved": entry_resolved,
            "entry_optional": entry_optional,
            "evidence_found": f.get("evidence_count", 0) > 0,
            "artifact_coverage": existing,
            "artifact_total": len(artifacts),
            "artifact_status": artifact_status,
            "goal_weighted_score": float(f.get("goal_weighted_score", 0.0)),
            "complexity_hint": float(f.get("complexity_hint", 0.0)),
            "needs_optimization": bool((f.get("optimization_review") or {}).get("needs_optimization", False)),
            "review_priority": str((f.get("optimization_review") or {}).get("priority", "low")),
        }
        score = 0
        if coverage["entry_resolved"]:
            score += 1
        if coverage["evidence_found"]:
            score += 1
        if coverage["artifact_total"] == 0 or coverage["artifact_coverage"] > 0:
            score += 1
        coverage["coverage_score"] = score
        coverage_factor = score / 3.0
        # 优先级：目标价值 * (复杂度加权) * 质量门禁反向系数
        coverage_penalty = float(3 - score)
        priority = coverage["goal_weighted_score"] * (1.0 + 0.15 * coverage["complexity_hint"] + 0.5 * coverage_penalty)
        coverage["optimization_priority"] = round(priority, 3)
        rows.append(coverage)
    return {"generated_at": now_iso(), "rows": rows}


def write_json(path: Path, data: dict) -> None:
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")


def _node_short_label(node: dict) -> str:
    return f"{node.get('name', '?')} ({node.get('file', '?')}:{node.get('start_line', '?')})"


def _path_to_human(path_ids: Sequence[str], node_map: Dict[str, dict]) -> str:
    parts: List[str] = []
    for pid in path_ids:
        node = node_map.get(pid)
        if node is None:
            parts.append(pid)
        else:
            parts.append(f"{node.get('name')}[{node.get('file')}:{node.get('start_line')}]")
    return " -> ".join(parts)


def write_summary_md(
    path: Path,
    root: Path,
    scanned_files: Sequence[Path],
    rust_files: Sequence[Path],
    goal_weights: Dict[str, float],
    functions: Sequence[FunctionNode],
    nodes: Sequence[dict],
    edges: Sequence[dict],
    unresolved: Dict[str, List[str]],
    ambiguous_count: int,
    feature_graph: dict,
    coverage: dict,
    errors: Sequence[str],
) -> None:
    node_map = {n["id"]: n for n in nodes}
    out_degree_all: Dict[str, int] = {}
    in_degree_all: Dict[str, int] = {}
    out_degree_stable: Dict[str, int] = {}
    in_degree_stable: Dict[str, int] = {}
    for e in edges:
        out_degree_all[e["from"]] = out_degree_all.get(e["from"], 0) + 1
        in_degree_all[e["to"]] = in_degree_all.get(e["to"], 0) + 1
        if not bool(e.get("ambiguous", False)):
            out_degree_stable[e["from"]] = out_degree_stable.get(e["from"], 0) + 1
            in_degree_stable[e["to"]] = in_degree_stable.get(e["to"], 0) + 1

    parser_usage: Dict[str, int] = {}
    for fn in functions:
        parser_usage[fn.parser] = parser_usage.get(fn.parser, 0) + 1

    lines: List[str] = []
    lines.append("# Project Insight Summary")
    lines.append("")
    lines.append(f"- generated_at: {now_iso()}")
    lines.append("- build_mode: auto-generated (script)")
    lines.append("- manual_audit_status: pending (当前为结构化扫描结果，不等于人工代码审计结论)")
    lines.append(f"- root: {root.resolve()}")
    lines.append(f"- scanned_files: {len(scanned_files)}")
    lines.append(f"- rust_files: {len(rust_files)}")
    lines.append(f"- function_nodes: {len(nodes)}")
    lines.append(f"- call_edges: {len(edges)}")
    lines.append(f"- unresolved_call_sites: {len(unresolved)}")
    lines.append(f"- ambiguous_resolved_edges: {ambiguous_count}")
    lines.append(f"- parser_usage: {json.dumps(parser_usage, ensure_ascii=False)}")
    lines.append(f"- features: {len(feature_graph.get('features', []))}")

    lines.append("")
    lines.append("## 项目目标权重")
    lines.append("")
    lines.append("| goal | weight |")
    lines.append("| --- | ---: |")
    for k, v in goal_weights.items():
        lines.append(f"| {k} | {v:.2f} |")

    lines.append("")
    lines.append("## 功能点清单")
    lines.append("")
    for f in feature_graph.get("features", []):
        lines.append(
            f"- `{f.get('id')}` | {f.get('title', '')} | category={f.get('category', '')} | "
            f"owner={f.get('owner', '')} | priority={f.get('priority', '')} | "
            f"goal_weighted_score={f.get('goal_weighted_score', 0)}"
        )

    lines.append("")
    lines.append("## Feature Coverage")
    lines.append("")
    lines.append("| feature | entry | evidence | artifacts | score |")
    lines.append("| --- | --- | --- | --- | --- |")
    for row in coverage["rows"]:
        lines.append(
            f"| {row['feature_id']} | {row['entry_resolved']} | {row['evidence_found']} | "
            f"{row['artifact_coverage']}/{row['artifact_total']} | {row['coverage_score']}/3 |"
        )

    lines.append("")
    lines.append("## 目标驱动优化优先级")
    lines.append("")
    lines.append("| feature | goal_score | complexity | coverage | opt_priority |")
    lines.append("| --- | ---: | ---: | ---: | ---: |")
    by_priority = sorted(coverage["rows"], key=lambda r: r.get("optimization_priority", 0.0), reverse=True)
    for row in by_priority:
        lines.append(
            f"| {row['feature_id']} | {row.get('goal_weighted_score', 0):.2f} | "
            f"{row.get('complexity_hint', 0):.2f} | {row.get('coverage_score', 0)}/3 | "
            f"{row.get('optimization_priority', 0):.2f} |"
        )

    lines.append("")
    lines.append("## 功能点调用链（示例）")
    lines.append("")
    for f in feature_graph.get("features", []):
        lines.append(f"### {f.get('id')} - {f.get('title', '')}")
        lines.append("")
        entry_functions = f.get("entry_functions", [])
        entry_ids = f.get("entry_ids", [])
        lines.append(f"- entry_functions: {', '.join(entry_functions) if entry_functions else '(none)'}")
        if entry_ids:
            labels = []
            for eid in entry_ids[:8]:
                n = node_map.get(eid)
                labels.append(_node_short_label(n) if n else eid)
            lines.append(f"- resolved_entries: {', '.join(labels)}")
        else:
            lines.append("- resolved_entries: (none)")
        lines.append(f"- callchain_path_count: {f.get('callchain_path_count', 0)}")

        sample_paths = f.get("callchain_paths", [])[:5]
        if sample_paths:
            lines.append("- sample_call_paths:")
            for p in sample_paths:
                human = _path_to_human(p.get("path", []), node_map)
                lines.append(f"  - {human}")
        else:
            lines.append("- sample_call_paths: (none)")

        ev = f.get("evidences", [])[:3]
        if ev:
            lines.append("- evidence_samples:")
            for item in ev:
                excerpt = str(item.get("excerpt", "")).replace("|", "\\|")
                fn_tail = ""
                if item.get("function"):
                    fn_tail = f" function=`{item.get('function')}`"
                lines.append(
                    f"  - {item.get('file')}:{item.get('line')} pattern=`{item.get('pattern')}` excerpt=`{excerpt}`{fn_tail}"
                )
        else:
            lines.append("- evidence_samples: (none)")

        involved_functions = f.get("involved_functions", [])
        lines.append(f"- involved_function_count: {f.get('involved_function_count', len(involved_functions))}")
        if involved_functions:
            lines.append("- involved_functions_sample:")
            for fn in involved_functions[:FEATURE_FUNCTION_SAMPLE_LIMIT]:
                fid = str(fn.get("id", ""))
                lines.append(
                    f"  - {fn.get('name')} [{fn.get('file')}:{fn.get('start_line')}] "
                    f"| stable_out={out_degree_stable.get(fid, 0)} "
                    f"| stable_in={in_degree_stable.get(fid, 0)} "
                    f"| unresolved={len(unresolved.get(fid, []))}"
                )
        else:
            lines.append("- involved_functions_sample: (none)")

        review = f.get("optimization_review", {}) or {}
        lines.append(
            f"- optimization_review: needs_optimization={review.get('needs_optimization', False)} "
            f"| priority={review.get('priority', 'low')}"
        )
        review_reasons = review.get("reasons", []) or []
        if review_reasons:
            lines.append("- review_reasons:")
            for rs in review_reasons[:4]:
                lines.append(f"  - {rs}")
        review_suggestions = review.get("suggestions", []) or []
        if review_suggestions:
            lines.append("- review_suggestions:")
            for sg in review_suggestions[:4]:
                lines.append(f"  - {sg}")
        lines.append("")

    lines.append("## 函数热点（可优先优化）")
    lines.append("")
    lines.append("### 高扇出（调用很多函数，优先统计非歧义边）")
    lines.append("")
    top_out_candidates = sorted(out_degree_stable.items(), key=lambda x: x[1], reverse=True)
    top_out = []
    for nid, degree in top_out_candidates:
        node = node_map.get(nid, {})
        if node.get("name") in HOTSPOT_NOISE_NAMES:
            continue
        top_out.append((nid, degree))
        if len(top_out) >= 12:
            break
    if top_out:
        for nid, degree in top_out:
            node = node_map.get(nid, {})
            lines.append(
                f"- {_node_short_label(node)} | stable_out={degree} | stable_in={in_degree_stable.get(nid, 0)} | "
                f"all_out={out_degree_all.get(nid, 0)}"
            )
    else:
        lines.append("- (none)")

    lines.append("")
    lines.append("### 高扇入（被很多函数调用，优先统计非歧义边）")
    lines.append("")
    top_in_candidates = sorted(in_degree_stable.items(), key=lambda x: x[1], reverse=True)
    top_in = []
    for nid, degree in top_in_candidates:
        node = node_map.get(nid, {})
        if node.get("name") in HOTSPOT_NOISE_NAMES:
            continue
        top_in.append((nid, degree))
        if len(top_in) >= 12:
            break
    if top_in:
        for nid, degree in top_in:
            node = node_map.get(nid, {})
            lines.append(
                f"- {_node_short_label(node)} | stable_in={degree} | stable_out={out_degree_stable.get(nid, 0)} | "
                f"all_in={in_degree_all.get(nid, 0)}"
            )
    else:
        lines.append("- (none)")

    lines.append("")
    lines.append("## 优化余地建议")
    lines.append("")
    suggestions: List[str] = []
    if by_priority:
        top = by_priority[0]
        suggestions.append(
            f"按目标权重计算，当前最高优化优先级是 `{top.get('feature_id')}` "
            f"(opt_priority={top.get('optimization_priority', 0):.2f})。"
        )
    if top_out:
        nid, degree = top_out[0]
        node = node_map.get(nid, {})
        if degree >= 18:
            suggestions.append(
                f"高扇出函数 `{node.get('name', '?')}`（{degree}）建议拆分职责（路由判定/IO/格式化分层）。"
            )
    if top_in:
        nid, degree = top_in[0]
        node = node_map.get(nid, {})
        if degree >= 12:
            suggestions.append(
                f"高扇入函数 `{node.get('name', '?')}`（{degree}）建议补强契约测试，降低跨模块回归风险。"
            )
    if unresolved:
        suggestions.append("存在 unresolved 调用点，建议补充解析规则或映射白名单，减少调用链盲区。")
    if ambiguous_count > 0:
        suggestions.append(
            f"存在 {ambiguous_count} 条同名函数歧义边按启发式归并，建议后续增加 fully-qualified 解析。"
        )
    if not suggestions:
        suggestions.append("当前图谱质量稳定；建议新增功能时先补 config feature 规则，再做实现，保证审计可追踪。")
    for s in suggestions:
        lines.append(f"- {s}")

    if errors:
        lines.append("")
        lines.append("## Warnings")
        lines.append("")
        for err in errors[:80]:
            lines.append(f"- {err}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def _suggest_actions_for_feature(feature: dict) -> List[str]:
    fid = str(feature.get("id", ""))
    cat = str(feature.get("category", ""))
    actions: List[str] = []
    if fid == "cli_run_pipeline":
        actions.extend(
            [
                "拆分 `run_cli`：参数解析、路由决策、执行/输出处理分离到独立函数。",
                "为 run 链路增加契约测试：隐式 run、`--explain-route`、无参数场景。",
                "将错误提示统一为用户可执行建议（命令示例 + 下一步）。",
            ]
        )
    elif fid == "cli_explain_plugin":
        actions.extend(
            [
                "稳定输出 `selected/alternatives/recommendation/confidence_gap` 字段契约。",
                "新增 explain 回放样本：误路由、低分差、fallback 重试场景。",
                "把 explain 结论接入审计 prompt 的自动复核段落。",
            ]
        )
    elif "audit" in fid or cat == "audit":
        actions.extend(
            [
                "把功能点覆盖结果（goal_score/coverage）写入 audit_health 附录。",
                "对 frozen_changed/frozen_missing 加强阻断与提示。",
                "补齐 route replay 失败样本的自动追踪任务。",
            ]
        )
    elif "encoding" in fid or cat == "encoding":
        actions.extend(
            [
                "补充混合污染文本样本（UTF-8 + 误解码片段）并增加修复回归测试。",
                "细化修复策略分级：仅清 BOM、重编码、不可安全修复。",
                "统一 doctor/repair 输出，让 IDE/CLI 都可直接消费。",
            ]
        )
    elif "web_log" in fid:
        actions.extend(
            [
                "继续补真实边界样本（代理/云包装）但保持非 APM 边界。",
                "确保 SCAN/BURST/SLOW 的统计锚点在 compact 中稳定可见。",
                "对健康检查降噪策略补审计断言，防止误吞异常。",
            ]
        )
    elif "cloud_log" in fid:
        actions.extend(
            [
                "扩充剥壳格式 case（table/csv/jsonl/plain）并校验内层日志转发。",
                "加强 provider 标识与时间轴保留，避免语义脱节。",
                "对剥壳失败路径给出 fallback 建议和 replay case。",
            ]
        )
    elif cat == "doctor":
        actions.extend(
            [
                "增强 workspace doctor 的 IDE/编码风险诊断提示质量。",
                "补充跨平台 shell/路径/编码差异样本。",
                "为 doctor 输出增加稳定字段契约，便于后续自动化消费。",
            ]
        )
    else:
        actions.extend(
            [
                "补充该功能点的入口函数契约测试。",
                "提升功能点与审计产物映射完整性。",
                "补充真实样本和 route replay 覆盖。",
            ]
        )
    return actions


def _parse_existing_checkbox_status(path: Path) -> Dict[Tuple[str, str], str]:
    status_map: Dict[Tuple[str, str], str] = {}
    if not path.exists():
        return status_map

    current_feature = ""
    for raw in path.read_text(encoding="utf-8", errors="ignore").splitlines():
        line = raw.strip()
        if line.startswith("### ") and "`" in line:
            m = re.search(r"`([^`]+)`", line)
            current_feature = m.group(1) if m else ""
            continue
        if not current_feature or not line.startswith("- [") or "] " not in line:
            continue
        m = re.match(r"- \[([ xX])\]\s+(.+)$", line)
        if not m:
            continue
        status = m.group(1).lower()
        text = m.group(2).strip()
        status_map[(current_feature, text)] = status
        # 对函数级任务行额外记录“稳定函数键”（剥离易变的 out/in/unresolved 统计），
        # 使 [x] 在统计数字漂移后仍能保留。
        if " | 问题:" in text:
            func_key = text.split(" | 问题:")[0].strip()
            status_map[("__fn__", current_feature, func_key)] = status
    return status_map


def _node_id_to_short_label(node_id: str, node_map: Dict[str, dict]) -> str:
    n = node_map.get(node_id, {})
    if not n:
        return node_id
    return f"{n.get('name', '?')} ({n.get('file', '?')}:{n.get('start_line', '?')})"


def _function_hotspots_for_feature(
    feature: dict,
    node_map: Dict[str, dict],
    out_degree_stable: Dict[str, int],
    in_degree_stable: Dict[str, int],
    unresolved_calls: Dict[str, List[str]],
    assigned_hotspot_ids: Optional[Set[str]] = None,
) -> List[dict]:
    rust_patterns = [p for p in (feature.get("paths", []) or []) if _is_rust_pattern(str(p))]
    items = []
    for f in feature.get("involved_functions", []) or []:
        fid = str(f.get("id", ""))
        if not fid:
            continue
        if assigned_hotspot_ids is not None and fid in assigned_hotspot_ids:
            continue
        file_path = str(f.get("file", ""))
        if _is_excluded_hotspot_file(file_path):
            continue
        if rust_patterns and not any(_path_matches(file_path, pat) for pat in rust_patterns):
            continue
        out_v = int(out_degree_stable.get(fid, 0))
        in_v = int(in_degree_stable.get(fid, 0))
        unresolved_v = len(unresolved_calls.get(fid, []))
        score = out_v * 3 + in_v * 2 + unresolved_v
        items.append(
            {
                "id": fid,
                "name": f.get("name", ""),
                "file": f.get("file", ""),
                "start_line": f.get("start_line", 0),
                "stable_out": out_v,
                "stable_in": in_v,
                "unresolved": unresolved_v,
                "priority_score": score,
                "label": _node_id_to_short_label(fid, node_map),
            }
        )

    # Fallback: if strict path scoping yields nothing, retry without path filter.
    if not items:
        for f in feature.get("involved_functions", []) or []:
            fid = str(f.get("id", ""))
            if not fid:
                continue
            if assigned_hotspot_ids is not None and fid in assigned_hotspot_ids:
                continue
            file_path = str(f.get("file", ""))
            if _is_excluded_hotspot_file(file_path):
                continue
            out_v = int(out_degree_stable.get(fid, 0))
            in_v = int(in_degree_stable.get(fid, 0))
            unresolved_v = len(unresolved_calls.get(fid, []))
            score = out_v * 3 + in_v * 2 + unresolved_v
            items.append(
                {
                    "id": fid,
                    "name": f.get("name", ""),
                    "file": f.get("file", ""),
                    "start_line": f.get("start_line", 0),
                    "stable_out": out_v,
                    "stable_in": in_v,
                    "unresolved": unresolved_v,
                    "priority_score": score,
                    "label": _node_id_to_short_label(fid, node_map),
                }
            )

    items.sort(key=lambda x: (x["priority_score"], x["stable_out"], x["unresolved"], x["stable_in"]), reverse=True)
    return items


def write_function_task_md(
    path: Path,
    feature_graph: dict,
    nodes: Sequence[dict],
    edges: Sequence[dict],
    unresolved_calls: Dict[str, List[str]],
    generated_at: str,
) -> None:
    existing_status = _parse_existing_checkbox_status(path)
    node_map: Dict[str, dict] = {str(n.get("id", "")): n for n in nodes}
    out_degree_stable: Dict[str, int] = {}
    in_degree_stable: Dict[str, int] = {}
    for e in edges:
        if bool(e.get("ambiguous", False)):
            continue
        frm = str(e.get("from", ""))
        to = str(e.get("to", ""))
        out_degree_stable[frm] = out_degree_stable.get(frm, 0) + 1
        in_degree_stable[to] = in_degree_stable.get(to, 0) + 1

    target_features = [
        f for f in feature_graph.get("features", [])
        if bool((f.get("optimization_review") or {}).get("needs_optimization", False))
        and str((f.get("optimization_review") or {}).get("priority", "low")) in {"high", "medium"}
    ]
    assigned_hotspot_ids: Set[str] = set()

    lines: List[str] = []
    lines.append("# PROJECT_INSIGHT_FUNCTION_TASKS")
    lines.append("")
    lines.append(f"- generated_at: {generated_at}")
    lines.append("- build_mode: auto-generated (script)")
    lines.append("- manual_audit_status: pending (需要人工复核后再冻结任务优先级)")
    lines.append("- source: docs/audit/project_insight_summary.md + project_capability_graph.json")
    lines.append("- selection: features with `optimization_review.needs_optimization=true` and priority high/medium")
    lines.append("")

    if not target_features:
        lines.append("当前没有 high/medium 的函数级优化任务。")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        return

    lines.append("## Function-Level Tasks")
    lines.append("")
    for idx, feature in enumerate(target_features, start=1):
        fid = str(feature.get("id", ""))
        title = str(feature.get("title", ""))
        review = feature.get("optimization_review", {}) or {}
        reasons = review.get("reasons", []) or []
        lines.append(f"### P{idx} - `{fid}` | {title}")
        lines.append("")
        lines.append(
            f"- review: needs_optimization={review.get('needs_optimization', False)} | "
            f"priority={review.get('priority', 'low')} | "
            f"involved_functions={feature.get('involved_function_count', 0)}"
        )
        if reasons:
            lines.append("- reasons:")
            for r in reasons[:4]:
                lines.append(f"  - {r}")

        hotspots = _function_hotspots_for_feature(
            feature=feature,
            node_map=node_map,
            out_degree_stable=out_degree_stable,
            in_degree_stable=in_degree_stable,
            unresolved_calls=unresolved_calls,
            assigned_hotspot_ids=assigned_hotspot_ids,
        )[:8]
        for h in hotspots[:5]:
            assigned_hotspot_ids.add(str(h.get("id", "")))
        if hotspots:
            lines.append("- hotspot_functions:")
            for h in hotspots:
                lines.append(
                    f"  - {h['label']} | out={h['stable_out']} in={h['stable_in']} unresolved={h['unresolved']} score={h['priority_score']}"
                )

        lines.append("- tasks:")
        for h in hotspots[:5]:
            task_text = (
                f"{h['label']} | 问题: out={h['stable_out']}, in={h['stable_in']}, unresolved={h['unresolved']} | "
                f"建议: 拆分职责并补契约测试（含错误路径）"
            )
            status = existing_status.get((fid, task_text), " ")
            # 优先用稳定函数键匹配，避免 out/in/unresolved 漂移导致已完成 [x] 丢失
            if status == " ":
                func_key = task_text.split(" | 问题:")[0].strip()
                status = existing_status.get(("__fn__", fid, func_key), " ")
            lines.append(f"  - [{status}] {task_text}")
        if not hotspots:
            fallback_text = "功能点函数集合不足 | 建议: 补 entry_functions/evidence_patterns 以定位函数"
            status = existing_status.get((fid, fallback_text), " ")
            lines.append(f"  - [{status}] {fallback_text}")

        lines.append("- acceptance:")
        accept_items = [
            "目标函数完成拆分或结构优化，并通过 `cargo check`。",
            "新增/更新对应函数级测试（至少 1 个契约测试）。",
            "重新运行 project_insight，`review_reasons` 至少减少 1 项。",
        ]
        for ac in accept_items:
            status = existing_status.get((fid, ac), " ")
            lines.append(f"  - [{status}] {ac}")
        lines.append("")

    lines.append("## Execution Commands")
    lines.append("")
    lines.append("```powershell")
    lines.append("python scripts/project_insight.py --config config/project_insight.toml --out-dir docs/audit")
    lines.append("tokenslim run cargo check")
    lines.append("```")
    lines.append("")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_opt_plan_md(
    path: Path,
    feature_graph: dict,
    coverage: dict,
    topn: int,
    generated_at: str,
    goal_weights: Dict[str, float],
) -> None:
    existing_status = _parse_existing_checkbox_status(path)

    features = {f.get("id"): f for f in feature_graph.get("features", [])}
    rows = sorted(coverage.get("rows", []), key=lambda r: float(r.get("optimization_priority", 0.0)), reverse=True)
    selected = rows[: max(1, topn)]

    lines: List[str] = []
    lines.append("# PROJECT_INSIGHT_OPT_PLAN")
    lines.append("")
    lines.append(f"- generated_at: {generated_at}")
    lines.append(f"- source: docs/audit/project_insight_summary.md")
    lines.append(f"- selection: top {len(selected)} by optimization_priority")
    lines.append("")
    lines.append("## Goal Weights")
    lines.append("")
    for k, v in goal_weights.items():
        lines.append(f"- {k}: {v:.2f}")

    lines.append("")
    lines.append("## Prioritized Tasks")
    lines.append("")
    for idx, row in enumerate(selected, start=1):
        fid = row.get("feature_id", "")
        f = features.get(fid, {})
        title = f.get("title", fid)
        lines.append(f"### P{idx} - `{fid}` | {title}")
        lines.append("")
        lines.append(
            f"- priority_score: {float(row.get('optimization_priority', 0.0)):.2f} "
            f"(goal={float(row.get('goal_weighted_score', 0.0)):.2f}, complexity={float(row.get('complexity_hint', 0.0)):.2f}, coverage={row.get('coverage_score', 0)}/3)"
        )
        lines.append(f"- owner: {f.get('owner', '')} | category: {f.get('category', '')}")
        lines.append(f"- why_now: 对项目目标贡献高，且当前复杂度/风险值得优先治理。")
        lines.append("- actions:")
        for act in _suggest_actions_for_feature(f):
            status = existing_status.get((fid, act), " ")
            lines.append(f"  - [{status}] {act}")
        lines.append("- acceptance:")
        accept_1 = "对应 case 审计通过（无 regression / frozen drift）"
        accept_2 = "补充或更新最小必要测试与样本"
        accept_3 = "更新 docs/audit 与相关计划文档状态"
        lines.append(
            f"  - [{existing_status.get((fid, accept_1), ' ')}] {accept_1}"
        )
        lines.append(
            f"  - [{existing_status.get((fid, accept_2), ' ')}] {accept_2}"
        )
        lines.append(
            f"  - [{existing_status.get((fid, accept_3), ' ')}] {accept_3}"
        )
        lines.append("")

    lines.append("## Execution Commands")
    lines.append("")
    lines.append("```powershell")
    lines.append("python scripts/project_insight.py --config config/project_insight.toml --out-dir docs/audit")
    lines.append("tokenslim run powershell -File scripts/audit_all_case_metrics.ps1 -RequireSemanticGate -FailOnRegression -FailOnFrozenChange -FailOnAnyFailure")
    lines.append("```")
    lines.append("")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    args = parse_args()
    root = Path(args.root).resolve()
    cfg_path = Path(args.config).resolve()
    out_dir = Path(args.out_dir).resolve()

    cfg = load_config(cfg_path)
    goal_weights = parse_goal_weights(cfg)
    include = cfg.get("scan", {}).get("include", ["src/**/*.rs"])
    exclude = cfg.get("scan", {}).get("exclude", ["target/**", "tmp/**"])
    all_files = expand_globs(root, include, exclude)
    rust_files = [p for p in all_files if p.suffix == ".rs"]

    functions: List[FunctionNode] = []
    for rf in rust_files:
        content = read_text(rf)
        functions.extend(extract_rust_functions(rf, content, root))

    nodes, edges, unresolved, ambiguous_count = build_call_graph(functions)
    feature_graph, errors = build_feature_graph(
        cfg=cfg,
        root=root,
        all_files=all_files,
        nodes=nodes,
        edges=edges,
        goal_weights=goal_weights,
        max_depth_override=args.max_depth,
        strict=args.strict,
    )

    parser_usage: Dict[str, int] = {}
    for fn in functions:
        parser_usage[fn.parser] = parser_usage.get(fn.parser, 0) + 1
    if parser_usage and set(parser_usage.keys()) == {"regex"}:
        errors.append(
            "tree-sitter rust parser not available; using regex fallback only. "
            "Install `tree_sitter_languages` or `tree_sitter_rust` to improve callchain accuracy."
        )

    enrich_feature_reviews(feature_graph, edges, unresolved)

    capability_graph = {
        "generated_at": now_iso(),
        "project": cfg.get("project", {}),
        "goal_weights": goal_weights,
        "config_path": relpath(cfg_path, root),
        "scan": {"include": include, "exclude": exclude},
        "parser_usage": parser_usage,
        "feature_graph": feature_graph,
        "warnings": errors,
    }
    callchain_map = {
        "generated_at": now_iso(),
        "node_count": len(nodes),
        "edge_count": len(edges),
        "ambiguous_resolved_edges": ambiguous_count,
        "parser_usage": parser_usage,
        "nodes": nodes,
        "edges": edges,
        "unresolved_calls": unresolved,
    }
    coverage_matrix = build_coverage_matrix(root, feature_graph)
    coverage_matrix["warnings"] = errors

    ensure_dir(out_dir)
    write_json(out_dir / "project_capability_graph.json", capability_graph)
    write_json(out_dir / "callchain_map.json", callchain_map)
    write_json(out_dir / "coverage_matrix.json", coverage_matrix)
    write_summary_md(
        out_dir / "project_insight_summary.md",
        root=root,
        scanned_files=all_files,
        rust_files=rust_files,
        goal_weights=goal_weights,
        functions=functions,
        nodes=nodes,
        edges=edges,
        unresolved=unresolved,
        ambiguous_count=ambiguous_count,
        feature_graph=feature_graph,
        coverage=coverage_matrix,
        errors=errors,
    )
    opt_plan_path = (root / args.opt_plan_out).resolve()
    write_opt_plan_md(
        path=opt_plan_path,
        feature_graph=feature_graph,
        coverage=coverage_matrix,
        topn=args.opt_plan_topn,
        generated_at=now_iso(),
        goal_weights=goal_weights,
    )
    function_task_path = (root / args.function_task_out).resolve()
    write_function_task_md(
        path=function_task_path,
        feature_graph=feature_graph,
        nodes=nodes,
        edges=edges,
        unresolved_calls=unresolved,
        generated_at=now_iso(),
    )

    print(f"project_capability_graph={out_dir / 'project_capability_graph.json'}")
    print(f"callchain_map={out_dir / 'callchain_map.json'}")
    print(f"coverage_matrix={out_dir / 'coverage_matrix.json'}")
    print(f"project_insight_summary={out_dir / 'project_insight_summary.md'}")
    print(f"project_insight_opt_plan={opt_plan_path}")
    print(f"project_insight_function_tasks={function_task_path}")
    print(f"scanned_files={len(all_files)}")
    print(f"rust_files={len(rust_files)}")
    print(f"function_nodes={len(nodes)}")
    print(f"call_edges={len(edges)}")
    print(f"features={len(feature_graph.get('features', []))}")
    print(f"warnings={len(errors)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
