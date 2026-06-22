#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
audit_llm_common.py
===================

TokenSlim audit 脚本的公共基座（设计见 docs/design/audit_llm_kb_design.md）。

提供：
  1. LLM HTTP 调用封装（call_llm_chat / LLMConfig / 错误类型）
  2. LLM 提示词模板 cascade 加载（load_prompt_template / build_*_prompt）
  3. 知识库加载（project.yaml / plugin_capability_index.json / case sidecar）
  4. 源码 / 物理文件读取层（parse_mod_rs_registry / walk_physical_samples_with_sidecar / ...）
  5. Preflight Drift Detection（drift_audit，7 条漂移轴）

所有外部资源缺文件时走 fallback 兜底（除 project.yaml 缺失会 raise）。

**唯一硬性依赖**：Python 标准库 + 已存在的 audit_case_metrics.py 中的
parse_showcase_rs_cases / get_physical_samples。**不**依赖 PyYAML / tree-sitter 等
第三方库（yaml 解析用超轻量内置实现，详见 _MiniYaml）。
"""

import os
import re
import io
import json
import time
import random
import argparse
import hashlib
import urllib.request
import urllib.error
import pathlib
import tempfile
import datetime
import sys
import traceback
from typing import (
    Any, Dict, List, Optional, Set, Tuple, Iterable, Callable, Union,
)

# 让本模块能 import 同目录下的 audit_case_metrics 与 generate_plugin_capability_index
_SCRIPTS_DIR = os.path.dirname(os.path.abspath(__file__))
if _SCRIPTS_DIR not in sys.path:
    sys.path.insert(0, _SCRIPTS_DIR)

# 兼容 stdout 在 Windows 下乱码
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        try:
            _stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:
            pass


# ============================================================================
# .env 自动加载（公共工具，所有 LLM 调用方共享）
# ============================================================================
_DOTENV_LOADED = False
_DOTENV_LOADED_PATH: Optional[str] = None


def _ensure_dotenv_loaded(env: Dict[str, str]) -> Optional[str]:
    """把 .env 文件加载到 env dict（仅当 key 不已存在时），返回加载路径或 None。

    探测顺序：
      1. cwd/.env
      2. 本模块所在目录的父目录的 .env（scripts/ 的项目根 .env）
      3. 父目录的父目录 .env
      4. 父目录的父目录的父目录 .env
      5. 递归向上到文件系统根
    """
    global _DOTENV_LOADED, _DOTENV_LOADED_PATH
    if _DOTENV_LOADED:
        return _DOTENV_LOADED_PATH
    # 1) cwd
    candidates: List[str] = []
    try:
        cwd = os.getcwd()
        candidates.append(os.path.join(cwd, ".env"))
        # 2-5) 递归向上
        cur = cwd
        for _ in range(6):
            parent = os.path.dirname(cur)
            if not parent or parent == cur:
                break
            candidates.append(os.path.join(parent, ".env"))
            cur = parent
    except OSError:
        pass
    # 3) 本模块所在目录的父目录（即 scripts/ 的项目根）
    module_dir = os.path.dirname(os.path.abspath(__file__))
    if module_dir and module_dir not in (cwd, os.getcwd()):
        candidates.append(os.path.join(os.path.dirname(module_dir), ".env"))
    # 去重，保持顺序
    seen = set()
    uniq = []
    for c in candidates:
        if c not in seen:
            seen.add(c)
            uniq.append(c)
    for path in uniq:
        try:
            if not os.path.isfile(path):
                continue
            with open(path, "r", encoding="utf-8") as ef:
                for line in ef:
                    line = line.strip()
                    if not line or line.startswith("#") or "=" not in line:
                        continue
                    k, v = line.split("=", 1)
                    k = k.strip()
                    v = v.strip().strip('"').strip("'")
                    if k and k not in env:
                        env[k] = v
            _DOTENV_LOADED = True
            _DOTENV_LOADED_PATH = path
            return path
        except Exception:
            continue
    _DOTENV_LOADED = True  # 标记为已尝试（哪怕没找到），避免每次调用都遍历
    return None


# ============================================================================
# 0. 通用工具
# ============================================================================

def parse_env(env_path: str = ".env") -> Dict[str, str]:
    """与 audit_case_metrics.parse_env 保持一致：优先 os.environ，叠加 .env。"""
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
                    if (value.startswith('"') and value.endswith('"')) or (
                        value.startswith("'") and value.endswith("'")
                    ):
                        value = value[1:-1]
                    key = k.strip()
                    if key not in os.environ:
                        env[key] = value
    return env


def ensure_dir(path: str) -> None:
    if not path:
        return
    if not os.path.exists(path):
        os.makedirs(path, exist_ok=True)


def atomic_write_json(path: str, obj: Any) -> None:
    target_dir = os.path.dirname(path) or "."
    ensure_dir(target_dir)
    fd, tmp_path = tempfile.mkstemp(
        prefix=f".{os.path.basename(path)}.",
        suffix=".tmp",
        dir=target_dir,
        text=True,
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            json.dump(obj, f, indent=2, ensure_ascii=False)
            f.write("\n")
            f.flush()
            try:
                os.fsync(f.fileno())
            except OSError:
                pass
        os.replace(tmp_path, path)
    except Exception:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass
        raise


def read_text(path: str, default: str = "") -> str:
    if not os.path.exists(path):
        return default
    try:
        with open(path, "r", encoding="utf-8", errors="ignore") as f:
            return f.read()
    except Exception:
        return default


def read_json_file(path: str) -> Optional[dict]:
    if not os.path.exists(path):
        return None
    try:
        with open(path, "r", encoding="utf-8", errors="ignore") as f:
            return json.load(f)
    except Exception:
        return None


# ----------------------------------------------------------------------------
# 0.1 YAML 解析
# ----------------------------------------------------------------------------
# 优先用 PyYAML（事实标准）。缺它时降级到极简解析器（仅支持 flat key:value 与
# 顶层 [a, b] 内联列表，不支持嵌套结构 — 嵌套 KB 文件应装 PyYAML）。
# 不把 PyYAML 写进硬依赖是因为它不在 Python 标准库。

def _yaml_loads(text: str) -> Any:
    try:
        import yaml  # type: ignore
        return yaml.safe_load(text)
    except ImportError:
        return _MiniYaml.loads(text)


class _MiniYaml:
    """PyYAML 不可用时的降级实现。仅支持本设计 KB 文件的子集：
    - 顶层 key: scalar
    - 顶层 key: [a, b, c]（内联列表）
    - 二级 key: scalar
    - 顶层 - item（纯标量列表）
    嵌套结构需要 PyYAML。
    """
    _LINE = re.compile(r'^(?P<k>[^:#\s][^:]*?)\s*:\s*(?P<v>.*)$')
    _ITEM = re.compile(r'^-\s+(?P<v>.*)$')
    _INLINE_LIST = re.compile(r'^\[(?P<v>.+)\]$')

    @classmethod
    def loads(cls, text: str) -> Any:
        if text is None:
            return None
        root: Dict[str, Any] = {}
        list_root: List[Any] = []
        saw_list_item = False
        for raw in text.splitlines():
            line = raw.rstrip()
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            indent = len(line) - len(line.lstrip(" "))
            if indent == 0 and stripped.startswith("- "):
                m = cls._ITEM.match(stripped)
                if m:
                    list_root.append(cls._coerce(m.group("v").strip()))
                    saw_list_item = True
            elif ":" in stripped:
                m = cls._LINE.match(stripped)
                if m:
                    k = m.group("k").strip().strip('"').strip("'")
                    v = m.group("v").strip()
                    inline = cls._INLINE_LIST.match(v)
                    if inline:
                        root[k] = cls._parse_inline_list(inline.group("v"))
                    else:
                        root[k] = cls._coerce(v)
        if saw_list_item and not root:
            return list_root
        return root

    @staticmethod
    def _coerce(v: str) -> Any:
        if v == "":
            return ""
        if (v.startswith('"') and v.endswith('"')) or (v.startswith("'") and v.endswith("'")):
            return v[1:-1]
        if v in ("true", "True", "yes"):
            return True
        if v in ("false", "False", "no"):
            return False
        if v in ("null", "None", "~"):
            return None
        if "\\n" in v:
            v = v.replace("\\n", "\n")
        try:
            if v.startswith("-") or v.isdigit():
                return int(v)
            return float(v)
        except ValueError:
            return v

    @staticmethod
    def _parse_inline_list(s: str) -> List[Any]:
        items: List[Any] = []
        depth = 0
        cur = ""
        for ch in s:
            if ch == "," and depth == 0:
                if cur.strip():
                    items.append(_MiniYaml._coerce(cur.strip()))
                cur = ""
            else:
                if ch in "[{":
                    depth += 1
                elif ch in "]}":
                    depth -= 1
                cur += ch
        if cur.strip():
            items.append(_MiniYaml._coerce(cur.strip()))
        return items


def load_yaml(path: str) -> Optional[dict]:
    """cascade 加载 YAML。文件不存在返回 None；解析错误打 warning 后返回 None。"""
    if not os.path.exists(path):
        return None
    try:
        with open(path, "r", encoding="utf-8", errors="ignore") as f:
            text = f.read()
        return _yaml_loads(text)
    except Exception as exc:
        print(f"[audit_llm_common] WARN load_yaml failed for {path}: {exc}", file=sys.stderr)
        return None


# ============================================================================
# 1. 错误类型
# ============================================================================

class LLMError(Exception):
    """所有 LLM 调用错误的基类。"""


class AuthenticationError(LLMError):
    """401 / 403 — 不重试，立即 raise。"""


class NonRetryableHTTPError(LLMError):
    """其他 4xx（除 401/403/408/429）— 不重试。"""


class LLMCallExhausted(LLMError):
    """重试 N 次后仍失败。"""


# ============================================================================
# 2. LLMConfig & HTTP 调用
# ============================================================================

class LLMConfig:
    """LLM 调用配置。设计见 §3.1.1。"""

    def __init__(
        self,
        api_key: str = "",
        base_url: str = "https://api.openai.com/v1",
        model: str = "gpt-4o-mini",
        max_tokens: int = 1024,
        timeout: int = 600,
        retries: int = 3,
        retry_sleep: int = 3,
        json_mode: bool = True,
        reasoning_effort: Optional[str] = None,
        audit_kind: str = "case_quality",
        temperature: float = 0.2,
    ) -> None:
        self.api_key = api_key
        self.base_url = base_url.rstrip("/")
        self.model = model
        self.max_tokens = max_tokens
        self.timeout = timeout
        self.retries = retries
        self.retry_sleep = retry_sleep
        self.json_mode = json_mode
        self.reasoning_effort = reasoning_effort
        self.audit_kind = audit_kind
        self.temperature = temperature

    @classmethod
    def from_env(
        cls,
        env: Optional[Dict[str, str]] = None,
        *,
        max_tokens: int = 1024,
        timeout: int = 600,
        retries: int = 3,
        retry_sleep: int = 3,
        audit_kind: str = "case_quality",
    ) -> "LLMConfig":
        if env is None:
            env = os.environ
            _ensure_dotenv_loaded(env)
        base_url = env.get("OPENAI_BASE_URL", "https://api.openai.com/v1")
        # base_url → default model auto-mapping
        # 避免 "default gpt-4o-mini 发到 DeepSeek API → 200+空 content" 的乌龙
        if "deepseek" in base_url.lower():
            default_model = "deepseek-chat"
        elif "dashscope" in base_url.lower() or "aliyun" in base_url.lower():
            default_model = "qwen-turbo"
        elif "anthropic" in base_url.lower():
            default_model = "claude-3-5-sonnet-latest"
        else:
            default_model = "gpt-4o-mini"
        return cls(
            api_key=env.get("OPENAI_API_KEY", "") or env.get("LLM_API_KEY", ""),
            base_url=base_url,
            model=env.get("OPENAI_MODEL", env.get("LLM_MODEL", default_model)),
            max_tokens=int(env.get("OPENAI_MAX_TOKENS", str(max_tokens))),
            timeout=int(env.get("OPENAI_TIMEOUT", str(timeout))),
            retries=int(env.get("OPENAI_RETRIES", str(retries))),
            retry_sleep=int(env.get("OPENAI_RETRY_SLEEP", str(retry_sleep))),
            json_mode=(env.get("OPENAI_JSON_MODE", "1").lower() not in ("0", "false", "no")),
            reasoning_effort=env.get("OPENAI_REASONING_EFFORT") or None,
            audit_kind=audit_kind,
            temperature=float(env.get("OPENAI_TEMPERATURE", "0.2")),
        )

    def is_ready(self) -> bool:
        return bool(self.api_key)

    def to_payload(self, user_prompt: str, system_prompt: str, *, max_tokens: Optional[int] = None) -> dict:
        messages = []
        if system_prompt:
            messages.append({"role": "system", "content": system_prompt})
        messages.append({"role": "user", "content": user_prompt})
        payload: Dict[str, Any] = {
            "model": self.model,
            "messages": messages,
            "temperature": self.temperature,
            "max_tokens": max_tokens or self.max_tokens,
        }
        if self.json_mode:
            payload["response_format"] = {"type": "json_object"}
        # o1/o3 reasoning
        if self.reasoning_effort and ("o1" in self.model or "o3" in self.model):
            payload["reasoning_effort"] = self.reasoning_effort
        return payload


def _strip_markdown_fence(text: str) -> str:
    """剥离 ```` ```json ... ``` ```` 或 ```` ``` ... ``` ```` 围栏。"""
    s = text.strip()
    if s.startswith("```"):
        # 去掉首行 ``` / ```json
        first_nl = s.find("\n")
        if first_nl == -1:
            return s.strip("`").strip()
        s = s[first_nl + 1:]
        # 去掉尾部 ```
        if s.endswith("```"):
            s = s[:-3]
    return s.strip()


def call_llm_chat(
    config: LLMConfig,
    user_prompt: str,
    system_prompt: str = "",
    *,
    max_tokens_override: Optional[int] = None,
    reasoning_effort_override: Optional[str] = None,
    extra_payload: Optional[Dict[str, Any]] = None,
) -> Optional[dict]:
    """统一 HTTP 调用。返回已 parse 的 JSON dict；失败返回 None。

    行为契约（设计 §3.2）：
      - 401/403 → 立即 raise AuthenticationError
      - 408/429/5xx → 重试 retries 次，指数退避 + jitter
      - 其他 4xx → 立即 raise NonRetryableHTTPError
      - API key 缺失 / 网络错 / JSON 解析失败 → 返回 None
    """
    if not config.is_ready():
        print("[audit_llm_common] WARN no OPENAI_API_KEY, call_llm_chat returns None", file=sys.stderr)
        return None

    # reasoning model（deepseek-v4-pro / v4-flash / o1 / o3）在 json_mode 下
    # 经常把 token 花在 reasoning_content 上，content 为空或截断。
    # 对审计这种"严格 JSON 短输出"任务不友好 → 临时切到 non-reasoning model。
    _reasoning_keywords = ("-v4-pro", "-v4-pro-", "v4-pro", "v4-flash", "reasoner", "o1-", "o3-", "deepseek-r1")
    if any(kw in config.model.lower() for kw in _reasoning_keywords):
        if "deepseek" in config.base_url.lower():
            fallback_model = "deepseek-chat"
        elif "dashscope" in config.base_url.lower():
            fallback_model = "qwen-turbo"
        else:
            fallback_model = "gpt-4o-mini"
        config = LLMConfig(
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

    url = f"{config.base_url}/chat/completions"
    payload = config.to_payload(user_prompt, system_prompt, max_tokens=max_tokens_override)
    if reasoning_effort_override is not None:
        payload["reasoning_effort"] = reasoning_effort_override
    if extra_payload:
        payload.update(extra_payload)
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")

    last_err: Optional[Exception] = None
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
            # reasoning model (deepseek-reasoner / o1 / o3) 把"思考"放在 reasoning_content
            # 字段，"回答"放在 content 字段。response_format=json_object + reasoning
            # model 经常导致 content 为空、reasoning 里有完整 JSON — 回退到 reasoning_content
            # 拿最后一段 ```json ... ``` 块
            if not content.strip() and message.get("reasoning_content"):
                rc = message["reasoning_content"]
                # 取最后 ```json ... ``` 块
                m = re.search(r"```(?:json)?\s*(\{.*?\})\s*```", rc, re.DOTALL)
                if m:
                    content = m.group(1)
                else:
                    # 没围栏就尝试拿最后一个 {...} 块
                    m2 = re.search(r"\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}", rc, re.DOTALL)
                    if m2:
                        content = m2.group(0)
            content = _strip_markdown_fence(content)
            try:
                return json.loads(content)
            except json.JSONDecodeError as exc:
                # JSON 解析失败 → 不重试（重试只会拿到同样格式错误的字符串）
                # debug: 打印 finish_reason + 完整 raw，方便排查 200+空 content 等诡异问题
                finish_reason = (
                    data.get("choices", [{}])[0].get("finish_reason", "?")
                )
                usage = data.get("usage", {})
                print(
                    f"[audit_llm_common] WARN LLM returned non-JSON: {exc}; "
                    f"finish_reason={finish_reason} usage={usage} "
                    f"raw_resp[:300]={raw[:300]!r}",
                    file=sys.stderr,
                )
                return None
        except urllib.error.HTTPError as exc:
            code = getattr(exc, "code", 0)
            if code in (401, 403):
                raise AuthenticationError(f"HTTP {code}: {exc.reason}") from exc
            if code in (408, 429, 500, 502, 503, 504):
                last_err = exc
                sleep_s = config.retry_sleep * (2 ** attempt) + random.uniform(0, 1)
                print(f"[audit_llm_common] WARN HTTP {code} attempt {attempt + 1}/{config.retries}, sleep {sleep_s:.1f}s", file=sys.stderr)
                time.sleep(sleep_s)
                continue
            # NonRetryable 4xx：把 response body 带出来，方便定位 schema 错
            try:
                body = exc.read().decode("utf-8", errors="ignore")[:500]
            except Exception:
                body = "<no body readable>"
            raise NonRetryableHTTPError(f"HTTP {code}: {exc.reason}; body[:500]={body}") from exc
        except urllib.error.URLError as exc:
            last_err = exc
            sleep_s = config.retry_sleep * (2 ** attempt) + random.uniform(0, 1)
            print(f"[audit_llm_common] WARN URLError attempt {attempt + 1}/{config.retries}, sleep {sleep_s:.1f}s: {exc}", file=sys.stderr)
            time.sleep(sleep_s)
        except Exception as exc:  # noqa: BLE001
            last_err = exc
            sleep_s = config.retry_sleep * (2 ** attempt) + random.uniform(0, 1)
            print(f"[audit_llm_common] WARN unexpected attempt {attempt + 1}/{config.retries}, sleep {sleep_s:.1f}s: {exc}", file=sys.stderr)
            time.sleep(sleep_s)
    raise LLMCallExhausted(f"exhausted {config.retries} retries, last error: {last_err}")


def add_llm_args(parser: argparse.ArgumentParser) -> None:
    """注册 audit 脚本共用的 LLM CLI 参数。"""
    parser.add_argument("--llm-audit", action="store_true", help="对每个 case 调 LLM 判真实性。")
    parser.add_argument("--require-llm-audit", action="store_true", help="LLM 调用失败时 exit(1)。")
    parser.add_argument("--allow-llm-missing", action="store_true", help="API key 缺失时降级到 lint-only（不阻塞）。")
    parser.add_argument("--llm-model", default=None, help="覆盖 OPENAI_MODEL。")
    parser.add_argument("--llm-base-url", default=None, help="覆盖 OPENAI_BASE_URL。")
    parser.add_argument("--strict-drift", action="store_true", help="preflight 有漂移 finding 时 exit(1)。")
    parser.add_argument("--drift-allow", default="", help="逗号分隔 plugin 白名单，preflight 不视为漂移。")


def has_llm_available(env: Optional[Dict[str, str]] = None) -> bool:
    if env is None:
        env = os.environ
    return bool(env.get("OPENAI_API_KEY", "") or env.get("LLM_API_KEY", ""))


# ============================================================================
# 3. 知识库加载
# ============================================================================

def load_project_kb(path: str = "tokenslim_kb/project.yaml") -> dict:
    """加载 project.yaml。**唯一会 raise 的 KB 加载函数**（项目身份卡必须有）。"""
    data = load_yaml(path)
    if data is None:
        raise FileNotFoundError(
            f"project.yaml not found at {path}. audit_llm_common requires it as the project identity card."
        )
    return data


def load_plugin_capability_index(path: str = "docs/audit/plugin_capability_index.json") -> Optional[dict]:
    """加载生成器写好的 plugin_capability_index.json。"""
    return read_json_file(path)


def find_plugin_in_index(index: Optional[dict], plugin: str) -> Optional[dict]:
    """从索引中找单个 plugin 记录。plugin 可以是 `shell_session_plugin` 或 `shell_session`。"""
    if not index or "plugins" not in index:
        return None
    targets = {plugin}
    if plugin.endswith("_plugin"):
        targets.add(plugin[: -len("_plugin")])
    else:
        targets.add(f"{plugin}_plugin")
    for p in index["plugins"]:
        if p.get("name") in targets:
            return p
    return None


def plugin_index_to_narrative(rec: Optional[dict]) -> str:
    """把 {description, capability_tags, detect_patterns, route_keywords, priority} 拼成 LLM narrative。"""
    if not rec:
        return "[no plugin capability index record found; judge on structural rules only]"
    parts: List[str] = []
    name = rec.get("name", "?")
    parts.append(f"Plugin: {name}")
    # 缺 type 字段时按 _plugin 后缀推断
    if "type" in rec and rec["type"]:
        parts.append(f"Type: {rec['type']}")
    if "priority" in rec and rec["priority"] is not None:
        parts.append(f"Priority: {rec['priority']}")
    if "description" in rec and rec["description"]:
        parts.append(f"Mission: {rec['description']}")
    if "capability_tags" in rec and rec["capability_tags"]:
        parts.append(f"Capability tags: {', '.join(rec['capability_tags'])}")
    if "detect_patterns" in rec and rec["detect_patterns"]:
        parts.append(f"Detection: {', '.join(rec['detect_patterns'])}")
    if "route_keywords" in rec and rec["route_keywords"]:
        parts.append(f"Routes first to: {', '.join(rec['route_keywords'])}")
    return "\n".join(parts)


def load_plugin_compress_config(plugin: str, config_dir: str = "config/plugins") -> Optional[dict]:
    """加载 config/plugins/<name>.json 的完整配置。

    plugin 接受 `shell_session_plugin` 或 `shell_session`（自动去 `_plugin`）。
    返回 {"detect": {...}, "compress": {...}, "description": "...",
           "keep_signals": [...], "compress_targets": [...], "design_intent": "..."} 或 None。
    """
    name = plugin[:-len("_plugin")] if plugin.endswith("_plugin") else plugin
    candidates = [
        os.path.join(config_dir, f"{name}.json"),
    ]
    for path in candidates:
        if os.path.exists(path):
            data = read_json_file(path)
            if data:
                return {
                    "description": data.get("description", ""),
                    "detect": data.get("detect", {}),
                    "compress": data.get("compress", {}),
                    "decompress": data.get("decompress", {}),
                    "keep_signals": data.get("keep_signals", []),
                    "compress_targets": data.get("compress_targets", []),
                    "design_intent": data.get("design_intent", ""),
                }
    return None


def plugin_config_to_narrative(config: Optional[dict]) -> str:
    """把 config/plugins/<name>.json 转成 LLM 可理解的叙事。

    优先使用 design_intent / keep_signals / compress_targets（从 methods.rs
    代码分析提取的精确知识），fallback 到 detect/compress 段的配置字段。

    这是 fill 和 audit prompt 的"插件能力锚点"——告诉 LLM 这个插件
    保留什么、压缩什么，让 LLM 写 scenario 和审计时有事实依据。
    """
    if not config:
        return "[no plugin config found; judge on plugin name only]"

    parts: List[str] = []

    # ── 优先路径：design_intent + keep_signals + compress_targets ──
    design_intent = config.get("design_intent", "")
    keep_signals = config.get("keep_signals", [])
    compress_targets = config.get("compress_targets", [])

    if design_intent:
        parts.append(f"Design intent: {design_intent}")

    if keep_signals:
        parts.append("Keep signals (保留):")
        for sig in keep_signals:
            parts.append(f"  - {sig}")

    if compress_targets:
        parts.append("Compress targets (压缩):")
        for tgt in compress_targets:
            parts.append(f"  - {tgt}")

    # 如果 design_intent + keep_signals + compress_targets 都有，直接返回
    if design_intent and (keep_signals or compress_targets):
        return "\n".join(parts)

    # ── fallback：从 detect/compress 段推断 ──
    desc = config.get("description", "")
    if desc and not design_intent:
        parts.append(f"Plugin mission: {desc}")

    detect = config.get("detect", {})
    if detect and not keep_signals:
        rules = detect.get("rules", [])
        patterns = []
        for rule in rules:
            if isinstance(rule, dict):
                pats = rule.get("patterns", [])
                if pats:
                    patterns.extend(pats[:5])
        if patterns:
            parts.append(f"Keep signals (detect patterns): {', '.join(patterns[:8])}")
        min_ratio = detect.get("min_match_ratio")
        if min_ratio is not None:
            parts.append(f"Detection threshold: {min_ratio*100:.0f}% of lines must match")

    compress = config.get("compress", {})
    if compress and not compress_targets:
        token_prefix = compress.get("token_prefix", "")
        if token_prefix:
            parts.append(f"Path compression: replaces long paths with {token_prefix}N tokens")

        path_patterns = compress.get("path_patterns", [])
        if path_patterns:
            parts.append(f"Path patterns compressed: {', '.join(path_patterns[:5])}")

        dedup = compress.get("dedup", {})
        if dedup and dedup.get("enabled"):
            threshold = dedup.get("threshold", 1)
            parts.append(f"Deduplication: consecutive identical lines beyond {threshold} are folded")

        for key in ("macro_patterns", "gradle_task_patterns", "resource_patterns",
                     "stack_trace_patterns", "error_patterns"):
            val = compress.get(key)
            if val and isinstance(val, list):
                parts.append(f"{key}: {', '.join(str(v) for v in val[:5])}")

    decompress = config.get("decompress", {})
    if decompress:
        prefixes = decompress.get("token_prefixes", [])
        if prefixes:
            parts.append(f"Decompression tokens: {', '.join(prefixes[:5])}")

    return "\n".join(parts) if parts else "[plugin config has no design details]"


def load_case_scenario_sidecar(samples_dir: str, plugin: str, case_id: str) -> Optional[dict]:
    """加载 samples/<plugin>/<case_id>.scenario.yaml。缺返回 None。

    plugin 接受 `shell_session_plugin` 或 `shell_session`（自动补 `_plugin`）。
    sidecar 命名规则：与 .log 文件 stem 相同 + .scenario.yaml。
    """
    plugin_norm = plugin[:-len("_plugin")] if plugin.endswith("_plugin") else plugin
    candidates = [
        os.path.join(samples_dir, f"{plugin_norm}_plugin", f"{case_id}.scenario.yaml"),
        os.path.join(samples_dir, plugin_norm, f"{case_id}.scenario.yaml"),
    ]
    for path in candidates:
        if os.path.exists(path):
            return load_yaml(path)
    return None


# ============================================================================
# 4. 源码 / 物理文件读取层（preflight 用）
# ============================================================================

_LINE_COMMENT_RE = re.compile(r'//[^\n]*')
_BLOCK_COMMENT_RE = re.compile(r'/\*.*?\*/', re.DOTALL)
_REGISTRY_RE = re.compile(
    r'^\s*pub\s+mod\s+([a-z][a-z0-9_]*)\s*;',
    re.MULTILINE,
)


def _strip_rust_comments(src: str) -> str:
    """剥 Rust 注释，避免行内 `// pub mod foo;` / `/* pub mod foo; */` 误匹配。"""
    src = _BLOCK_COMMENT_RE.sub('', src)
    src = _LINE_COMMENT_RE.sub('', src)
    return src


def parse_mod_rs_registry(mod_rs_path: str) -> Set[str]:
    """从 src/plugins/mod.rs 抓取所有 `pub mod <name>;` 声明。

    护栏：
      1. 剥 // 与 /* */ 注释
      2. 限定 name 模式 `^[a-z][a-z0-9_]*$`（合法 Rust 模块标识符）
      3. 调用方负责维护白名单烟雾测试（见 tests/test_audit_llm_common.py）
    """
    if not os.path.exists(mod_rs_path):
        return set()
    raw = read_text(mod_rs_path)
    if not raw:
        return set()
    src = _strip_rust_comments(raw)
    return {m.group(1) for m in _REGISTRY_RE.finditer(src)}


# 合法 case 文件后缀清单 — fix: case 可能是 .log/.json/.hex/.md/.xml/.txt 等多种后缀
# （实测 samples/ 下有 6 种：.log=1106, .json=14, .hex=9, .md=9, .xml=7, .txt=3）。
# 排除 sidecar (.scenario.yaml) 和多扩展名元数据。
#
# 关键：前缀部分用 [^.]*（无点），否则贪婪 .* 会吞掉中间的点，导致
# `case_xxx.log.bak`（备份）被误识别为合法 case。
_CASE_FILE_RE = re.compile(r'^case_[^.]*\.(?!scenario\.yaml$)[^.]+$')


def walk_physical_samples(samples_dir: str = "samples") -> Dict[str, List[str]]:
    """samples/ → {plugin_dir_name: [case_filename, ...]}。

    统计所有 ``case_*.{log,json,hex,md,xml,txt,...}`` 物理 case。
    排除 sidecar ``.scenario.yaml`` 和多扩展名 ``.tar.gz`` 类元数据。
    与 generate_plugin_capability_index.count_case_files / list_ghost_cases 同语义。
    """
    out: Dict[str, List[str]] = {}
    if not os.path.isdir(samples_dir):
        return out
    for entry in sorted(os.listdir(samples_dir)):
        full = os.path.join(samples_dir, entry)
        if not os.path.isdir(full):
            continue
        try:
            files = sorted(
                f for f in os.listdir(full)
                if os.path.isfile(os.path.join(full, f)) and _CASE_FILE_RE.match(f)
            )
        except OSError:
            files = []
        out[entry] = files
    return out


def walk_physical_samples_with_sidecar(samples_dir: str = "samples") -> Dict[str, Dict[str, bool]]:
    """samples/ → {plugin_dir_name: {case_filename: has_sidecar_bool}}。"""
    out: Dict[str, Dict[str, bool]] = {}
    base = walk_physical_samples(samples_dir)
    for plugin, files in base.items():
        sub = os.path.join(samples_dir, plugin)
        flags: Dict[str, bool] = {}
        for fn in files:
            stem = pathlib.Path(fn).stem
            sidecar = os.path.join(sub, f"{stem}.scenario.yaml")
            flags[fn] = os.path.exists(sidecar)
        out[plugin] = flags
    return out


def list_ghost_cases(
    samples_dir: str,
    plugin: str,
    source_dir: str = "src/plugins",
) -> List[str]:
    """在 showcase.rs 登记但 samples/<plugin>/ 下找不到 .log 文件的 case_id 列表。"""
    try:
        from audit_case_metrics import parse_showcase_rs_cases  # type: ignore
    except Exception:
        return []
    cases = parse_showcase_rs_cases(plugin, source_dir=source_dir) or []
    if not cases:
        return []
    # 物理 case
    plugin_norm = plugin[:-len("_plugin")] if plugin.endswith("_plugin") else plugin
    physical_dirs = [
        os.path.join(samples_dir, f"{plugin_norm}_plugin"),
        os.path.join(samples_dir, plugin_norm),
    ]
    physical_stems: Set[str] = set()
    for d in physical_dirs:
        if not os.path.isdir(d):
            continue
        for f in os.listdir(d):
            if re.match(r'^case_[^.]*\.(?!scenario\.yaml$)[^.]+$', f):
                physical_stems.add(pathlib.Path(f).stem)
    ghosts: List[str] = []
    for c in cases:
        case_id = c.get("case_id", "")
        if case_id and case_id not in physical_stems:
            ghosts.append(case_id)
    return ghosts


def find_capability_index_stale(
    samples_dir: str = "samples",
    source_dir: str = "src/plugins",
    index_path: str = "docs/audit/plugin_capability_index.json",
) -> Optional[Dict[str, Any]]:
    """比较 samples/ + src/ 最新 mtime 与 plugin_capability_index.json 的 generated_at。"""
    idx = read_json_file(index_path)
    if not idx:
        return None
    gen_at_str = idx.get("generated_at")
    if not gen_at_str:
        return None
    try:
        gen_at = datetime.datetime.fromisoformat(gen_at_str)
    except ValueError:
        return None
    gen_ts = gen_at.timestamp()

    newer_paths: List[Tuple[str, float]] = []
    for root in (samples_dir, source_dir):
        if not os.path.isdir(root):
            continue
        for dirpath, _, filenames in os.walk(root):
            for fn in filenames:
                fp = os.path.join(dirpath, fn)
                try:
                    m = os.path.getmtime(fp)
                except OSError:
                    continue
                if m > gen_ts + 1:  # 1s 容差
                    newer_paths.append((fp, m))
    if not newer_paths:
        return None
    # 取最 new 的 5 条
    newer_paths.sort(key=lambda x: x[1], reverse=True)
    return {
        "index_path": index_path,
        "generated_at": gen_at_str,
        "newer_files": [p for p, _ in newer_paths[:5]],
        "max_age_seconds": max(int(m - gen_ts) for _, m in newer_paths),
    }


# ============================================================================
# 5. Preflight Drift Detection
# ============================================================================

def drift_audit(
    plugin: str,
    *,
    samples_dir: str = "samples",
    source_dir: str = "src/plugins",
    mod_rs_path: str = "src/plugins/mod.rs",
    cap_index_path: str = "docs/audit/plugin_capability_index.json",
) -> List[Dict[str, Any]]:
    """聚合 7 条漂移轴（设计 §12.2）。返回 findings 列表（不抛异常）。"""
    findings: List[Dict[str, Any]] = []
    plugin_norm = plugin[:-len("_plugin")] if plugin.endswith("_plugin") else plugin

    # 1+7. mod.rs ↔ samples/ 双向 diff
    try:
        mod_plugins = parse_mod_rs_registry(mod_rs_path)
    except Exception as exc:  # noqa: BLE001
        findings.append({
            "axis": "mod-rs-parse-failed",
            "severity": "warning",
            "message": f"Failed to parse {mod_rs_path}: {exc}",
        })
        mod_plugins = set()

    physical = walk_physical_samples(samples_dir)
    physical_dirs = set(physical.keys())
    # 物理目录名 → 标准化 plugin 名（去 _plugin）
    physical_norm = {p[:-len("_plugin")] if p.endswith("_plugin") else p for p in physical_dirs}
    mod_norm = {p[:-len("_plugin")] if p.endswith("_plugin") else p for p in mod_plugins}

    extra_in_samples = physical_norm - mod_norm
    missing_in_samples = mod_norm - physical_norm
    for p in sorted(extra_in_samples):
        findings.append({
            "axis": "samples-vs-mod-rs",
            "severity": "warning",
            "message": f"Plugin '{p}' has samples in {samples_dir}/{p}_plugin/ but is NOT declared in {mod_rs_path}",
        })
    for p in sorted(missing_in_samples):
        # mod.rs 有但 samples/ 没有 → info（很多 plugin 还没补 sample）
        findings.append({
            "axis": "samples-vs-mod-rs",
            "severity": "info",
            "message": f"Plugin '{p}' declared in {mod_rs_path} but no samples/ directory at {samples_dir}/{p}_plugin/",
        })

    # 2. 物理 case 数 ≠ showcase.rs 登记数
    try:
        from generate_plugin_capability_index import (  # type: ignore
            count_case_files,
            parse_showcase_rs_cases,
        )
        physical_n = count_case_files(plugin, samples_dir)
        showcase = parse_showcase_rs_cases(plugin, source_dir=source_dir) or []
        showcase_n = len(showcase)
        if physical_n != showcase_n:
            findings.append({
                "axis": "case-count-mismatch",
                "severity": "warning",
                "message": f"{plugin}: physical cases = {physical_n}, showcase.rs cases = {showcase_n}",
            })
    except Exception as exc:  # noqa: BLE001
        findings.append({
            "axis": "case-count-lookup-failed",
            "severity": "warning",
            "message": f"Failed to compare case counts for {plugin}: {exc}",
        })

    # 3. case 缺 sidecar（plugin 维度）
    flags = walk_physical_samples_with_sidecar(samples_dir).get(f"{plugin_norm}_plugin", {})
    missing_sidecar = [fn for fn, has in flags.items() if not has]
    if missing_sidecar:
        findings.append({
            "axis": "sidecar-missing",
            "severity": "warning",
            "message": f"{plugin}: {len(missing_sidecar)} case(s) missing scenario sidecar",
            "cases": missing_sidecar,
        })

    # 4. ghost case（showcase 有物理无）
    try:
        ghosts = list_ghost_cases(samples_dir, plugin, source_dir=source_dir)
        if ghosts:
            findings.append({
                "axis": "ghost-case",
                "severity": "warning",
                "message": f"{plugin}: {len(ghosts)} case(s) in showcase.rs but no physical sample",
                "cases": ghosts,
            })
    except Exception as exc:  # noqa: BLE001
        findings.append({
            "axis": "ghost-case-lookup-failed",
            "severity": "warning",
            "message": f"Failed to list ghost cases for {plugin}: {exc}",
        })

    # 5. plugin 字典缺失（cap_index 漏该 plugin）
    try:
        idx = read_json_file(cap_index_path)
        if idx is None:
            findings.append({
                "axis": "cap-index-missing",
                "severity": "warning",
                "message": f"{cap_index_path} not found. Run scripts/generate_plugin_capability_index.py first.",
            })
        elif not find_plugin_in_index(idx, plugin):
            findings.append({
                "axis": "plugin-missing-in-index",
                "severity": "warning",
                "message": f"{plugin} not found in {cap_index_path}. Re-run generate_plugin_capability_index.py.",
            })
    except Exception as exc:  # noqa: BLE001
        findings.append({
            "axis": "cap-index-lookup-failed",
            "severity": "warning",
            "message": f"Failed to inspect cap_index: {exc}",
        })

    # 6. cap_index 过期
    try:
        stale = find_capability_index_stale(samples_dir, source_dir, cap_index_path)
        if stale:
            findings.append({
                "axis": "cap-index-stale",
                "severity": "info",
                "message": f"{cap_index_path} is older than {stale['max_age_seconds']}s; newer files: {stale['newer_files'][:2]}",
                "details": stale,
            })
    except Exception as exc:  # noqa: BLE001
        findings.append({
            "axis": "cap-index-staleness-lookup-failed",
            "severity": "warning",
            "message": f"Failed staleness check: {exc}",
        })

    return findings


def write_drift_report(findings: List[Dict[str, Any]], out_path: str) -> None:
    """落盘 preflight 报告（设计 §12.6）。"""
    summary = {"warning": 0, "info": 0, "error": 0}
    for f in findings:
        sev = f.get("severity", "warning")
        summary[sev] = summary.get(sev, 0) + 1
    obj = {
        "generated_at": datetime.datetime.now().isoformat(timespec="seconds"),
        "findings": findings,
        "summary": summary,
    }
    atomic_write_json(out_path, obj)


# ============================================================================
# 6. 提示词模板加载（cascade）
# ============================================================================

# 内嵌 fallback（最末兜底 — 设计 §6.1）
# 占位符语义见 build_case_quality_prompt / build_case_metrics_prompt 的 mapping。
EMBEDDED_FALLBACK_BASE = """# Your Identity
You are {{ project.display_name }}'s sample case quality auditor.
{{ project.display_name }}: {{ project.mission }}
{{ project.tagline }}

