#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
fill_case_sidecars.py
=====================

调 LLM 为 ``samples/<plugin>/`` 下的 case 填充 sidecar 的 scenario 字段。

⚠️  重要前提（fix #31）
----------------------
- 原设计："LLM 不得自动填 scenario 字段 — 大规模 LLM 推导 = 幻觉"
- 新设计：用户判断"空 sidecar = 设计落空"，**接受 LLM 自动填**，但要
    1. 试点 1 plugin → 抽检 → 决定是否推广
    2. 每 case 单独 LLM call，prompt 含 case 内容 + showcase title + 插件能力
    3. 写回时**不覆盖**已有 scenario（人类编辑优先），**不破坏** audit 块
    4. 失败 case 留空 + 记录，不污染产物
    5. 跑完 audit 用 LLM 二次验证 scenario 与 case 一致性，给出幻觉率

用法：
    # 1) dry-run：只打 prompt（5 case），看 prompt 质量
    python scripts/fill_case_sidecars.py --plugin shell_session_plugin --limit 5 --dry-run

    # 2) 真跑 5 case，看 LLM 输出质量
    python scripts/fill_case_sidecars.py --plugin shell_session_plugin --limit 5

    # 3) 批量跑 80 case
    python scripts/fill_case_sidecars.py --plugin shell_session_plugin

    # 4) 跨 plugin
    python scripts/fill_case_sidecars.py --all --limit 50

    # 5) 强制覆盖已有 scenario
    python scripts/fill_case_sidecars.py --plugin shell_session_plugin --force

副作用：
    - 改 samples/<plugin>/case_NNN_*.scenario.yaml 的 scenario/target_capability/
      expected_keep/expected_compress/source 字段
    - 写回时 audit 块**不动**
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import sys
import time
from typing import Any, Dict, List, Optional, Tuple

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPT_DIR)

import audit_llm_common as _alc  # noqa: E402

# ============================================================================
# 字段 schema
# ============================================================================
#
# scenario:           一句话（10-200 字）描述本 case 的真实场景
# target_capability:  本 case 在测插件的哪个能力（短标签，< 30 字）
# expected_keep:      压缩后期望被保留的行/模式（可空，但建议非空）
# expected_compress:  压缩后期望被压缩或丢弃的行/模式（可空，但建议非空）
# source:             来源（llm / manual / imported）
# llm_filled_at:      LLM 填写时间戳（ISO8601）
#
# 写回时不动 audit 块（content_hash / final_status / llm_invoked /
# llm_verified_at / last_audit_tool / skip），那是 audit_sample_case_quality.py
# 自动维护的。

CONTENT_PREVIEW_CHARS = 2000  # 截断 case 内容避免 prompt 过长
MIN_SCENARIO_LEN = 8          # scenario 至少 8 字符
MAX_SCENARIO_LEN = 300
MIN_TARGET_CAP_LEN = 2
MAX_TARGET_CAP_LEN = 60


# ============================================================================
# System prompt — 引导 LLM 进入"插件测试场景描述者"角色
# ============================================================================
FILL_SYSTEM_PROMPT = """你是 TokenSlim 项目的测试场景审计员。

TokenSlim 是一个 shell / 日志 / 数据结构压缩工具，每个 plugin 负责一种
"原始文本 → 紧凑摘要"的转换。"case"就是被 plugin 处理的原始文本样本。

你的任务：根据一个 case 文件的**实际内容** + showcase.rs 中注册时的 title
+ 插件能力描述，**用一句话**写出这个 case 的真实场景描述。

要求：
- scenario 字段：10-200 字中文（或英文），必须**基于 case 实际内容**，
  不能凭插件名脑补
- target_capability 字段：短标签（< 30 字），说明本 case 测的是
  "错误保留"/"路径字典压缩"/"命令族覆盖"/"信号锚点"等哪个能力
- expected_keep 字段：压缩后期望被保留的 1-3 行/模式关键词（看 case 实际有什么）
- expected_compress 字段：压缩后期望被压缩或丢弃的 1-3 行/模式关键词
- expected_dispatch_chain 字段（**重要**）：
    - 如果 case 是 "在 shell 中执行外部命令"（如 bash 里跑 git status、
      PowerShell 里跑 cargo test、CMD 里跑 mvn clean install），
      这种 case 期望**二段 dispatch 链**：
        1) host plugin（shell_session / ps_session / cmd_session）先处理
           prompt/行长度熵/错误锚点
        2) 检测到外部命令（git/cargo/kubectl/mvn/docker/npm/pytest/...）
           → 让渡到 specialist 插件处理命令输出
      此时写：["<host>", "<specialist>"]，例如 ["shell_session_plugin", "vcs_git_plugin"]
    - 如果 case 不涉及外部命令（纯 shell 内置命令如 echo/ls/cd/grep），
      字段留空字符串 ""
    - **反幻觉**：只填你能在 case 内容里实际看到的命令族，不要猜

**反幻觉纪律**：
- 只能描述 case 文件**实际**展示的内容（命令、错误、输出），不要脑补
  case 没有的命令
- 不知道就说"无法确定"，不要编
- 不要复述 case 的每一行，1-2 句**场景总结**即可

输出严格 JSON，5 个字段缺一不可（expected_dispatch_chain 允许为空字符串）：
{
  "scenario": "...",
  "target_capability": "...",
  "expected_keep": "...",
  "expected_compress": "...",
  "expected_dispatch_chain": "<host>,<specialist>"  // 单字符串，逗号分隔，无 dispatch 链时留空
}
"""


