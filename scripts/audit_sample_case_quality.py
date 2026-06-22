#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
audit_sample_case_quality.py
============================

TokenSlim 原始 sample case 质量审计脚本（压缩审计**前置**脚本）。

定位
----
与现有脚本分工：

* `audit_sample_case_quality.py`（本脚本）—— 审计 ``samples/<plugin>/`` 下的
  物理 sample 本身是否合格。这是压缩审计**之前**的门禁，先证明 case 是好 case。
* `audit_case_metrics.py` —— 审计 ``target/<plugin>_compact_showcase_report.txt``
  中 original vs compact 的对齐、压缩、语义保真与冻结。
* `audit_all_case_metrics.py` —— 聚合执行 + 全局健康报告。
* `generate_plugin_alignment_report.py` —— 插件 / 配置 / 能力矩阵对齐。

运行顺序
--------
1. ``tokenslim run python scripts/audit_sample_case_quality.py --plugin shell_session_plugin``
2. 修正 / 删除 / 合并低质量 case，补齐缺失命令族。
3. 全部注册到 ``showcase.rs``。
4. ``tokenslim run cargo test <plugin>``
5. ``tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --require-semantic-gate``
6. 冻结。

适用检查项（每个 case）
----------------------
* 物理 case 是否都进入 ``showcase.rs``。
* case 文件名、title、内容是否一致。
* 是否真的是 shell session，而不是单行玩具文本。
* 是否包含明确命令锚点。
* 是否覆盖目标命令族（ls/dir/tree/cp/rm/grep/ps/curl/rsync/PowerShell/CMD …）。
* 是否包含 stdout/stderr/exit code 线索。
* 是否有重复 case 或近似重复 case。
* 是否过短，无法证明压缩能力。
* 是否只是为了凑数，内容不真实。
* 是否包含敏感信息或不该出现的真实 token。
* 是否有 mojibake / 编码问题。
* 是否和专用插件边界冲突（git/cargo/kubectl/dot-net-mvn/pytest/terraform/helm
  等应作为路由让渡 case，而不是 shell 插件吞掉业务语义）。

LLM 质量判定（不直接覆盖样本）
-------------------------------
LLM 输出 ``pass / needs_fix / duplicate / too_small / not_registered /
missing_anchor / weak_coverage / routing_boundary_unclear``，并附修正建议。
但 LLM **不**直接批量改写样本——所有改动必须再过一遍：

1. deterministic lint（本脚本）
2. showcase 三向对齐（``audit_case_metrics.py``）
3. ``tokenslim run cargo test <plugin>``
4. semantic gate（``audit_case_metrics.py --require-semantic-gate``）
5. 冻结

输出产物
--------
* ``docs/audit/<plugin>/sample_quality/case_quality_report.json`` —— 机器可读报告。
* ``docs/audit/<plugin>/sample_quality/case_quality_report.md`` —— 人类可读报告。
* ``docs/audit/<plugin>/sample_quality/cases/case_XXX/quality.json`` —— 每个 case 的质量 snapshot。
* ``docs/audit/<plugin>/sample_quality/case_quality_latest.json`` —— 最新一次质量快照。
* ``docs/audit/<plugin>/sample_quality/command_family_coverage.json`` —— 命令族覆盖矩阵。

