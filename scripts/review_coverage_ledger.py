# -*- coding: utf-8 -*-
"""复审覆盖账本（Review Coverage Ledger）生成与校验工具。

背景：调用链复审此前靠 grep 抽样 + 人工记忆，无法机器化证明
"哪些符号审过、哪些没审、发现了什么"。本脚本把覆盖状态固化为
JSON 账本（docs/reports/review_coverage_ledger.json），做到：

1. scan    —— 扫描 src/**/*.rs 枚举全部函数符号（权威清单），
              按文件级复审状态映射展开为符号级账本；
2. graph   —— tree-sitter AST 抽取每个函数体内的全部调用点
              （自由函数/方法/路径调用/宏），按名字解析为本 crate
              符号（唯一命中→resolved；多 impl→ambiguous；trait 声明
              +多实现→trait_dispatch；未命中→unresolved/external），
              产出调用图 JSON + 从 main 出发的入口可达性分析；
3. mark    —— 复审断点续跑的关键：把某文件的复审状态写入账本
              JSON（mark <file> <status> --round N --findings P1-01,P2-02 --note 说明）。
              JSON 中的 file_status 是状态权威，优先于脚本内置表；
4. verify  —— 校验账本与磁盘同步（代码漂移检测：文件增删/函数
              增删/行号漂移都会报错），并输出统计；
5. 对账    —— 与 callwarden 图侧 fn 数（get_comment_coverage 实测
              3740）对账，差异超过阈值时在 summary 中告警。

账本原则：
- 符号级状态继承自文件级状态（file_status 是唯一人工维护面）；
- findings 字段把复审报告的问题编号（P1-01 等）关联到符号；
- 账本可 diff、可断言、可进 CI（verify 退出码非 0 即漂移）。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC_DIR = os.path.join(ROOT, "src")
LEDGER_PATH = os.path.join(ROOT, "docs", "reports", "review_coverage_ledger.json")

# callwarden 图侧 fn 数（2026-09-06 comment-coverage 实测：fn 27.4%，1026/3740）
CW_GRAPH_FN_COUNT = 3740
CW_FN_TOLERANCE = 0.15  # 图侧与本地扫描允许 ±15% 差异（解析器口径不同）

FN_RE = re.compile(
    r"^[ \t]*(?:pub(?:\((?:crate|super|self|in\s+\w+)\))?[ \t]+)?"
    r"(?:const[ \t]+)?(?:async[ \t]+)?(?:unsafe[ \t]+)?(?:extern[ \t]+\"[^\"]+\"[ \t]+)?"
    r"fn[ \t]+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)

# 文件级复审状态（唯一人工维护面）。
# status: reviewed=逐行审读 | partial=部分审读/接线审计 | unreviewed
# round: 复审轮次；findings: 关联问题编号；note: 审读范围说明
FILE_STATUS: dict[str, dict] = {
    # ===== 第一轮：压缩/解压主干 + 共享子系统 =====
    "src/main.rs": {"status": "reviewed", "round": 1, "findings": [], "note": "入口逐行"},
    "src/cli/commands/compress.rs": {"status": "reviewed", "round": 1, "findings": ["P1-08", "P2-08", "P2-17", "P2-18", "P3-09"], "note": "全量逐行"},
    "src/cli/commands/decompress.rs": {"status": "reviewed", "round": 1, "findings": ["P2-08", "P2-16", "P3-10"], "note": "全量逐行"},
    "src/core/compression_pipeline/methods.rs": {"status": "reviewed", "round": 1, "findings": ["P1-04", "P1-05", "P2-04", "P2-06", "P2-12", "P2-13", "P3-11", "P3-12"], "note": "全量逐行"},
    "src/core/plugin_dispatcher/methods.rs": {"status": "reviewed", "round": 1, "findings": ["P2-05", "P3-04", "P3-07"], "note": "全量逐行"},
    "src/core/dictionary_engine/methods.rs": {"status": "reviewed", "round": 1, "findings": ["P3-01", "P3-03"], "note": "全量逐行"},
    "src/core/dictionary_manager/methods.rs": {"status": "reviewed", "round": 1, "findings": ["P1-03", "P2-01", "P2-02", "P2-03", "P2-07", "P3-02", "P3-05", "P3-06"], "note": "全量逐行"},
    "src/core/dedup_engine/methods.rs": {"status": "reviewed", "round": 1, "findings": ["P2-09", "P2-10"], "note": "全量逐行"},
    "src/core/rehydration_pipeline/methods.rs": {"status": "reviewed", "round": 1, "findings": ["P1-01", "P1-02", "P2-14", "P2-15", "P2-16"], "note": "全量逐行"},
    # ===== 插件层抽样（第一轮）=====
    "src/plugins/ci_log_plugin/methods.rs": {"status": "partial", "round": 1, "findings": ["P1-06"], "note": "仅 peel_document 段逐行"},
    "src/plugins/gcc_log_plugin/methods.rs": {"status": "partial", "round": 1, "findings": ["P2-19", "P2-20", "P2-21"], "note": "compress/compress_with_context 双路径"},
    "src/plugins/java_stack_plugin/methods.rs": {"status": "partial", "round": 1, "findings": ["P1-07"], "note": "仅 compress_with_context/truncate_deep_stack 段"},
    # ===== 第二轮：输入/切片/配置装载层 =====
    "src/core/stream_reader/methods.rs": {"status": "reviewed", "round": 2, "findings": ["P1-08", "P2-22", "P2-23", "P2-24", "P3-15", "P3-17", "P3-18"], "note": "全量逐行"},
    "src/core/text_slicer/methods.rs": {"status": "reviewed", "round": 2, "findings": ["P1-09", "P2-25", "P3-14"], "note": "主体逐行（段落缓冲细节抽样）"},
    "src/core/text_slicer/config_loader.rs": {"status": "reviewed", "round": 2, "findings": ["P1-09", "P2-25"], "note": "全量逐行"},
    "src/core/text_slicer/types.rs": {"status": "reviewed", "round": 2, "findings": ["P3-14"], "note": "全量逐行"},
    # 注：src/core/dynamic_plugin_loader/mod.rs（P3-16）已随 P3-196 裁决整模块删除（批 H，4d1fbf65），
    # 条目移除；findings 索引保留 P3-16 历史记录。
    # 接线审计（非逐行）
    "src/core/encoding_fallback/mod.rs": {"status": "partial", "round": 2, "findings": ["P1-08"], "note": "仅调用点接线审计，内部 1959 行未逐行"},
}

# 与 callwarden 图对账的分母口径：排除测试/展示文件后的生产代码
EXCLUDE_PATTERNS = ("test.rs", "showcase.rs", "bench.rs")


def scan_functions() -> list[dict]:
    """扫描 src 下全部 .rs 文件，枚举 fn 符号（行级）。"""
    symbols: list[dict] = []
    for dirpath, _dirnames, filenames in os.walk(SRC_DIR):
        for fn in sorted(filenames):
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(dirpath, fn)
            rel = os.path.relpath(path, ROOT).replace("\\", "/")
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                lines = f.readlines()
            for i, line in enumerate(lines, 1):
                m = FN_RE.match(line)
                if m:
                    # 粗排注释行（简易：行内 // 在 fn 之前出现且 fn 在其后）
                    code = line.split("//")[0]
                    if "fn " not in code:
                        continue
                    is_test = ("test" in fn) or (fn == "showcase.rs") or (fn == "bench.rs")
                    symbols.append({"file": rel, "line": i, "name": m.group("name"), "is_test": is_test})
    return symbols


def module_of(rel_file: str) -> str:
    parts = rel_file.split("/")
    if len(parts) >= 3:
        return "/".join(parts[:3])
    if len(parts) >= 2:
        return "/".join(parts[:2])
    return rel_file


# ================= AST 符号树构建（tree-sitter） =================
# 符号树 = mod 层级 + impl/trait 归属 + fn 全名（qname）+ 可见性 + 行范围。
# 相比正则扫描：多行签名、注释/字符串误检、归属关系三类问题全部消除；
# 与 callwarden 图侧同源（daemon 亦为 tree-sitter 级解析），qname 风格对齐
# （`lib::cli::commands::compress.read_compress_input` → 本工具
#   `crate::cli::commands::compress.read_compress_input`）。

_AST_LANG = None


def _ast_language():
    global _AST_LANG
    if _AST_LANG is None:
        from tree_sitter import Language
        import tree_sitter_rust
        _AST_LANG = Language(tree_sitter_rust.language())
    return _AST_LANG


def _child_by_field_names(node, *names):
    for n in names:
        c = node.child_by_field_name(n)
        if c is not None:
            return c
    return None


def _walk_fn_items(node, mods: list[str], impl_stack: list[str], out: list[dict]) -> None:
    """深度优先遍历 AST，收集命名函数与 trait 函数声明，维护归属栈。"""
    for child in node.children:
        t = child.type
        if t == "mod_item":
            name_node = _child_by_field_names(child, "name")
            if name_node is not None:
                _walk_fn_items(child, mods + [name_node.text.decode()], impl_stack, out)
            continue
        if t == "impl_item":
            # impl Trait for Type / impl Type —— 归属取 trait（若有），否则 for 的类型
            trait_node = _child_by_field_names(child, "trait")
            type_node = _child_by_field_names(child, "type")
            owner = None
            if trait_node is not None:
                owner = "impl " + trait_node.text.decode().split()[-1] + " for " + type_node.text.decode()
            elif type_node is not None:
                owner = "impl " + type_node.text.decode()
            _walk_fn_items(child, mods, impl_stack + ([owner] if owner else []), out)
            continue
        if t == "trait_item":
            name_node = _child_by_field_names(child, "name")
            if name_node is not None:
                _walk_fn_items(child, mods, impl_stack + [f"trait {name_node.text.decode()}"], out)
            continue
        if t in ("function_item", "function_signature_item"):
            name_node = _child_by_field_names(child, "name")
            if name_node is None:
                continue
            vis = "pub" if any(c.type == "visibility_modifier" for c in child.children) else "private"
            kind = ("trait_fn" if t == "function_signature_item"
                    else "method" if impl_stack else "fn")
            owner = impl_stack[-1] if impl_stack and impl_stack[-1].startswith(("impl ", "trait ")) else None
            mod_path = "crate::" + "::".join(mods) if mods else "crate"
            qname = mod_path + (f".{{{owner}}}" if owner else "") + "." + name_node.text.decode()
            out.append({
                "qname": qname,
                "name": name_node.text.decode(),
                "kind": kind,
                "owner": owner,
                "visibility": vis,
                "line_start": child.start_point[0] + 1,
                "line_end": child.end_point[0] + 1,
                "body": child.child_by_field_name("body"),  # tree-sitter Node，供调用点抽取；不序列化
            })
            # 函数体内还可能有嵌套 fn / closure 内 fn —— 继续下钻（归属保持当前 impl）
            _walk_fn_items(child, mods, impl_stack, out)
            continue
        # 其余节点：只要不是函数体外的重复下钻风险，统一递归
        _walk_fn_items(child, mods, impl_stack, out)


def scan_functions_ast() -> tuple[list[dict], list[str]]:
    """tree-sitter AST 符号树扫描。返回 (symbols, errors)。"""
    from tree_sitter import Parser
    parser = Parser(_ast_language())
    symbols: list[dict] = []
    errors: list[str] = []
    for dirpath, _dirnames, filenames in os.walk(SRC_DIR):
        for fn in sorted(filenames):
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(dirpath, fn)
            rel = os.path.relpath(path, ROOT).replace("\\", "/")
            with open(path, "rb") as f:
                tree = parser.parse(f.read())
            if tree.root_node.has_error:
                errors.append(rel)
            fns: list[dict] = []
            _walk_fn_items(tree.root_node, [], [], fns)
            is_test = ("test" in fn) or (fn == "showcase.rs") or (fn == "bench.rs")
            for item in fns:
                item.update({"file": rel, "is_test": is_test})
                symbols.append(item)
    return symbols, errors


# ================= 调用图构建（AST 调用点抽取 + 名字解析） =================
# 调用边三类来源：
#   - call_expression.function 为 identifier        → 自由函数调用
#   - call_expression.function 为 field_expression  → 方法调用（receiver.method）
#   - call_expression.function 为 scoped_identifier → 路径调用（mod::fn）
#   - macro_invocation                              → 宏调用（只记录，不解析）
# 解析规则（名字级，非类型级——无 borrow checker/泛型单态化，诚实标注局限）：
#   自由函数：crate 全局唯一→resolved；同文件有同名→优先同文件；
#             多处同名且不同文件→ambiguous
#   方法：    本 crate impl 中唯一→resolved；多个 impl 同名→若存在
#             trait 声明则 trait_dispatch(N impls)，否则 ambiguous(N)
CALLGRAPH_PATH = os.path.join(ROOT, "docs", "reports", "review_callgraph.json")

_CALLS_RECURSE_SKIP = {"function_item", "function_signature_item"}


def _iter_calls(node):
    """先序遍历，收集调用点；跳过嵌套 fn（它们作为独立符号单独处理）。"""
    for child in node.children:
        t = child.type
        if t == "call_expression":
            f = child.child_by_field_name("function")
            if f is not None:
                ft = f.type
                if ft == "identifier":
                    yield ("fn", f.text.decode(), child.start_point[0] + 1)
                elif ft == "field_expression":
                    fld = f.child_by_field_name("field")
                    if fld is not None:
                        yield ("method", fld.text.decode(), child.start_point[0] + 1)
                elif ft in ("scoped_identifier", "scoped_type_identifier"):
                    yield ("path", f.text.decode().split("::")[-1], child.start_point[0] + 1)
                elif ft == "generic_function":
                    nm = f.child_by_field_name("function")
                    if nm is not None:
                        yield ("generic", nm.text.decode().split("::")[-1], child.start_point[0] + 1)
        elif t == "macro_invocation":
            m = child.child_by_field_name("macro")
            if m is not None:
                yield ("macro", m.text.decode().split("::")[-1], child.start_point[0] + 1)
        if t not in _CALLS_RECURSE_SKIP:
            yield from _iter_calls(child)


def _fn_body_node(fn_item_node):
    return fn_item_node.child_by_field_name("body")


def build_call_graph() -> dict:
    """从 AST 符号树 + 函数体调用点构建 crate 内调用图与入口可达性。"""
    from tree_sitter import Parser
    parser = Parser(_ast_language())

    # 1) 符号索引：name → 候选（自由函数 / impl 方法 / trait 声明）
    free_fns: dict[str, list[dict]] = {}
    impl_methods: dict[str, list[dict]] = {}
    trait_fns: set[str] = set()
    all_syms: list[dict] = []  # 每项附 body_node 供第二步使用

    for dirpath, _dirnames, filenames in os.walk(SRC_DIR):
        for fn in sorted(filenames):
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(dirpath, fn)
            rel = os.path.relpath(path, ROOT).replace("\\", "/")
            with open(path, "rb") as f:
                tree = parser.parse(f.read())
            fns: list[dict] = []
            _walk_fn_items(tree.root_node, [], [], fns)
            for item in fns:
                item["file"] = rel
                all_syms.append(item)
                if item["kind"] == "trait_fn" or (item["owner"] or "").startswith("trait "):
                    # trait 声明 + trait 默认实现都算 trait 方法集合成员；
                    # 默认实现同时是 dispatch 候选（无人覆写时运行的就是它）
                    trait_fns.add(item["name"])
                    impl_methods.setdefault(item["name"], []).append(item)
                elif item["owner"] and (item["owner"].startswith("impl ") or " for " in item["owner"]):
                    # impl 块方法与 trait impl 方法都按"方法候选"归类
                    impl_methods.setdefault(item["name"], []).append(item)
                else:
                    free_fns.setdefault(item["name"], []).append(item)

    # 2) 抽取每个符号的调用点并解析
    edges: list[dict] = []
    per_symbol_calls: dict[str, set[str]] = {}   # caller qname → resolved callee qnames
    unresolved_by_callee: dict[str, int] = {}
    stat = {"resolved": 0, "ambiguous": 0, "trait_dispatch": 0, "macro": 0, "external": 0}

    def resolve(kind: str, raw: str) -> tuple[str, list[str], int]:
        """返回 (resolution, target_qnames, impl_count)。
        trait_dispatch 的 targets = 全部 impl 候选（任何一个都可能在运行时执行）。"""
        if kind in ("method", "generic"):
            cands = impl_methods.get(raw, [])
            if len(cands) == 1:
                return "resolved", [cands[0]["qname"]], 1
            if len(cands) > 1:
                if raw in trait_fns:
                    return "trait_dispatch", [c["qname"] for c in cands], len(cands)
                return "ambiguous", [], len(cands)
            ff = free_fns.get(raw, [])
            if len(ff) == 1:
                return "resolved", [ff[0]["qname"]], 1
            if len(ff) > 1:
                return "ambiguous", [], len(ff)
            return "external", [], 0
        # fn / path：自由函数优先，同文件优先消歧
        cands = free_fns.get(raw, [])
        if len(cands) == 1:
            return "resolved", [cands[0]["qname"]], 1
        if len(cands) > 1:
            return "ambiguous", [], len(cands)
        cands = impl_methods.get(raw, [])
        if len(cands) == 1:
            return "resolved", [cands[0]["qname"]], 1
        if len(cands) > 1:
            return ("trait_dispatch" if raw in trait_fns else "ambiguous"), [c["qname"] for c in cands], len(cands)
        return "external", [], 0

    for sym in all_syms:
        body = sym.get("body")
        if body is None:
            continue
        for kind, raw, line in _iter_calls(body):
            if kind == "macro":
                stat["macro"] += 1
                edges.append({"caller": sym["qname"], "site": f"{sym['file']}:{line}",
                              "kind": kind, "callee_raw": raw, "resolution": "macro",
                              "targets": [], "impl_count": 0})
                continue
            res, targets, n_impl = resolve(kind, raw)
            stat[res] += 1
            edges.append({"caller": sym["qname"], "site": f"{sym['file']}:{line}",
                          "kind": kind, "callee_raw": raw, "resolution": res,
                          "targets": targets, "impl_count": n_impl})
            if targets:
                per_symbol_calls.setdefault(sym["qname"], set()).update(targets)

    # 3) 入口可达性：从 src/main.rs 与 src/bin/** 的 main 出发（仅 resolved 边）
    roots = sorted({s["qname"] for s in all_syms
                    if s["name"] == "main" and (s["file"] == "src/main.rs" or s["file"].startswith("src/bin/"))})
    callers_of: dict[str, set[str]] = {}
    for caller, callees in per_symbol_calls.items():
        for c in callees:
            callers_of.setdefault(c, set()).add(caller)
    reachable: set[str] = set()
    stack = list(roots)
    while stack:
        q = stack.pop()
        if q in reachable:
            continue
        reachable.add(q)
        stack.extend(per_symbol_calls.get(q, ()))

    no_caller = sorted(
        s["qname"] for s in all_syms
        if s["qname"] not in callers_of and s["qname"] not in roots and not s.get("is_test")
    )
    prod_total = sum(1 for s in all_syms if not s.get("is_test"))

    return {
        "version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "roots": roots,
        "summary": {
            "symbols_total": len(all_syms),
            "symbols_production": prod_total,
            "edges_total": len(edges),
            **{f"edges_{k}": v for k, v in stat.items()},
            "reachable_from_roots": len(reachable & {s['qname'] for s in all_syms if not s.get('is_test')}),
            "no_resolved_callers": len(no_caller),
        },
        "resolution_legend": {
            "resolved": "本 crate 内唯一命中（自由函数/方法/唯一 trait 实现）",
            "trait_dispatch": "trait 方法多实现，targets 列出全部候选（运行时任一可执行）",
            "ambiguous": "本 crate 内多处同名且非 trait 分发（如多个 impl 同名私有方法），需类型信息消歧",
            "macro": "宏调用，只记录不解析（宏体展开不在 AST 内）",
            "external": "本 crate 无此名（std/外部 crate/外部 trait 方法/闭包参数回调）",
        },
        "reachable_from_roots": sorted(reachable),
        "no_resolved_callers": no_caller,
        "edges": edges,
    }


def cmd_graph() -> int:
    graph = build_call_graph()
    s = graph["summary"]
    os.makedirs(os.path.dirname(CALLGRAPH_PATH), exist_ok=True)
    with open(CALLGRAPH_PATH, "w", encoding="utf-8", newline="\n") as f:
        json.dump(graph, f, ensure_ascii=False)
        f.write("\n")
    print(f"callgraph written: {os.path.relpath(CALLGRAPH_PATH, ROOT)}")
    print(f"  edges total={s['edges_total']} resolved={s['edges_resolved']} "
          f"trait_dispatch={s['edges_trait_dispatch']} ambiguous={s['edges_ambiguous']} "
          f"macro={s['edges_macro']} external={s['edges_external']}")
    print(f"  roots: {graph['roots']}")
    print(f"  reachable from roots: {s['reachable_from_roots']}/{s['symbols_production']} (仅 resolved 边)")
    print(f"  no-resolved-caller symbols: {s['no_resolved_callers']}（含 trait impl 方法/回调/死代码，需人工甄别）")
    return 0


def _merged_file_status() -> dict[str, dict]:
    """合并复审状态：账本 JSON 中已标记的 file_status 优先（断点续跑的权威），
    脚本内置 FILE_STATUS 兜底（首次 scan 的种子）。"""
    merged = {k: dict(v) for k, v in FILE_STATUS.items()}
    if os.path.exists(LEDGER_PATH):
        try:
            with open(LEDGER_PATH, "r", encoding="utf-8") as f:
                old = json.load(f)
            for k, v in (old.get("file_status") or {}).items():
                merged[k] = v
        except (json.JSONDecodeError, OSError) as e:
            # 账本损坏/不可读绝不能静默吞掉——那会把状态退回种子表，历史标记全部丢失。
            print(f"[fail] 账本 JSON 不可读，拒绝静默降级到种子表: {e}")
            raise
    return merged


def build_ledger() -> dict:
    parser_used = "ast"
    parse_errors: list[str] = []
    try:
        symbols, parse_errors = scan_functions_ast()
    except ImportError:
        parser_used = "regex"
        symbols = scan_functions()
    # 统一字段口径：regex 扫描缺 line_end/qname 等字段时补齐
    for s in symbols:
        s.setdefault("kind", "fn")
        s.setdefault("owner", None)
        s.setdefault("visibility", "")
        s.setdefault("qname", "")
        s.setdefault("line_end", s.get("line", 0))
    if parse_errors:
        print(f"[warn] {len(parse_errors)} 个文件 AST 解析报错（语法不完整），已按部分树处理: {parse_errors[:5]}")
    file_counts: dict[str, int] = {}
    for s in symbols:
        file_counts[s["file"]] = file_counts.get(s["file"], 0) + 1

    unknown_files = sorted(set(FILE_STATUS) - set(file_counts))
    if unknown_files:
        print(f"[warn] FILE_STATUS 中配置了磁盘上不存在的文件（已忽略）: {unknown_files}")

    out_symbols = []
    merged_status = _merged_file_status()
    for s in symbols:
        st = merged_status.get(s["file"], {"status": "unreviewed", "round": 0, "findings": [], "note": ""})
        out_symbols.append({
            "file": s["file"],
            "line": s.get("line_start", s.get("line", 0)),
            "line_end": s.get("line_end", 0),
            "name": s["name"],
            "qname": s.get("qname", ""),
            "kind": s.get("kind", "fn"),
            "owner": s.get("owner"),
            "visibility": s.get("visibility", ""),
            "module": module_of(s["file"]),
            "status": st["status"],
            "round": st["round"],
            "findings": list(st["findings"]),
        })

    # 模块级汇总
    modules: dict[str, dict] = {}
    for s in out_symbols:
        m = modules.setdefault(s["module"], {"total": 0, "reviewed": 0, "partial": 0, "unreviewed": 0, "findings": set()})
        m["total"] += 1
        m[s["status"]] += 1
        m["findings"].update(s["findings"])
    modules_out = {
        k: {**{kk: vv for kk, vv in v.items() if kk != "findings"},
            "findings": sorted(v["findings"]),
            "coverage_pct": round(100.0 * (v["reviewed"] + v["partial"]) / v["total"], 1)}
        for k, v in sorted(modules.items())
    }

    total = len(out_symbols)
    reviewed = sum(1 for s in out_symbols if s["status"] == "reviewed")
    partial = sum(1 for s in out_symbols if s["status"] == "partial")
    all_findings = sorted({f for s in out_symbols for f in s["findings"]}
                          | {f for v in merged_status.values() for f in (v.get("findings") or [])})
    # 对账口径：callwarden 图侧 fn 数对应生产代码（排除 test/showcase/bench）
    prod_total = sum(1 for s in symbols if not s.get("is_test"))
    cw_delta = round(100.0 * abs(prod_total - CW_GRAPH_FN_COUNT) / CW_GRAPH_FN_COUNT, 1) if CW_GRAPH_FN_COUNT else None

    return {
        "version": 2,
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "parser": {"mode": parser_used, "syntax_errors": len(parse_errors), "files_with_errors": parse_errors[:10]},
        "summary": {
            "total_symbols": total,
            "reviewed": reviewed,
            "partial": partial,
            "unreviewed": total - reviewed - partial,
            "coverage_pct": round(100.0 * (reviewed + partial) / total, 1) if total else 0.0,
            "total_files": len(file_counts),
            "findings_linked": all_findings,
            "production_symbols": prod_total,
            "cw_graph_fn_count": CW_GRAPH_FN_COUNT,
            "cw_graph_delta_pct": cw_delta,
        },
        "file_status": {
            k: v for k, v in sorted(merged_status.items())
            # 无 fn 符号的文件（mod.rs/纯类型 test.rs）也有复审状态，按磁盘存在性保留，
            # 防止 mark 后被重建静默丢弃。
            if k in file_counts or os.path.exists(os.path.join(ROOT, k))
        },
        "modules": modules_out,
        "symbols": out_symbols,
    }


def cmd_scan() -> int:
    ledger = build_ledger()
    s = ledger["summary"]
    os.makedirs(os.path.dirname(LEDGER_PATH), exist_ok=True)
    with open(LEDGER_PATH, "w", encoding="utf-8", newline="\n") as f:
        json.dump(ledger, f, ensure_ascii=False, indent=2)
        f.write("\n")
    print(f"ledger written: {os.path.relpath(LEDGER_PATH, ROOT)}")
    print(f"  symbols total={s['total_symbols']} reviewed={s['reviewed']} partial={s['partial']} "
          f"unreviewed={s['unreviewed']} coverage={s['coverage_pct']}%")
    print(f"  findings linked: {len(s['findings_linked'])} -> {s['findings_linked']}")
    if s["cw_graph_delta_pct"] is not None and s["cw_graph_delta_pct"] > CW_FN_TOLERANCE * 100:
        print(f"  [warn] 与 callwarden 图侧 fn 数({CW_GRAPH_FN_COUNT})差异 {s['cw_graph_delta_pct']}% 超阈值，需人工对账口径")
    else:
        print(f"  对账: 与 callwarden 图侧 fn 数差异 {s['cw_graph_delta_pct']}%（阈值 {CW_FN_TOLERANCE*100:.0f}%）OK")
    return 0


def cmd_verify() -> int:
    if not os.path.exists(LEDGER_PATH):
        print(f"[fail] 账本不存在: {LEDGER_PATH}，先运行 scan")
        return 2
    with open(LEDGER_PATH, "r", encoding="utf-8") as f:
        old = json.load(f)
    fresh = build_ledger()

    old_sig = {(x["file"], x["line"], x["name"]) for x in old["symbols"]}
    new_sig = {(x["file"], x["line"], x["name"]) for x in fresh["symbols"]}
    added = sorted(new_sig - old_sig)
    removed = sorted(old_sig - new_sig)

    # 状态漂移：同一 (file,line,name) 的状态/发现变化
    old_map = {(x["file"], x["line"], x["name"]): x for x in old["symbols"]}
    status_drift = [
        {"symbol": k, "old": old_map[k]["status"], "new": fresh_map[k]["status"]}
        for k in sorted(new_sig & old_sig)
        if (fresh_map := {(x["file"], x["line"], x["name"]): x for x in fresh["symbols"]}) and
        old_map[k]["status"] != fresh_map[k]["status"]
    ]

    ok = True
    if added:
        ok = False
        print(f"[fail] 新增 {len(added)} 个符号未入账（代码漂移），示例: {added[:5]}")
    if removed:
        ok = False
        print(f"[fail] 账本中 {len(removed)} 个符号已从磁盘消失，示例: {removed[:5]}")
    if status_drift:
        ok = False
        print(f"[fail] {len(status_drift)} 个符号状态漂移，示例: {status_drift[:3]}")
    if ok:
        s = fresh["summary"]
        print(f"[ok] 账本与磁盘同步: total={s['total_symbols']} reviewed={s['reviewed']} "
              f"partial={s['partial']} coverage={s['coverage_pct']}%")
        return 0
    return 1


def cmd_summary() -> int:
    if not os.path.exists(LEDGER_PATH):
        print("账本不存在，先运行 scan")
        return 2
    with open(LEDGER_PATH, "r", encoding="utf-8") as f:
        ledger = json.load(f)
    s = ledger["summary"]
    print(f"总符号 {s['total_symbols']} | reviewed {s['reviewed']} | partial {s['partial']} "
          f"| unreviewed {s['unreviewed']} | 覆盖 {s['coverage_pct']}%")
    print(f"关联问题 {len(s['findings_linked'])} 项: {s['findings_linked']}")
    print("\n按模块（coverage < 30% 且 total >= 20 的优先补审区）:")
    for name, m in ledger["modules"].items():
        if m["total"] >= 20 and m["coverage_pct"] < 30:
            print(f"  {name:50s} total={m['total']:4d} cov={m['coverage_pct']:5.1f}% findings={m['findings']}")
    return 0


def cmd_mark(file: str, status: str, round_no: int, findings: list[str], note: str) -> int:
    """断点续跑核心：把一个文件的复审状态写入账本 JSON（状态权威），
    随后重建符号级展开。窗口超时后，新会话读 JSON 即恢复全部进度。"""
    file = file.replace("\\", "/").strip()
    if status not in ("reviewed", "partial", "unreviewed"):
        print(f"[fail] 非法 status: {status}（仅 reviewed/partial/unreviewed）")
        return 2
    if not os.path.exists(os.path.join(ROOT, file)):
        print(f"[fail] 文件不存在: {file}")
        return 2
    if not os.path.exists(LEDGER_PATH):
        print("[fail] 账本不存在，先运行 scan")
        return 2
    with open(LEDGER_PATH, "r", encoding="utf-8") as f:
        ledger = json.load(f)
    fs = ledger.setdefault("file_status", {})
    old_entry = fs.get(file)
    fs[file] = {"status": status, "round": round_no, "findings": sorted(set(findings)), "note": note}
    with open(LEDGER_PATH, "w", encoding="utf-8", newline="\n") as f:
        json.dump(ledger, f, ensure_ascii=False, indent=2)
        f.write("\n")
    # 重建符号级展开（build_ledger 会读回 JSON 中的 file_status）
    old_cov = 0
    if isinstance(ledger.get("summary"), dict):
        old_cov = ledger["summary"].get("reviewed", 0) + ledger["summary"].get("partial", 0)
    ledger = build_ledger()
    s = ledger["summary"]
    new_cov = s["reviewed"] + s["partial"]
    if new_cov < old_cov:
        # 覆盖率回归 = file_status 与 summary 块不一致（历史标记丢失/陈旧固化）。
        # 首次写入已保留 old+本次 mark，不落重建结果，响亮报警交由人工恢复。
        print(f"[fail] 覆盖率回归：reviewed+partial {old_cov} -> {new_cov}，"
              f"账本 file_status 疑似早于 summary 块陈旧（历史标记已丢失）。"
              f"本次 mark 的原始写入已保留，重建结果未落盘；请先恢复 file_status 再操作。")
        return 3
    with open(LEDGER_PATH, "w", encoding="utf-8", newline="\n") as f:
        json.dump(ledger, f, ensure_ascii=False, indent=2)
        f.write("\n")
    chg = f"{old_entry['status']} -> {status}" if old_entry else f"(新) {status}"
    print(f"[marked] {file}: {chg} | round={round_no} findings={findings}")
    print(f"  coverage: reviewed={s['reviewed']} partial={s['partial']} "
          f"unreviewed={s['unreviewed']} => {s['coverage_pct']}%")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="复审覆盖账本工具")
    ap.add_argument("command", choices=["scan", "graph", "verify", "summary", "mark"])
    ap.add_argument("--file", help="mark: 目标文件（相对仓库根，如 src/core/xxx/methods.rs）")
    ap.add_argument("--status", help="mark: reviewed|partial|unreviewed")
    ap.add_argument("--round", type=int, default=3, help="mark: 复审轮次（默认 3）")
    ap.add_argument("--findings", default="", help="mark: 逗号分隔的问题编号，如 P2-26,P3-19")
    ap.add_argument("--note", default="", help="mark: 审读范围说明")
    args = ap.parse_args()
    if args.command == "mark":
        if not args.file or not args.status:
            print("[fail] mark 需要 --file 与 --status")
            return 2
        findings = [x.strip() for x in args.findings.split(",") if x.strip()]
        return cmd_mark(args.file, args.status, args.round, findings, args.note)
    return {"scan": cmd_scan, "graph": cmd_graph, "verify": cmd_verify, "summary": cmd_summary}[args.command]()


if __name__ == "__main__":
    sys.exit(main())