# Current target
- Plugin: {{ plugin.name }} (type: {{ plugin.type }})
- Plugin narrative: {{ plugin.narrative }}

# Scenario context (from sidecar)
- Scenario: {{ scenario.scenario }}
- Target capability: {{ scenario.target_capability }}
- Expected keep: {{ scenario.expected_keep }}
- Expected compress: {{ scenario.expected_compress }}

# Tactical rules (case_metrics only)
{{ tactical_rules }}

Judge whether the case is real, well-shaped, and decision-useful for compression
of the named plugin's target output.
"""

EMBEDDED_FALLBACK_FOOTER = """# Required JSON schema
{"status": "valid|needs_fix|duplicate|too_small|not_registered|missing_anchor|weak_coverage|routing_boundary_unclear|fabricated", "confidence": 0.0-1.0, "explanation": "..."}
# Output: JSON only.
"""

EMBEDDED_FALLBACK_TYPE_RULES = {
    "shell": "R1: shell prompt style consistent. R2: command/exit_code aligned. R3: error block contiguous.",
    "access_log": "R1: timestamp monotonic. R2: status code distribution plausible. R3: path format matches server type.",
    "data_struct": "R1: schema consistent. R2: field types preserved. R3: indentation level matches dialect.",
    "vcs": "R1: revision IDs unique. R2: commit messages plausible. R3: file paths under repo root.",
    "build": "R1: error block contiguous. R2: file:line format. R3: compiler name/version present.",
    "error_trace": "R1: exception class + message present. R2: frames in source order. R3: cause chain present.",
    "default": "R1: structure consistent with declared format. R2: error/info separation clear. R3: locale plausible.",
}


def _strip_leading_yaml_comments(text: str) -> str:
    """剥掉文本开头的 `# ...` 注释行（保留第一个非 `#` 起始行起的内容）。

    约定：模板作者可以放注释头解释"这块放什么"，加载时清掉不让 LLM 看到。
    不会动中段的 `#` 行 — 中段 `#` 视作内容。
    """
    lines = text.splitlines()
    out: List[str] = []
    seen_content = False
    for ln in lines:
        if not seen_content and (not ln.strip() or ln.lstrip().startswith("#")):
            continue
        seen_content = True
        out.append(ln)
    # 保留尾部空行整洁
    while out and not out[-1].strip():
        out.pop()
    return "\n".join(out)


def load_prompt_template(audit_kind: str, plugin_type: str) -> Tuple[str, str]:
    """cascade 加载 (base, type_specific) 模板。

    顺序：磁盘文件 > 嵌入 default。
    路径约定：scripts/prompts/audit/<audit_kind>/<name>.md
    """
    base = ""
    type_specific = ""
    base_candidates = [
        os.path.join("scripts", "prompts", "audit", audit_kind, "_base.md"),
        os.path.join("prompts", "audit", audit_kind, "_base.md"),
    ]
    for p in base_candidates:
        t = read_text(p)
        if t:
            base = _strip_leading_yaml_comments(t)
            break
    if not base:
        base = EMBEDDED_FALLBACK_BASE

    type_candidates = [
        os.path.join("scripts", "prompts", "audit", audit_kind, f"{plugin_type}.md"),
        os.path.join("prompts", "audit", audit_kind, f"{plugin_type}.md"),
        os.path.join("scripts", "prompts", "audit", audit_kind, "default.md"),
        os.path.join("prompts", "audit", audit_kind, "default.md"),
    ]
    for p in type_candidates:
        t = read_text(p)
        if t:
            type_specific = _strip_leading_yaml_comments(t)
            break
    if not type_specific:
        type_specific = EMBEDDED_FALLBACK_TYPE_RULES.get(
            plugin_type, EMBEDDED_FALLBACK_TYPE_RULES["default"]
        )
    return base, type_specific


def _safe_str(v: Any, default: str = "") -> str:
    if v is None:
        return default
    if isinstance(v, str):
        return v
    if isinstance(v, list):
        return ", ".join(str(x) for x in v)
    if isinstance(v, dict):
        return json.dumps(v, ensure_ascii=False)
    return str(v)


def _render_template(template: str, mapping: Dict[str, str]) -> str:
    """简单的 {{ key }} 替换。"""
    out = template
    for k, v in mapping.items():
        out = out.replace("{{ " + k + " }}", v)
    return out


def build_case_quality_prompt(
    plugin_type: str,
    plugin: str = "",
    case_id: str = "",
    kb: Optional[dict] = None,
    *,
    samples_dir: str = "samples",
) -> str:
    """给 audit_sample_case_quality.py 用的 system prompt 拼装器。

    kb=None 时按需从 tokenslim_kb/ + samples/<plugin>/sidecar 加载。
    """
    # 1. project
    try:
        project = kb["project"] if kb and "project" in kb else load_project_kb()
    except FileNotFoundError:
        project = {"display_name": "TokenSlim", "mission": "compress structured logs", "tagline": ""}

    # 2. architecture（可选）
    architecture = (kb or {}).get("architecture") or load_yaml("tokenslim_kb/architecture.yaml") or {}

    # 3. plugin narrative (from capability index)
    plugin_narrative = ""
    if plugin:
        idx = (kb or {}).get("plugin_index") or load_plugin_capability_index()
        plugin_narrative = plugin_index_to_narrative(find_plugin_in_index(idx, plugin))

    # 3b. plugin compress config (from config/plugins/<name>.json)
    # 这是"插件能力锚点"——告诉 LLM 这个插件保留什么、压缩什么
    plugin_compress_narrative = ""
    if plugin:
        pcc = (kb or {}).get("plugin_compress_config")
        if pcc is None:
            pcc = load_plugin_compress_config(plugin)
        if pcc:
            plugin_compress_narrative = plugin_config_to_narrative(pcc)

    # 4. scenario
    scenario = None
    if plugin and case_id:
        scenario = (kb or {}).get("scenario")
        if scenario is None:
            scenario = load_case_scenario_sidecar(samples_dir, plugin, case_id)

    # 5+6+7. cascade 加载 base / type / footer
    base, type_specific = load_prompt_template("case_quality", plugin_type)
    footer = read_text("scripts/prompts/audit/case_quality/_footer.md")
    if not footer:
        footer = read_text("prompts/audit/case_quality/_footer.md")
    if not footer:
        footer = EMBEDDED_FALLBACK_FOOTER

    mapping = {
        "project.display_name": _safe_str(project.get("display_name", "TokenSlim")),
        "project.mission": _safe_str(project.get("mission", "")),
        "project.tagline": _safe_str(project.get("tagline", "")),
        "plugin.name": plugin or "(unspecified)",
        "plugin.narrative": plugin_narrative or "[no plugin narrative available]",
        "plugin.compress_narrative": plugin_compress_narrative or "[no compress config available]",
        "plugin.type": plugin_type,
        "scenario.scenario": _safe_str((scenario or {}).get("scenario"), "[no scenario sidecar]"),
        "scenario.target_capability": _safe_str((scenario or {}).get("target_capability"), ""),
        "scenario.expected_keep": _safe_str((scenario or {}).get("expected_keep"), ""),
        "scenario.expected_compress": _safe_str((scenario or {}).get("expected_compress"), ""),
        "tactical_rules": "",  # case_quality 不需要，但 base 共用故补占位
    }
    if architecture:
        mapping["architecture.summary"] = _safe_str(architecture.get("routing", {}).get("principle", ""))

    sys_prompt = _render_template(base, mapping)
    sys_prompt += "\n\n" + type_specific
    sys_prompt += "\n\n" + footer
    return sys_prompt


def build_case_metrics_prompt(
    plugin_type: str,
    tactical_rules: str = "",
    kb: Optional[dict] = None,
) -> str:
    """给 audit_case_metrics.py 用的 system prompt 拼装器。"""
    try:
        project = kb["project"] if kb and "project" in kb else load_project_kb()
    except FileNotFoundError:
        project = {"display_name": "TokenSlim", "mission": "compress structured logs"}

    plugin_narrative = ""
    if kb and "plugin" in kb:
        plugin_narrative = plugin_index_to_narrative(kb["plugin"])

    base, type_specific = load_prompt_template("case_metrics", plugin_type)
    footer = read_text("scripts/prompts/audit/case_metrics/_footer.md")
    if not footer:
        footer = read_text("prompts/audit/case_metrics/_footer.md")
    if not footer:
        footer = EMBEDDED_FALLBACK_FOOTER

    mapping = {
        "project.display_name": _safe_str(project.get("display_name", "TokenSlim")),
        "project.mission": _safe_str(project.get("mission", "")),
        "project.tagline": _safe_str(project.get("tagline", "")),
        "plugin.narrative": plugin_narrative or "[no plugin narrative available]",
        "plugin.type": plugin_type,
        "tactical_rules": tactical_rules.strip() or "No plugin-specific tactical rules were found. Apply the stable audit constitution above.",
    }
    sys_prompt = _render_template(base, mapping)
    sys_prompt += "\n\n" + type_specific
    sys_prompt += "\n\n" + footer
    return sys_prompt


# ============================================================================
# 7. 烟雾测试自检（python -m audit_llm_common）
# ============================================================================

_SMOKE_TEST = """
[synthetic mod.rs]
//! Statically linked plugins.

