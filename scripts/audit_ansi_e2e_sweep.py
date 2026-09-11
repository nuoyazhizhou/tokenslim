# scripts/audit_ansi_e2e_sweep.py
# ANSI 剥壳端到端巡检脚本：把含 ANSI 控制码/裸 CSI 残留的 sample 文件喂给 run 命令，
# 走真实压缩链路（run --input + --audit-jsonl），断言剥净、coverage、audit 落盘三件事。
#
# 背景：
#   以往单元测试直接把文本喂给 plugin 的压缩函数，只覆盖"函数级"行为，发现不了
#   CLI 解析 → 流水线 → 插件调度 → 审计落盘 这条链路里的接线缺陷。本脚本用
#   `tokenslim run --input <file> --audit-jsonl <tmp>` 把物理 sample 喂进完整 run 链路，
#   以 `--audit-jsonl` 拉取遥测（ansi_strip_bytes_removed / coverage / plugin_effects）
#   做红灯断言。
#
# 判定规则（对每个含 ANSI/裸码的 sample）：
#   - run 进程退出码 == 0                              → 报告 ok / 命令失败
#   - audit JSONL 文件生成且至少 1 行                   → audit 落盘
#   - 输出文本不含真实 ESC 字节 (\x1b)                 → 真 ANSI 剥净
#   - 输入含裸码时 ansi_strip_bytes_removed > 0        → 裸 CSI 剥壳生效（红灯）
#   - coverage > 0                                    → 专用插件真实参与压缩（红灯）
#
# 用法：
#   python scripts/audit_ansi_e2e_sweep.py                                # 全量巡检
#   python scripts/audit_ansi_e2e_sweep.py --bin target/debug/tokenslim   # 指定二进制
#   python scripts/audit_ansi_e2e_sweep.py --out docs/audit/ansi_e2e.json # 落盘 JSON 汇总
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

# run --input 仍依据「命令程序名」路由到专用插件（与真实 `tokenslim run git status`
# 无差别）。巡检必须传对命令名，否则所有样本回退 generic_text 使 coverage=0，
# 无法验证专用插件是否真实参与。故按插件目录名推断其代表命令。
# 命令类插件 → 真实命令名；纯内容类插件（无对应可执行命令）→ run 场景不适用，
# coverage 不作为红灯，只做提示。
#
# requires_coverage：是否要求 coverage>0。构建/错误类插件会真实折叠噪音行（字节显著
# 缩减），要求 coverage>0 才合理；VCS 插件是「保留型」——把 K-V 行符号化、折叠空行，
# 但字节量不见得显著减少，故 VCS 类不要求 coverage，只要求剥净+落盘。
COMMANDS_BY_PLUGIN = {
    # 命令一律用 tuple/list 表达（字符串会在 list() 时被拆成单字符，导致 run 收不到正确命令）
    "rust_go_plugin": ("cargo", True),
    "bazel_plugin": ("bazel", True),
    "pulumi_plugin": ("pulumi", True),
    "terraform_plugin": ("terraform", True),
    "vcs_az_plugin": (("az", "repos", "show"), False),
    "vcs_gerrit_plugin": (("git", "review"), False),
}
# 这些插件族的样本是内容兜底/全局清理，无对应可执行命令，run 场景下不要求 coverage
NO_RUN_COVERAGE_PLUGINS = {"ansi_cleaner_plugin", "generic_text_plugin"}


def _as_cmd_tokens(cmd):
    """把命令表达式统一转成 token 列表：字符串按空白切分，tuple 直接转 list。"""
    if isinstance(cmd, str):
        return cmd.split()
    return list(cmd)


def infer_command_for_sample(sample: Path):
    """依据 sample 所处插件目录推断真实命令名，返回 (命令 token 列表, 是否要求 coverage)。"""
    for plugin_dir, (cmd, requires_coverage) in COMMANDS_BY_PLUGIN.items():
        if plugin_dir in sample.parts:
            return _as_cmd_tokens(cmd), requires_coverage
    for plugin_dir in NO_RUN_COVERAGE_PLUGINS:
        if plugin_dir in sample.parts:
            return ["cat"], False
    return ["cat"], False


# 真实 ANSI 控制序列：ESC [ ... m/c/n
ESC = "\x1b"
ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
# 裸 CSI 残留（脱色后的垃圾，不含 ESC）：如 [1m、[91m、[36m
NAKED_CSI_RE = re.compile(r"\[[0-9]{1,3}m")
# 非法转义残留：如 [1m 前面带 \ 或普通字符（易误报，仅提示级）


def sample_has_ansi(path: Path) -> bool:
    """判断 sample 是否含真 ANSI 控制码（含真实 ESC 字节 \x1b）。"""
    try:
        data = path.read_bytes()
    except OSError:
        return False
    return b"\x1b[" in data


def sample_has_literal_escape_text(path: Path) -> bool:
    """判断 sample 是否含「字面 \u001b 转义文本」而非真实 ESC 字节。

    JSON/字符串模板样本常把 ESC 写成 6 个字符的 `\u001b`（反斜杠+u001b），
    这类内容不是真实控制码，剥壳/插件都不会对它产生有意义的压缩，
    不能作为 ANSI 链路的巡检目标（会误报 coverage=0）。"""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    return "\\u001b" in text or "\\u001B" in text


def sample_has_naked_csi(path: Path) -> bool:
    """判断 sample 是否含裸 CSI 残留（无 ESC，脱色后留下的 [NNm 垃圾）。

    排除紧跟在字面 `\u001b` 之后的 `[NNm`（那是转义文本的组成部分，不是独立裸码）；
    去掉所有 `\u001b` 前缀后若仍有 `[NNm`，才是真正的裸码残留。"""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    cleaned = re.sub(r"\\u001[Bb]", "", text)
    return bool(NAKED_CSI_RE.search(cleaned))