# ============================================================================
# Case 物理读取 + sidecar 写回
# ============================================================================

def _read_case_content(case_path: str) -> str:
    try:
        with open(case_path, "r", encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError as exc:
        return f"<<read-failed: {exc}>>"


def _truncate(text: str, max_chars: int) -> str:
    if len(text) <= max_chars:
        return text
    half = max_chars // 2
    return text[:half] + f"\n\n... [中间 {len(text) - max_chars} 字符省略] ...\n\n" + text[-half:]


def _load_showcase_titles(ascq_module) -> Dict[str, str]:
    """返回 {case_filename: registered_title}。"""
    try:
        cases = ascq_module.parse_showcase_rs_cases()
    except Exception as exc:
        print(f"  [warn] parse_showcase_rs_cases failed: {exc}", file=sys.stderr)
        return {}
    out: Dict[str, str] = {}
    for c in cases or []:
        fn = c.get("filename", "")
        title = c.get("title", "")
        if fn:
            out[fn] = title
    return out


def _load_existing_sidecar_fields(sc_path: str) -> Dict[str, Any]:
    """读 sidecar 已有的 scenario 字段（用于：人类编辑优先 / 保留 audit 块）。"""
    if not os.path.isfile(sc_path):
        return {}
    try:
        with open(sc_path, "r", encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return {}
    try:
        import yaml  # type: ignore
        parsed = yaml.safe_load(text) or {}
        if isinstance(parsed, dict):
            return parsed
    except ImportError:
        pass
    except Exception:
        pass
    # fallback: 用 ascq 的 _parse_yaml_minimal
    try:
        from importlib import import_module
        # ascq 已经在 import audit_sample_case_quality 时加载过
        import audit_sample_case_quality as ascq
        parsed = ascq._parse_yaml_minimal(text)
        if isinstance(parsed, dict):
            return parsed
    except Exception:
        pass
    return {}


def _sidecar_path(samples_dir: str, plugin: str, case_filename: str) -> str:
    stem = os.path.splitext(case_filename)[0]
    return os.path.join(samples_dir, plugin, f"{stem}.scenario.yaml")


def _write_sidecar_scenario_fields(
    sc_path: str,
    scenario: str,
    target_capability: str,
    expected_keep: str,
    expected_compress: str,
    source: str,
    llm_filled_at: str,
    expected_dispatch_chain: Optional[List[str]] = None,
) -> bool:
    """把 scenario 字段 + source + llm_filled_at 写回 sidecar。

    保留 audit 块（机器字段）。使用 audit_sample_case_quality.py 的
    _dump_yaml_with_audit 工具（它已经实现了"scenario 字段 + audit 块"格式）。
    """
    try:
        import audit_sample_case_quality as ascq
    except ImportError as exc:
        print(f"  [error] import audit_sample_case_quality failed: {exc}", file=sys.stderr)
        return False

    existing = _load_existing_sidecar_fields(sc_path)
    scenario_fields = {
        "scenario": scenario,
        "target_capability": target_capability,
        "expected_keep": expected_keep,
        "expected_compress": expected_compress,
        "source": source,
        "generated_at": existing.get("generated_at", llm_filled_at),  # 保留原 generated_at
    }
    # 期望 dispatch 链：仅当非空时写（避免 YAML 噪声）
    if expected_dispatch_chain:
        scenario_fields["expected_dispatch_chain"] = list(expected_dispatch_chain)
    audit_fields = existing.get("audit", {}) or {}
    # 确保 audit 6 字段都在（不写新值，只保留旧值）
    audit_fields.setdefault("content_hash", "")
    audit_fields.setdefault("final_status", "")
    audit_fields.setdefault("llm_invoked", False)
    audit_fields.setdefault("llm_verified_at", "")
    audit_fields.setdefault("last_audit_tool", "")
    audit_fields.setdefault("skip", False)

    new_text = ascq._dump_yaml_with_audit(scenario_fields, audit_fields)
    try:
        with open(sc_path, "w", encoding="utf-8") as f:
            f.write(new_text)
        return True
    except OSError as exc:
        print(f"  [error] write {sc_path} failed: {exc}", file=sys.stderr)
        return False


# ============================================================================
# 字段验证
# ============================================================================

def _validate_fields(d: Dict[str, Any]) -> Tuple[bool, str]:
    """验证 LLM 返回的 5 字段是否合规。返回 (ok, reason)。

    expected_dispatch_chain 允许为空字符串/空 list（普通单插件 case）。
    """
    s = (d.get("scenario") or "").strip()
    t = (d.get("target_capability") or "").strip()
    _ek = d.get("expected_keep") or ""
    _ec = d.get("expected_compress") or ""
    k = (", ".join(_ek) if isinstance(_ek, list) else _ek).strip()
    c = (", ".join(_ec) if isinstance(_ec, list) else _ec).strip()
    if not (MIN_SCENARIO_LEN <= len(s) <= MAX_SCENARIO_LEN):
        return False, f"scenario 长度 {len(s)} 不在 [{MIN_SCENARIO_LEN},{MAX_SCENARIO_LEN}]"
    if not (MIN_TARGET_CAP_LEN <= len(t) <= MAX_TARGET_CAP_LEN):
        return False, f"target_capability 长度 {len(t)} 不在 [{MIN_TARGET_CAP_LEN},{MAX_TARGET_CAP_LEN}]"
    # scenario 至少包含 1 个 ASCII 字母 / 中文，避免全是标点
    if not re.search(r'[A-Za-z一-鿿]', s):
        return False, "scenario 不含字母/中文"
    # expected_keep / expected_compress 至少有一个非空
    if not k and not c:
        return False, "expected_keep 和 expected_compress 都为空"
    return True, "ok"


def _parse_dispatch_chain(v: Any) -> List[str]:
    """把 LLM 返回的 expected_dispatch_chain 解析成 list。

    接受：
      - "host,specialist"  字符串，逗号分隔
      - ["host", "specialist"]  数组
      - "" / None / []  →  []
    """
    if v is None:
        return []
    if isinstance(v, list):
        return [str(x).strip() for x in v if str(x).strip()]
    s = str(v).strip()
    if not s:
        return []
    # 接受中英文逗号
    parts = re.split(r"[,，;；\s]+", s)
    return [p.strip() for p in parts if p.strip()]


# ============================================================================
# LLM 解析（严格 JSON）
# ============================================================================

def _parse_llm_json(raw: str) -> Optional[Dict[str, Any]]:
    """从 LLM 返回里 extract JSON dict。"""
    if not raw:
        return None
    # 移除 markdown fence
    raw = re.sub(r"^```(?:json)?\s*", "", raw.strip())
    raw = re.sub(r"\s*```$", "", raw.strip())
    # 找 JSON {...}
    m = re.search(r"\{[\s\S]*\}", raw)
    if not m:
        return None
    try:
        return json.loads(m.group(0))
    except (ValueError, TypeError):
        return None


def _build_user_prompt(
    case_filename: str,
    case_content_preview: str,
    showcase_title: str,
    plugin: str,
    plugin_type: str,
    plugin_kb_snippet: str,
) -> str:
    title_line = f"showcase.rs title: {showcase_title}" if showcase_title else "showcase.rs title: <未注册>"
    return f"""请根据下面这个 case 的实际内容，写出场景描述。

plugin: {plugin} (类型: {plugin_type})
{title_line}
filename: {case_filename}

=== case 文件内容（前 {CONTENT_PREVIEW_CHARS} 字符）===
{case_content_preview}
=== 结束 ===

=== 插件能力 KB 摘要 ===
{plugin_kb_snippet}

请输出严格 JSON（4 字段，缺一不可）：
{{
  "scenario": "（10-200 字）",
  "target_capability": "（< 30 字短标签）",
  "expected_keep": "（1-3 行/模式关键词，压缩后期望被保留）",
  "expected_compress": "（1-3 行/模式关键词，压缩后期望被压缩或丢弃）"
}}
"""


# ============================================================================
# 主流程
# ============================================================================

# stderr 镜像 helper：PowerShell 管道经常吞掉 stderr，把 LLM 警告写到独立文件
_orig_stderr_write = sys.stderr.write
_mirror_active_path: Optional[str] = None


def _mirror_stderr(log_path: str) -> None:
    """开启后 sys.stderr.write 同步 append 到 log_path（UTF-8）。幂等。"""
    global _mirror_active_path
    if _mirror_active_path == log_path:
        return
    os.makedirs(os.path.dirname(log_path), exist_ok=True)
    f = open(log_path, "a", encoding="utf-8", buffering=1)  # line-buffered
    def _write(s):
        try:
            f.write(s)
        except Exception:
            pass
        return _orig_stderr_write(s)
    sys.stderr.write = _write
    _mirror_active_path = log_path

def _write_progress_md(
    log_path: str,
    plugin: str,
    scanned: int,
    filled: int,
    skipped_existing: int,
    errors: int,
    recent: List[Dict[str, Any]],
    last_case: str = "",
    last_status: str = "",
) -> None:
    """每次跑完一个 case 立即 update progress.md（轻量版实时反馈）。

    progress.md 设计：
    - 顶部：plugin + 总进度（X/Y filled, errors, skipped）
    - 中部：最近 10 个 case 的 scenario 列表（便于实时抽检）
    - 底部：最近一次错误信息
    """
    try:
        with open(log_path, "w", encoding="utf-8") as f:
            f.write(f"# Fill case sidecars — progress\n\n")
            f.write(f"plugin: `{plugin}`  \n")
            f.write(f"scanned: {scanned}  \n")
            f.write(f"**filled: {filled}** / {scanned}  \n")
            f.write(f"skipped_existing: {skipped_existing}  \n")
            f.write(f"errors: {errors}  \n")
            f.write(f"last_update: {datetime.datetime.now(datetime.timezone.utc).isoformat()}  \n")
            if last_case:
                f.write(f"last_case: `{last_case}` → {last_status}  \n")
            f.write(f"\n## 最近 10 case 的 scenario\n\n")
            if not recent:
                f.write("_（暂无完成 case）_\n")
            else:
                f.write("| # | case | scenario | status |\n")
                f.write("|---|---|---|---|\n")
                for r in recent[-10:]:
                    scen = (r.get("scenario") or "").replace("|", "\\|")[:80]
                    f.write(
                        f"| {r.get('i','?')}/{scanned} "
                        f"| `{r.get('case','?')}` "
                        f"| {scen} "
                        f"| {r.get('status','?')} |\n"
                    )
    except OSError as exc:
        print(f"  [warn] failed to write progress.md: {exc}", file=sys.stderr)


def fill_for_plugin(
    plugin: str,
    *,
    samples_dir: str = "samples",
    limit: Optional[int] = None,
    force: bool = False,
    dry_run: bool = False,
    sleep_between: float = 0.5,
    verbose: bool = True,
) -> Dict[str, Any]:
    """为一个 plugin 填充 case sidecar 的 scenario 字段。

    Returns: {"scanned": N, "filled": M, "skipped_existing": K,
              "skipped_unfilled": U, "errors": E, "duration_s": ...}
    """
    import audit_sample_case_quality as ascq

    # 0) 探测 samples 目录：从 cwd 找不到时回退到脚本同级的 ../samples/
    # 让 user 在 scripts/ 或 c:\git_work\TokenSlim 下都能跑通
    if not os.path.isdir(samples_dir):
        alt = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", samples_dir)
        alt = os.path.abspath(alt)
        if os.path.isdir(alt):
            if verbose:
                print(f"  [path] samples/ not in cwd, falling back to {alt}")
            os.chdir(os.path.dirname(alt))  # chdir 到项目根，docs/audit/ 也对
            samples_dir = os.path.basename(alt)

    # 1) 拿 case 列表（用 _alc.walk_physical_samples 避免重复定义）
    cases = sorted(_alc.walk_physical_samples(samples_dir).get(plugin, []))
    if not cases:
        if verbose:
            print(f"  [skip] {plugin}: no case files in {samples_dir}/{plugin}/")
        return {"scanned": 0, "filled": 0, "skipped_existing": 0,
                "skipped_unfilled": 0, "errors": 0, "duration_s": 0.0}

    if limit:
        cases = cases[:limit]
    if verbose:
        print(f"  [plan] {plugin}: {len(cases)} case(s) to process"
              + (" (dry-run)" if dry_run else ""))

    # 2) 拿 showcase title 索引
    try:
        cases_raw = ascq.parse_showcase_rs_cases(plugin)
    except Exception as exc:
        print(f"  [warn] parse_showcase_rs_cases({plugin}) failed: {exc}", file=sys.stderr)
        cases_raw = None
    showcase_titles = {}
    for c in cases_raw or []:
        fn = c.get("filename", "")
        title = c.get("title", "")
        if fn:
            showcase_titles[fn] = title

    # 3) 拿 LLM config + 插件 KB
    # 自动加载 .env（如果存在）— 让 user 不用每次 source 一下
    _env_path = os.path.join(os.getcwd(), ".env")
    if os.path.isfile(_env_path):
        try:
            with open(_env_path, "r", encoding="utf-8") as ef:
                for line in ef:
                    line = line.strip()
                    if not line or line.startswith("#") or "=" not in line:
                        continue
                    k, v = line.split("=", 1)
                    k, v = k.strip(), v.strip().strip('"').strip("'")
                    if k and k not in os.environ:  # 已有 env 优先
                        os.environ[k] = v
        except Exception as exc:
            print(f"  [warn] failed to load .env: {exc}", file=sys.stderr)
    config = _alc.LLMConfig.from_env()
    # reasoning model（deepseek-v4-pro / o1 / o3-mini）输出时把 800+ tokens 花在
    # reasoning_content 上，content 经常留空，response_format=json_object 也经常被
    # 截断。对 fill 这种"严格 JSON 短输出"任务不友好。
    # → 临时切到 non-reasoning 默认 model，保留原 LLMConfig 的 base_url/api_key
    _reasoning_keywords = ("-v4-pro", "-v4-pro-", "v4-pro", "v4-flash", "reasoner", "o1-", "o3-", "deepseek-r1")
    if any(kw in config.model.lower() for kw in _reasoning_keywords):
        if "deepseek" in config.base_url.lower():
            fallback_model = "deepseek-chat"
        elif "dashscope" in config.base_url.lower():
            fallback_model = "qwen-turbo"
        else:
            fallback_model = "gpt-4o-mini"
        if verbose:
            print(
                f"  [model] {config.model} is reasoning model → "
                f"temporarily switch to {fallback_model} for fill"
            )
        config = _alc.LLMConfig(
            api_key=config.api_key,
            base_url=config.base_url,
            model=fallback_model,
            max_tokens=config.max_tokens,
            timeout=config.timeout,
            retries=config.retries,
            retry_sleep=config.retry_sleep,
            json_mode=config.json_mode,
            reasoning_effort=None,  # 强制清掉，non-reasoning model 不需要
            audit_kind=config.audit_kind,
            temperature=config.temperature,
        )
    if not dry_run and not config.is_ready():
        print(f"  [error] OPENAI_API_KEY missing; abort. (use --dry-run to preview prompts)")
        return {"scanned": len(cases), "filled": 0, "skipped_existing": 0,
                "skipped_unfilled": 0, "errors": 1, "duration_s": 0.0}
    kb = _alc.load_project_kb()
    cap_index = _alc.load_plugin_capability_index()
    plugin_info = _alc.find_plugin_in_index(cap_index, plugin) or {}
    # plugin_type 用 PLUGIN_TYPE_REGISTRY（audit_sample_case_quality.py 的注册表）作为单一来源
    # 避免 capability_index 里 type 字段缺失（实测 shell_session 是空）
    plugin_type = ascq.PLUGIN_TYPE_REGISTRY.get(plugin, plugin_info.get("type", "default"))
    plugin_kb_snippet_parts = []
    if plugin_info.get("capability_tags"):
        plugin_kb_snippet_parts.append(
            f"capability_tags: {','.join(plugin_info['capability_tags'])}"
        )
    if plugin_info.get("description"):
        plugin_kb_snippet_parts.append(f"description: {plugin_info['description']}")
    if plugin_info.get("coverage_claims"):
        plugin_kb_snippet_parts.append(
            f"coverage_claims: {'; '.join(str(c) for c in plugin_info['coverage_claims'][:5])}"
        )
    plugin_kb_snippet = "\n".join(plugin_kb_snippet_parts) or (
        f"（capability index 里无摘要；插件类型为 {plugin_type}）"
    )

    # fix #14b：从 config/plugins/<name>.json 加载压缩策略，作为 LLM 写 scenario 的锚点
    plugin_compress_config = _alc.load_plugin_compress_config(plugin)
    plugin_compress_narrative = _alc.plugin_config_to_narrative(plugin_compress_config)
    if plugin_compress_narrative and plugin_compress_narrative != "[no plugin config found; judge on plugin name only]":
        plugin_kb_snippet += "\n\n=== 插件压缩策略（来自 config/plugins/<name>.json）===\n" + plugin_compress_narrative

    if verbose:
        registered = sum(1 for fn in cases if fn in showcase_titles)
        print(f"  [showcase] {registered}/{len(cases)} case(s) registered in showcase.rs")
        print(f"  [plugin] type={plugin_type}, kb_snippet_len={len(plugin_kb_snippet)}")

    # progress.md 路径
    progress_md = os.path.join("docs", "audit", plugin, "sample_quality", "fill_progress.md")
    recent: List[Dict[str, Any]] = []  # 每个 case 完成后 append

    # 4) 逐 case 跑
    scanned = len(cases)
    filled = 0
    skipped_existing = 0
    skipped_unfilled = 0
    errors = 0
    start = time.time()

    for i, case_fn in enumerate(cases, 1):
        case_path = os.path.join(samples_dir, plugin, case_fn)
        sc_path = _sidecar_path(samples_dir, plugin, case_fn)
        if not os.path.isfile(sc_path):
            errors += 1
            if verbose:
                print(f"  [{i}/{scanned}] {case_fn}: NO SIDECAR (run generate_case_sidecars.py first)")
            continue

        existing = _load_existing_sidecar_fields(sc_path)
        cur_scenario = (existing.get("scenario") or "").strip()
        cur_source = (existing.get("source") or "").strip()

        if cur_scenario and cur_source not in ("placeholder", "") and not force:
            skipped_existing += 1
            if verbose:
                print(f"  [{i}/{scanned}] {case_fn}: SKIP (scenario 已被填 source={cur_source})")
            recent.append({
                "i": i, "case": case_fn, "scenario": "(已存在)",
                "status": "SKIP",
            })
            if not dry_run:
                _write_progress_md(
                    progress_md, plugin, scanned,
                    filled=filled, skipped_existing=skipped_existing,
                    errors=errors, recent=recent,
                    last_case=case_fn, last_status="SKIP",
                )
            continue

        # 调试断点：--limit 1 时直接 return 第一个 case 的 raw response
        if os.environ.get("FILL_DEBUG_RAW", "0") == "1":
            print(f"  [debug] FILL_DEBUG_RAW=1 → 1 case 调 LLM 后立即退出")
            content = _read_case_content(os.path.join(samples_dir, plugin, case_fn))
            preview = _truncate(content, CONTENT_PREVIEW_CHARS)
            title = showcase_titles.get(case_fn, "")
            user_prompt = _build_user_prompt(
                case_filename=case_fn,
                case_content_preview=preview,
                showcase_title=title,
                plugin=plugin,
                plugin_type=plugin_type,
                plugin_kb_snippet=plugin_kb_snippet,
            )
            print(f"\n=== [debug] case {case_fn} ===")
            print(f"--- system prompt ---\n{FILL_SYSTEM_PROMPT}\n")
            print(f"--- user prompt ---\n{user_prompt}\n")
            print(f"--- calling LLM (model={config.model}, max_tokens=800, json_mode={config.json_mode}) ---")
            try:
                r = _alc.call_llm_chat(
                    config, user_prompt, FILL_SYSTEM_PROMPT,
                    max_tokens_override=800,
                )
            except Exception as exc:
                print(f"EXC: {exc!r}")
                return {"scanned": 1, "filled": 0, "skipped_existing": 0,
                        "skipped_unfilled": 0, "errors": 1, "duration_s": 0.0}
            print(f"--- raw LLM result (type={type(r).__name__}) ---")
            print(repr(r)[:2000])
            if isinstance(r, dict):
                content_str = (
                    r.get("choices", [{}])[0]
                    .get("message", {})
                    .get("content", "")
                )
                print(f"--- extracted content (len={len(content_str)}) ---")
                print(repr(content_str)[:1000])
            return {"scanned": 1, "filled": 0, "skipped_existing": 0,
                    "skipped_unfilled": 0, "errors": 0, "duration_s": 0.0}

        # 构造 prompt
        content = _read_case_content(case_path)
        preview = _truncate(content, CONTENT_PREVIEW_CHARS)
        title = showcase_titles.get(case_fn, "")
        user_prompt = _build_user_prompt(
            case_filename=case_fn,
            case_content_preview=preview,
            showcase_title=title,
            plugin=plugin,
            plugin_type=plugin_type,
            plugin_kb_snippet=plugin_kb_snippet,
        )

        if dry_run:
            # dry-run 时把每个 case 的 prompt 写到 docs/audit/<plugin>/fill_prompts_dryrun.txt
            # 避免 PowerShell cp936 控制台把中文乱码化；用 UTF-8 文件直接 cat 看
            log_dir = os.path.join("docs", "audit", plugin, "sample_quality")
            log_path = os.path.join(log_dir, "fill_prompts_dryrun.txt")
            os.makedirs(log_dir, exist_ok=True)
            with open(log_path, "a", encoding="utf-8") as lf:
                lf.write(f"\n\n========== [{i}/{scanned}] {case_fn} ==========\n")
                lf.write(f"plugin: {plugin} (type={plugin_type})\n")
                lf.write(f"showcase title: {title or '<未注册>'}\n")
                lf.write(f"content preview: {preview[:200]}...\n")
                lf.write(f"--- system prompt ---\n{FILL_SYSTEM_PROMPT}\n")
                lf.write(f"--- user prompt ---\n{user_prompt}\n")
                lf.write("--- end ---\n")
            print(f"  [{i}/{scanned}] {case_fn}: prompt logged → {log_path}")
            continue

        # 调试模式：把 stderr 同步镜像到 fill_stderr.log（PS 管道经常吞 stderr）
        stderr_log = os.path.join("docs", "audit", plugin, "sample_quality", "fill_stderr.log")
        _mirror_stderr(stderr_log)

        # 调 LLM
        try:
            result = _alc.call_llm_chat(
                config,
                user_prompt,
                FILL_SYSTEM_PROMPT,
                max_tokens_override=800,
            )
        except Exception as exc:
            errors += 1
            if verbose:
                print(f"  [{i}/{scanned}] {case_fn}: LLM EXC {exc!r}")
            recent.append({"i": i, "case": case_fn, "scenario": "", "status": "LLM_EXC"})
            if not dry_run:
                _write_progress_md(
                    progress_md, plugin, scanned,
                    filled=filled, skipped_existing=skipped_existing,
                    errors=errors, recent=recent,
                    last_case=case_fn, last_status="LLM_EXC",
                )
            time.sleep(sleep_between)
            continue

        if not result:
            errors += 1
            if verbose:
                print(f"  [{i}/{scanned}] {case_fn}: LLM returned None")
            recent.append({"i": i, "case": case_fn, "scenario": "", "status": "LLM_NONE"})
            if not dry_run:
                _write_progress_md(
                    progress_md, plugin, scanned,
                    filled=filled, skipped_existing=skipped_existing,
                    errors=errors, recent=recent,
                    last_case=case_fn, last_status="LLM_NONE",
                )
            time.sleep(sleep_between)
            continue

        # 解析 LLM 响应
        # call_llm_chat 契约：成功 → return parsed dict；失败 → None
        # 但有时 json 解析被绕过，return raw dict 也可能带 choices[0].message.content
        if isinstance(result, dict) and "scenario" in result and "target_capability" in result:
            parsed = result
        else:
            content_str = (
                result.get("choices", [{}])[0]
                .get("message", {})
                .get("content", "")
            )
            parsed = _parse_llm_json(content_str)
            if not parsed:
                errors += 1
                if verbose:
                    print(f"  [{i}/{scanned}] {case_fn}: LLM output not JSON: {content_str[:100]}")
                recent.append({"i": i, "case": case_fn, "scenario": content_str[:60], "status": "NOT_JSON"})
                if not dry_run:
                    _write_progress_md(
                        progress_md, plugin, scanned,
                        filled=filled, skipped_existing=skipped_existing,
                        errors=errors, recent=recent,
                        last_case=case_fn, last_status="NOT_JSON",
                    )
                time.sleep(sleep_between)
                continue

        ok, reason = _validate_fields(parsed)
        if not ok:
            errors += 1
            if verbose:
                print(f"  [{i}/{scanned}] {case_fn}: VALIDATE FAIL: {reason}")
            recent.append({"i": i, "case": case_fn, "scenario": parsed.get("scenario", "")[:60], "status": "VALIDATE_FAIL"})
            if not dry_run:
                _write_progress_md(
                    progress_md, plugin, scanned,
                    filled=filled, skipped_existing=skipped_existing,
                    errors=errors, recent=recent,
                    last_case=case_fn, last_status="VALIDATE_FAIL",
                )
            time.sleep(sleep_between)
            continue

        # 写回
        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()
        dispatch_chain = _parse_dispatch_chain(parsed.get("expected_dispatch_chain", ""))
        _ek_val = parsed.get("expected_keep", "")
        _ec_val = parsed.get("expected_compress", "")
        write_ok = _write_sidecar_scenario_fields(
            sc_path,
            scenario=parsed["scenario"],
            target_capability=parsed["target_capability"],
            expected_keep=", ".join(_ek_val) if isinstance(_ek_val, list) else _ek_val,
            expected_compress=", ".join(_ec_val) if isinstance(_ec_val, list) else _ec_val,
            source="llm",
            llm_filled_at=now_iso,
            expected_dispatch_chain=dispatch_chain or None,
        )
        if write_ok:
            filled += 1
            dc_str = f" chain={dispatch_chain}" if dispatch_chain else ""
            if verbose:
                print(f"  [{i}/{scanned}] {case_fn}: FILLED{dc_str}")
                print(f"      scenario: {parsed['scenario'][:80]}...")
            recent.append({
                "i": i,
                "case": case_fn,
                "scenario": parsed["scenario"],
                "status": "FILLED",
            })
        else:
            errors += 1
            if verbose:
                print(f"  [{i}/{scanned}] {case_fn}: WRITE FAILED")
            recent.append({
                "i": i,
                "case": case_fn,
                "scenario": "",
                "status": "WRITE_FAIL",
            })

        # 实时 update progress.md
        if not dry_run:
            _write_progress_md(
                progress_md, plugin, scanned,
                filled=filled, skipped_existing=skipped_existing,
                errors=errors, recent=recent,
                last_case=case_fn, last_status=("FILLED" if write_ok else "WRITE_FAIL"),
            )
        time.sleep(sleep_between)

    duration = round(time.time() - start, 1)
    return {
        "scanned": scanned,
        "filled": filled,
        "skipped_existing": skipped_existing,
        "skipped_unfilled": skipped_unfilled,
        "errors": errors,
        "duration_s": duration,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="调 LLM 为 samples 下 case 填充 sidecar scenario 字段。"
    )
    parser.add_argument("--plugin", help="只处理单个 plugin")
    parser.add_argument("--all", action="store_true", help="处理 samples/ 下所有 plugin")
    parser.add_argument("--samples-dir", default="samples")
    parser.add_argument("--limit", type=int, help="每个 plugin 最多处理的 case 数")
    parser.add_argument("--force", action="store_true",
                        help="覆盖已有 scenario（人类/LLM 填过的也会重写）")
    parser.add_argument("--dry-run", action="store_true",
                        help="只打 prompt 不调 LLM 不写回")
    parser.add_argument("--sleep", type=float, default=0.5,
                        help="每个 case 之间的 sleep 秒数（防 rate limit）")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args()

    if not args.plugin and not args.all:
        print("ERROR: must specify --plugin <name> or --all", file=sys.stderr)
        return 2

    if args.plugin:
        targets = [args.plugin]
    else:
        # 列出 samples 下所有 plugin 目录
        targets = sorted(
            e for e in os.listdir(args.samples_dir)
            if os.path.isdir(os.path.join(args.samples_dir, e))
        )

    grand_total = {
        "scanned": 0, "filled": 0, "skipped_existing": 0,
        "skipped_unfilled": 0, "errors": 0, "duration_s": 0.0,
    }
    for plugin in targets:
        print(f"\n=== plugin={plugin} ===")
        result = fill_for_plugin(
            plugin,
            samples_dir=args.samples_dir,
            limit=args.limit,
            force=args.force,
            dry_run=args.dry_run,
            sleep_between=args.sleep,
            verbose=not args.quiet,
        )
        for k, v in result.items():
            grand_total[k] += v

    if not args.dry_run:
        print(f"\n=== GRAND TOTAL ===")
        for k, v in grand_total.items():
            print(f"  {k}: {v}")
    return 0 if grand_total["errors"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
