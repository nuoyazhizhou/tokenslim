# 扫描 CLI「UI 渲染函数」内的硬编码用户可见文案，作为 i18n 门禁的补充。
#
# 与 audit_messages_coverage.py 的分工：
#   - audit_messages_coverage.py：查「某 locale 相对参考语言缺键」（词表内部一致性）
#   - 本脚本：查「UI 渲染代码写了硬编码文案却没建键」（代码↔词表的接线缺口）
#   4 处历史缺陷（cli_desc_config/cli_desc_serve_static 缺键、
#   render_serve_static_usage 整函数零 i18n、render_config_usage 正文、
#   [tokenslim] 压缩无收益）正是从后者这个缝里漏过去的。
#
# 口径（为什么只扫 UI 渲染函数）：
#   全仓中文串扫描会产生大量误报 —— VCS 插件的正则字面量、压缩产物标记、
#   测试断言消息、日志模板都会命中，且无法靠正则可靠区分「用户文案」与
#   「匹配中文输出的正则」。因此本脚本只扫**输出直达终端用户的函数**：
#     - fn render_*    （用法帮助 / 报告渲染）
#     - fn print_*usage*
#   在这些函数体内，任何含 CJK 且未走 t()/t1()/t2()/t3() 的字符串字面量
#   即为缺口候选。
#
# 豁免规则：
#   a. 压缩产物协议标记（[MARKER] / $ 前缀）—— 冻结基线哈希组成，故意硬编码。
#   b. #[cfg(test)] / test.rs / tests.rs 内的函数 —— 开发者可见断言消息。
#   c. 双语渲染器自身（src/utils/i18n.rs 的 render_terminal）—— i18n 机制本体。
#   d. 基线清单（scripts/hardcoded_i18n_baseline.json）中已登记的存量条目：
#      门禁以「基线冻结 + 防增量」运行 —— 存量不阻塞，新增即失败。
#      清理存量后请运行 --update-baseline 收敛基线。
#
# 用法：
#   python scripts/audit_hardcoded_i18n.py                  # 只报新增（相对基线）
#   python scripts/audit_hardcoded_i18n.py --strict         # 有新增则非 0 退出
#   python scripts/audit_hardcoded_i18n.py --all            # 显示全部命中（含基线）
#   python scripts/audit_hardcoded_i18n.py --update-baseline
#   python scripts/audit_hardcoded_i18n.py --report docs/audit/hardcoded_i18n.md
import argparse
import json
import re
import sys
from pathlib import Path

# CJK 统一表意文字（含扩展 A）—— 主判据
CJK_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff]")

# UI 渲染函数：输出直达终端用户
UI_FN_RE = re.compile(r"\bfn\s+((?:render|print)_[A-Za-z0-9_]*)\s*\(")

# 测试函数命名（生产文件内的 #[test] 常以 render_*_returns_* 等命名）：
# 这些函数名形似 UI 渲染，实为断言用例，需排除。
TEST_FN_HINT_RE = re.compile(
    r"(?:^|_)(?:returns|renders|should|asserts|uses|maps|numbers|skips|"
    r"reports|filters|requires|detects|handles|test|when|given)(?:_|$)"
)

# 已走 i18n 的行（含 t/t1/t2/t3/t_en/t_zh/t_dynamic 调用）
I18N_CALL_RE = re.compile(r"\bt(?:_en|_zh|_dynamic|1|2|3)?\s*\(")
# 其它已本地化来源
OTHER_I18N_RE = re.compile(r"\b(UserFacingMessage|format_invalid_args_message|t_for_lang)\b")

# 压缩产物协议标记
MARKER_RE = re.compile(r"\[[A-Za-z!][A-Za-z0-9_|!\-]*\]")
DOLLAR_RE = re.compile(r"\$[A-Za-z_]+")

# 内部开发 / 运维工具的 UI 渲染函数豁免。
# 这些命令面向项目维护者（分类器特征库治理、盲测基线诊断），
# 不属终端用户日常使用的产品面；其输出固定中文不构成 i18n 缺口。
# 注意：tokenslim-server、gain、run、config 等**不在此列** —— 它们是产品面。
INTERNAL_TOOL_FILES = (
    "src/bin/classifier_holdout.rs",
    "src/bin/corpus_profile.rs",
    "src/cli/commands/feature.rs",
)


def is_internal_tool(path: Path) -> bool:
    return path.as_posix() in INTERNAL_TOOL_FILES

# 字符串字面量：raw string 需 r 前缀
RAW_STRING_RE = re.compile(r'r(#+)?"(.*?)"\1', re.DOTALL)


def iter_string_literals(text: str):
    """顺序扫描字符串字面量，产出 (offset, literal)。

    不用负向后顾 —— 多行上下文中前一行以 `"` 结尾会让 (?<!") 误失配，
    曾导致漏报（反测已证实）。改为顺序状态机，能正确处理转义引号。
    """
    raw_spans = [(m.start(), m.end()) for m in RAW_STRING_RE.finditer(text)]
    for m in RAW_STRING_RE.finditer(text):
        yield m.start(), m.group(2)

    def in_raw(pos: int) -> bool:
        return any(a <= pos < b for a, b in raw_spans)

    i = 0
    n = len(text)
    while i < n:
        if text[i] != '"' or in_raw(i):
            i += 1
            continue
        j = i + 1
        buf = []
        while j < n:
            ch = text[j]
            if ch == "\\" and j + 1 < n:
                buf.append(text[j : j + 2])
                j += 2
                continue
            if ch == '"':
                break
            buf.append(ch)
            j += 1
        if j >= n:
            break
        yield i, "".join(buf)
        i = j + 1