def run_one(bin_path: Path, sample: Path, audit_path: Path, cmd_tokens: list):
    """对单个 sample 执行 run --input + --audit-jsonl，返回 (退出码, stdout 文本, audit 事件或 None)。"""
    if audit_path.exists():
        audit_path.unlink()
    cmd = [
        str(bin_path),
        "run",
        "--input",
        str(sample),
        "--audit-jsonl",
        str(audit_path),
    ] + cmd_tokens
    proc = subprocess.run(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    stdout = proc.stdout or ""
    event = None
    if audit_path.exists() and audit_path.stat().st_size > 0:
        for line in audit_path.read_text(encoding="utf-8", errors="replace").splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(event, dict) and event.get("event") == "compression":
                break
    return proc.returncode, stdout, event


def main() -> int:
    parser = argparse.ArgumentParser(description="ANSI 剥壳端到端巡检")
    parser.add_argument(
        "--bin", default=None, help="tokenslim 二进制路径（默认自动探测 target/debug）"
    )
    parser.add_argument(
        "--samples", default=None, help="samples 根目录（默认仓库根下的 samples/）"
    )
    parser.add_argument("--out", default=None, help="汇总 JSON 落盘路径")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    bin_path = Path(args.bin) if args.bin else root / "target" / "debug" / "tokenslim"
    if not bin_path.exists():
        # Windows 下追加 .exe 重试
        bin_path = Path(str(bin_path) + ".exe")
    if not bin_path.exists():
        print(f"找不到 tokenslim 二进制: {bin_path}", file=sys.stderr)
        return 2
    samples_dir = Path(args.samples) if args.samples else root / "samples"
    if not samples_dir.is_dir():
        print(f"samples 目录不存在: {samples_dir}", file=sys.stderr)
        return 2

    # 收集所有物理 sample（*.log 为主，排除 scenario.yaml）
    candidates = sorted(samples_dir.rglob("*.log"))
    targets = [s for s in candidates if sample_has_ansi(s) or sample_has_naked_csi(s)]
    # 排除「字面 \u001b 转义文本」样本——它们不含真实 ESC/裸码，演练不了 ANSI 链路
    escaped_text = [s for s in candidates if sample_has_literal_escape_text(s)]
    targets = [s for s in targets if not sample_has_literal_escape_text(s)]
    if escaped_text:
        print(f"跳过 {len(escaped_text)} 个含字面 \\u001b 转义文本的样本（非真实 ANSI，无法演练剥壳链路）:")
        for s in escaped_text:
            print(f"  - {s.relative_to(root).as_posix()}")
    if not targets:
        print("未找到任何含真实 ANSI/裸码的 sample，无事可巡检。")
        return 0

    tmp_dir = root / "target" / "ansi_e2e_sweep_tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)

    results = []
    failed = 0
    for idx, sample in enumerate(targets, 1):
        audit_path = tmp_dir / f"{sample.stem}.jsonl"
        has_naked = sample_has_naked_csi(sample)
        has_true = sample_has_ansi(sample)
        cmd_tokens, requires_coverage = infer_command_for_sample(sample)
        rc, stdout, event = run_one(bin_path, sample, audit_path, cmd_tokens)

        record = {
            "sample": str(sample.relative_to(root)).replace("\\", "/"),
            "has_true_ansi": has_true,
            "has_naked_csi": has_naked,
            "cmd": " ".join(cmd_tokens),
            "requires_coverage": requires_coverage,
            "exit_code": rc,
            "rc_ok": rc == 0,
            "audit_written": audit_path.exists() and audit_path.stat().st_size > 0,
            "ansi_strip_bytes_removed": (
                event.get("ansi_strip_bytes_removed", 0) if event else 0
            ),
            "coverage": round(float(event.get("coverage", 0.0)), 4) if event else 0.0,
            "plugin_effects": (
                [
                    {"plugin_id": p.get("plugin_id"), "changed": p.get("changed")}
                    for p in event.get("plugin_effects", [])
                ]
                if event
                else []
            ),
        }

        # 红灯断言
        violations = []
        if rc != 0:
            violations.append("run 退出码非零")
        if not record["audit_written"]:
            violations.append("audit 未落盘")
        if ANSI_RE.search(stdout):
            violations.append("输出含真实 ANSI 控制码")
        if has_naked and record["ansi_strip_bytes_removed"] <= 0:
            violations.append("含裸码但 ansi_strip_bytes_removed=0（剥壳失效）")
        if requires_coverage and record["coverage"] <= 0.0:
            violations.append("coverage=0（专用插件未真实参与）")

        record["violations"] = violations
        record["status"] = "FAIL" if violations else "ok"
        if violations:
            failed += 1
        results.append(record)

        mark = "FAIL" if violations else "ok  "
        reason = "; ".join(violations) if violations else "剥净+coverage+落盘全部通过"
        print(f"[{mark}] {record['sample']} (cmd={' '.join(cmd_tokens)}, strip={record['ansi_strip_bytes_removed']}, cov={record['coverage']}) {reason}")

    summary = {
        "bin": str(bin_path),
        "samples_scanned": len(candidates),
        "samples_with_ansi": len(targets),
        "passed": len(results) - failed,
        "failed": failed,
        "results": results,
    }
    if args.out:
        out_path = Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(
            json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(f"\n汇总已写入: {out_path}")

    print(f"\n巡检完成: {len(targets)} 个含 ANSI/裸码 sample，通过 {summary['passed']}，失败 {summary['failed']}。")
    # 清理临时 audit 文件
    for p in tmp_dir.iterdir():
        try:
            p.unlink()
        except OSError:
            pass
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())