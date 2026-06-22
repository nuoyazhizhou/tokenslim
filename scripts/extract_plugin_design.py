#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
extract_plugin_design.py
========================

从 src/plugins/<name>/methods.rs（及 mod.rs / parser.rs）提取插件的设计意图，
调 LLM 生成 keep_signals / compress_targets / design_intent，
写入 config/plugins/<name>.json。

核心思路：
  脚本读取 methods.rs 源码 → 截取关键函数 → 调 LLM 分析 →
  LLM 输出结构化 JSON → 写入 config

用法：
    # dry-run：只打印提取结果
    python scripts/extract_plugin_design.py --dry-run

    # 真写：更新 config/plugins/<name>.json
    python scripts/extract_plugin_design.py

    # 只处理指定插件
    python scripts/extract_plugin_design.py --plugin gcc_log_plugin

    # 强制覆盖已有 design_intent（默认跳过已有）
    python scripts/extract_plugin_design.py --force
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.request
from typing import Any, Dict, List, Optional, Tuple

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, ".."))

sys.path.insert(0, SCRIPT_DIR)
import audit_llm_common as _alc  # noqa: E402


# ============================================================================
# 源码读取
# ============================================================================

MAX_CODE_CHARS = 6000  # 截断代码避免 prompt 过长


def _find_plugin_dirs() -> List[str]:
    """返回 src/plugins/ 下所有 *_plugin 目录名"""
    plugins_dir = os.path.join(PROJECT_ROOT, "src", "plugins")
    if not os.path.isdir(plugins_dir):
        return []
    return sorted(
        d for d in os.listdir(plugins_dir)
        if os.path.isdir(os.path.join(plugins_dir, d)) and d.endswith("_plugin")
    )


def _read_source_file(path: str) -> str:
    """读文件，失败返回空字符串"""
    if not os.path.isfile(path):
        return ""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def _truncate(text: str, max_chars: int = MAX_CODE_CHARS) -> str:
    if len(text) <= max_chars:
        return text
    half = max_chars // 2
    return text[:half] + f"\n\n... [中间 {len(text) - max_chars} 字符省略] ...\n\n" + text[-half:]


def _extract_key_functions(code: str) -> str:
    """从 methods.rs 提取关键函数：detect / compress / classify / compact_* / optimize_*

    保留函数签名 + 前 30 行实现，去掉其余代码。
    """
    if not code:
        return ""

    # 匹配 pub fn / fn 开头的函数
    func_pattern = re.compile(
        r'^(\s*(?:pub\s+)?fn\s+\w+[<(].*?(?=\n\s*(?:pub\s+)?fn\s|\n\s*impl\s|\Z))',
        re.MULTILINE | re.DOTALL,
    )

    key_fn_names = (
        "detect", "compress", "classify", "compact_", "optimize_",
        "should_preserve", "should_fold", "should_skip",
        "truncate_", "dedupe_", "dedup_", "fold_",
        "generate_summary", "generate_fold",
    )

    parts = []
    for m in func_pattern.finditer(code):
        func_text = m.group(1)
        fn_name_match = re.search(r'fn\s+(\w+)', func_text)
        if not fn_name_match:
            continue
        fn_name = fn_name_match.group(1)

        if any(kw in fn_name.lower() for kw in key_fn_names):
            lines = func_text.split("\n")
            if len(lines) > 30:
                kept = "\n".join(lines[:30])
                kept += f"\n    // ... [函数共 {len(lines)} 行，省略后 {len(lines)-30} 行]"
                parts.append(kept)
            else:
                parts.append(func_text)

    if not parts:
        return _truncate(code, 2000)

    result = "\n\n".join(parts)
    return _truncate(result, MAX_CODE_CHARS)


def load_plugin_sources(plugin_name: str) -> Dict[str, str]:
    """读取插件的 mod.rs + methods.rs + parser.rs 源码"""
    plugin_dir = os.path.join(PROJECT_ROOT, "src", "plugins", plugin_name)
    sources = {}
    for filename in ("mod.rs", "methods.rs", "parser.rs"):
        path = os.path.join(plugin_dir, filename)
        content = _read_source_file(path)
        if content:
            sources[filename] = content
    return sources