注意：所有产物统一放在 ``sample_quality/`` 子目录下，避免和
``audit_case_metrics.py`` 写出的 ``cases/`` / ``original.txt`` 等镜像
目录产生命名冲突或被压缩审计脚本覆盖（fix #6）。
"""

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
import difflib
import random
from datetime import datetime

# audit_llm_common：LLM 调用 + 提示词加载 + 漂移检测 公共模块
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import audit_llm_common as _alc  # noqa: E402

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# 让本脚本能 import 同目录下的 audit_case_metrics（fix #1：
# 不要复制 showcase 解析器，保持单一来源）。
_SCRIPTS_DIR = os.path.dirname(os.path.abspath(__file__))
if _SCRIPTS_DIR not in sys.path:
    sys.path.insert(0, _SCRIPTS_DIR)
try:
    from audit_case_metrics import (
        parse_showcase_rs_cases as _acm_parse_showcase_rs_cases,
        get_physical_samples as _acm_get_physical_samples,
    )
    # 公开重命名，让本脚本其余部分继续使用本地名
    parse_showcase_rs_cases = _acm_parse_showcase_rs_cases
    get_physical_samples = _acm_get_physical_samples
except ImportError as exc:  # pragma: no cover - 启动期硬错误
    raise ImportError(
        "audit_sample_case_quality.py 需要 import audit_case_metrics.py 中的"
        " parse_showcase_rs_cases / get_physical_samples 作为单一来源；"
        "请确认 scripts/audit_case_metrics.py 存在且可导入。"
    ) from exc


# ============================================================================
# 工具函数（与 audit_case_metrics.py 保持一致）
# ============================================================================

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


def atomic_write_json(path, obj):
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
            json.dump(obj, f, indent=4, ensure_ascii=False)
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


# ============================================================================
# 频率命令族 & 路由边界（shell_session_plugin 专用，但脚本可适配其他插件）
# ============================================================================

# 用户在审计结论中明确列出的 60 类高频 shell 命令族（按用户口径的
# “18 已覆盖 / 42 缺失” 拆分）。本脚本在 coverage matrix 中将按这个目标集
# 判定哪些家族仍需补 case。
HIGH_FREQUENCY_SHELL_FAMILIES = [
    # bash 基础文件操作（user 已覆盖: ls/dir/tree/find；缺失: ll/cp/mv/rm/mkdir/rmdir/chmod/chown/touch/set）
    "ls", "ll", "dir", "dir_s", "tree", "cp", "mv", "rm", "mkdir", "rmdir",
    "chmod", "chown", "touch", "find", "set",
    # 文本处理
    "grep", "rg", "awk", "sed", "sort", "uniq", "head", "tail", "wc",
    # 进程 / 系统
    "ps", "top", "kill", "jobs", "systemctl", "service",
    # 磁盘 / 网络
    "df", "du", "mount", "netstat", "ss", "ip", "ifconfig", "ipconfig", "ping",
    # 下载 / 传输
    "curl", "wget", "rsync", "scp", "tar", "zip", "unzip",
    # Windows CMD
    "copy", "xcopy", "robocopy", "where", "type", "findstr",
    # PowerShell
    "get_childitem", "get_process", "select_object", "invoke_webrequest",
    "copy_item", "remove_item", "where_object", "convertfrom_json",
]

# 这些命令族属于专用插件（VCS / 构建 / 容器 / IaC / K8s / 测试）。
# 任何 case 的主体如果被这些工具的输出主导，shell_session_plugin 就应该
# 视为 routing_boundary_unclear；理想做法是让渡给专用插件。
ROUTING_BOUNDARY_TOOLS = {
    # VCS
    "git", "svn", "hg", "p4", "cvs", "bzr", "fossil", "darcs",
    "gh", "glab", "az", "bb", "repo", "gerrit",
    # 构建 / 包管理
    "cargo", "go", "mvn", "gradle", "gradlew", "dotnet", "xcodebuild",
    "npm", "yarn", "pnpm", "npx",
    # 容器 / K8s
    "docker", "docker-compose", "kubectl", "helm",
    # 测试
    "pytest", "py.test",
    # IaC
    "terraform", "ansible", "pulumi", "bazel",
}

# 命令 token 归一化映射（小写 + 去后缀）。判定命令族时使用。
SHELL_FAMILY_ALIASES = {
    "ll": ["ll"],
    "ls": ["ls"],
    "dir": ["dir"],
    "dir_s": ["dir /s", "dir/s", "dir_-s"],
    "tree": ["tree"],
    "find": ["find"],
    "set": ["set"],
    "cp": ["cp", "copy"],
    "mv": ["mv", "move"],
    "rm": ["rm", "del", "erase", "remove-item"],
    "remove_item": ["remove-item"],
    "mkdir": ["mkdir", "md"],
    "rmdir": ["rmdir", "rd"],
    "chmod": ["chmod"],
    "chown": ["chown"],
    "touch": ["touch"],
    "grep": ["grep", "findstr"],
    "findstr": ["findstr"],
    "rg": ["rg"],
    "awk": ["awk"],
    "sed": ["sed"],
    "sort": ["sort"],
    "uniq": ["uniq"],
    "head": ["head"],
    "tail": ["tail"],
    "wc": ["wc"],
    "ps": ["ps"],
    "top": ["top"],
    "kill": ["kill"],
    "jobs": ["jobs"],
    "systemctl": ["systemctl"],
    "service": ["service"],
    "df": ["df"],
    "du": ["du"],
    "mount": ["mount"],
    "netstat": ["netstat"],
    "ss": ["ss"],
    "ip": ["ip"],
    "ifconfig": ["ifconfig"],
    "ipconfig": ["ipconfig"],
    "ping": ["ping"],
    "curl": ["curl"],
    "wget": ["wget"],
    "rsync": ["rsync"],
    "scp": ["scp"],
    "tar": ["tar"],
    "zip": ["zip"],
    "unzip": ["unzip"],
    "copy": ["copy"],
    "xcopy": ["xcopy"],
    "robocopy": ["robocopy"],
    "where": ["where"],
    "type": ["type"],
    "get_childitem": ["get-childitem", "gci", "ls_alias_powershell"],
    "get_process": ["get-process", "gps"],
    "select_object": ["select-object"],
    "invoke_webrequest": ["invoke-webrequest", "iwr", "curl_alias_powershell"],
    "copy_item": ["copy-item"],
    "where_object": ["where-object", "?"],
    "convertfrom_json": ["convertfrom-json"],
}


# ============================================================================
# Prompt stripping & 命令 token / 段 / 全命令行提取（fix #2/#3/#4/#5）
# ============================================================================
#
# 设计要点：
# 1. 不再用“在 prompt 后取首 token”这种隐式定位；改成显式的
#    (line, prompt, cmd) 三元组，先把行分成“是否是 prompt 起的行”两类。
# 2. prompt 拆掉后，剩下的 cmd 行还要再按 shell 分割符 | / || / && / ;
#    拆成 segment，每个 segment 提首 token。
# 3. 覆盖判断基于“完整 cmd 行 + 所有 segment”，避免 `dir /s` 被误判为 `dir`，
#    `cat x | grep foo | awk ...` 漏检 `awk`。
# 4. PowerShell cmdlet（Verb-Noun）必须作为一个 token 整段取出。
# ============================================================================

# 按优先级排序：PS C:\> 必须排在 $ 之前，避免被通用 $ 截胡
PROMPT_RE = re.compile(
    r"""^
    (?P<prompt>
        PS\s+[^>]+>                  \s+    # PowerShell: PS C:\>  PS /home/x>
      | [A-Z]:\\[^>\n]*>             \s+    # CMD:        C:\>     C:\Users\user>
      # bash / zsh：user@host[:?path]?$   或  user@host %（zsh 无 colon 形式）
      | [\w.\-]+@[\w.\-]+
            (?::[~/.\w\-]*)?         \s*
        [$#%]                        \s+
      # fish：user@host ~>  /  user@host /path>
      | [\w.\-]+@[\w.\-]+\s+[~/.\w\-]*>      \s+
    )
    (?P<cmd>.*)$
    """,
    re.VERBOSE,
)

# shell 命令分隔符。注意 || 与 | 顺序无关（用 regex alternation 一次性吃）。
SHELL_SPLIT_RE = re.compile(r'\s*(?:\|\||&&|;|\|)\s*')

# Windows-only 通用分隔符 (cmd 不支持 &&/||，仅顺序执行)
# 我们在所有场景都按这套分隔符拆，简单且安全。

# PowerShell cmdlet 正则：Get-ChildItem / Select-Object / ConvertFrom-Json / 等
PS_CMDLET_RE = re.compile(r'^[A-Z][A-Za-z]+-[A-Z][A-Za-z0-9]+$')

# 去后缀（小写化前的清理步骤）
_BIN_SUFFIX_RE = re.compile(r'\.(exe|cmd|bat|ps1|sh|zsh|fish)$', re.IGNORECASE)


# ============================================================================
# fix #12: Plugin-type-aware 配置
# ============================================================================
# 背景：audit_sample_case_quality.py 原本为 shell_session 单一插件设计。
# lint 规则（命令锚点 / 最小长度 / 路由让渡）和 LLM 提示词（R1-R7
# shell 人格/错误格式）都硬编码 shell 假设，直接套到 web_log / yaml / vcs /
# traceback 等插件会产生大量假阳性 needs_fix。
#
# 设计原则：
# - PLUGIN_TYPE_REGISTRY：plugin 名 → 类型，全部分类。
# - LINT_CONFIG_PER_TYPE：每种类型的 lint 阈值 + 哪些检查生效。
# - universal 检查（mojibake / secrets / encoding）对所有类型都跑。
# - shell-specific 检查（command_anchor / routing_boundary / signal_anchor）
#   只对 routing_boundary_applicable=True 的类型生效。
# - LLM prompt 按类型 dispatch（build_llm_prompt(plugin_type) →
#   audit_llm_common.build_case_quality_prompt 走 cascade 加载磁盘 .md）。
# - 未知插件 fallback 到 "default"（=access_log 行为：单行也合法）。
# ============================================================================

PLUGIN_TYPE_REGISTRY = {
    # ── shell：唯一会跑命令锚点 / 路由让渡 的类型 ──
    "shell_session_plugin": "shell",

    # ── access_log：HTTP/网络/系统访问日志，单行结构是规范 ──
    "web_log_plugin": "access_log",
    "syslog_plugin": "access_log",
    "cloud_log_plugin": "access_log",
    "db_log_plugin": "access_log",
    "sql_plugin": "access_log",

    # ── data_struct：YAML / JSON / XML / Protobuf / ndjson / 配置类 ──
    "yaml_plugin": "data_struct",
    "json_plugin": "data_struct",
    "ndjson_plugin": "data_struct",
    "xml_html_plugin": "data_struct",
    "protobuf_plugin": "data_struct",
    "template_driven_plugin": "data_struct",
    "markdown_plugin": "data_struct",
    "cloudformation_plugin": "data_struct",   # 内部是 YAML/JSON
    "helm_plugin": "data_struct",             # Chart.yaml / values.yaml
    "pulumi_plugin": "data_struct",           # 内部用 YAML 声明资源

    # ── vcs：VCS 命令输出（已经被专用插件承载，不算 routing 让渡） ──
    "vcs_az_plugin": "vcs",
    "vcs_bitbucket_plugin": "vcs",
    "vcs_bzr_plugin": "vcs",
    "vcs_cvs_plugin": "vcs",
    "vcs_darcs_plugin": "vcs",
    "vcs_fossil_plugin": "vcs",
    "vcs_gerrit_plugin": "vcs",
    "vcs_gh_plugin": "vcs",
    "vcs_git_plugin": "vcs",
    "vcs_glab_plugin": "vcs",
    "vcs_hg_plugin": "vcs",
    "vcs_p4_plugin": "vcs",
    "vcs_repo_plugin": "vcs",
    "vcs_svn_plugin": "vcs",
    "git_diff_plugin": "vcs",

    # ── build：编译器 / 构建 / 包管理输出 ──
    "xcode_log_plugin": "build",
    "gcc_log_plugin": "build",
    "maven_plugin": "build",
    "android_gradle_plugin": "build",
    "bazel_plugin": "build",
    "dotnet_plugin": "build",
    "rust_go_plugin": "build",
    "spring_boot_plugin": "build",
    "unity_unreal_plugin": "build",
    "webpack_vite_plugin": "build",
    "ci_log_plugin": "build",                 # CI service log 偏 build/error
    "ansible_plugin": "build",                # playbook 运行日志
    "terraform_plugin": "build",              # IaC 计划/应用输出
    "kubernetes_docker_plugin": "build",      # k8s describe / docker inspect

    # ── error_trace：语言运行时 traceback / 错误栈 ──
    "python_traceback_plugin": "error_trace",
    "node_error_plugin": "error_trace",
    "nodejs_plugin": "error_trace",
    "java_stack_plugin": "error_trace",
    "php_ruby_plugin": "error_trace",
    "pytest_plugin": "error_trace",

    # ── utility：通用工具，样本无强结构 ──
    "ansi_cleaner_plugin": "default",
    "encoding_fallback": "default",
    "generic_text_plugin": "default",
    "noise_filter_plugin": "default",
    "smart_code_plugin": "default",
    "smart_path_plugin": "default",
    "static_rule_plugin": "default",
    "explain_plugin": "default",              # 0 samples，兜底即可
    "artifact_summary_plugin": "default",     # 0 samples
}

# 每种类型的 lint 配置。
# 字段说明：
# - min_bytes / min_lines：极小样本上限（低于此判 too_small）
# - standard_min_bytes / standard_min_lines：硬下限（低于此判 very_short）
# - requires_command_anchor：是否要求首行有命令
# - requires_signal_anchors：是否要求 stdout/stderr/exit-code 信号
# - routing_boundary_applicable：是否启用 ROUTING_BOUNDARY_TOOLS 让渡判定
# - anchor_patterns：该类型的合法"锚点"正则列表（首行命中任一即视为有锚点）
# - signal_patterns：该类型的合法"信号"正则列表（任一命中即视为有信号）
LINT_CONFIG_PER_TYPE = {
    "shell": {
        "min_bytes": 50, "min_lines": 2,
        "standard_min_bytes": 80, "standard_min_lines": 3,
        "requires_command_anchor": True,
        "requires_signal_anchors": True,
        "routing_boundary_applicable": True,
        "anchor_patterns": [],  # 用 extract_command_tokens 判定
        "signal_patterns": [],   # 用 SIGNAL_PATTERNS 判定
    },
    "access_log": {
        "min_bytes": 30, "min_lines": 1,
        "standard_min_bytes": 50, "standard_min_lines": 1,
        "requires_command_anchor": False,
        "requires_signal_anchors": False,
        "routing_boundary_applicable": False,
        # access log 的"锚点"是 IP / timestamp / HTTP method 之一
        "anchor_patterns": [
            re.compile(r"^\d{1,3}(?:\.\d{1,3}){3}"),                      # IPv4
            re.compile(r"^[a-f0-9:]+:+[a-f0-9:]+\s"),                      # IPv6
            re.compile(r"\b(?:GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)\b"),  # HTTP method
            re.compile(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b"),          # 任意位置的 IP
            re.compile(r"\[\d{1,2}/[A-Za-z]{3}/\d{4}:\d{2}:\d{2}:\d{2}"),  # CLF 日期
            re.compile(r"\b(?:ERROR|WARN|INFO|DEBUG|FATAL)\b"),            # 日志级别
        ],
        "signal_patterns": [
            re.compile(r"\b\d{3}\b"),                  # HTTP 状态码
            re.compile(r"\b(?:ERROR|WARN|FATAL)\b"),    # 日志级别
        ],
    },
    "data_struct": {
        "min_bytes": 20, "min_lines": 1,
        "standard_min_bytes": 30, "standard_min_lines": 1,
        "requires_command_anchor": False,
        "requires_signal_anchors": False,
        "routing_boundary_applicable": False,
        # data_struct 的"锚点"是结构特征
        "anchor_patterns": [
            re.compile(r"^[ \t]*[A-Za-z_][A-Za-z0-9_\-]*\s*:"),  # YAML/JSON 风格 key:
            re.compile(r"^[{<\[]"),                                # JSON/XML 起始
            re.compile(r"^\s*<\?xml|^\s*<!DOCTYPE"),               # XML 声明
            re.compile(r"^\s*---"),                                # YAML doc separator
        ],
        "signal_patterns": [],
    },
    "vcs": {
        "min_bytes": 12, "min_lines": 1,
        "standard_min_bytes": 15, "standard_min_lines": 1,
        "requires_command_anchor": False,  # vcs case 本身就是命令输出，不是"被 shell 执行"
        "requires_signal_anchors": False,
        "routing_boundary_applicable": False,  # vcs 插件承载 vcs 输出，无让渡问题
        # vcs 锚点 = VCS 命令头部
        "anchor_patterns": [
            re.compile(r"^\s*(?:[a-zA-Z_./-]*[/\\])?(git|svn|hg|p4|cvs|bzr|fossil|darcs|gh|glab|az|repo|gerrit)\b", re.IGNORECASE),
            re.compile(r"^\s*commit\s+[0-9a-f]{7,}", re.IGNORECASE),       # git/svn/hg commit
            re.compile(r"^\s*Changeset\s+\#?\d+", re.IGNORECASE),          # p4/hg
        ],
        "signal_patterns": [
            re.compile(r"\b[0-9a-f]{7,40}\b"),        # commit hash
            re.compile(r"^(?:[A-Z][\w ]*)$", re.MULTILINE),  # 文件状态行 (M/A/D/?)
        ],
    },
    "build": {
        "min_bytes": 30, "min_lines": 1,
        "standard_min_bytes": 50, "standard_min_lines": 1,
        "requires_command_anchor": False,
        "requires_signal_anchors": True,  # build 输出通常包含错误/警告
        "routing_boundary_applicable": False,
        # build 锚点 = 编译目标 / 错误位置
        "anchor_patterns": [
            re.compile(r"\b\w+\.(?:cpp|cc|c|h|hpp|rs|go|java|kt|swift|m|mm|ts|tsx|js|jsx)\b"),  # 源文件
            re.compile(r"^\s*\[(?:\s*\d+\s*/\s*\d+\s*|\s*\++\s*|\s*==\s*)"),  # gradle/progress
            re.compile(r"\b(?:error|warning|fatal|undefined|undeclared):", re.IGNORECASE),  # 编译器诊断
            re.compile(r"^\s*>>>"),  # cmake / p4 prompt
            re.compile(r"\bLinking\s+"),  # linker step
            # xcodebuild CompileC / Linking / Using response file 等单行命令
            re.compile(r"^(?:CompileC|Linking|Ld|CompileSwift|Cd|GenerateAssetSymbols|Using response file|RegisterExecutionPolicyException|ProcessInfoPlistFile|CompileStoryboard|CompileAssetCatalog)", re.IGNORECASE),
        ],
        "signal_patterns": [
            re.compile(r"\b(?:error|warning|fatal|panic|undefined|undeclared)\b", re.IGNORECASE),
            re.compile(r":\d+:\d+:"),  # gcc-style file:line:col
        ],
    },
    "error_trace": {
        "min_bytes": 60, "min_lines": 2,
        "standard_min_bytes": 80, "standard_min_lines": 3,
        "requires_command_anchor": False,
        "requires_signal_anchors": True,  # traceback 必须有 "Traceback" 关键词
        "routing_boundary_applicable": False,
        "anchor_patterns": [
            re.compile(r"\bTraceback\s*\(most recent call last\)", re.IGNORECASE),
            re.compile(r"^\s*at\s+[\w.$<>]+\s*\("),                         # Java/Node stack frame
            re.compile(r"\b(?:Error|Exception|Panic):"),                      # 错误名
        ],
        "signal_patterns": [
            re.compile(r"\bTraceback\b", re.IGNORECASE),
            re.compile(r"^\s*File\s+\"[^\"]+\"\s*, line \d+", re.MULTILINE),
        ],
    },
    "default": {
        "min_bytes": 30, "min_lines": 1,
        "standard_min_bytes": 50, "standard_min_lines": 1,
        "requires_command_anchor": False,
        "requires_signal_anchors": False,
        "routing_boundary_applicable": False,
        "anchor_patterns": [],  # 任何首行都接受
        "signal_patterns": [],
    },
}


def get_plugin_type(plugin_name):
    """根据 PLUGIN_TYPE_REGISTRY 查 plugin 类型，未注册返回 'default'。"""
    return PLUGIN_TYPE_REGISTRY.get(plugin_name, "default")


def get_lint_config(plugin_name):
    """根据 plugin 名取 lint 配置，未注册用 default。"""
    ptype = get_plugin_type(plugin_name)
    return ptype, LINT_CONFIG_PER_TYPE.get(ptype, LINT_CONFIG_PER_TYPE["default"])


# bash/cmd 行首的 env-prefix 序列 ``KEY=value`` / ``KEY='value'`` /
# ``KEY="value"``，可能多组连用（``A=1 B=2 node index.js``）。
# 用一个松散 token 级正则来匹配，\s+ 分隔后被 strip_env_prefix 反复剥除。
# 注意：只匹配 ``^[A-Za-z_][A-Za-z0-9_]*=`` 形式，避免把真正的 ``foo=bar``
# 命令参数误判为环境变量（命令参数通常带 ``--``/``-`` 前缀或出现在命令后）。
_ENV_PREFIX_TOK_RE = re.compile(
    r"""^\s*
    (?P<key>[A-Za-z_][A-Za-z0-9_]*)     # 变量名
    =                                    # =
    (?:
        "[^"\n]*"                        # "value"
      | '[^'\n]*'                        # 'value'
      | \$\([^)]+\)                      # $(cmd) / $var
      | \$\{[^}]+\}                      # ${var}
      | \$[A-Za-z_][A-Za-z0-9_]*         # $var
      | [^\s|&;]+                        # 裸值，到空白/管道/分号为止
    )
    \s+
    """,
    re.VERBOSE,
)
_LEADING_PATH_RE = re.compile(r'^[./\\]+')


def strip_prompt(line):
    """返回 (had_prompt, cmd_part)；cmd_part 已 strip。"""
    if not line:
        return False, ""
    m = PROMPT_RE.match(line)
    if m:
        return True, m.group("cmd").strip()
    return False, line.strip()


def split_command_segments(cmd_line):
    """把 `cat x | grep y | awk ...` 拆成 ['cat x','grep y','awk ...']。"""
    if not cmd_line:
        return []
    return [s.strip() for s in SHELL_SPLIT_RE.split(cmd_line) if s.strip()]


def strip_env_prefix(segment):
    """
    剥除 bash/cmd 行首的 ``KEY=value`` env-prefix 序列（可能多组）。

    示例：
        ``FOO=bar node index.js``        → ``node index.js``
        ``A=1 B=2 ./bin/run --help``     → ``./bin/run --help``
        ``NODE_ENV=production npm test`` → ``npm test``
        ``ls /tmp``                      → ``ls /tmp`` （不变）
        ``echo FOO=bar``                 → ``echo FOO=bar`` （不变，FOO=bar 不在行首）

    限制：仅在 segment 起点剥离；嵌套 ``"a b"`` 之类含空白的值需要更复杂的
    shell 词法分析，当前实现只对常见写法（``A=1``、``A='x y'``、``A="x y"``、
    ``A=$VAR``、``A=$(cmd)``）生效。返回值始终为 ``str``，未匹配时返回原 segment。
    """
    if not segment:
        return segment
    prev = None
    cur = segment
    # 多次剥除，处理 ``A=1 B=2 cmd`` 这种连用
    while prev != cur:
        prev = cur
        m = _ENV_PREFIX_TOK_RE.match(cur)
        if not m:
            break
        cur = cur[m.end():]
    return cur


def normalize_token(token):
    """统一 token：去路径前缀、去二进制后缀、转小写。"""
    if not token:
        return ""
    t = _LEADING_PATH_RE.sub("", token)
    t = _BIN_SUFFIX_RE.sub("", t)
    return t.lower()


def extract_ps_cmdlet(segment, ps_context=False):
    """
    从 PowerShell segment 头部提取 cmdlet（Verb-Noun）作为一个 token。
    例：`Get-Process | Where-Object ...` 第一个 segment 拿 `Get-Process`。

    重要：ps_context=True 时才应用 PowerShell 别名展开（?/%/gci/gps/iwr/ls/dir/cat 等）。
    bash/cmd 上下文中 ``ls``/``dir``/``cat`` 仍是 bash/cmd 命令，不能被改写。

    注：env-prefix 仅在 ps_context=False 时剥离（PowerShell 不支持 ``FOO=bar cmd``
    语法，避免误剥 ``-Key=Value`` 之类 cmdlet 参数）。
    """
    if not ps_context:
        segment = strip_env_prefix(segment)
    s = segment.lstrip()
    if not s:
        return ""
    head = s.split(None, 1)[0] if s.split(None, 1) else ""
    if not head:
        return ""
    head_norm = normalize_token(head)
    if ps_context:
        aliases = {
            "?": "where-object",
            "%": "foreach-object",
            "gci": "get-childitem",
            "gls": "get-location",
            "gps": "get-process",
            "iwr": "invoke-webrequest",
            "curl": "invoke-webrequest",
            "ls": "get-childitem",
            "dir": "get-childitem",
            "cat": "get-content",
            "cp": "copy-item",
            "rm": "remove-item",
            "mv": "move-item",
        }
        if head_norm in aliases:
            return aliases[head_norm]
    if PS_CMDLET_RE.match(head):
        return head.lower()
    return head_norm


def looks_like_command_token(tok):
    """判断 token 是否像真实命令（字母/数字/_/./- 构成，非纯数字、非带特殊符号）。"""
    if not tok:
        return False
    if not re.match(r'^[a-zA-Z][a-zA-Z0-9_./-]*$', tok):
        return False
    return True


def extract_segment_tokens(segment, ps_context=False):
    """
    从单个 segment 提 token。
    - bash/cmd 通用：第一个空白前的 token 视为命令（env-prefix 会被预先剥除）。
    - PowerShell 优先：识别 Verb-Noun cmdlet（仅在 ps_context=True 时展开别名）。

    若第一段首 token 不像命令（字符串字面量、纯数字、特殊符号），
    继续往后找第一个像命令的 token。
    """
    if not ps_context:
        segment = strip_env_prefix(segment)
    s = segment.lstrip()
    if not s:
        return []
    # 在所有 segments 中尝试找一个像命令的 token
    head = s.split(None, 1)[0] if s.split(None, 1) else ""
    if not head:
        return []
    head_norm = normalize_token(head)
    # PowerShell cmdlet 模式在 ps_context 时优先
    if ps_context and PS_CMDLET_RE.match(head):
        return [head.lower()]
    # 非 PS 上下文：保留首 token（即使它“看起来不像命令”，也保留作 lint 信号）
    return [head_norm]


def is_ps_prompt(prompt_text):
    """判断 prompt 是否属于 PowerShell 类型。"""
    if not prompt_text:
        return False
    return bool(re.match(r'\s*PS\s+[^>]+>', prompt_text))


# bash/zsh ``set -x`` trace 前缀：一行 ``+ command`` 等价于命令。
# 这种行严格说不是 prompt 行，但 token 提取必须照顾 CI 类 case。
SET_X_TRACE_RE = re.compile(r'^\s*\+\s+(?P<cmd>.+)$')

# PowerShell 续行 prompt ``>>``：未完成语句的下一行（had_prompt=True，
# prompt_text="ps_continuation"，与正常 ``PS C:\>`` 等价以计入 cmd 行）。
PS_CONTINUATION_RE = re.compile(r'^\s*>>\s*(?P<cmd>.*)$')

# PowerShell 错误诊断行（不应被当命令）。这些行在 PS 报错的 multi-line
# output 中出现，会被 lint 误判为命令并污染 tokens / command_lines：
#   - ``    + CategoryInfo          : ...``
#   - ``    + FullyQualifiedErrorId : ...``
#   - ``+ ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~``  （错误 caret 指示）
#   - ``At line:1 char:1``  （错误位置头）
#   - 单独的 ``~~~~~``  /  ``-----``  分隔符
PS_DIAGNOSTIC_RE = re.compile(
    r"""^
    \s*\+\s*(?:CategoryInfo|FullyQualifiedErrorId|[~]+)  # + CategoryInfo... / + ~~~~
  | At\s+line:\d+\s+char:\d+                               # At line:1 char:30
  | \s*~\s*$                                              # 纯 ~~~  行
  | \s*-\s*$                                              # 纯 ---  行
    """,
    re.VERBOSE,
)


def strip_prompt_with_prompt(line):
    """
    返回 (had_prompt, cmd_part, prompt_text)。
    同时识别：
    - 标准 shell prompt（bash / zsh / fish / cmd / PowerShell）
    - bash/zsh ``set -x`` 的 ``+ cmd`` trace 行（had_prompt=False, prompt_text="set_x"）
    - PowerShell ``>>`` 续行 prompt（had_prompt=True, prompt_text="ps_continuation"）
    - PowerShell 错误诊断行（had_prompt=False, prompt_text="ps_diagnostic"）→ 调用方应跳过
    """
    if not line:
        return False, "", ""
    if PS_DIAGNOSTIC_RE.match(line):
        return False, "", "ps_diagnostic"
    m = PROMPT_RE.match(line)
    if m:
        return True, m.group("cmd").strip(), m.group("prompt").strip()
    m_continue = PS_CONTINUATION_RE.match(line)
    if m_continue:
        return True, m_continue.group("cmd").strip(), "ps_continuation"
    m2 = SET_X_TRACE_RE.match(line)
    if m2:
        return False, m2.group("cmd").strip(), "set_x"
    return False, line.strip(), ""


def extract_main_command_token(cmd_line, ps_context=False):
    """
    从一行命令中找"主命令" token（第一个看起来像命令的 token）。

    - PowerShell pipeline 中首段可能是字符串字面量/管道输入（如
      ``'{"k":1}' | ConvertFrom-Json``），应跳到下一段找 ``ConvertFrom-Json``。
    - bash/cmd 简单情况下取首段首 token 即可。
    """
    segs = split_command_segments(cmd_line)
    for seg in segs:
        toks = extract_segment_tokens(seg, ps_context=ps_context)
        if not toks:
            continue
        if looks_like_command_token(toks[0]):
            return toks[0]
        # PowerShell 下 Verb-Noun cmdlet 即使带 - 也算合法（已有正则处理）
        if ps_context and toks[0] in SHELL_FAMILY_ALIASES.get("get_childitem", []) + \
                SHELL_FAMILY_ALIASES.get("where_object", []):
            return toks[0]
    return ""


def extract_all_segment_tokens(cmd_line, ps_context=False):
    """
    从一行命令中提取所有 segment 的首 token（用于 pipeline 覆盖）。
    即使首段不是命令也会保留（作为 lint 信号）。
    """
    out = []
    for seg in split_command_segments(cmd_line):
        toks = extract_segment_tokens(seg, ps_context=ps_context)
        if toks:
            out.append(toks[0])
    return out


def extract_command_tokens(text):
    """
    从 sample 文本中提取所有命令 token（去重，保序）。
    - 严格模式：默认只从 prompt 起的行提取命令，避免把 stdout/stderr 输出
      （如 ``FINDSTR: Cannot open ...``、``+ docker build .``、PowerShell
      diagnostic continuation 行 ``+ CategoryInfo ...``）误当命令。
      唯一例外是 ``set -x`` 的 ``+ cmd`` trace 行（已知 trace 模式）和
      PowerShell ``>>`` 续行 prompt。
    - 命令行按 |/||/&&/; 拆段，每段提 token，跨段都进入 list 用于覆盖矩阵。
    - ps_context 由 prompt 类型决定；PS 别名展开仅在 PS 上下文生效。
    - PS diagnostic 行（``+ CategoryInfo``、``~~~~``、``At line:N char:M``）
      显式排除，避免噪声污染 downstream LLM 判定。
    - PS 错误块（``At line:N char:M`` 之后直到空行/下一个 prompt）中出现的
      ``+ cmd`` echo 行被识别为错误回显，与 ``PS C:\\\\> cmd`` 重复，剔除。
    """
    tokens = []
    seen = set()
    in_ps_error_block = False
    for raw_line in text.splitlines():
        # 先识别 PS 错误块的入口 ``At line:N char:M``，因为它本身也是
        # ``ps_diagnostic`` 分类，必须在状态机转换里优先处理。
        if re.match(r'^\s*At\s+line:\d+\s+char:\d+', raw_line):
            in_ps_error_block = True
            continue
        had_prompt, cmd_line, prompt_text = strip_prompt_with_prompt(raw_line)
        if prompt_text == "ps_diagnostic":
            continue
        if had_prompt:
            in_ps_error_block = False
        if not had_prompt and prompt_text != "set_x":
            continue
        # 处于 PS 错误块时，``+ cmd`` 是错误回显，跳过以避免与 prompt 行重复
        if in_ps_error_block and prompt_text == "set_x":
            continue
        if not cmd_line:
            continue
        ps_ctx = is_ps_prompt(prompt_text)
        for tok in extract_all_segment_tokens(cmd_line, ps_context=ps_ctx):
            if tok and tok not in seen:
                seen.add(tok)
                tokens.append(tok)
    return tokens


def extract_command_lines(text):
    """
    返回 (had_prompt, full_cmd_line) 列表，供“全命令行覆盖”检查。
    同样允许 set -x trace 行和 PS ``>>`` 续行；显式排除 PS diagnostic 行
    与 PS 错误块中的 ``+ cmd`` echo 回显。
    """
    out = []
    in_ps_error_block = False
    for raw_line in text.splitlines():
        if re.match(r'^\s*At\s+line:\d+\s+char:\d+', raw_line):
            in_ps_error_block = True
            continue
        had_prompt, cmd_line, prompt_text = strip_prompt_with_prompt(raw_line)
        if prompt_text == "ps_diagnostic":
            continue
        if had_prompt:
            in_ps_error_block = False
            if cmd_line:
                out.append((True, cmd_line))
            continue
        if prompt_text == "set_x" and cmd_line and not in_ps_error_block:
            out.append((False, cmd_line))
    return out


def map_tokens_to_families(tokens, full_lines=None):
    """
    把 token 与完整命令行都拿来匹配 SHELL_FAMILY_ALIASES。
    返回 (matched_families_set, family_to_alias_tokens_dict)。
    """
    matched = set()
    matched_aliases = {}

    def _match(tok):
        for family, aliases in SHELL_FAMILY_ALIASES.items():
            if tok in [a.lower() for a in aliases]:
                matched.add(family)
                matched_aliases.setdefault(family, []).append(tok)

    for tok in tokens:
        _match(tok)

    # 全命令行覆盖：例如 `dir /s` 在 token 里只有 `dir`，但整行里
    # 出现 `dir /s` 这样的别名时也要计入 `dir_s`。
    if full_lines:
        for _, line in full_lines:
            line_l = line.lower()
            for family, aliases in SHELL_FAMILY_ALIASES.items():
                for alias in aliases:
                    a = alias.lower()
                    if " " in a or "/" in a:  # 带参数/路径别名单独处理
                        if a in line_l:
                            matched.add(family)
                            matched_aliases.setdefault(family, []).append(alias)

    return matched, matched_aliases


def get_first_non_empty_line(text):
    for line in text.splitlines():
        if line.strip():
            return line
    return ""


def get_command_anchor_token(line):
    """返回 first non-empty 行的首命令 token（跨 segment 找第一个像命令的）。"""
    had_prompt, cmd_line, prompt_text = strip_prompt_with_prompt(line)
    if not cmd_line:
        cmd_line = line.strip()
        ps_ctx = False
    else:
        ps_ctx = is_ps_prompt(prompt_text)
    return extract_main_command_token(cmd_line, ps_context=ps_ctx)


def is_routing_boundary_dominant(text, tokens):
    """
    若 sample 的第一 token 大概率属于专用插件，判为路由让渡 case。
    """
    if not tokens:
        return False, ""
    head = tokens[0].lower()
    for tool in ROUTING_BOUNDARY_TOOLS:
        if head == tool or head.startswith(tool + ".") or head.startswith(tool + "-"):
            return True, tool
    return False, ""


# fix #7：路由让渡按"段占比"判定，而不再只看首 token。
# 单个 `git status` 后跟若干 `cd`/`ls` 的混合 case 不应被一刀切归到 git 插件；
# 反之如果 case 的命令段里 ≥ ROUTING_DOMINANT_THRESHOLD 都是路由工具的输出，
# 才是真正的路由让渡 case。
# 阈值默认 0.5；首段是 routing tool 也算"至少有一段"，避免阈值过严漏掉短 case。
ROUTING_DOMINANT_THRESHOLD = 0.5


def compute_routing_boundary_dominant(content, tokens):
    """
    返回 (dominant, tool, ratio, routing_segments, total_segments)。
    - dominant: 是否应判为 routing_boundary_unclear
    - tool: 命中的 ROUTING_BOUNDARY_TOOL 名（占比最高的那个）
    - ratio: routing_segments / total_segments
    """
    # fix #7 修订：用去重后的 command tokens（不是 segments）做分母。
    # 原因：set -x 的 trace 行（`+ create` / `+ resource` ...）会被
    # extract_command_lines 当成 cmd_line，污染分母，把 `terraform plan`
    # 这种 routing case 的比例稀释到 1/5 = 20%，导致假阴。
    # extract_command_tokens 已经是 dedupe 集合，能反映"case 实际包含的命令"。
    if not tokens:
        # 兜底沿用旧逻辑：取首 token
        return is_routing_boundary_dominant(content, tokens) + (0.0, 0, 0)

    per_token = [t.lower() for t in tokens]
    total = len(per_token)
    if total == 0:
        return is_routing_boundary_dominant(content, tokens) + (0.0, 0, 0)

    per_tool = {}
    for tok in per_token:
        for tool in ROUTING_BOUNDARY_TOOLS:
            if tok == tool or tok.startswith(tool + ".") or tok.startswith(tool + "-"):
                per_tool[tool] = per_tool.get(tool, 0) + 1
                break

    if not per_tool:
        return False, "", 0.0, 0, total

    top_tool, top_count = max(per_tool.items(), key=lambda x: x[1])
    ratio = top_count / total
    # 双触发：①首 token 是 routing（保持旧行为，向后兼容）
    # ②routing 占比 ≥ 阈值（多 routing 共存时由占比兜底）
    first_is_routing = any(
        per_token[0] == tool
        or per_token[0].startswith(tool + ".")
        or per_token[0].startswith(tool + "-")
        for tool in ROUTING_BOUNDARY_TOOLS
    )
    dominant = first_is_routing or (top_count / total) >= ROUTING_DOMINANT_THRESHOLD
    return dominant, top_tool, ratio, top_count, total


# ============================================================================
# 确定性 lint 检查
# ============================================================================

# mojibake / 编码痕迹：常见 cp1252 → utf8 乱码
MOJIBAKE_PATTERNS = [
    r"Ã.", r"Â.", r"â‚¬", r"â€", r"ï¿½",
    r"锘縖", r"锘", r"锛", r"鈥", r"锝",
]
MOJIBAKE_RE = re.compile("|".join(MOJIBAKE_PATTERNS))

# 敏感信息（保守白名单 + 严格模式：出现即报警）
SECRET_PATTERNS = [
    (re.compile(r"AKIA[0-9A-Z]{16}"), "aws_access_key_id"),
    (re.compile(r"AIza[0-9A-Za-z\-_]{35}"), "google_api_key"),
    (re.compile(r"ghp_[A-Za-z0-9]{36,}"), "github_personal_token"),
    (re.compile(r"gho_[A-Za-z0-9]{36,}"), "github_oauth_token"),
    (re.compile(r"xox[abposr]-[A-Za-z0-9-]{10,}"), "slack_token"),
    (re.compile(r"-----BEGIN (?:RSA|EC|DSA|OPENSSH|PRIVATE) (?:PRIVATE )?KEY-----"), "private_key_block"),
    (re.compile(r"(?i)password\s*[:=]\s*\S+"), "literal_password_field"),
    (re.compile(r"(?i)api[_-]?key\s*[:=]\s*\S+"), "literal_api_key_field"),
    (re.compile(r"(?i)secret\s*[:=]\s*\S+"), "literal_secret_field"),
    (re.compile(r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}"), "jwt_token"),
]
# 已知占位 / 演示 token，命中后降级为 warning，不算 fail
PLACEHOLDER_RE = re.compile(
    r"(?i)(xxxx+|y{4,}|z{4,}|<\s*your[_-]?\w+\s*>|<\s*token\s*>|\[REDACTED\]|changeme|<TOKEN>|<API_KEY>|<PASSWORD>|example|placeholder|dummy|fake|sample\.|test\.|demo\.)"
)

# 最小可用样本阈值（字节 / 行数）
# fix #9：原值 60B / 2 行过松，导致 case_040 (78B / 2 行) 这种
# "只有 prompt + 命令 + 空 prompt" 的退化样本被当成正常 case 放行。
# 新值 80B / 3 行要求：①prompt + 命令 + 至少 1 行可观察输出；
# ②或 prompt + 多行命令（here-doc / set -x 块） + 收尾。
# 极小样本阈值同步上调到 50B / 2 行，让"只有 prompt+一行命令"被标 too_small。
MIN_BYTES = 80
MIN_LINES = 3
# 极小样本阈值：低于此值直接判 too_small
TOO_SMALL_BYTES = 50
TOO_SMALL_LINES = 2

# fix #2：title 中的"断言关键词"必须在 content 中能匹配到对应特征。
# 设计原则：把 case 标题里高频出现的语义声明归纳成 (title_pat, content_pat)
# 对，每条 title_pat 命中则要求 content_pat 至少一处命中；缺失则降级 title_consistent。
TITLE_ASSERTION_KEYWORDS = [
    (r"command[ _-]not[ _-]found", r"command[\- ]?not[\- ]?found|not[\- ]?recognized|is not recognized|cannot find|is not a (recognized|known)"),
    (r"permission[ _-]denied|denied", r"permission[\- ]?denied|access[\- ]?is[\- ]?denied|access[\- ]?denied|operation not permitted"),
    (r"path[ _-]not[ _-]found", r"path[\- ]?not[\- ]?found|the system cannot find the (path|file)"),
    (r"syntax[ _-]?error", r"syntax[\- ]?error"),
    (r"parsererror", r"parsererror"),
    (r"parameterbinding", r"parameterbinding"),
    (r"errorlevel", r"errorlevel|%errorlevel%"),
    (r"no[ _-]match|nomatch", r"no[\- ]?matches?[\- ]?found|glob.*no[\- ]?match|nomatches?"),
    (r"recursive|recursion", r"(?:^|[\s|])(?:find|chmod|grep|robocopy|xcopy|tar|cp|copy|rm|del|remove-item|copy-item|get-childitem)\b[^\n]*?-[a-z\- ]*r\b|/[sq]\b|-recurse\b|--recursive|-rf\b"),
    (r"\bpipefail\b", r"pipefail"),
    (r"set[ _-]?x|\btrace\b", r"\\+\s|PS4\+|\+\s"),
    (r"secrets?|sensitive", r"(?i)akia[0-9a-z]{8,}|api[\-_.]?key|secret|password|bearer|token[\-_: ]"),
]


# 信号锚点：是否包含 stdout / stderr / exit code 任意一种
SIGNAL_PATTERNS = [
    re.compile(r"(?i)\b(error|errors|err|fatal|panic|exception|failed|failure|traceback)\b"),
    re.compile(r"\$\?"),
    re.compile(r"(?i)exit\s*(?:code|status)\s*[:=]?\s*\d+"),
    re.compile(r"(?i)errorlevel\s*[:=]?\s*\d+"),
    re.compile(r"(?i)\breturncode\s*[:=]?\s*\d+"),
    re.compile(r"\[stderr\]|\[stdout\]"),
    re.compile(r"^>+\s", re.MULTILINE),
    re.compile(r"PS\s+[^>]+>\s"),
    re.compile(r"\$ echo \$"),
]
# exit code 模式额外（避免与 SIGNAL_PATTERNS 重复）
EXIT_CODE_RE = re.compile(r"(?i)(?:exit code|errorlevel|returncode|\$?)[\s:=]+(\d{1,3})")


INTENTIONAL_EDGE_CASE_TOKENS = {
    # fix #13: case_id 中出现以下 token，说明该 case 是有意构造的边界场景，
    # empty/tiny/single-line 是该 case 的"目标态"，不是缺陷。
    # 用 word-boundary 匹配，避免在 "compact" / "compressed" 中误命中 "act" / "sed" 等。
    "empty", "blank", "no_output", "nooutput", "silent",
    "minimal", "no_compress", "no_compression", "no_compress_target",
    "single_line", "single-line", "one_liner", "oneline",
    "tiny", "very_short", "trivial", "trivially", "no_change", "unchanged",
    "none", "null", "absent", "missing_only",
}


def is_intentional_edge_case(case_id):
    """fix #13：判断 case_id 是否在描述一个"故意的边界场景"。

    约定：TokenSlim 样本命名是 case_NNN_<descriptor>.log，descriptor 可以由
    下划线拼接多个子 token。本函数同时检查：
      1. 单段 token：case_NNN_no_compress → "no_compress" 直接命中
      2. 相邻段拼接：case_NNN_single_line → ["single","line"] → "single_line" 命中
      3. 双段拼接的"切分版"：覆盖 case_NNN_no_output 等连字符/下划线变体。
    """
    if not case_id:
        return False
    parts = re.split(r"[_\-.]+", case_id.lower())
    parts = [p for p in parts if p]  # 去空
    candidates = set(parts)
    # 相邻两段拼接（覆盖 single_line, no_compress, very_short, no_change 等）
    for i in range(len(parts) - 1):
        candidates.add(parts[i] + "_" + parts[i + 1])
    return any(c in INTENTIONAL_EDGE_CASE_TOKENS for c in candidates)


def check_mojibake(text):
    hits = []
    for m in MOJIBAKE_RE.finditer(text):
        snippet = text[max(0, m.start() - 12):m.end() + 12]
        hits.append(snippet)
    return hits[:5]  # 上报前 5 个样本，避免噪声


def check_secrets(text):
    findings = []
    for pat, name in SECRET_PATTERNS:
        for m in pat.finditer(text):
            matched = m.group(0)
            if PLACEHOLDER_RE.search(matched):
                continue
            findings.append({"pattern": name, "sample": matched[:40]})
    return findings[:5]


def check_command_anchor(text, lint_config=None):
    """检查首行是否有"锚点"。

    fix #12：plugin-type-aware。
    - shell 类型：用 extract_command_tokens 判定命令族（保留旧行为）。
    - 其他类型：用 lint_config.anchor_patterns 列表中任一正则命中即视为有锚点。
    - 默认（无 lint_config 或空 anchor_patterns）：任何首行都接受。
    """
    first = get_first_non_empty_line(text)
    if not first:
        return False, "", "empty_first_line"

    if lint_config is None:
        # 向后兼容：默认行为（=shell）
        tok = get_command_anchor_token(first)
        if not tok:
            return False, first, "no_command_anchor"
        return True, tok, ""

    if not lint_config.get("requires_command_anchor", False):
        # 该类型不要求命令锚点：永远 OK
        return True, first[:32], ""

    # shell 类型：保持旧命令锚点逻辑
    tok = get_command_anchor_token(first)
    if tok:
        return True, tok, ""

    # 非 shell 但仍要求锚点：用 anchor_patterns 兜底
    for pat in lint_config.get("anchor_patterns") or []:
        if pat.search(first) or pat.search(text):
            return True, first[:32], ""

    return False, first, "no_command_anchor"


def check_minimum_length(text, lint_config=None):
    """fix #12：plugin-type-aware 长度阈值。

    - shell：保留 MIN_BYTES=80 / MIN_LINES=3 + TOO_SMALL_BYTES=50 / TOO_SMALL_LINES=2。
    - 其他类型：取 lint_config 里的 min_bytes / min_lines / standard_min_*。
    - 缺省：与 shell 同（向后兼容）。
    """
    if not text:
        return False, "empty"
    stripped = text.strip()
    byte_count = len(stripped.encode("utf-8"))
    line_count = len(stripped.splitlines())

    if lint_config is None:
        too_small_b, too_small_l = TOO_SMALL_BYTES, TOO_SMALL_LINES
        std_b, std_l = MIN_BYTES, MIN_LINES
    else:
        too_small_b = lint_config.get("min_bytes", TOO_SMALL_BYTES)
        too_small_l = lint_config.get("min_lines", TOO_SMALL_LINES)
        std_b = lint_config.get("standard_min_bytes", MIN_BYTES)
        std_l = lint_config.get("standard_min_lines", MIN_LINES)

    if byte_count < too_small_b or line_count < too_small_l:
        return False, "too_small"
    if byte_count < std_b or line_count < std_l:
        return False, "very_short"
    return True, ""


def check_signal_anchors(text, lint_config=None):
    """fix #12：plugin-type-aware 信号锚点。
    - shell：用 SIGNAL_PATTERNS。
    - 其他：用 lint_config.signal_patterns。
    - 不要求信号的类型：返回空列表（不算阻断）。
    """
    if lint_config is not None and not lint_config.get("requires_signal_anchors", True):
        return []
    patterns = SIGNAL_PATTERNS
    if lint_config is not None and lint_config.get("signal_patterns"):
        patterns = lint_config["signal_patterns"] + list(SIGNAL_PATTERNS)
    hits = []
    for pat in patterns:
        if pat.search(text):
            hits.append(pat.pattern)
    return hits


def detect_duplicates(samples_dict):
    """基于文本相似度找出近似重复 case。"""
    items = []
    for fname, path in sorted(samples_dict.items()):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                content = f.read()
        except Exception:
            continue
        items.append({
            "filename": fname,
            "case_id": pathlib.Path(fname).stem,
            "content": content,
        })

    groups = []
    used = set()
    for i, a in enumerate(items):
        if a["case_id"] in used:
            continue
        group = [a["case_id"]]
        used.add(a["case_id"])
        for j in range(i + 1, len(items)):
            b = items[j]
            if b["case_id"] in used:
                continue
            ratio = difflib.SequenceMatcher(None, a["content"], b["content"]).ratio()
            if ratio >= 0.85:
                group.append(b["case_id"])
                used.add(b["case_id"])
        if len(group) > 1:
            groups.append({
                "members": sorted(group),
                "primary": group[0],
                "duplicates": group[1:],
            })
    return groups


# ============================================================================
# Sidecar L1 缓存（fix #30: 把 case 自己的 hash 写进 sidecar audit 块）
# ============================================================================
#
# 设计：
# - sidecar 路径：samples/<plugin>/<case_stem>.scenario.yaml
# - L1 读取：parse yaml → 取 audit 块 → 与 sha256(content) 比对
# - L1 写回：merge audit 块到原 yaml（保留 scenario/target_capability 等
#   人类字段不动），不覆盖 scenario 等语义字段
# - 写回条件：final_status ∈ {valid, not_registered, duplicate, title_mismatch,
#   routing_boundary_unclear}，且本次真的跑了 lint/LLM（cache_hit=False）
# - 不写 needs_fix / fabricated（缺陷判定可能因规则/提示词改进翻成 valid）
# - 缺 PyYAML 时退化到极简文本解析（保留 audit 6 字段）
# ============================================================================

# 写回合法的 final_status 集合（与 main() 里 L2 latest.json 索引判定同语义）
_SIDECAR_WRITABLE_STATUS = {
    "valid", "not_registered", "duplicate",
    "title_mismatch", "routing_boundary_unclear",
}


def _sidecar_path_for_case(plugin: str, case_filename: str):
    """由 case 文件名推 sidecar 路径。返回 None 表示找不到 samples 目录。

    samples 目录约定：samples/<plugin>（plugin 名保留 _plugin 后缀）。
    """
    # 兼容 plugin 名带不带 _plugin 后缀
    candidates = [
        os.path.join("samples", plugin),
        os.path.join("samples", plugin.replace("_plugin", "")),
    ]
    for d in candidates:
        if os.path.isdir(d):
            stem = os.path.splitext(case_filename)[0]
            sc = os.path.join(d, f"{stem}.scenario.yaml")
            if os.path.isfile(sc):
                return sc
    return None


def _parse_yaml_minimal(text: str):
    """极简 yaml 解析（fallback）。只支持 flat 键值 + 一层嵌套 mapping。

    当 PyYAML 不可用或 sidecar 内容异常时使用，目的是"不破坏人类字段"
    把 audit 块写回去。复杂 sidecar 应该由人类用真 yaml 编辑。
    """
    out: Dict[str, Any] = {}
    lines = text.splitlines()
    i = 0
    n = len(lines)
    while i < n:
        line = lines[i]
        stripped = line.strip()
        # 跳过空行/注释
        if not stripped or stripped.startswith("#"):
            i += 1
            continue
        # 顶层 key: value
        if not line.startswith((" ", "\t")) and ":" in line:
            key, _, val = line.partition(":")
            val = val.strip()
            if not val:
                # 嵌套块：往下读缩进行
                sub: Dict[str, Any] = {}
                j = i + 1
                while j < n:
                    subline = lines[j]
                    if not subline.strip() or subline.lstrip().startswith("#"):
                        j += 1
                        continue
                    if not subline.startswith((" ", "\t")):
                        break
                    sk, _, sv = subline.partition(":")
                    sv = sv.strip()
                    if sv.lower() in ("true", "false"):
                        sub[sk.strip()] = (sv.lower() == "true")
                    else:
                        # 去引号
                        if (sv.startswith('"') and sv.endswith('"')) or (
                            sv.startswith("'") and sv.endswith("'")
                        ):
                            sv = sv[1:-1]
                        sub[sk.strip()] = sv
                    j += 1
                out[key.strip()] = sub
                i = j
                continue
            # 单值
            if val.lower() in ("true", "false"):
                out[key.strip()] = (val.lower() == "true")
            else:
                if (val.startswith('"') and val.endswith('"')) or (
                    val.startswith("'") and val.endswith("'")
                ):
                    val = val[1:-1]
                out[key.strip()] = val
        i += 1
    return out


def _dump_yaml_with_audit(scenario_fields: Dict[str, Any], audit_fields: Dict[str, Any]) -> str:
    """把 scenario 字段 + audit 块 dump 回 yaml 文本。

    维持原 schema 的字段顺序 + 顶部注释（如果原 sidecar 有的话）。

    Quote style：
      - 含双引号 → 双引号 + escape 双引号
      - 含反斜杠（YAML 双引号里 \\、\\" 是合法转义，\\X 非法）→ 双引号 + escape 反斜杠
      - 含单引号 → 单引号 + escape 单引号（' → ''）
      - 否则 → 单引号（最稳，不需要 escape 任何字符）
    """
    lines: List[str] = []
    # scenario 字段
    for k in ("scenario", "target_capability", "expected_keep",
              "expected_compress", "source", "generated_at",
              "expected_dispatch_chain"):
        if k not in scenario_fields:
            continue
        v = scenario_fields[k]
        if isinstance(v, list):
            # 列表：每项独立 quote
            item_quoted = []
            for item in v:
                sv = str(item)
                if '"' in sv:
                    item_quoted.append('"' + sv.replace('\\', '\\\\').replace('"', '\\"') + '"')
                elif "'" in sv:
                    item_quoted.append("'" + sv.replace("'", "''") + "'")
                else:
                    item_quoted.append("'" + sv + "'")
            lines.append(f"{k}: [{', '.join(item_quoted)}]")
            continue
        sv = str(v or "")
        if '"' in sv or '\\' in sv:
            # 双引号 + escape 反斜杠和双引号
            sv_safe = sv.replace('\\', '\\\\').replace('"', '\\"')
            lines.append(f'{k}: "{sv_safe}"')
        elif "'" in sv:
            # 单引号 + escape 单引号（''）
            lines.append(f"{k}: '{sv.replace(chr(39), chr(39)*2)}'")
        else:
            # 默认单引号
            lines.append(f"{k}: '{sv}'")
    # 空行分隔
    lines.append("")
    # audit 块
    lines.append("# === audit 字段（自动维护，请勿手填内容字段）===")
    lines.append("audit:")
    for k in ("content_hash", "final_status", "llm_invoked",
              "llm_verified_at", "last_audit_tool", "skip"):
        v = audit_fields.get(k, "")
        if k == "skip" or k == "llm_invoked":
            lines.append(f"  {k}: {'true' if v else 'false'}")
        else:
            sv = str(v or "")
            if '"' in sv or '\\' in sv:
                sv_safe = sv.replace('\\', '\\\\').replace('"', '\\"')
                lines.append(f'  {k}: "{sv_safe}"')
            elif "'" in sv:
                lines.append(f"  {k}: '{sv.replace(chr(39), chr(39)*2)}'")
            else:
                lines.append(f"  {k}: '{sv}'")
    return "\n".join(lines) + "\n"


def read_sidecar_audit_block(plugin: str, case_filename: str) -> Optional[Dict[str, Any]]:
    """读 sidecar 的 audit 块。返回 dict（含 content_hash/final_status 等）
    或 None（sidecar 不存在 / 解析失败 / 没有 audit 块）。"""
    sc = _sidecar_path_for_case(plugin, case_filename)
    if not sc:
        return None
    try:
        with open(sc, "r", encoding="utf-8") as f:
            text = f.read()
    except (OSError, UnicodeDecodeError):
        return None
    parsed: Optional[Dict[str, Any]] = None
    try:
        import yaml  # type: ignore
        parsed = yaml.safe_load(text) or {}
    except ImportError:
        parsed = _parse_yaml_minimal(text)
    except Exception:
        parsed = _parse_yaml_minimal(text)
    if not isinstance(parsed, dict):
        return None
    audit = parsed.get("audit")
    if not isinstance(audit, dict):
        return None
    return audit


def read_sidecar_dispatch_chain(plugin: str, case_filename: str) -> Optional[List[str]]:
    """读 sidecar 的 expected_dispatch_chain 字段。

    返回 list（如 ["shell_session_plugin", "vcs_git_plugin"]）或 None。
    None 表示 sidecar 不存在 / 无此字段 / 字段为空。
    """
    sc = _sidecar_path_for_case(plugin, case_filename)
    if not sc:
        return None
    try:
        with open(sc, "r", encoding="utf-8") as f:
            text = f.read()
    except (OSError, UnicodeDecodeError):
        return None
    try:
        import yaml  # type: ignore
        parsed = yaml.safe_load(text) or {}
    except ImportError:
        parsed = _parse_yaml_minimal(text)
    except Exception:
        parsed = _parse_yaml_minimal(text)
    if not isinstance(parsed, dict):
        return None
    chain = parsed.get("expected_dispatch_chain")
    if isinstance(chain, list) and len(chain) >= 2:
        return chain
    return None


def read_sidecar_scenario_fields(plugin: str, case_filename: str) -> Optional[Dict[str, Any]]:
    """读 sidecar 的 scenario 语义字段（scenario/target_capability/expected_keep/
    expected_compress/source）。

    返回 dict 或 None（sidecar 不存在 / 解析失败）。
    用于规则校验 expected_keep/expected_compress 是否与 case 内容一致。
    """
    sc = _sidecar_path_for_case(plugin, case_filename)
    if not sc:
        return None
    try:
        with open(sc, "r", encoding="utf-8") as f:
            text = f.read()
    except (OSError, UnicodeDecodeError):
        return None
    try:
        import yaml  # type: ignore
        parsed = yaml.safe_load(text) or {}
    except ImportError:
        parsed = _parse_yaml_minimal(text)
    except Exception:
        parsed = _parse_yaml_minimal(text)
    if not isinstance(parsed, dict):
        return None
    return {
        "scenario": parsed.get("scenario", ""),
        "target_capability": parsed.get("target_capability", ""),
        "expected_keep": parsed.get("expected_keep", ""),
        "expected_compress": parsed.get("expected_compress", ""),
        "source": parsed.get("source", ""),
    }


def check_sidecar_field_consistency(
    scenario_fields: Optional[Dict[str, Any]],
    case_content: str,
    case_id: str,
) -> Dict[str, Any]:
    """规则校验 sidecar 的 expected_keep/expected_compress 是否出现在 case 内容里。

    返回 {"keep_mismatch": [...], "compress_mismatch": [...], "scenario_empty": bool}。
    keep_mismatch/compress_mismatch 是不在 case 内容中出现的关键词列表。
    """
    result: Dict[str, Any] = {
        "keep_mismatch": [],
        "compress_mismatch": [],
        "scenario_empty": False,
    }
    if not scenario_fields:
        return result
    scenario = (scenario_fields.get("scenario") or "").strip()
    if not scenario or scenario == '""':
        result["scenario_empty"] = True

    content_lower = case_content.lower()

    # 豁免标记：当 expected_keep/expected_compress 的值表示"无内容"时跳过检查
    _no_content_markers = ('""', "（无", "(无", "无 —", "无—", "无内容", "N/A", "n/a", "none", "—")

    def _token_matches_content(token: str, content: str) -> bool:
        """检查 token 是否在 content 中出现（宽松匹配）。

        策略：
        1. 精确子串匹配（大小写不敏感）→ 直接通过
        2. 如果 token 含空格（如 "gcc -c main.c -o main.o -Wall"），
           拆成空格分隔的片段，每个片段在 content 中出现 → 通过
           （LLM 常写缩略路径如 main.c，实际是 /long/path/main.c）
        3. 如果 token 含省略号 "..."，去掉 "..." 后的前后片段分别匹配 → 通过
        4. 编辑距离 ≤ 2 的近似匹配（处理 compile vs compiler 这类小差异）
        """
        tok_low = token.lower()
        # 1) 精确子串
        if tok_low in content:
            return True

        # 2) 处理省略号 "..."：LLM 用 ... 缩略，如 "foo.c ..." / "make: *** [Makefile:10: ...] Error 1"
        #    去掉 "..." 后，剩余部分按空格拆分，每个片段在 content 中出现即可
        if "..." in token:
            cleaned = token.replace("...", " ").strip()
            parts = [p for p in cleaned.split() if p and len(p) >= 2]
            if parts and all(p.lower() in content for p in parts):
                return True

        # 3) 多词 token：按空格拆分，每个片段在 content 中出现
        parts = token.split()
        if len(parts) >= 2:
            # 过滤掉太短的片段（如单字符 flag "-"）
            meaningful = [p for p in parts if len(p) >= 2]
            if meaningful and all(p.lower() in content for p in meaningful):
                return True

        # 4) 近似匹配：token 与 content 中任意等长子串的编辑距离 ≤ 2
        #    处理 "compile" vs "compiler" 这类差异
        tok_len = len(tok_low)
        if tok_len >= 4:  # 太短的词不做近似匹配，避免误判
            for i in range(len(content) - tok_len + 1):
                substr = content[i:i + tok_len]
                # 快速编辑距离：最多 2 个字符不同
                diffs = sum(1 for a, b in zip(tok_low, substr) if a != b)
                if diffs <= 2:
                    return True

        return False

    # expected_keep: 逗号/分号/斜杠分隔的关键词，每个应在 case 内容中出现
    keep_raw = (scenario_fields.get("expected_keep") or "").strip()
    compress_raw = (scenario_fields.get("expected_compress") or "").strip()
    if keep_raw or compress_raw:
        import re as _re
    if keep_raw and not any(keep_raw.startswith(m) or keep_raw == m for m in _no_content_markers):
        keep_tokens = [t.strip() for t in _re.split(r'[,;/；，、]', keep_raw) if t.strip()]
        for tok in keep_tokens:
            tok = tok.strip("'\"")
            if tok and len(tok) >= 2 and not _token_matches_content(tok, content_lower):
                result["keep_mismatch"].append(tok)

    # expected_compress: 同理
    if compress_raw and not any(compress_raw.startswith(m) or compress_raw == m for m in _no_content_markers):
        compress_tokens = [t.strip() for t in _re.split(r'[,;/；，、]', compress_raw) if t.strip()]
        for tok in compress_tokens:
            tok = tok.strip("'\"")
            if tok and len(tok) >= 2 and not _token_matches_content(tok, content_lower):
                result["compress_mismatch"].append(tok)

    return result


def write_sidecar_audit_block(sc_path: str, audit_fields: Dict[str, Any]) -> bool:
    """把 audit 字段合并写回 sidecar。

    保留原 scenario/target_capability/expected_keep/expected_compress/
    source/generated_at 等人类字段不动；只覆盖 audit 块。
    返回 True 表示写成功，False 表示失败。
    """
    try:
        with open(sc_path, "r", encoding="utf-8") as f:
            text = f.read()
    except (OSError, UnicodeDecodeError):
        return False

    # 解析原字段
    parsed: Dict[str, Any] = {}
    try:
        import yaml  # type: ignore
        parsed = yaml.safe_load(text) or {}
    except ImportError:
        parsed = _parse_yaml_minimal(text)
    except Exception:
        parsed = _parse_yaml_minimal(text)
    if not isinstance(parsed, dict):
        parsed = {}

    # 取出 scenario 字段（6 个 + expected_dispatch_chain）
    scenario_fields = {
        k: parsed.get(k, "")
        for k in ("scenario", "target_capability", "expected_keep",
                  "expected_compress", "source", "generated_at")
    }
    # 保留 expected_dispatch_chain（如果有的话）
    dispatch_chain = parsed.get("expected_dispatch_chain")
    if dispatch_chain:
        scenario_fields["expected_dispatch_chain"] = dispatch_chain
    # 写回
    new_text = _dump_yaml_with_audit(scenario_fields, audit_fields)
    try:
        with open(sc_path, "w", encoding="utf-8") as f:
            f.write(new_text)
    except OSError:
        return False
    return True


def _sidecar_cache_hit(audit: Optional[Dict[str, Any]],
                       content_hash: str,
                       curr_llm_invoked: bool) -> bool:
    """判定 L1 sidecar 是否可作为本次审计的缓存命中。

    复用条件（与 L2 latest.json 同语义）：
    - audit 块存在
    - content_hash 匹配
    - final_status ∈ {valid, not_registered, duplicate, title_mismatch, routing_boundary_unclear}
    - 上下次 LLM 开关状态一致
    - audit.skip != true
    """
    if not audit:
        return False
    if audit.get("skip"):
        return False
    if audit.get("content_hash") != content_hash:
        return False
    if audit.get("final_status", "") not in _SIDECAR_WRITABLE_STATUS:
        return False
    prev_llm = bool(audit.get("llm_invoked"))
    return prev_llm == curr_llm_invoked


def _reconstruct_cached_record(audit: Dict[str, Any],
                               content: str,
                               case_filename: str,
                               source: str = "sidecar",
                               plugin: str = "",
                               ) -> Dict[str, Any]:
    """从 L1 sidecar audit 块重建一个 case_record（用于跳过 lint/LLM 复用结论）。"""
    case_id = pathlib.Path(case_filename).stem
    # 如果 sidecar 有 expected_dispatch_chain，也带进 record
    dispatch_chain = None
    if plugin:
        dispatch_chain = read_sidecar_dispatch_chain(plugin, case_filename)
    llm_judgment = {
        "llm_invoked": bool(audit.get("llm_invoked")),
        "verdict": "valid",
        "source": source,
        "verified_at": audit.get("llm_verified_at", ""),
        "tool": audit.get("last_audit_tool", ""),
    }
    if dispatch_chain:
        llm_judgment["dispatch_chain_confirmed"] = dispatch_chain
    return {
        "case_id": case_id,
        "filename": case_filename,
        "final_status": audit.get("final_status", "valid"),
        "registered": True,  # L1 命中意味着上次 audit 通过；registered 默认 True
        "title_consistent": True,
        "command_tokens": [],
        "command_families": [],
        "routing_boundary_tool": "",
        "deterministic": {
            "ok_length": True, "anchor_ok": True, "mojibake_clean": True,
            "secrets_clean": True, "signal_anchors": True, "routing_dominant": False,
        },
        "llm_judgment": llm_judgment,
        "size_bytes": len(content.encode("utf-8")),
        "line_count": len(content.splitlines()),
        "content_hash": audit.get("content_hash", ""),
        "cache_hit": True,
        "cache_source": source,
        "content": content,
    }


# ============================================================================
# LLM 入口（fix #29: 提示词移到 scripts/prompts/audit/case_quality/）
# ============================================================================
# 原 inline LLM_SYSTEM_PROMPT / LLM_SYSTEM_PROMPT_BASE / LLM_SYSTEM_PROMPT_FOOTER
# / TYPE_REALISM_RULES 已抽到磁盘 .md 模板：
#   scripts/prompts/audit/case_quality/_base.md       — STRUCTURE_RULES S1-S6 + KB 注入段
#   scripts/prompts/audit/case_quality/_footer.md     — DECISION DISCIPLINE + JSON schema
#   scripts/prompts/audit/case_quality/{type}.md      — type-specific REALISM_RULES
#   scripts/prompts/audit/case_quality/default.md     — 未知 type 兜底
#
# 这里只保留 thin wrapper，真实加载走 audit_llm_common.build_case_quality_prompt。
# 保留 LLM_SYSTEM_PROMPT 常量名做向后兼容（旧测试可能 import 它），内容通过
# build_llm_prompt 实际从磁盘拼装。
LLM_SYSTEM_PROMPT = "__lazy__"  # 占位；运行时通过 build_llm_prompt 实际拼装


def call_llm(env, prompt, system_prompt=None):
    """fix #29: 转发到 audit_llm_common.call_llm_chat。

    保留旧签名 `(env, prompt, system_prompt=None)` 以兼容历史调用方。
    - env 可以是 dict 或 argparse.Namespace
    - system_prompt 不传则用 build_llm_prompt("shell") 拼装的完整 prompt
    """
    # 兼容 env 为 dict 或 Namespace
    def _get(key: str, default: str = "") -> str:
        if isinstance(env, dict):
            return env.get(key, default) or default
        return getattr(env, key, default) or default

    if system_prompt is None:
        system_prompt = build_llm_prompt("shell")

    cfg = _alc.LLMConfig.from_env(
        env={
            "OPENAI_API_KEY": _get("OPENAI_API_KEY"),
            "OPENAI_BASE_URL": _get("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            "OPENAI_MODEL": _get("LLM_MODEL"),
            "OPENAI_MAX_TOKENS": _get("LLM_MAX_TOKENS", "1024"),
            "OPENAI_TIMEOUT": _get("LLM_HTTP_TIMEOUT", "600"),
            "OPENAI_RETRIES": _get("LLM_RETRIES", "3"),
            "OPENAI_RETRY_SLEEP": _get("LLM_RETRY_SLEEP", "3"),
            "OPENAI_JSON_MODE": "1" if _get("LLM_JSON_MODE", "true").lower() == "true" else "0",
            "OPENAI_REASONING_EFFORT": _get("LLM_MERGE_REASONING_EFFORT"),
        },
        audit_kind="case_quality",
    )
    return _alc.call_llm_chat(cfg, prompt, system_prompt)


def build_llm_user_prompt(plugin, case_id, content, deterministic_findings, plugin_type="shell"):
    snippet = content if len(content) <= 4000 else content[:4000] + "\n... (truncated)"
    return f"""Task: judge the quality of one TokenSlim sample case.