pub mod android_gradle_plugin;
/* block comment with 'pub mod ghost_plugin;' */
pub mod shell_session_plugin;
pub mod vcs_git_plugin;
"""

if __name__ == "__main__":
    # 烟雾测试：parse_mod_rs_registry
    import tempfile as _tf
    with _tf.NamedTemporaryFile("w", suffix=".rs", delete=False, encoding="utf-8") as f:
        f.write(_SMOKE_TEST)
        rs_path = f.name
    got = parse_mod_rs_registry(rs_path)
    expected = {"android_gradle_plugin", "shell_session_plugin", "vcs_git_plugin"}
    print("parse_mod_rs_registry smoke test:", got)
    assert got == expected, f"expected {expected}, got {got}"
    os.unlink(rs_path)

    # walk_physical_samples smoke test
    with _tf.TemporaryDirectory() as tmp:
        # samples/foo_plugin/case_001_xxx.log
        os.makedirs(os.path.join(tmp, "shell_session_plugin"))
        with open(os.path.join(tmp, "shell_session_plugin", "case_001_xxx.log"), "w") as f:
            f.write("hello")
        with open(os.path.join(tmp, "shell_session_plugin", "case_001_xxx.scenario.yaml"), "w") as f:
            f.write("case_id: case_001_xxx\n")
        ws = walk_physical_samples_with_sidecar(tmp)
        print("walk_physical_samples_with_sidecar:", ws)
        assert "shell_session_plugin" in ws
        assert ws["shell_session_plugin"]["case_001_xxx.log"] is True

    # _yaml_loads smoke（PyYAML 优先）
    y = _yaml_loads("a: 1\nb: [x, y]\n# c\nd:\n  e: f\ng:\n- h\n- i\n")
    print("_yaml_loads:", y)
    assert y["a"] == 1
    assert y["b"] == ["x", "y"]
    assert y["d"]["e"] == "f"
    assert y["g"] == ["h", "i"]

    # drift_audit 烟雾（用空 samples/）
    with _tf.TemporaryDirectory() as tmp:
        f = drift_audit(
            "shell_session_plugin",
            samples_dir=tmp,
            source_dir=tmp,
            mod_rs_path=os.path.join(tmp, "nonexistent.rs"),
            cap_index_path=os.path.join(tmp, "nonexistent.json"),
        )
        print("drift_audit on empty:", f)
    print("ALL SMOKE TESTS PASSED")