# ============================================================================
# LLM 提取
# ============================================================================

EXTRACT_SYSTEM_PROMPT = """你是一个 Rust 代码分析专家。你的任务是阅读 TokenSlim 项目的插件源码，
提取插件的设计意图（design intent）。

TokenSlim 是一个结构化日志/命令输出压缩库。每个插件负责一种文本的压缩转换。
插件的核心逻辑在 methods.rs 的 compress / detect / classify 等函数中。

你需要从代码中提取三个信息：

1. **design_intent**：一句话（30-80 字中文）描述这个插件的设计意图。
   格式："XX脱水：保留YY，折叠ZZ，压缩WW。"
   必须基于代码实际逻辑，不要脑补。

2. **keep_signals**：列表（3-8 项），描述插件**保留**什么内容。
   每项格式："具体内容（简短说明）"
   例如："error: 行（编译错误）"
   来源：detect 函数的匹配模式、compress 函数中未被 skip/continue 的行、
   should_preserve 函数的白名单等。

3. **compress_targets**：列表（3-8 项），描述插件**压缩/折叠/丢弃**什么内容。
   每项格式："具体内容（简短说明）"
   例如："长路径替换为 $GCC 令牌字典"
   来源：compress 函数中的 skip/continue 逻辑、折叠阈值、
   dedup 去重、字典替换等。

**反幻觉纪律**：
- 只能从代码中提取，代码里没有的逻辑不要写
- 如果代码太短看不出逻辑，keep_signals/compress_targets 可以只写 1-2 项
- 不要写通用废话（如"保留重要信息"），要写具体（如"保留 error: 行"）

输出严格 JSON：
{
  "design_intent": "...",
  "keep_signals": ["...", "..."],
  "compress_targets": ["...", "..."]
}"""


def _build_extract_prompt(
    plugin_name: str,
    sources: Dict[str, str],
    existing_config: Optional[dict],
) -> str:
    """构建 LLM 提取 prompt"""
    parts = [f"插件名: {plugin_name}\n"]

    # mod.rs 的文档注释
    mod_rs = sources.get("mod.rs", "")
    if mod_rs:
        doc_lines = [l for l in mod_rs.split("\n") if l.strip().startswith("//!")]
        if doc_lines:
            parts.append("=== mod.rs 文档注释 ===")
            parts.append("\n".join(doc_lines))
            parts.append("")

    # methods.rs 关键函数
    methods_rs = sources.get("methods.rs", "")
    if methods_rs:
        key_funcs = _extract_key_functions(methods_rs)
        if key_funcs:
            parts.append("=== methods.rs 关键函数 ===")
            parts.append(key_funcs)
            parts.append("")

    # parser.rs（如果有）
    parser_rs = sources.get("parser.rs", "")
    if parser_rs:
        key_funcs = _extract_key_functions(parser_rs)
        if key_funcs:
            parts.append("=== parser.rs 关键函数 ===")
            parts.append(key_funcs)
            parts.append("")

    # 已有 config 的 description/detect/compress（作为参考）
    if existing_config:
        ref_parts = []
        desc = existing_config.get("description", "")
        if desc:
            ref_parts.append(f"现有 description: {desc}")
        detect = existing_config.get("detect", {})
        if detect:
            ref_parts.append(f"现有 detect: {json.dumps(detect, ensure_ascii=False, indent=2)[:500]}")
        compress = existing_config.get("compress", {})
        if compress:
            ref_parts.append(f"现有 compress: {json.dumps(compress, ensure_ascii=False, indent=2)[:500]}")
        if ref_parts:
            parts.append("=== 已有 config 参考（仅供参考，以代码为准）===")
            parts.append("\n".join(ref_parts))

    return "\n".join(parts)


def _parse_llm_json(raw: str) -> Optional[Dict[str, Any]]:
    """从 LLM 返回里 extract JSON dict"""
    if not raw:
        return None
    raw = re.sub(r"^```(?:json)?\s*", "", raw.strip())
    raw = re.sub(r"\s*```$", "", raw.strip())
    m = re.search(r"\{[\s\S]*\}", raw)
    if not m:
        return None
    try:
        result = json.loads(m.group(0))
        if not isinstance(result, dict):
            return None
        if "design_intent" not in result:
            return None
        result.setdefault("keep_signals", [])
        result.setdefault("compress_targets", [])
        return result
    except (ValueError, TypeError):
        return None


