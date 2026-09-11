# scripts/audit_classifier_e2e_sweep.py
# 贝叶斯分类器（内容语义路由）链路巡检脚本：验证 T-C（quick_analyze 兜底分类）
# 与 T-D（dispatch_slice 候选插件路由优先级）在真实 `run --input` 全链路上的表现。
#
# 背景：
#   自研朴素贝叶斯分类器将 cargo/gcc/test/git_diff 等构建/编译/测试输出归入语义类别，
#   并通过 `candidate_plugins_for_slice` 把候选插件注入 dispatch 提升专用插件路由优先级，
#   避免其落到 generic_text/smart_path 兜底。本脚本用 `tokenslim run --input <file>`
#   把物理样本喂进完整 run 链路，从 `--audit-jsonl` 拉取 `plugin_effects` 判定：
#   强语义信号样本是否被路由到预期专用插件（而非 generic_text/smart_path 兜底）。
#
# 断言基准（红灯）：
#   - 对具备强语义信号（插件 detect 可命中，conf>0.1）的样本：
#       · audit 落盘
#       · 主导插件 ∈ 预期专用插件集（非 generic_text / smart_path 兜底）
#   - 输出剥净真实 ANSI 控制码（不残留 \x1b）
#
# 说明/已知边界（不是失败条件，仅记录）：
#   - `quick_skip` 阈值：文本 <1000 字节且无关键字时直接 passthrough，不进入 dispatch，
#     故短样本测不到分类器——本脚本聚焦长文本/强信号样本。
#   - 贝叶斯候选只对 detect_parallel 已命中（conf>0.1）的候选做排序提升，无法把
#     候选插件本身加入候选集；弱信号（纯 compiling 无 error）样本落在 smart_path 兜底
#     是引擎预期边界，不判红。
#
# 用法：
#   python scripts/audit_classifier_e2e_sweep.py                                # 全量巡检
#   python scripts/audit_classifier_e2e_sweep.py --bin target/debug/tokenslim
#   python scripts/audit_classifier_e2e_sweep.py --out docs/audit/classifier_e2e.json
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

# 四类语义类别 →（插件目录, run 占位命令 token, 预期专用插件集）。
# 断言主导插件 ∈ 预期集；非这些即视为落兜底（generic_text/smart_path）缺陷。
# run 模式需要命令名占位，命令名不影响分类器链路（分类器按文本内容兜底）。
CLASSIFIERS = {
    "cargo": ("rust_go_plugin", ["cargo", "build"], {"rust_go"}),
    "gcc": ("gcc_log_plugin", ["gcc", "-c"], {"gcc_log"}),
    "test": ("pytest_plugin", ["pytest", "-q"], {"pytest"}),
    "git_diff": ("git_diff_plugin", ["git", "diff"], {"git_diff"}),
}

# 兜底插件名：出现即视为路由塌方（红灯）。
FALLBACK_PLUGINS = {"generic_text", "smart_path"}

# 设计预期的「接管插件」白名单：样本名 `looks_*_but_*`（故意跨类别）或 merge conflict 归 vcs
# 等场景会被这些专用插件接管，属正确行为，断言不判红。
KNOWN_OVERRIDES = {"vcs", "ci_log", "java_stack", "gcc_log", "noise_filter", "python_traceback"}

ESC = "\x1b"
ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")