Plugin: {plugin} (type: {plugin_type})
Case ID: {case_id}

Deterministic findings (run before LLM, you can confirm or override):
{json.dumps(deterministic_findings, ensure_ascii=False, indent=2)}

[Sample content]
{snippet}

Return JSON only.
"""


# fix #12: per-type 真实性规则（REALISM_RULES 的多态版本）。
# shell 类型用 LLM_SYSTEM_PROMPT（已含 R1-R7 shell 专属规则）；
# 其他类型用这里定义的"type-specific 规则段"——与 R1-R7 形态对齐，
# 但把"shell 提示符/错误格式/locale"换成对应领域的等价物。
def build_llm_prompt(plugin_type, plugin="", case_id=""):
    """fix #29: 转发到 audit_llm_common.build_case_quality_prompt。

    提示词模板从 scripts/prompts/audit/case_quality/ 磁盘加载：
      - _base.md（含 STRUCTURE_RULES S1-S7 + KB 注入段 + scenario 上下文）
      - {plugin_type}.md（type-specific REALISM_RULES，shell 用 R1-R7）
      - _footer.md（DECISION DISCIPLINE + JSON schema + sidecar_accuracy）
    未知 plugin_type 走 default.md 兜底。

    fix #14b：透传 plugin + case_id，让 scenario 上下文注入到 system prompt。
    """
    return _alc.build_case_quality_prompt(
        plugin_type=plugin_type, plugin=plugin, case_id=case_id
    )


VALID_STATUSES = {
    "valid", "needs_fix", "duplicate", "too_small", "not_registered",
    "missing_anchor", "weak_coverage", "routing_boundary_unclear",
    "fabricated",  # LLM 真实性仲裁发现 fabrication 迹象（R1-R7 任一命中）
}


# 7 个 REALISM_RULES 的字段名常量。LLM 输出/兜底都使用这套 schema。
REALISM_AUDIT_FIELDS = (
    "shell_personality_ok",
    "line_length_entropy_ok",
    "error_cause_effect_ok",
    "error_multiline_ok",
    "empty_output_authentic",
    "exit_stderr_consistent",
    "locale_plausible",
)


def _empty_realism_audit():
    """LLM 未参与时返回的兜底 schema。所有 ok 字段为 None（未知），indicators 为空。"""
    return {field: None for field in REALISM_AUDIT_FIELDS} | {"fabrication_indicators": []}


def _normalize_realism_audit(raw):
    """从 LLM 输出中安全提取 realism_audit，缺失字段填 None。

    输入可能是 dict（正常）、None（LLM 漏字段）、str（LLM 把字段写成字符串）、
    list 等异常类型。统一收敛到 _empty_realism_audit 形状。
    """
    if not isinstance(raw, dict):
        return _empty_realism_audit()
    out = _empty_realism_audit()
    for f in REALISM_AUDIT_FIELDS:
        v = raw.get(f)
        if isinstance(v, bool):
            out[f] = v
        # 非 bool（如 None / 字符串）保持 None
    raw_indicators = raw.get("fabrication_indicators", [])
    if isinstance(raw_indicators, list):
        out["fabrication_indicators"] = [str(t).strip() for t in raw_indicators if str(t).strip()]
    return out


SIDECAR_ACCURACY_FIELDS = (
    "scenario_accurate",
    "target_capability_accurate",
    "expected_keep_accurate",
    "expected_compress_accurate",
)


def _empty_sidecar_accuracy():
    """LLM 未参与时返回的兜底 sidecar_accuracy schema。"""
    return {f: None for f in SIDECAR_ACCURACY_FIELDS} | {"inaccurate_fields": []}


def _normalize_sidecar_accuracy(raw):
    """从 LLM 输出中安全提取 sidecar_accuracy。"""
    if not isinstance(raw, dict):
        return _empty_sidecar_accuracy()
    out = _empty_sidecar_accuracy()
    for f in SIDECAR_ACCURACY_FIELDS:
        v = raw.get(f)
        if isinstance(v, bool):
            out[f] = v
    raw_inaccurate = raw.get("inaccurate_fields", [])
    if isinstance(raw_inaccurate, list):
        out["inaccurate_fields"] = [str(t).strip() for t in raw_inaccurate if str(t).strip()]
    return out


def normalize_llm_judgment(llm_result, deterministic_findings):
    """合并 LLM 与确定性 lint。确定性 lint 优先于 LLM。"""
    if not llm_result:
        return {
            "status": "needs_fix" if deterministic_findings.get("blocking_issues") else "valid",
            "confidence": 0.0,
            "explanation": "LLM 未参与（无 OPENAI_API_KEY 或调用失败），仅依赖确定性 lint。",
            "fix_hint": "",
            "duplicate_of": "",
            "realism_audit": _empty_realism_audit(),
            "sidecar_accuracy": _empty_sidecar_accuracy(),
            "llm_invoked": False,
        }
    status = str(llm_result.get("status", "")).strip()
    if status not in VALID_STATUSES:
        status = "needs_fix"
    realism_audit = _normalize_realism_audit(llm_result.get("realism_audit"))
    fabrication_indicators = realism_audit.get("fabrication_indicators") or []
    # fix #11：fabrication 强信号自动升级。
    # ① LLM 显式 status="fabricated"
    # ② LLM 在 realism_audit 列出 indicators 但 status 漏标 → 强制 fabrication。
    # 优先级：fabricated > valid/needs_fix；但不覆盖 routing/not_registered/duplicate/title_mismatch
    # （这些是结构/路由层问题，会在 final 合成阶段处理）。
    if fabrication_indicators and status in ("valid", "needs_fix"):
        status = "fabricated"
    # 确定性升级
    if deterministic_findings.get("blocking_issues"):
        if status == "valid":
            status = "needs_fix"
    if deterministic_findings.get("mojibake"):
        status = "needs_fix"
    if deterministic_findings.get("secrets"):
        status = "needs_fix"
    if deterministic_findings.get("routing_boundary_tool"):
        if status == "valid":
            status = "routing_boundary_unclear"
    return {
        "status": status,
        "confidence": float(llm_result.get("confidence", 0.0) or 0.0),
        "explanation": str(llm_result.get("explanation", "")).strip(),
        "fix_hint": str(llm_result.get("fix_hint", "")).strip(),
        "duplicate_of": str(llm_result.get("duplicate_of", "")).strip(),
        "realism_audit": realism_audit,
        "sidecar_accuracy": _normalize_sidecar_accuracy(llm_result.get("sidecar_accuracy")),
        "llm_invoked": True,
    }


# ============================================================================
# 命令族覆盖矩阵
# ============================================================================

def compute_command_family_coverage(samples, target_families=HIGH_FREQUENCY_SHELL_FAMILIES):
    """
    对每个 case，提取其命令 token + 完整命令行（fix #4）映射到目标命令族；
    然后聚合得到已覆盖 / 缺失 / 部分覆盖的家族。
    """
    case_families = {}
    family_cases = {fam: [] for fam in target_families}
    family_aliases = {fam: [] for fam in target_families}

    for fname, path in sorted(samples.items()):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                content = f.read()
        except Exception:
            case_families[fname] = {"families": [], "tokens": [], "command_lines": []}
            continue
        tokens = extract_command_tokens(content)
        full_lines = extract_command_lines(content)
        families, aliases = map_tokens_to_families(tokens, full_lines)
        case_families[fname] = {
            "families": sorted(families),
            "tokens": tokens,
            "command_lines": [line for _, line in full_lines],
            "aliases": {k: sorted(set(v)) for k, v in aliases.items()},
        }
        for fam in families:
            family_cases[fam].append(pathlib.Path(fname).stem)
            family_aliases[fam].extend(aliases.get(fam, []))

    covered = sorted([f for f, cs in family_cases.items() if cs])
    missing = sorted([f for f, cs in family_cases.items() if not cs])
    return {
        "target_families": sorted(target_families),
        "covered": covered,
        "missing": missing,
        "covered_count": len(covered),
        "missing_count": len(missing),
        "target_count": len(target_families),
        "case_to_families": case_families,
        "family_to_cases": family_cases,
        "family_to_aliases": {f: sorted(set(v)) for f, v in family_aliases.items() if v},
    }


def build_missing_case_recommendations(missing_families, existing_case_ids):
    """为缺失命令族生成建议的新 case 草案（仅建议，不写文件）。"""
    recommendations = []
    for fam in missing_families:
        candidates = SHELL_FAMILY_ALIASES.get(fam, [fam])
        primary = candidates[0] if candidates else fam
        recommendations.append({
            "missing_family": fam,
            "primary_command": primary,
            "alt_commands": candidates,
            "suggested_filename": suggest_filename(fam, existing_case_ids),
            "rationale": (
                f"目标命令族 `{fam}` 在 60 类高频 shell 命令族中，"
                f"在当前 {len(existing_case_ids)} 个物理 sample 中均未出现。"
                f"建议补充一个 case，覆盖 `{primary}` 的真实 shell 输出与命令锚点。"
            ),
        })
    return recommendations


def suggest_filename(family, existing_case_ids):
    """
    根据已有 case 编号续号；保持命名风格 `<idx>_<shell>_<command>.log`。
    """
    used = set()
    for cid in existing_case_ids:
        m = re.match(r"^case_(\d+)_", cid)
        if m:
            used.add(int(m.group(1)))
    next_idx = max(used) + 1 if used else 1
    return f"case_{next_idx:03d}_shell_{family}.log"


# ============================================================================
# 报告输出
# ============================================================================

def export_case_quality(case_record, base_dir):
    case_dir = os.path.join(base_dir, case_record["case_id"])
    ensure_dir(case_dir)
    payload = {
        "case_id": case_record["case_id"],
        "filename": case_record["filename"],
        "size_bytes": case_record["size_bytes"],
        "line_count": case_record["line_count"],
        "registered": case_record["registered"],
        "registered_title": case_record.get("registered_title", ""),
        "title_consistent": case_record.get("title_consistent", None),
        "deterministic": case_record["deterministic"],
        "llm_judgment": case_record["llm_judgment"],
        "command_tokens": case_record["command_tokens"],
        "command_families": case_record["command_families"],
        "routing_boundary_tool": case_record.get("routing_boundary_tool", ""),
        "content_hash": case_record["content_hash"],
        "final_status": case_record["final_status"],
    }
    with open(os.path.join(case_dir, "quality.json"), "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=4, ensure_ascii=False)
        f.write("\n")
    # 同步写 original.txt 以便人读
    with open(os.path.join(case_dir, "original.txt"), "w", encoding="utf-8") as f:
        f.write(case_record.get("content", ""))


# ============================================================================
# fix #6：从 case_id 命名段提取命令族，用于与 content 的 command_families 交叉验证
# ============================================================================

_CASE_ID_PREFIX_RE = re.compile(r"^case_\d+_")


def _derive_families_from_case_id(case_id):
    """把 `case_009_powershell_parameter_binding_exception` 这种 case_id
    拆成 token，去掉 `case_NNN_` 前缀，逐 token 喂给 map_tokens_to_families，
    收集命中的 family 集合。

    命名约定是 case_NNN_<shell>_<cmd>_<sub...>，因此 <cmd> 段通常会落到一个
    命令族（如 `set`、`get-process`），<shell> 段（bash/powershell/cmd/zsh/fish）
    不是 family 但也不应误命中。返回空集合是正常的（意味着 case_id 没有
    显式命令族 token，仅靠 <sub> 段描述场景）。
    """
    if not case_id:
        return set()
    stripped = _CASE_ID_PREFIX_RE.sub("", case_id.lower())
    tokens = [t for t in stripped.split("_") if t]
    if not tokens:
        return set()
    # 用 map_tokens_to_families 的全行版本：把整段拼接当一行
    # 让"get-process"这种带连字符的命令也能命中
    pseudo_line = " ".join(tokens)
    families, _aliases = map_tokens_to_families(tokens, [(0, pseudo_line)])
    return set(families)


# ============================================================================
# fix #4：hard-block 状态专属解释生成器
# 设计：把 final_status → (explanation, fix_hint) 写成查表 + 字段拼装，
# 取代之前所有 hard-block 都拿 'LLM 未参与...' 兜底的做法。
# ============================================================================

def _compute_status_explanation(r):
    """根据 final_status + deterministic 字段拼出可读的具体原因。"""
    status = r.get("final_status", "")
    det = r.get("deterministic", {}) or {}

    if status == "routing_boundary_unclear":
        tool = r.get("routing_boundary_tool") or det.get("routing_boundary_tool", "")
        seg = det.get("routing_boundary_segments", "?/?")
        ratio = det.get("routing_boundary_ratio", 0.0)
        anchor = det.get("anchor_token", "")
        first_token = (r.get("command_tokens") or [""])[0].lower() if r.get("command_tokens") else ""
        if tool:
            # 区分双触发：①首 token 是 routing tool  ②routing 占比 ≥ 阈值
            first_is_routing = any(
                first_token == t or first_token.startswith(t + ".") or first_token.startswith(t + "-")
                for t in ROUTING_BOUNDARY_TOOLS
            )
            if first_is_routing:
                trigger = f"first_token=`{anchor}` 命中 ROUTING_BOUNDARY_TOOLS"
            else:
                trigger = (f"{seg} 段中 routing 工具 `{tool}` 占 {ratio*100:.0f}% "
                           f"≥ 阈值 {ROUTING_DOMINANT_THRESHOLD*100:.0f}%")
            # 这不是 "case 放错目录"，而是 "二段 dispatch 链" 期望：
            # 1) shell_session 先处理 prompt/行长度熵/错误锚点
            # 2) 检测到外部命令（git/cargo/kubectl/...）→ 让渡到专门插件处理命令输出
            host = r.get("plugin") or "shell_session_plugin"
            return (f"{trigger}；期望 dispatch 链：`{host}` → `{tool}`")
        return ("首命令命中专用工具让渡规则（routing_boundary_tool 为空时 LLM 也未参与仲裁）；"
                "期望 dispatch 链：shell_session → <specialist>")

    if status == "not_registered":
        return f"物理文件 `{r.get('filename', '')}` 未注册到 showcase.rs 的 SHOWCASE_CASES 数组中"

    if status == "missing_anchor":
        return f"未找到合法命令锚点：{det.get('anchor_reason', 'anchor token missing')}"

    if status == "too_small":
        return f"样本过小：size={r.get('size_bytes', 0)}B / lines={r.get('line_count', 0)} < MIN_BYTES={MIN_BYTES} / MIN_LINES={MIN_LINES}"

    if status == "duplicate":
        dup_of = (r.get("llm_judgment") or {}).get("duplicate_of", "")
        return f"与 {('`' + dup_of + '`') if dup_of else '另一 case'} 内容重复，应合并或删除次要副本"

    if status == "weak_coverage":
        return f"命令族覆盖不足：families={','.join(r.get('command_families', []))}，未命中目标高频族"

    if status == "title_mismatch":
        missing = det.get("title_assertions_missing", [])
        if missing:
            return f"title 中声明的关键词 `{', '.join(missing)}` 在 content 中未找到对应特征"
        return "title 命令族关键词与 content 实际命令族不一致"

    if status == "fabricated":
        indicators = ((r.get("llm_judgment") or {}).get("realism_audit") or {}).get("fabrication_indicators", [])
        if indicators:
            return "LLM 真实性仲裁发现 fabrication 迹象（R1-R7）：" + "; ".join(indicators[:3])
        return "LLM 真实性仲裁判定 case 为 LLM 合成（详见 realism_audit）"

    if status == "needs_fix":
        bi = det.get("blocking_issues", [])
        parts = list(bi) if bi else []
        keep_mm = det.get("sidecar_keep_mismatch", [])
        compress_mm = det.get("sidecar_compress_mismatch", [])
        if keep_mm:
            parts.append(f"sidecar expected_keep 幻觉: {', '.join(keep_mm[:3])}")
        if compress_mm:
            parts.append(f"sidecar expected_compress 幻觉: {', '.join(compress_mm[:3])}")
        return "确定性 lint 阻断：" + ", ".join(parts) if parts else "LLM 判定 needs_fix 且 deterministic 端无具体字段"

    # 兜底：原 LLM 解释（仅在没有匹配的状态时）
    return (r.get("llm_judgment") or {}).get("explanation", "")


def _compute_status_fix_hint(r):
    status = r.get("final_status", "")
    det = r.get("deterministic", {}) or {}

    if status == "routing_boundary_unclear":
        tool = r.get("routing_boundary_tool") or det.get("routing_boundary_tool", "")
        if tool:
            return (f"将本 case 从 shell_session 移到 `{tool}` 插件的 samples；"
                    f"或改写为纯 shell 主体（让专用工具调用变成次要 segment）")
        return "改写 case 让首段不再命中 ROUTING_BOUNDARY_TOOLS"

    if status == "not_registered":
        return f"在 `src/plugins/{r.get('case_id', '').split('_', 1)[0]}_plugin/showcase.rs` 中追加 `(\"{r.get('case_id', '')}\", \"<title>\")`"

    if status == "missing_anchor":
        return "在样本首段加合法 shell prompt + 命令（参考 PROMPT_RE）"

    if status == "too_small":
        return f"补全 sample 至 size ≥ {MIN_BYTES}B、lines ≥ {MIN_LINES}"

    if status == "duplicate":
        dup_of = (r.get("llm_judgment") or {}).get("duplicate_of", "")
        return f"删除本 case，或在 showcase 中合并到 {('`' + dup_of + '`') if dup_of else '主 case'}"

    if status == "weak_coverage":
        return "在样本里加入目标命令族的实际输出段落"

    if status == "title_mismatch":
        return "要么改 title 名称使其与 content 实际命令族/错误类一致，要么改写 content 兑现 title 的语义"

    if status == "fabricated":
        return ("用真实终端会话重录 case（不要让 LLM 直接生成）；"
                "或基于真实工具的 --help/--version 输出来构造；"
                "若 traceback/npmm 等多行错误只生成 1 行，需补全上下文")

    if status == "needs_fix":
        keep_mm = det.get("sidecar_keep_mismatch", [])
        compress_mm = det.get("sidecar_compress_mismatch", [])
        hints = ["根据 blocking_issues 逐项修正：长度 / 锚点 / mojibake / secrets"]
        if keep_mm:
            hints.append(f"sidecar expected_keep 中的 `{', '.join(keep_mm[:3])}` 不在 case 内容里，需修正或重新生成")
        if compress_mm:
            hints.append(f"sidecar expected_compress 中的 `{', '.join(compress_mm[:3])}` 不在 case 内容里，需修正或重新生成")
        return "；".join(hints)

    return (r.get("llm_judgment") or {}).get("fix_hint", "")


def render_markdown_report(
    plugin,
    version,
    out_dir,
    case_records,
    coverage,
    duplicates,
    recommendations,
):
    status_counter = {}
    for r in case_records:
        status_counter[r["final_status"]] = status_counter.get(r["final_status"], 0) + 1

    # fix #11：统计 LLM 真实性仲裁判定。仅 LLM 真正参与且 realism_audit 有值才计数。
    llm_invoked_count = sum(1 for r in case_records if r["llm_judgment"].get("llm_invoked"))
    fabricated_cases = [
        r for r in case_records
        if r["llm_judgment"].get("llm_invoked")
        and (r["llm_judgment"].get("realism_audit") or {}).get("fabrication_indicators")
    ]
    # R1-R7 各维度被判定为不通过的次数（用于揭示 audit 偏重）
    realism_misses = {f: 0 for f in REALISM_AUDIT_FIELDS}
    for r in case_records:
        ra = (r.get("llm_judgment") or {}).get("realism_audit") or {}
        for f in REALISM_AUDIT_FIELDS:
            if ra.get(f) is False:
                realism_misses[f] += 1

    md = []
    md.append(f"# Sample Case Quality Report - {plugin}")
    md.append("")
    md.append(f"- generated_at: {datetime.now().isoformat()}")
    md.append(f"- version: {version}")
    md.append(f"- physical_samples: {len(case_records)}")
    md.append(f"- registered_in_showcase: {sum(1 for r in case_records if r['registered'])}")
    md.append(f"- duplicate_groups: {len(duplicates)}")
    md.append(f"- llm_invoked: {llm_invoked_count} / {len(case_records)}")
    md.append("")
    md.append("## Status Distribution")
    md.append("")
    md.append("| status | count | human-readable meaning |")
    md.append("| --- | ---: | --- |")
    _STATUS_LEGEND = {
        "valid": "case 与 showcase title 一致，无 lint/LLM 阻断，可直接进 case_fixtures；"
                 "含 dispatch_chain_confirmed 时表示二段路由已确认",
        "needs_fix": "确定性 lint 命中硬阻断（mojibake / secrets / blocking_issues），需修 case 文件本身",
        "fabricated": "LLM 真实性仲裁发现 case 像 LLM 合成（行长度熵失真/无错误因果链），不收",
        "title_mismatch": "showcase.rs 声明的 title 关键词在 case content 中未兑现，需对齐",
        "routing_boundary_unclear": "检测到外部命令让渡但 sidecar 无 expected_dispatch_chain；"
                                    "需运行 fill_dispatch_chain.py 补全后重审",
        "not_registered": "物理文件存在但未注册到 showcase.rs 的 SHOWCASE_CASES 数组",
        "duplicate": "与同 plugin 另一 case 内容重复",
        "missing_anchor": "未找到合法命令锚点（首行不是 prompt/命令格式）",
        "weak_coverage": "命令族覆盖不足，未命中目标高频族",
        "too_small": "样本过小，size/line_count 低于 lint 阈值",
    }
    for status, count in sorted(status_counter.items(), key=lambda x: -x[1]):
        legend = _STATUS_LEGEND.get(status, "")
        md.append(f"| {status} | {count} | {legend} |")
    md.append("")
    if llm_invoked_count > 0:
        md.append("## Realism Audit Summary")
        md.append("")
        md.append(f"- llm_invoked: {llm_invoked_count} / {len(case_records)}")
        md.append(f"- fabricated_cases: {len(fabricated_cases)}")
        md.append("")
        md.append("### R1-R7 fail counters (only counted when LLM was invoked)")
        md.append("")
        md.append("| rule | field | fail_count |")
        md.append("| --- | --- | ---: |")
        rule_to_field = [
            ("R1 shell personality", "shell_personality_ok"),
            ("R2 line-length entropy", "line_length_entropy_ok"),
            ("R3 error cause-effect", "error_cause_effect_ok"),
            ("R4 error multi-line", "error_multiline_ok"),
            ("R5 empty-output authenticity", "empty_output_authentic"),
            ("R6 exit/stderr consistency", "exit_stderr_consistent"),
            ("R7 locale/region plausibility", "locale_plausible"),
        ]
        for label, f in rule_to_field:
            md.append(f"| {label} | `{f}` | {realism_misses[f]} |")
        md.append("")
        if fabricated_cases:
            md.append("### Fabricated case details")
            md.append("")
            for r in fabricated_cases:
                indicators = (r["llm_judgment"].get("realism_audit") or {}).get("fabrication_indicators", [])
                md.append(f"- **{r['case_id']}** (status={r['final_status']}): " +
                          "; ".join(indicators[:3]))
            md.append("")
    else:
        md.append("## Realism Audit Summary")
        md.append("")
        md.append("- llm_invoked: 0（默认 lint-only 模式，未调 LLM 真实性仲裁）")
        md.append("- 提示：`--llm-audit` 启用 LLM 仲裁；无 API key 时加 `--allow-llm-missing`")
        md.append("  降级到 lint-only；硬要求时用 `--require-llm-audit`。")
        md.append("")
    md.append("## Command Family Coverage")
    md.append("")
    md.append(f"- target_families: {coverage['target_count']}")
    md.append(f"- covered: {coverage['covered_count']}")
    md.append(f"- missing: {coverage['missing_count']}")
    md.append("")
    md.append("### Covered families")
    if coverage["covered"]:
        md.append("- " + ", ".join(f"`{f}`" for f in coverage["covered"]))
    else:
        md.append("- None")
    md.append("")
    md.append("### Missing families (target_high_frequency ∖ current_coverage)")
    if coverage["missing"]:
        md.append("- " + ", ".join(f"`{f}`" for f in coverage["missing"]))
    else:
        md.append("- None")
    md.append("")
    md.append("## Duplicate Groups")
    md.append("")
    if not duplicates:
        md.append("None")
    else:
        for g in duplicates:
            md.append(f"- primary: `{g['primary']}`  →  duplicates: {', '.join(f'`{x}`' for x in g['duplicates'])}")
    md.append("")
    md.append("## Recommended Missing Cases")
    md.append("")
    if not recommendations:
        md.append("All target families are already covered.")
    else:
        for r in recommendations:
            md.append(f"- **{r['missing_family']}** → `{r['suggested_filename']}` (primary: `{r['primary_command']}`)")
        md.append("")
    md.append("## Per-Case Detail")
    md.append("")
    md.append("| case_id | status | registered | title_consistent | first_token | mojibake | secrets | routing_tool | dispatch_chain | sidecar_mismatch |")
    md.append("| --- | --- | :-: | :-: | --- | :-: | :-: | --- | --- | --- |")
    for r in sorted(case_records, key=lambda x: x["case_id"]):
        tc = r.get("title_consistent")
        tc_str = "✓" if tc is True else ("✗" if tc is False else "?")
        dc = r.get("llm_judgment", {}).get("dispatch_chain_confirmed")
        dc_str = " → ".join(dc) if dc else "-"
        det = r.get("deterministic", {})
        keep_mm = det.get("sidecar_keep_mismatch", [])
        compress_mm = det.get("sidecar_compress_mismatch", [])
        mm_parts = []
        if keep_mm:
            mm_parts.append(f"keep:{','.join(keep_mm[:3])}")
        if compress_mm:
            mm_parts.append(f"compress:{','.join(compress_mm[:3])}")
        mm_str = " ".join(mm_parts) if mm_parts else "✓"
        md.append(
            f"| {r['case_id']} | {r['final_status']} | "
            f"{'Y' if r['registered'] else 'N'} | "
            f"{tc_str} | "
            f"`{(r['command_tokens'][:1] or ['-'])[0]}` | "
            f"{'Y' if r['deterministic'].get('mojibake') else 'N'} | "
            f"{'Y' if r['deterministic'].get('secrets') else 'N'} | "
            f"`{r.get('routing_boundary_tool') or '-'}` |"
            f"`{dc_str}` |"
            f"{mm_str} |"
        )
    md.append("")
    # fix #3 + #5：title_consistent=False 已被升级为独立 final_status=title_mismatch，
    # 并已纳入下方 Hard-Block List 渲染（解释走 _compute_status_explanation）。
    # 故此处不再单列 "Title Mismatch Warning" 章节，避免与 hard-block 重复。
    md.append("## Hard-Block List (must fix before `audit_case_metrics.py`)")
    md.append("")
    blocking = [r for r in case_records if r["final_status"] in {
        "needs_fix", "duplicate", "too_small", "not_registered",
        "missing_anchor", "routing_boundary_unclear", "weak_coverage",
        "title_mismatch", "fabricated",  # fix #11：fabrication 仲裁失败也是 hard-block
    }]
    if not blocking:
        md.append("None")
    else:
        md.append("| case_id | status | explanation | fix_hint |")
        md.append("| --- | --- | --- | --- |")
        for r in sorted(blocking, key=lambda x: x["case_id"]):
            # fix #4：基于 deterministic 字段生成状态专属解释，取代
            # 之前所有 11 行都写 'LLM 未参与...' 这种同质化字符串。
            explanation = _compute_status_explanation(r)
            fix_hint = _compute_status_fix_hint(r)
            md.append(
                f"| {r['case_id']} | {r['final_status']} | "
                f"{explanation} | {fix_hint} |"
            )
    md.append("")
    # fix #14：Sidecar 字段规则校验结果。
    # 不论 final_status 是什么，只要 expected_keep/expected_compress 里的
    # 关键词不在 case 内容中出现，就列出来。这是纯规则校验，能捕获 LLM 幻觉。
    sidecar_mismatches = []
    for r in case_records:
        det = r.get("deterministic", {}) or {}
        keep_mm = det.get("sidecar_keep_mismatch", [])
        compress_mm = det.get("sidecar_compress_mismatch", [])
        if keep_mm or compress_mm:
            sidecar_mismatches.append((r["case_id"], keep_mm, compress_mm))
    md.append("## Sidecar Field Mismatch (expected_keep / expected_compress not in case content)")
    md.append("")
    if not sidecar_mismatches:
        md.append("None — all sidecar fields match case content ✓")
    else:
        md.append(f"**{len(sidecar_mismatches)} case(s)** with mismatched sidecar fields:")
        md.append("")
        md.append("| case_id | keep_mismatch | compress_mismatch |")
        md.append("| --- | --- | --- |")
        for cid, km, cm in sidecar_mismatches:
            km_str = ", ".join(f"`{k}`" for k in km[:5]) if km else "-"
            cm_str = ", ".join(f"`{c}`" for c in cm[:5]) if cm else "-"
            md.append(f"| {cid} | {km_str} | {cm_str} |")
    md.append("")
    # fix #14b：LLM 语义审计 sidecar_accuracy 结果。
    # 当 LLM 参与审计时，它会判断 scenario/target_capability/expected_keep/
    # expected_compress 是否准确描述了 case 内容。这是规则校验之上的语义层。
    sidecar_llm_issues = []
    for r in case_records:
        sa = (r.get("llm_judgment") or {}).get("sidecar_accuracy")
        if not sa or not isinstance(sa, dict):
            continue
        inaccurate = sa.get("inaccurate_fields", [])
        if inaccurate or any(sa.get(f) is False for f in SIDECAR_ACCURACY_FIELDS):
            sidecar_llm_issues.append((r["case_id"], sa))
    md.append("## Sidecar Semantic Accuracy (LLM judgment)")
    md.append("")
    if not sidecar_llm_issues:
        md.append("None — no LLM-audited sidecar inaccuracies found ✓")
    else:
        md.append(f"**{len(sidecar_llm_issues)} case(s)** with inaccurate sidecar fields per LLM:")
        md.append("")
        md.append("| case_id | scenario | target_cap | keep | compress | inaccurate_fields |")
        md.append("| --- | :-: | :-: | :-: | :-: | --- |")
        for cid, sa in sidecar_llm_issues:
            def _mark(v):
                if v is True: return "✓"
                if v is False: return "✗"
                return "?"
            md.append(
                f"| {cid} | {_mark(sa.get('scenario_accurate'))} | "
                f"{_mark(sa.get('target_capability_accurate'))} | "
                f"{_mark(sa.get('expected_keep_accurate'))} | "
                f"{_mark(sa.get('expected_compress_accurate'))} | "
                f"{', '.join(f'`{f}`' for f in sa.get('inaccurate_fields', [])[:5])} |"
            )
    md.append("")
    md.append("## Output Artifacts")
    md.append("")
    md.append(f"- `{os.path.join(out_dir, 'case_quality_report.json')}`")
    md.append(f"- `{os.path.join(out_dir, 'case_quality_report.md')}`")
    md.append(f"- `{os.path.join(out_dir, 'case_quality_latest.json')}`")
    md.append(f"- `{os.path.join(out_dir, 'command_family_coverage.json')}`")
    md.append(f"- `{os.path.join(out_dir, 'cases/<case_id>/quality.json')}`")
    md.append("")
    return "\n".join(md)


# ============================================================================
# 主流程
# ============================================================================

def parse_args():
    p = argparse.ArgumentParser(
        description="Audit the quality of raw sample cases for a TokenSlim plugin.",
    )
    p.add_argument("--plugin", required=True)
    p.add_argument("--out-dir", default="")
    p.add_argument("--version", default="")
    p.add_argument("--target-families", default="",
                   help="覆盖目标命令族文件（可选，每行一族；缺省使用内置 shell 列表）")
    p.add_argument("--llm-audit", action="store_true",
                   help="显式调用 LLM 做质量判定（默认 lint-only）。"
                        "无 OPENAI_API_KEY 时配合 --allow-llm-missing 降级为 lint-only。")
    p.add_argument("--require-llm-audit", action="store_true",
                   help="强制调用 LLM 做质量判定。无 OPENAI_API_KEY 时直接报错（硬失败）。"
                        "相当于 --llm-audit 的强一致版本。")
    p.add_argument("--allow-llm-missing", action="store_true",
                   help="在无 OPENAI_API_KEY 时允许静默退化为 lint-only。"
                        "不与 --require-llm-audit 同时生效（后者优先级更高）。")
    p.add_argument("--skip-duplicate-detection", action="store_true")
    p.add_argument("--no-cache", action="store_true",
                   help="fix: 禁用增量审计缓存。默认按 content_hash 复用 case_quality_latest.json"
                        "中 final_status=valid 的结论，跳过 lint+LLM 重新执行。"
                        "改了 lint 规则或 LLM 提示词后用此开关强制全量重跑。")
    p.add_argument("--strict-drift", action="store_true",
                   help="fix #29: 若 preflight 漂移检测发现任何 warning/error，立即 sys.exit(1)。"
                        "info 级 finding 不影响退出码。")
    p.add_argument("--skip-drift", action="store_true",
                   help="fix #29: 完全跳过 preflight 漂移检测（不推荐）。")
    # PowerShell 兼容别名
    p.add_argument("--Plugin", dest="plugin")
    p.add_argument("--OutDir", dest="out_dir")
    p.add_argument("--Version", dest="version")
    p.add_argument("--TargetFamilies", dest="target_families")
    p.add_argument("--LlmAudit", dest="llm_audit", action="store_true")
    p.add_argument("--RequireLlmAudit", dest="require_llm_audit", action="store_true")
    p.add_argument("--AllowLlmMissing", dest="allow_llm_missing", action="store_true")
    p.add_argument("--SkipDuplicateDetection", dest="skip_duplicate_detection", action="store_true")
    p.add_argument("--StrictDrift", dest="strict_drift", action="store_true")
    p.add_argument("--SkipDrift", dest="skip_drift", action="store_true")
    return p.parse_args()


def main():
    args = parse_args()
    plugin = args.plugin
    if not re.match(r'^[a-z0-9_]+$', plugin):
        raise ValueError(f"Invalid plugin name: '{plugin}'. Must be lowercase/digits/underscores.")
    # fix #6: 默认输出到 docs/audit/<plugin>/sample_quality/，
    # 避免与 audit_case_metrics.py 的 cases/ 镜像目录冲突。
    base_dir = args.out_dir or os.path.join("docs", "audit", plugin)
    if not args.out_dir and os.path.basename(base_dir.rstrip("/\\")) != "sample_quality":
        out_dir = os.path.join(base_dir, "sample_quality")
    else:
        out_dir = base_dir
    version = args.version or datetime.now().strftime("%Y%m%d-%H%M%S")
    ensure_dir(out_dir)

    target_families = HIGH_FREQUENCY_SHELL_FAMILIES
    if args.target_families and os.path.exists(args.target_families):
        with open(args.target_families, "r", encoding="utf-8") as f:
            custom = [line.strip() for line in f if line.strip() and not line.startswith("#")]
        if custom:
            target_families = custom

    print(f"plugin={plugin}")
    print(f"out_dir={out_dir}")
    print(f"version={version}")
    print(f"target_families={len(target_families)}")

    # fix #29: preflight 漂移检测（在一切 lint/LLM 之前）
    # ────────────────────────────────────────────────────────────
    # 7 条漂移轴：samples-vs-mod-rs / case-count-mismatch / sidecar-missing /
    # ghost-case / plugin-dict-missing / cap-index-stale / samples-vs-mod-rs
    # 目的：samples/ 目录与 src/plugins/mod.rs / showcase.rs / capability_index.json
    # 不一致时，第一时间发现"多了一个插件/多了一个 case/少了一个 sidecar"。
    if not args.skip_drift:
        preflight_findings = _alc.drift_audit(
            plugin,
            samples_dir="samples",
            source_dir="src/plugins",
            mod_rs_path="src/plugins/mod.rs",
            cap_index_path="docs/audit/plugin_capability_index.json",
        )
        if preflight_findings:
            warning_count = sum(1 for f in preflight_findings if f["severity"] in ("warning", "error"))
            info_count = len(preflight_findings) - warning_count
            print(f"\n[preflight] drift_audit: {len(preflight_findings)} finding(s) "
                  f"(warning/error={warning_count}, info={info_count})")
            for f in preflight_findings:
                print(f"  [{f['severity']:7s}] {f['axis']:24s} {f['message']}")
            # 写一份 JSON 报告（与 lint 报告平级）
            drift_report_path = os.path.join(out_dir, f"drift_audit_{version}.json")
            ensure_dir(os.path.dirname(drift_report_path))
            with open(drift_report_path, "w", encoding="utf-8") as fp:
                json.dump(
                    {
                        "plugin": plugin,
                        "version": version,
                        "findings": preflight_findings,
                        "summary": {
                            "total": len(preflight_findings),
                            "warning_or_error": warning_count,
                            "info": info_count,
                        },
                    },
                    fp,
                    indent=2,
                    ensure_ascii=False,
                )
            print(f"[preflight] report: {drift_report_path}")
            if args.strict_drift and warning_count > 0:
                raise SystemExit(
                    f"ERROR: --strict-drift set and {warning_count} warning/error-level "
                    f"drift finding(s) detected. Fix drift first, then re-run."
                )
        else:
            print("[preflight] drift_audit: clean (0 findings)")
    else:
        print("[preflight] drift_audit: skipped (--skip-drift)")

    # fix #7: --require-llm-audit 必须有可用的 API key，否则直接硬失败，
    # 避免“声明 require 却静默降级”造成的假绿。
    # 在做 coverage / lint 之前就检查，避免做了无用的工作再报错。
    # --allow-llm-missing 是显式允许降级的开关。
    env = parse_env()
    has_llm = bool(env.get("OPENAI_API_KEY"))
    if args.require_llm_audit and not args.allow_llm_missing and not has_llm:
        raise SystemExit(
            "ERROR: --require-llm-audit is set but OPENAI_API_KEY is missing or empty. "
            "Either provide OPENAI_API_KEY (in env or .env) or drop --require-llm-audit "
            "(or pass --allow-llm-missing to permit lint-only degradation)."
        )

    registered_cases = parse_showcase_rs_cases(plugin)
    physical_samples = get_physical_samples(plugin)
    if not physical_samples:
        raise FileNotFoundError(f"No physical samples found for plugin '{plugin}'.")

    registered_files = {}
    registered_ids = set()
    if registered_cases is not None:
        registered_files = {r["filename"]: r for r in registered_cases}
        registered_ids = {r["case_id"] for r in registered_cases}

    # 1) 重复检测
    duplicates = []
    if not args.skip_duplicate_detection:
        duplicates = detect_duplicates(physical_samples)
        for g in duplicates:
            print(f"duplicate_group={g['primary']} members={','.join(g['members'])}")

    # 2) 命令族覆盖矩阵
    # fix #12：HIGH_FREQUENCY_SHELL_FAMILIES 是 shell 专属，对非 shell 插件无意义。
    # 非 shell 类型用空 target_families 跑一遍，coverage=covered=0/missing=0，
    # 避免给 yaml/web_log 凭空报"missing 60 families"。
    _plugin_type_for_coverage = get_plugin_type(plugin)
    if _plugin_type_for_coverage == "shell":
        coverage = compute_command_family_coverage(physical_samples, target_families)
    else:
        coverage = compute_command_family_coverage(physical_samples, target_families=set())
    print(f"covered_families={coverage['covered_count']}")
    print(f"missing_families={coverage['missing_count']}")
    for fam in coverage["missing"]:
        print(f"missing_family={fam}")

    # 3) 每个 case 的确定性 lint
    case_records = []
    env = parse_env()
    has_llm = bool(env.get("OPENAI_API_KEY"))

    # fix: 增量审计双层缓存
    #   L1 — samples/<plugin>/case_NNN.scenario.yaml 里的 audit: 块（持久化，
    #        跨 run / 跨机器 / 跨分支都可用；人类可手填 audit.skip: true 强制跳过）
    #   L2 — docs/audit/<plugin>/sample_quality/case_quality_latest.json 的快照
    #        （只在本次产物目录里，丢了就重建；作用是把"刚跑过的有效结论"快速
    #         推回 L1）
    # 优先级：L1 hash 命中 → 直接用；L1 miss → 查 L2；L2 miss → 跑 lint+LLM
    # 写回：跑完一个 case 后把 {content_hash, final_status, llm_invoked,
    #         llm_verified_at, last_audit_tool, skip} 写回 sidecar 的 audit: 块
    # 复用条件（必须全部满足）：
    #   (1) content_hash 命中
    #   (2) cached final_status ∈ {valid, not_registered, duplicate, title_mismatch,
    #       routing_boundary_unclear}（结构性结论稳定；不缓存 needs_fix / fabricated
    #       这类缺陷判定，因为 lint 规则 / LLM 提示词改了可能变成 valid）
    #   (3) 上次是否调 LLM 与本次 --llm-audit 开关一致（避免"上次 lint-only valid
    #       这次开 LLM 该走 LLM"被静默跳过）
    #   (4) audit.skip != true（人类可以手动 skip: true 永久跳过）
    cache_index: Dict[str, dict] = {}  # content_hash -> cached record
    cache_disabled_reason = ""
    l1_hits = 0  # sidecar 命中
    l2_hits = 0  # latest.json 命中
    if args.no_cache:
        cache_disabled_reason = "--no-cache"
    else:
        # L1: 读每个 case 物理文件对应的 sidecar（在本轮循环里会预读，但
        # cache_index 阶段只能从 latest.json 收 — sidecar 的 hash 要等物理
        # 文件读出来才知道。简化方案：本轮先收集"哪些 case 有 sidecar"，
        # 进 case 循环时再查 sidecar cache。）
        # L2: 读 latest.json，按 content_hash 索引
        latest_cache_path = os.path.join(out_dir, "case_quality_latest.json")
        if os.path.exists(latest_cache_path):
            try:
                with open(latest_cache_path, "r", encoding="utf-8") as cf:
                    latest_obj = json.load(cf)
                for cr in latest_obj.get("cases", []) or []:
                    h = cr.get("content_hash")
                    st = cr.get("final_status", "")
                    lj = cr.get("llm_judgment", {}) or {}
                    prev_llm_invoked = bool(lj.get("llm_invoked"))
                    curr_llm_invoked = bool(args.llm_audit or args.require_llm_audit)
                    if (h and st in {
                        "valid", "not_registered", "duplicate",
                        "title_mismatch", "routing_boundary_unclear",
                    } and prev_llm_invoked == curr_llm_invoked):
                        cache_index[h] = cr
            except (OSError, ValueError) as exc:
                cache_disabled_reason = f"latest.json read failed: {exc}"
        else:
            cache_disabled_reason = "no previous case_quality_latest.json"

    cache_hits = 0
    cache_misses = 0
    sidecar_writes: List[Tuple[str, str, str, bool, str]] = []  # (case_id, hash, status, llm_invoked, path)

    if cache_index:
        print(f"[cache] indexed {len(cache_index)} prev case(s) by content_hash"
              + (f" (disabled: {cache_disabled_reason})" if cache_disabled_reason else ""))
    elif cache_disabled_reason:
        print(f"[cache] disabled: {cache_disabled_reason}")

    # fix #7: fail-fast 已在上方执行（argparse + env 解析之后立即校验）。
    # 这里不再重复检查。

    for fname, path in sorted(physical_samples.items()):
        case_id = pathlib.Path(fname).stem
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                content = f.read()
        except Exception as e:
            print(f"ERROR: failed to read {path}: {e}", file=sys.stderr)
            content = ""

        size_bytes = len(content.encode("utf-8"))
        line_count = len(content.splitlines())
        content_hash = sha256_hex(content)

        # fix: 增量审计双层缓存 — L1 sidecar 优先，L2 latest.json 兜底
        if not args.no_cache:
            # L1: 读 sidecar.audit 块。sidecar 与 case 同目录，命名约定
            # <case_stem>.scenario.yaml
            sidecar_audit = read_sidecar_audit_block(plugin, fname)
            curr_llm_invoked = bool(args.llm_audit or args.require_llm_audit)
            if _sidecar_cache_hit(sidecar_audit, content_hash, curr_llm_invoked):
                rec = _reconstruct_cached_record(sidecar_audit, content, fname, source="sidecar", plugin=plugin)
                case_records.append(rec)
                cache_hits += 1
                l1_hits += 1
                print(f"case={case_id} status={rec['final_status']} cache_hit=Y (L1 sidecar)")
                continue

        # L2: content_hash 命中 latest.json 缓存时复用 final_status 跳过 lint+LLM
        cached = cache_index.get(content_hash)
        if cached is not None:
            # 浅拷贝，避免 mutation 影响 cached 对象；标记 cache_hit 让报告/产物可追溯。
            rec = dict(cached)
            rec["cache_hit"] = True
            rec["cache_source"] = "latest_json"
            rec["content"] = content  # content 必须用最新的（产物要写 original.txt 镜像）
            case_records.append(rec)
            cache_hits += 1
            l2_hits += 1
            print(f"case={case_id} status={rec.get('final_status', '?')} cache_hit=Y (L2 latest.json)")
            continue

        cache_misses += 1

        registered = fname in registered_files
        registered_title = registered_files.get(fname, {}).get("title", "")

        # fix #12：根据 plugin 名取 lint_config，让阈值/锚点/信号检查走类型分支。
        plugin_type, lint_config = get_lint_config(plugin)

        ok_len, len_reason = check_minimum_length(content, lint_config=lint_config)
        anchor_ok, anchor_tok, anchor_reason = check_command_anchor(content, lint_config=lint_config)
        mojibake = check_mojibake(content)
        secrets = check_secrets(content)
        signals = check_signal_anchors(content, lint_config=lint_config)
        # fix #12：非 shell 类型不解析 command tokens / routing boundary，
        # 避免 extract_command_tokens 把 yaml 里的 `name: foo` 误判成命令族。
        if lint_config.get("routing_boundary_applicable", False):
            tokens = extract_command_tokens(content)
            full_lines = extract_command_lines(content)
            families, aliases = map_tokens_to_families(tokens, full_lines)
            # fix #7：用"段占比"取代"首 token"判定
            routing_dominant, routing_tool, routing_ratio, routing_count, routing_total = (
                compute_routing_boundary_dominant(content, tokens)
            )
        else:
            tokens = []
            full_lines = []
            families = set()
            aliases = []
            routing_dominant, routing_tool, routing_ratio, routing_count, routing_total = (
                False, "", 0.0, 0, 0
            )

        # title 一致性：title 中是否提到对应命令族关键词（粗略判定）
        # fix #1：用词边界正则替代裸子串匹配，避免 'ss' 命中 'success'、
        # 'rm' 命中 'firm'、'ps' 命中 'pset' 这类假阳性。
        # 同时按"别名长度从长到短"优先匹配，避免短别名提前抢命中。
        # fix #12：仅 shell 类型跑"命令族"title 一致性；其他类型走结构化
        # title 一致性（"title 里出现 Y/N keyword 与 content 结构吻合"）。
        title_consistent = None
        if registered and registered_title and lint_config.get("routing_boundary_applicable", False):
            t = registered_title.lower()
            # 把 (family, alias) 摊平为列表，按 alias 长度倒序，长别名优先
            flat_aliases = []
            for fam, als in SHELL_FAMILY_ALIASES.items():
                for a in als:
                    flat_aliases.append((fam, a.lower()))
            flat_aliases.sort(key=lambda x: len(x[1]), reverse=True)

            inferred = []
            consumed_spans = []  # 已匹配的 (start, end)，防止重叠
            for fam, a in flat_aliases:
                if not a:
                    continue
                # 词边界匹配：a 必须作为整词出现（两侧非 [a-z0-9_-]）
                pattern = re.compile(r"(?<![a-z0-9_\-])" + re.escape(a) + r"(?![a-z0-9_\-])")
                m = pattern.search(t)
                if not m:
                    continue
                span = (m.start(), m.end())
                # 防止"长别名尚未匹配、短别名部分重叠"的双计数
                if any(not (span[1] <= s or span[0] >= e) for s, e in consumed_spans):
                    continue
                consumed_spans.append(span)
                if fam not in inferred:
                    inferred.append(fam)
            if inferred and not (set(inferred) & families):
                title_consistent = False
            elif not inferred and not families:
                title_consistent = True
            elif inferred and (set(inferred) & families):
                title_consistent = True
            else:
                title_consistent = None

        # fix #2：title 断言关键词校验。把 title 里声明的语义（"command not found"、
        # "ParserError" 等）反向回归到 content，若缺失则降级 title_consistent。
        title_assertions_checked = 0
        title_assertions_satisfied = 0
        title_assertions_missing = []
        # fix #12：TITLE_ASSERTION_KEYWORDS 里的关键词多数是 shell 专属
        # （command not found / permission denied / pipefail 等）。非 shell 类型
        # 不跑这条校验，避免 traceback 标题里的 "Error" 关键字被误匹配到
        # TITLE_ASSERTION_KEYWORDS 中的某条 shell-specific content 模式。
        if (
            registered and registered_title and content
            and lint_config.get("routing_boundary_applicable", False)
        ):
            for title_pat, content_pat in TITLE_ASSERTION_KEYWORDS:
                if re.search(title_pat, registered_title, re.IGNORECASE):
                    title_assertions_checked += 1
                    if re.search(content_pat, content, re.IGNORECASE):
                        title_assertions_satisfied += 1
                    else:
                        title_assertions_missing.append(title_pat)
            if title_assertions_checked > 0 and title_assertions_satisfied < title_assertions_checked:
                # 存在 title 声明但 content 未兑现：覆盖之前的结果为 False
                title_consistent = False
            elif title_consistent is None and title_assertions_checked > 0 and title_assertions_satisfied == title_assertions_checked:
                # title 没命令族关键词但所有断言都满足：升 None → True
                title_consistent = True

        # fix #12：非 shell 类型的"轻量"title 一致性校验。
        # shell 类型已被上面 "命令族 + 断言关键词" 双检覆盖；其他类型走一个保守的
        # 兜底：title 中提到的"高置信度结构关键词"在 content 中应至少出现一次。
        # 没匹配到则保留 title_consistent=None（不强制 false），让 LLM 来最终判断。
        if (
            registered and registered_title and content
            and not lint_config.get("routing_boundary_applicable", False)
        ):
            t_low = registered_title.lower()
            c_low = content.lower()
            # 跨类型的"高置信度"关键词字典：title 提到就期望 content 也出现
            STRUCTURAL_TITLE_HINTS = {
                "access_log": [
                    ("access", r"\baccess\b"),
                    ("nginx", r"\bnginx\b"),
                    ("apache", r"\bapache\b"),
                    ("error", r"\berror\b"),
                ],
                "data_struct": [
                    ("yaml", r":\s|---"),
                    ("json", r"[\{\"]"),
                    ("xml", r"<[A-Za-z]"),
                    ("html", r"<(?:html|div|span|body)"),
                ],
                "vcs": [
                    ("git", r"\bgit\b|\bcommit\b"),
                    ("svn", r"\bsvn\b|\bsubversion\b"),
                    ("hg", r"\bmercurial\b|\bchangeset\b"),
                    ("p4", r"\bp4\b|\bperforce\b"),
                ],
                "build": [
                    ("compile", r"\b(?:compile|compiling|compiled)\b"),
                    ("link", r"\b(?:link|linking)\b"),
                    ("test", r"\b(?:test|tests|passed|failed)\b"),
                    ("error", r"\berror\b"),
                ],
                "error_trace": [
                    ("traceback", r"\btraceback\b"),
                    ("error", r"\berror\b"),
                    ("exception", r"\bexception\b"),
                    ("panic", r"\bpanic\b"),
                ],
            }
            hints = STRUCTURAL_TITLE_HINTS.get(plugin_type, [])
            hint_matched = any(
                kw in t_low and re.search(pattern, c_low, re.IGNORECASE)
                for kw, pattern in hints
            )
            # title 提到了类型关键词但 content 完全不匹配 → 标记 None（保留给 LLM）
            if not hint_matched and any(kw in t_low for kw, _ in hints):
                title_consistent = None
            elif hint_matched:
                title_consistent = True

        # fix #6：从 case_id（剥离 case_NNN_ 前缀后）抽取命令族，与 content 的
        # command_families 做交叉验证。case_id 命名约定是 case_NNN_<shell>_<cmd>_<sub>，
        # 因此 <cmd> 段通常会落到一个 family；如果 case_id 提到 X 而 content 没有 X，
        # 就是 case 命名与内容脱节的强信号。
        # 不去重、不影响 title_consistent，仅在 deterministic_findings 留痕
        # （在 #3 的 Title Mismatch Warning / #5 的 hard-block 流程中可被消费）。
        # fix #12：非 shell 类型 _derive_families_from_case_id 返回空集合，case_id
        # 命名约定不适用于 yaml/vcs/build/traceback 等。这里改为只在 shell 类型
        # 计算 case_id_families，避免给非 shell case 凭空加 mismatch 噪声。
        if lint_config.get("routing_boundary_applicable", False):
            case_id_families = _derive_families_from_case_id(case_id)
            case_id_family_overlap = sorted(case_id_families & families) if case_id_families else []
            case_id_family_mismatch = (
                bool(case_id_families)
                and not (case_id_families & families)
            )
        else:
            case_id_families = set()
            case_id_family_overlap = []
            case_id_family_mismatch = False

        blocking_issues = []
        # fix #13：case_id 语义豁免。
        # 当 case_id 形如 case_006_empty / case_005_blank / case_NNN_single_line /
        # case_NNN_no_compress / case_NNN_noisy ... 时，empty/small 是该 case 的"目标态"
        # （专门构造来测试边界行为），不是缺陷。把这种"size-only blocking"消化掉，
        # 不让 deterministic 把它当 needs_fix。
        intentional_edge = is_intentional_edge_case(case_id)
        if not ok_len and not (intentional_edge and len_reason in {"empty", "too_small", "very_short"}):
            blocking_issues.append(len_reason)
        if not anchor_ok and not (intentional_edge and anchor_reason == "empty_first_line"):
            blocking_issues.append(anchor_reason)
        if mojibake:
            blocking_issues.append("mojibake")
        if secrets:
            blocking_issues.append("secrets")

        # fix #14：sidecar 字段规则校验。
        # 检查 expected_keep/expected_compress 里的关键词是否真的出现在 case 内容里。
        # 这是纯规则校验，不依赖 LLM，能捕获 fill_case_sidecars.py 的 LLM 幻觉。
        sidecar_fields = read_sidecar_scenario_fields(plugin, fname)
        sidecar_check = check_sidecar_field_consistency(sidecar_fields, content, case_id)

        deterministic_findings = {
            "ok_length": ok_len,
            "length_reason": len_reason,
            "anchor_ok": anchor_ok,
            "anchor_token": anchor_tok,
            "anchor_reason": anchor_reason,
            "mojibake": mojibake,
            "secrets": secrets,
            "signals": signals,
            "blocking_issues": blocking_issues,
            "routing_boundary_tool": routing_tool,
            "routing_boundary_ratio": round(routing_ratio, 2),
            "routing_boundary_segments": f"{routing_count}/{routing_total}",
            "title_consistent": title_consistent,
            "title_assertions_checked": title_assertions_checked,
            "title_assertions_satisfied": title_assertions_satisfied,
            "title_assertions_missing": title_assertions_missing,
            "case_id_families": sorted(case_id_families),
            "case_id_family_overlap": case_id_family_overlap,
            "case_id_family_mismatch": case_id_family_mismatch,
            "sidecar_keep_mismatch": sidecar_check["keep_mismatch"],
            "sidecar_compress_mismatch": sidecar_check["compress_mismatch"],
            "sidecar_scenario_empty": sidecar_check["scenario_empty"],
        }

        llm_judgment = None
        # fix #8：补充 LLM 调用条件注释。
        # 关键原则：lint-only 是默认行为，LLM 调用必须有显式 opt-in。
        # 三个调用面（任一为真 + 有 key 才会真正调）：
        #   ① has_llm：检测到 OPENAI_API_KEY（env 或 .env 加载），
        #     没 key 时无论其他标志如何都不调，避免无 key 时打日志/超时。
        #   ② args.llm_audit：用户显式传 --llm-audit，希望走 LLM 仲裁。
        #   ③ args.require_llm_audit：用户传 --require-llm-audit，
        #     无 key 时在 main() 入口已 SystemExit（fail-fast），到达此处
        #     必定 has_llm=True。
        # 反例：早期版本的 ``has_llm and (or True)`` 让有 key 就隐式调，违反
        # 默认 lint-only 的可控性要求；本次复审已删，确保 opt-in 必为用户主动声明。
        should_call_llm = has_llm and (args.llm_audit or args.require_llm_audit)
        if should_call_llm:
            # fix #12：按 plugin_type dispatch system prompt。
            system_prompt = build_llm_prompt(plugin_type, plugin=plugin, case_id=case_id)
            prompt = build_llm_user_prompt(plugin, case_id, content, deterministic_findings, plugin_type)
            raw = call_llm(env, prompt, system_prompt=system_prompt)
            llm_judgment = normalize_llm_judgment(raw, deterministic_findings)
        else:
            llm_judgment = normalize_llm_judgment(None, deterministic_findings)

        # 终极 status 合成：先看 LLM/Lint 给的初值，再考虑 missing 家族 / 路由让渡 / 重复
        status = llm_judgment["status"]
        if not registered and status == "valid":
            status = "not_registered"
        if routing_dominant and status == "valid":
            status = "routing_boundary_unclear"
        # 二段 dispatch 链升级：sidecar 有合法 expected_dispatch_chain 时，
        # routing_boundary_unclear → valid（chain 证明"shell 里执行外部命令"是
        # 预期行为，不是 case 放错目录）。
        dispatch_chain = read_sidecar_dispatch_chain(plugin, fname)
        if status == "routing_boundary_unclear" and dispatch_chain:
            status = "valid"
            llm_judgment["dispatch_chain_confirmed"] = dispatch_chain
        # 重复：若本 case 是某个 group 的非主成员
        is_dup = False
        for g in duplicates:
            if case_id in g["duplicates"]:
                is_dup = True
                llm_judgment["duplicate_of"] = g["primary"]
                break
        if is_dup and status == "valid":
            status = "duplicate"
        # fix #5：title_consistent=False（标题声明的语义在 content 中没兑现）升级为
        # 独立桶 title_mismatch，作为 hard-block。仅从 valid 降级，不覆盖 routing /
        # not_registered / duplicate / needs_fix 等更高优先级 status。
        if title_consistent is False and status == "valid":
            status = "title_mismatch"
        # fix #11：fabrication（LLM 真实性仲裁失败）独立桶。仅从 valid/needs_fix
        # 升级，不覆盖 routing_boundary_unclear / not_registered / duplicate /
        # title_mismatch（这些是结构/路由层问题，优先级更高）。
        # 这里兜底处理 normalize_llm_judgment 没覆盖的情况（理论上 normalize 已经
        # 把 fabrication_indicators 非空时强制 fabricated，但加一道防线）。
        if llm_judgment.get("realism_audit", {}).get("fabrication_indicators"):
            if status in ("valid", "needs_fix"):
                status = "fabricated"

        rec = {
            "case_id": case_id,
            "filename": fname,
            "size_bytes": size_bytes,
            "line_count": line_count,
            "content_hash": content_hash,
            "registered": registered,
            "registered_title": registered_title,
            "title_consistent": title_consistent,
            "command_tokens": tokens,
            "command_families": sorted(families),
            "routing_boundary_tool": routing_tool,
            "deterministic": deterministic_findings,
            "llm_judgment": llm_judgment,
            "final_status": status,
            "content": content,
            # fix #12：plugin 类型透传到 case_records，供 LLM prompt 构造 +
            # 报告渲染使用。
            "plugin_type": plugin_type,
            # fix: 增量审计 — 标记本次是否命中缓存（命中则 final_status 是复用的）
            "cache_hit": False,
        }
        case_records.append(rec)
        print(f"case={case_id} status={status} families={','.join(sorted(families))} "
              f"anchor={anchor_tok} routing={routing_tool or '-'} "
              f"llm={'Y' if llm_judgment['llm_invoked'] else 'N'}")

    # 3.5) 缓存命中统计
    total = cache_hits + cache_misses
    if total:
        rate = cache_hits / total
        print(f"[cache] hits={cache_hits} misses={cache_misses} hit_rate={rate:.1%}"
              f" (--no-cache 全量重跑; 改 lint/LLM 提示词后必须 --no-cache)")

    # 4) 推荐补 case（仅对 missing 家族）
    existing_ids = {r["case_id"] for r in case_records}
    recommendations = build_missing_case_recommendations(coverage["missing"], existing_ids)

    # 5) 写产物
    case_out_dir = os.path.join(out_dir, "cases")
    for r in case_records:
        export_case_quality(r, case_out_dir)

    # 5.5) L1 写回：把"刚跑过的有效结论"写进 case 同目录的 sidecar.audit
    # 块，下次跑直接命中 L1 sidecar，跳过 lint+LLM。复用与 L2 同条件：
    # final_status ∈ {valid, not_registered, duplicate, title_mismatch,
    # routing_boundary_unclear}，且本次确实跑了（cache_hit=False）。
    # 不写 needs_fix / fabricated，因为这些是"缺陷判定"，lint/提示词改了
    # 可能变 valid，下次重跑才能识别。
    _SIDECAR_WRITABLE_STATUS = {
        "valid", "not_registered", "duplicate",
        "title_mismatch", "routing_boundary_unclear",
    }
    tool_version = f"audit_sample_case_quality@incremental"
    now_iso = datetime.now().isoformat()
    writeback_ok = 0
    writeback_skip = 0
    writeback_err = 0
    for r in case_records:
        if r.get("cache_hit"):
            # 缓存复用：本来就是从 sidecar/latest.json 读出来的，不再写回
            writeback_skip += 1
            continue
        fs = r.get("final_status", "")
        if fs not in _SIDECAR_WRITABLE_STATUS:
            # 缺陷判定不缓存，避免 lint 规则改进后误标 valid
            writeback_skip += 1
            continue
        sc_path = _sidecar_path_for_case(plugin, r["filename"])
        if not sc_path:
            # 该 case 没有 sidecar 文件（用户没跑 generate_case_sidecars.py）
            # → 跳过写回，不强建（避免在 case 目录撒新文件）
            writeback_skip += 1
            continue
        try:
            ok = write_sidecar_audit_block(
                sc_path,
                {
                    "content_hash": r.get("content_hash", ""),
                    "final_status": fs,
                    "llm_invoked": bool(r.get("llm_judgment", {}).get("llm_invoked")),
                    "llm_verified_at": now_iso,
                    "last_audit_tool": tool_version,
                    "skip": False,
                },
            )
            if ok:
                writeback_ok += 1
            else:
                writeback_skip += 1
        except Exception as exc:
            print(f"WARN: sidecar write-back failed for {r['filename']}: {exc}",
                  file=sys.stderr)
            writeback_err += 1
    print(f"[sidecar-writeback] ok={writeback_ok} skip={writeback_skip} err={writeback_err}")

    payload = {
        "plugin": plugin,
        "version": version,
        "generated_at": datetime.now().isoformat(),
        "case_count": len(case_records),
        "registered_count": sum(1 for r in case_records if r["registered"]),
        "duplicate_groups": duplicates,
        "command_family_coverage": {
            "target_count": coverage["target_count"],
            "covered_count": coverage["covered_count"],
            "missing_count": coverage["missing_count"],
            "covered": coverage["covered"],
            "missing": coverage["missing"],
        },
        "incremental": {
            "enabled": not args.no_cache,
            "cache_hits": cache_hits,
            "cache_misses": cache_misses,
            "cache_hit_rate": round(cache_hits / max(1, cache_hits + cache_misses), 3),
            "l1_hits_sidecar": l1_hits,
            "l2_hits_latest_json": l2_hits,
            "indexed_prev_cases": len(cache_index),
            "disabled_reason": cache_disabled_reason,
        },
        "cases": [
            {
                "case_id": r["case_id"],
                "filename": r["filename"],
                "final_status": r["final_status"],
                "registered": r["registered"],
                "title_consistent": r["title_consistent"],
                "command_tokens": r["command_tokens"],
                "command_families": r["command_families"],
                "routing_boundary_tool": r["routing_boundary_tool"],
                "deterministic": r["deterministic"],
                "llm_judgment": r["llm_judgment"],
                "size_bytes": r["size_bytes"],
                "line_count": r["line_count"],
                "content_hash": r["content_hash"],
                # fix: 增量审计 — 在产物中透出 cache_hit，方便外部工具
                # 区分本次真实跑了 lint/LLM 还是复用了上一轮结论。
                "cache_hit": r.get("cache_hit", False),
            }
            for r in case_records
        ],
        "missing_case_recommendations": recommendations,
    }

    json_path = os.path.join(out_dir, "case_quality_report.json")
    atomic_write_json(json_path, payload)

    latest_path = os.path.join(out_dir, "case_quality_latest.json")
    import shutil
    shutil.copyfile(json_path, latest_path)

    coverage_path = os.path.join(out_dir, "command_family_coverage.json")
    atomic_write_json(coverage_path, coverage)

    md_path = os.path.join(out_dir, "case_quality_report.md")
    md = render_markdown_report(
        plugin, version, out_dir, case_records, coverage, duplicates, recommendations
    )
    with open(md_path, "w", encoding="utf-8") as f:
        f.write(md)

    # 6) 总结打印
    hard_block = [r for r in case_records if r["final_status"] != "valid"]
    print("")
    print(f"hard_block_count={len(hard_block)}")
    for r in sorted(hard_block, key=lambda x: x["case_id"]):
        print(f"hard_block_case={r['case_id']} status={r['final_status']}")
    print(f"json={json_path}")
    print(f"md={md_path}")
    print(f"latest={latest_path}")
    print(f"coverage={coverage_path}")
    print(f"cases_dir={case_out_dir}")

    # 退出码：有任何 hard_block → 1；其余 0
    if hard_block:
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()