def _call_llm_raw(config: _alc.LLMConfig, user_prompt: str) -> Optional[Dict[str, Any]]:
    """直接 HTTP 调 LLM，拿 content 字符串，解析为 design dict。

    不走 call_llm_chat，因为 call_llm_chat 假设返回 audit 格式 JSON，
    而 LLM 这里返回的是 design 格式 JSON。
    """
    if not config.is_ready():
        return None

    url = f"{config.base_url}/chat/completions"
    messages = []
    if EXTRACT_SYSTEM_PROMPT:
        messages.append({"role": "system", "content": EXTRACT_SYSTEM_PROMPT})
    messages.append({"role": "user", "content": user_prompt})

    payload = {
        "model": config.model,
        "messages": messages,
        "temperature": config.temperature,
        "max_tokens": 1024,
        "response_format": {"type": "json_object"},
    }
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")

    for attempt in range(max(1, config.retries)):
        try:
            req = urllib.request.Request(
                url,
                data=body,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {config.api_key}",
                },
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=config.timeout) as resp:
                raw = resp.read().decode("utf-8", errors="ignore")
            data = json.loads(raw)
            message = data.get("choices", [{}])[0].get("message", {})
            content = message.get("content", "") or ""

            # reasoning model fallback
            if not content.strip() and message.get("reasoning_content"):
                rc = message["reasoning_content"]
                m = re.search(r"```(?:json)?\s*(\{.*?\})\s*```", rc, re.DOTALL)
                if m:
                    content = m.group(1)
                else:
                    m2 = re.search(r"\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}", rc, re.DOTALL)
                    if m2:
                        content = m2.group(0)

            design = _parse_llm_json(content)
            if design:
                return design

            # JSON 解析失败，打印调试信息
            finish_reason = data.get("choices", [{}])[0].get("finish_reason", "?")
            print(f"  [debug] attempt {attempt+1}: finish_reason={finish_reason}, "
                  f"content_len={len(content)}, content[:200]={content[:200]}", file=sys.stderr)

        except urllib.error.HTTPError as e:
            if e.code in (401, 403):
                raise RuntimeError(f"Auth error: {e.code}")
            if e.code in (408, 429) or e.code >= 500:
                if attempt < config.retries - 1:
                    time.sleep(config.retry_sleep * (2 ** attempt))
                    continue
            raise
        except (urllib.error.URLError, OSError) as e:
            if attempt < config.retries - 1:
                time.sleep(config.retry_sleep * (2 ** attempt))
                continue
            print(f"  [error] network error after {attempt+1} attempts: {e}", file=sys.stderr)
            return None

    return None


# ============================================================================
# Config 读写
# ============================================================================

def load_plugin_config(plugin_name: str) -> Optional[Tuple[dict, str]]:
    """加载 config/plugins/<name>.json，返回 (config, path) 或 None"""
    short_name = plugin_name
    if short_name.endswith("_plugin"):
        short_name = short_name[:-len("_plugin")]
    candidates = [
        os.path.join(PROJECT_ROOT, "config", "plugins", f"{short_name}.json"),
    ]
    for path in candidates:
        if os.path.isfile(path):
            try:
                with open(path, "r", encoding="utf-8") as f:
                    return json.load(f), path
            except (json.JSONDecodeError, OSError):
                pass
    return None


def save_plugin_config(config: dict, path: str) -> None:
    """保存 config/plugins/<name>.json"""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(config, f, indent=4, ensure_ascii=False)
        f.write("\n")


def create_minimal_config(plugin_name: str) -> Tuple[dict, str]:
    """为没有 config 的插件创建最小配置"""
    short_name = plugin_name
    if short_name.endswith("_plugin"):
        short_name = short_name[:-len("_plugin")]
    return {
        "name": short_name,
        "description": "",
        "priority": 200,
        "enabled": True,
        "detect": {"rules": [{"type": "any", "patterns": []}], "min_match_ratio": 0.05},
        "compress": {"token_prefix": f"${short_name.upper()[:6]}"},
    }, os.path.join(PROJECT_ROOT, "config", "plugins", f"{short_name}.json")