def main() -> int:
    parser = argparse.ArgumentParser(description="贝叶斯分类器链路巡检")
    parser.add_argument("--bin", default=None, help="tokenslim 二进制路径")
    parser.add_argument("--samples", default=None, help="samples 根目录")
    parser.add_argument("--out", default=None, help="汇总 JSON 落盘路径")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    bin_path = Path(args.bin) if args.bin else root / "target" / "debug" / "tokenslim"
    if not bin_path.exists():
        bin_path = Path(str(bin_path) + ".exe")
    if not bin_path.exists():
        print(f"找不到 tokenslim 二进制: {bin_path}", file=sys.stderr)
        return 2
    samples_dir = Path(args.samples) if args.samples else root / "samples"
    if not samples_dir.is_dir():
        print(f"samples 目录不存在: {samples_dir}", file=sys.stderr)
        return 2

    tmp_dir = root / "target" / "classifier_e2e_sweep_tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)

    results = []
    failed = 0
    scanned = 0

    for cat, (plugin_dir, cmd_tokens, expected_plugins) in CLASSIFIERS.items():
        pdir = samples_dir / plugin_dir
        if not pdir.is_dir():
            print(f"[skip] 类别 {cat}: 目录不存在 {pdir}")
            continue
        # 收集该插件目录下所有命令输出样本（*.log，排除 scenario 元数据）。
        samples = sorted(pdir.glob("*.log"))
        for sample in samples:
            scanned += 1
            audit_path = tmp_dir / f"{sample.stem}.jsonl"
            if audit_path.exists():
                audit_path.unlink()
            cmd = [
                str(bin_path), "run", "--input", str(sample),
                "--audit-jsonl", str(audit_path),
            ] + cmd_tokens
            proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            stdout = proc.stdout or ""

            event = None
            if audit_path.exists() and audit_path.stat().st_size > 0:
                for line in audit_path.read_text(encoding="utf-8", errors="replace").splitlines():
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        ev = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    if isinstance(ev, dict) and ev.get("event") == "compression":
                        event = ev
                        break

            plugin_effects = []
            if event:
                for p in event.get("plugin_effects", []):
                    plugin_effects.append({
                        "plugin_id": p.get("plugin_id"),
                        "changed": p.get("changed", False),
                    })
            # dominant = 执行链里「预期专用插件或已知接管插件」中第一个执行的插件（路由归属，
            #  反映 conf 降序调度优先级）；无则取 `changed:true` 首个；再退化为最后一个参与插件。
            # 不单看 changed:true：专用插件对不可压缩的 tiny 报错块常 changed:false，而 smart_path
            #  路径替换对其产生 1B 噪声 changed:true，误判会让路由归到兜底。
            dominant = None
            for p in plugin_effects:
                if p["plugin_id"] in expected_plugins or p["plugin_id"] in KNOWN_OVERRIDES:
                    dominant = p["plugin_id"]
                    break
            if dominant is None:
                for p in plugin_effects:
                    if p.get("changed"):
                        dominant = p["plugin_id"]
                        break
            if dominant is None and plugin_effects:
                dominant = plugin_effects[-1]["plugin_id"]

            coverage = round(float(event.get("coverage", 0.0)), 4) if event else 0.0
            raw_text = sample.read_text(encoding="utf-8", errors="replace").strip()

            violations = []
            if proc.returncode != 0:
                violations.append("run 退出码非零")
            if not (audit_path.exists() and audit_path.stat().st_size > 0) and raw_text:
                # 空/纯空白样例（空 diff、空日志）本就不产生压缩事件、不落盘，属预期不判红。
                violations.append("audit 未落盘")
            if ANSI_RE.search(stdout):
                violations.append("输出含真实 ANSI 控制码")
            if coverage > 0.0 and dominant is None:
                # 真正发生了字节缩减（专用插件确实参与）却没有 changed 主导插件 → 异常。
                violations.append("coverage>0 但无 changed 主导插件")
            elif coverage > 0.0 and dominant is not None and dominant in FALLBACK_PLUGINS:
                # 主导插件落在兜底栈（generic_text/smart_path）→ 路由塌方红灯。
                # 说明分类器/插件检测都未能把本次文本路由到专用插件。
                # coverage==0（透传/无压缩负样本、tiny 不可压缩块）时不判红：专用插件已接管
                # 但无可压缩量，兜底仅做透传，非路由塌方。
                violations.append(f"主导插件 {dominant} 落兜底（分类器未路由到专用插件）")
            elif dominant is not None and coverage > 0.0 and (
                dominant not in expected_plugins and dominant not in KNOWN_OVERRIDES
            ):
                # 被其他专用插件接管：一类是样本名即 `looks_*_but_*`（故意跨类别），
                # 一类是 merge conflict 归 vcs——属设计预期，仅提示不判红。
                # coverage==0（透传/无压缩负样本）不判红：负样本的「透传能力」目标本就不要求
                # 落到专用插件，generic_text 原样保留即正确结果。
                violations.append(
                    f"主导插件 {dominant} 不在预期 {sorted(expected_plugins)}"
                )

            record = {
                "category": cat,
                "sample": str(sample.relative_to(root)).replace("\\", "/"),
                "plugin_expected": sorted(expected_plugins),
                "dominant_plugin": dominant,
                "plugin_effects": plugin_effects,
                "coverage": coverage,
                "ansi_strip_bytes_removed": (
                    event.get("ansi_strip_bytes_removed", 0) if event else 0
                ),
                "violations": violations,
                "status": "FAIL" if violations else "ok",
            }
            if violations:
                failed += 1
            results.append(record)

            mark = "FAIL" if violations else "ok  "
            reason = "; ".join(violations) if violations else "路由到预期专用插件"
            print(
                f"[{mark}] {record['sample']} ({cat}) -> {dominant} "
                f"cov={record['coverage']} {reason}"
            )

        # 清理本次类别的临时 audit 文件
        for p in tmp_dir.iterdir():
            try:
                p.unlink()
            except OSError:
                pass

    summary = {
        "bin": str(bin_path),
        "samples_scanned": scanned,
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

    print(
        f"\n分类器链路巡检完成: {scanned} 个样本，通过 {summary['passed']}，"
        f"失败 {summary['failed']}。"
    )
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())