def line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def cfg_test_ranges(text: str):
    """`#[cfg(test)] mod tests { ... }` 覆盖的行号区间（1-based）。"""
    ranges = []
    for m in re.finditer(r"#\[cfg\(test\)\]", text):
        start_line = line_of(text, m.start())
        brace = text.find("{", m.end())
        if brace == -1:
            continue
        depth = 0
        i = brace
        while i < len(text):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        ranges.append((start_line, line_of(text, i)))
    return ranges


def fn_body_ranges(text: str):
    """产出 UI 渲染函数体区间 (start_line, end_line, name)，跳过测试函数。"""
    out = []
    for m in UI_FN_RE.finditer(text):
        name = m.group(1)
        if TEST_FN_HINT_RE.search(name):
            continue
        brace = text.find("{", m.end())
        if brace == -1:
            continue
        depth = 0
        i = brace
        while i < len(text):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        out.append((line_of(text, m.start()), line_of(text, i), name))
    return out


def scan_file(path: Path, skip_i18n_internal: bool):
    """扫描单文件，返回 [(line, fn_name, snippet)]。"""
    try:
        text = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return []

    hits = []
    test_ranges = cfg_test_ranges(text)

    def in_test(lineno: int) -> bool:
        return any(a <= lineno <= b for a, b in test_ranges)

    for start, end, fname in fn_body_ranges(text):
        # 逐行检查该函数体
        lines = text.split("\n")
        for lineno in range(start, min(end, len(lines)) + 1):
            if in_test(lineno):
                continue
            line = lines[lineno - 1]
            if "//" in line:
                head = line.split("//", 1)[0]
            else:
                head = line
            if not CJK_RE.search(head):
                continue
            if I18N_CALL_RE.search(head) or OTHER_I18N_RE.search(head):
                continue
            # 取该行所有字面量，确认其中确实含 CJK（排除仅注释含 CJK 的行）
            has_cjk_literal = False
            for lit in re.findall(r'"((?:\\.|[^"\\])*)"', head):
                if CJK_RE.search(lit):
                    has_cjk_literal = True
                    break
            if not has_cjk_literal:
                continue
            hits.append((lineno, fname, head.strip()[:100]))
    return hits


def load_baseline(path: Path):
    if not path.exists():
        return set()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return set()
    return {(e["file"], e["line"], e["text"]) for e in data.get("entries", [])}


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Audit hardcoded user-facing text inside CLI UI render functions"
    )
    parser.add_argument("--src", default="src", help="Source root (default: src)")
    parser.add_argument(
        "--baseline",
        default="scripts/hardcoded_i18n_baseline.json",
        help="Baseline file path",
    )
    parser.add_argument(
        "--strict", action="store_true", help="Exit non-zero when new hits are found"
    )
    parser.add_argument(
        "--all", action="store_true", help="Show every hit, including baselined ones"
    )
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Rewrite the baseline from the current scan result",
    )
    parser.add_argument("--report", default=None, help="Optional markdown report path")
    args = parser.parse_args()

    root = Path(args.src)
    if not root.exists():
        print(f"source root not found: {root}", file=sys.stderr)
        return 2

    all_hits = []
    scanned = 0
    skipped_internal = 0
    for path in sorted(root.rglob("*.rs")):
        posix = path.as_posix()
        if path.name in ("test.rs", "tests.rs") or "tests" in path.parts:
            continue
        # 双语渲染器本体豁免（i18n 机制自身）
        if posix.endswith("src/utils/i18n.rs"):
            continue
        if is_internal_tool(path):
            skipped_internal += 1
            continue
        scanned += 1
        for lineno, fname, snippet in scan_file(path, True):
            if MARKER_RE.search(snippet) or DOLLAR_RE.search(snippet):
                continue
            all_hits.append({"file": posix, "line": lineno, "fn": fname, "text": snippet})

    baseline_path = Path(args.baseline)
    baseline = load_baseline(baseline_path)

    if args.update_baseline:
        baseline_path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "_comment": (
                "存量硬编码 UI 文案基线。门禁以「基线冻结 + 防增量」运行："
                "存量不阻塞，新增即失败。清理存量后运行 --update-baseline 收敛。"
            ),
            "count": len(all_hits),
            "entries": all_hits,
        }
        baseline_path.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        print(f"baseline updated: {baseline_path} ({len(all_hits)} entries)")
        return 0

    new_hits = [
        h for h in all_hits if (h["file"], h["line"], h["text"]) not in baseline
    ]

    print(f"scanned files: {scanned}")
    print(f"skipped internal tools: {skipped_internal}")
    print(f"total hits: {len(all_hits)}")
    print(f"baselined hits: {len(all_hits) - len(new_hits)}")
    print(f"new hits: {len(new_hits)}")

    shown = all_hits if args.all else new_hits
    for h in shown:
        print(f"  {h['file']}:{h['line']} [{h['fn']}] {h['text']}")

    if args.report:
        rp = Path(args.report)
        rp.parent.mkdir(parents=True, exist_ok=True)
        lines = [
            "# 硬编码 UI 文案审计（UI 渲染函数口径）",
            "",
            f"扫描文件数：{scanned}",
            f"命中总数：{len(all_hits)}",
            f"基线内：{len(all_hits) - len(new_hits)}",
            f"新增：{len(new_hits)}",
            "",
            "| 文件 | 行 | 函数 | 片段 |",
            "|---|---|---|---|",
        ]
        for h in shown:
            safe = h["text"].replace("|", "\\|")
            lines.append(f"| `{h['file']}` | {h['line']} | `{h['fn']}` | {safe} |")
        rp.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"report written: {rp}")

    if new_hits:
        print("RESULT=FAIL" if args.strict else "RESULT=WARN")
        return 1 if args.strict else 0
    print("RESULT=PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