# ============================================================================
# mod.rs 更新
# ============================================================================

def _build_modrs_doc_block(design: Dict[str, Any], plugin_name: str) -> str:
    """从 design dict 生成 mod.rs 的 //! 文档注释块。"""
    di = design.get("design_intent", "")
    keep_signals = design.get("keep_signals", [])
    compress_targets = design.get("compress_targets", [])

    lines = [f"//! {di}", ""]

    if keep_signals:
        lines.append("//! ## 保留信号")
        for sig in keep_signals:
            lines.append(f"//! - {sig}")
        lines.append("")

    if compress_targets:
        lines.append("//! ## 压缩目标")
        for tgt in compress_targets:
            lines.append(f"//! - {tgt}")
        lines.append("")

    return "\n".join(lines)


def update_modrs_for_plugin(plugin_name: str, *, dry_run: bool = False, verbose: bool = True) -> str:
    """用 config 中的 design_intent 更新 mod.rs 的 //! 文档注释。

    策略：替换 mod.rs 开头的连续 //! 行为新的文档注释块。
    如果 mod.rs 不存在，跳过。

    Returns: "updated" | "skipped" | "no_config" | "no_modrs" | "dry_run"
    """
    # 1) 读 config
    existing = load_plugin_config(plugin_name)
    if not existing:
        if verbose:
            print(f"  [skip] {plugin_name}: no config found")
        return "no_config"
    cfg = existing[0]
    design_intent = cfg.get("design_intent", "")
    if not design_intent:
        if verbose:
            print(f"  [skip] {plugin_name}: no design_intent in config")
        return "no_config"

    design = {
        "design_intent": design_intent,
        "keep_signals": cfg.get("keep_signals", []),
        "compress_targets": cfg.get("compress_targets", []),
    }

    # 2) 读 mod.rs
    modrs_path = os.path.join(PROJECT_ROOT, "src", "plugins", plugin_name, "mod.rs")
    if not os.path.isfile(modrs_path):
        if verbose:
            print(f"  [skip] {plugin_name}: no mod.rs")
        return "no_modrs"

    with open(modrs_path, "r", encoding="utf-8") as f:
        content = f.read()

    # 3) 生成新文档块
    new_doc = _build_modrs_doc_block(design, plugin_name)

    if dry_run:
        if verbose:
            print(f"  [dry-run] {plugin_name}: would update mod.rs with:")
            for line in new_doc.split("\n")[:5]:
                print(f"    {line}")
            if new_doc.count("\n") > 5:
                print(f"    ... ({new_doc.count(chr(10))} lines total)")
        return "dry_run"

    # 4) 替换旧的 //! 行
    lines = content.split("\n")
    # 找到第一个非 //! 行（跳过空行也算 //! 块的一部分，只要 //! 还在继续）
    first_non_doc = 0
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped and not stripped.startswith("//!"):
            first_non_doc = i
            break
    else:
        first_non_doc = len(lines)

    # 去掉旧 //! 行前面的空行（如果有的话）
    # 保留 first_non_doc 之后的内容
    remaining = "\n".join(lines[first_non_doc:])
    # 去掉开头的空行
    remaining = remaining.lstrip("\n")

    # 清理紧跟其后的 /// 废话注释（agent 生成的通用注释）
    # 匹配模式：连续的 /// 行（可能穿插空行），内容是通用废话如"模块概述"/"主要功能"/"核心类型"/"协调各子组件"/"统一 API"
    _boilerplate_patterns = (
        "模块概述", "主要功能", "核心类型", "协调各子组件", "统一",
        "提供核心类型定义和接口", "协调各子组件的工作流程", "对外提供统一的 API 接口",
        "本模块实现了 TokenSlim", "本模块实现了TokenSlim",
    )
    remaining_lines = remaining.split("\n")
    cleaned_lines = []
    skip_boilerplate = False
    in_doc_comment_block = False  # 跟踪是否在 /// 块中
    for line in remaining_lines:
        stripped = line.strip()
        if stripped.startswith("///"):
            # 检查是否是废话注释
            if any(pat in stripped for pat in _boilerplate_patterns):
                skip_boilerplate = True
                continue
            # 空的 /// 行（只有 ///）
            if stripped == "///":
                if skip_boilerplate:
                    continue
            # 检查是否紧跟在 //! 块之后、mod 声明之前的 /// 块
            # 这些 /// 块通常是旧的功能描述，已被 //! 覆盖
            # 如果 /// 块后面紧跟 mod/struct/enum 等声明，保留
            # 如果 /// 块后面没有紧跟声明（只是独立的注释块），删除
            in_doc_comment_block = True
            if skip_boilerplate:
                continue
        elif stripped == "" and (skip_boilerplate or in_doc_comment_block):
            # 空行在 /// 块中间，也跳过
            if skip_boilerplate:
                continue
            # 空行结束 /// 块
            if in_doc_comment_block:
                in_doc_comment_block = False
                skip_boilerplate = False
                # 不保留这个空行（它是旧 /// 块的尾部空行）
                continue
        else:
            skip_boilerplate = False
            in_doc_comment_block = False
        cleaned_lines.append(line)

    remaining = "\n".join(cleaned_lines)

    # 5) 组合新内容
    new_content = new_doc + remaining

    # 确保文件末尾有换行
    if not new_content.endswith("\n"):
        new_content += "\n"

    with open(modrs_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(new_content)

    if verbose:
        print(f"  [ok] {plugin_name}: mod.rs updated with design_intent")

    return "updated"


# ============================================================================
# 主流程
# ============================================================================

def _get_effective_config(config: _alc.LLMConfig, verbose: bool = True) -> _alc.LLMConfig:
    """如果当前 model 是 reasoning model，切到 non-reasoning model。

    reasoning model（deepseek-v4-pro / o1 / o3-mini）输出时把 tokens 花在
    reasoning_content 上，content 经常留空。对 extract 这种"严格 JSON 短输出"
    任务不友好 → 临时切到 non-reasoning 默认 model。
    """
    _reasoning_keywords = ("-v4-pro", "reasoner", "o1-", "o3-", "deepseek-r1")
    if not any(kw in config.model.lower() for kw in _reasoning_keywords):
        return config

    if "deepseek" in config.base_url.lower():
        fallback_model = "deepseek-chat"
    elif "dashscope" in config.base_url.lower() or "aliyun" in config.base_url.lower():
        fallback_model = "qwen-turbo"
    else:
        fallback_model = "gpt-4o-mini"

    if verbose:
        print(f"  [model] {config.model} is reasoning model -> switch to {fallback_model}")

    return _alc.LLMConfig(
        api_key=config.api_key,
        base_url=config.base_url,
        model=fallback_model,
        max_tokens=config.max_tokens,
        timeout=config.timeout,
        retries=config.retries,
        retry_sleep=config.retry_sleep,
        json_mode=config.json_mode,
        reasoning_effort=None,
        audit_kind=config.audit_kind,
        temperature=config.temperature,
    )


def extract_for_plugin(
    plugin_name: str,
    config: _alc.LLMConfig,
    *,
    force: bool = False,
    dry_run: bool = False,
    verbose: bool = True,
) -> Dict[str, Any]:
    """为一个插件提取设计意图。

    Returns: {"plugin": str, "status": str, "design": dict|None, "error": str|None}
    """
    result = {"plugin": plugin_name, "status": "skipped", "design": None, "error": None}

    # 1) 读源码
    sources = load_plugin_sources(plugin_name)
    if not sources:
        result["status"] = "no_source"
        result["error"] = "no methods.rs/mod.rs found"
        return result

    # 2) 加载已有 config
    existing = load_plugin_config(plugin_name)
    existing_config = existing[0] if existing else None

    # 3) 检查是否已有 design_intent（跳过已有，除非 --force）
    if existing_config and existing_config.get("design_intent") and not force:
        result["status"] = "already_has_design"
        result["design"] = {
            "design_intent": existing_config.get("design_intent", ""),
            "keep_signals": existing_config.get("keep_signals", []),
            "compress_targets": existing_config.get("compress_targets", []),
        }
        if verbose:
            print(f"  [skip] {plugin_name}: already has design_intent (use --force to overwrite)")
        return result

    # 4) 构建 prompt
    user_prompt = _build_extract_prompt(plugin_name, sources, existing_config)

    if dry_run:
        result["status"] = "dry_run"
        if verbose:
            print(f"  [dry-run] {plugin_name}: prompt length = {len(user_prompt)} chars")
            print(f"    sources: {', '.join(sources.keys())}")
        return result

    # 5) 调 LLM
    if not config.is_ready():
        result["status"] = "no_api_key"
        result["error"] = "OPENAI_API_KEY not set"
        return result

    effective_config = _get_effective_config(config, verbose=verbose)

    design = _call_llm_raw(effective_config, user_prompt)

    if not design:
        result["status"] = "llm_failed"
        result["error"] = "LLM call failed or returned unparseable JSON"
        return result

    result["status"] = "extracted"
    result["design"] = design

    # 6) 写入 config
    if existing:
        cfg, path = existing
    else:
        cfg, path = create_minimal_config(plugin_name)

    cfg["design_intent"] = design.get("design_intent", "")
    cfg["keep_signals"] = design.get("keep_signals", [])
    cfg["compress_targets"] = design.get("compress_targets", [])

    save_plugin_config(cfg, path)

    if verbose:
        di = design.get("design_intent", "")
        ks = design.get("keep_signals", [])
        ct = design.get("compress_targets", [])
        print(f"  [ok] {plugin_name}:")
        print(f"    design_intent: {di}")
        print(f"    keep_signals: {len(ks)} items")
        print(f"    compress_targets: {len(ct)} items")

    return result


def main():
    parser = argparse.ArgumentParser(
        description="从 methods.rs 提取插件设计意图，调 LLM 分析，写入 config"
    )
    parser.add_argument("--plugin", help="只处理指定插件")
    parser.add_argument("--dry-run", action="store_true", help="只打印 prompt，不调 LLM")
    parser.add_argument("--force", action="store_true", help="覆盖已有 design_intent")
    parser.add_argument("--sleep", type=float, default=1.0, help="LLM 调用间隔（秒）")
    parser.add_argument("--update-modrs", action="store_true",
                        help="用 config 中的 design_intent 更新 mod.rs 的 //! 文档注释")
    args = parser.parse_args()

    # 加载 .env
    _alc._ensure_dotenv_loaded(os.environ)
    config = _alc.LLMConfig.from_env()

    # 获取插件列表
    if args.plugin:
        plugins = [args.plugin]
    else:
        plugins = _find_plugin_dirs()

    if not plugins:
        print("[error] no plugins found in src/plugins/")
        return

    print(f"Processing {len(plugins)} plugin(s)...")

    # --update-modrs 模式：不调 LLM，直接从 config 读 design_intent 写入 mod.rs
    if args.update_modrs:
        results = {"updated": 0, "no_config": 0, "no_modrs": 0, "dry_run": 0}
        for plugin_name in plugins:
            status = update_modrs_for_plugin(
                plugin_name,
                dry_run=args.dry_run,
            )
            results[status] = results.get(status, 0) + 1

        print(f"\nDone: {results.get('updated', 0)} updated, "
              f"{results.get('no_config', 0)} no_config, "
              f"{results.get('no_modrs', 0)} no_modrs, "
              f"{results.get('dry_run', 0)} dry-run")
        return

    # 正常模式：调 LLM 提取设计意图
    results = {"extracted": 0, "already_has": 0, "skipped": 0, "failed": 0, "dry_run": 0}

    for plugin_name in plugins:
        r = extract_for_plugin(
            plugin_name,
            config,
            force=args.force,
            dry_run=args.dry_run,
        )
        if r["status"] == "extracted":
            results["extracted"] += 1
        elif r["status"] == "already_has_design":
            results["already_has"] += 1
        elif r["status"] == "dry_run":
            results["dry_run"] += 1
        elif r["status"] in ("no_source", "no_api_key"):
            results["skipped"] += 1
        else:
            results["failed"] += 1

        if not args.dry_run and args.sleep > 0:
            time.sleep(args.sleep)

    print(f"\nDone: {results['extracted']} extracted, {results['already_has']} already had, "
          f"{results['skipped']} skipped, {results['failed']} failed, {results['dry_run']} dry-run")


if __name__ == "__main__":
    main()
