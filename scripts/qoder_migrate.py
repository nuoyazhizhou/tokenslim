"""从 .qoder/repowiki 挑选优质 .md 迁移到 docs/, 加 AI 生成标注。

候选 (11 个)：
  1. en/content/Project Overview/Architecture Overview.md  -> docs/development/architecture_overview.md
  2. en/content/Web UI Interface/Interface Overview and Features.md -> docs/development/webui_overview.md
  3. en/content/Deployment and Operations.md -> docs/development/deployment_operations.md
  4. en/content/Getting Started.md -> docs/development/getting_started_analysis.md
  5. en/content/Performance and Optimization.md -> docs/development/performance_optimization.md
  6. knowledge/zh/TokenSlim 多平台构建与发布体系/build_system.md -> docs/development/build_release_system.md
  7. knowledge/zh/TokenSlim 日志与可观测性系统/logging_system.md -> docs/development/logging_system.md
  8. knowledge/en/TokenSlim Configuration System/configuration_system.md -> docs/development/configuration_system.md
  9. knowledge/en/Error Handling and Isolation Architecture/error_handling.md -> docs/development/error_handling.md
 10. knowledge/en/Multi-Ecosystem Dependency Management Strategy/dependency_management.md -> docs/development/dependency_management.md
 11. en/content/Plugin System/Built-in Plugin Catalog.md -> docs/design/builtin_plugin_catalog.md
"""
from __future__ import annotations
import os
import shutil
from pathlib import Path

ROOT = Path(r"C:\git_work\TokenSlim")
SRC_BASE = ROOT / ".qoder" / "repowiki"
DST_BASE = ROOT / "docs"

GENERATED_DATE = "2026-06-23"
GENERATOR = "Qoder (阿里云 AI IDE)"

# (源相对路径, 目标相对路径, 语言, 用途摘要)
CANDIDATES = [
    # 顶层英文 content (覆盖广)
    ("en/content/Project Overview/Architecture Overview.md",
     "development/architecture_overview.md",
     "en", "整体架构 (含 mermaid 图, 跨模块引用)"),
    ("en/content/Web UI Interface/Interface Overview and Features.md",
     "development/webui_overview.md",
     "en", "WebUI SPA 架构与功能"),
    ("en/content/Deployment and Operations.md",
     "development/deployment_operations.md",
     "en", "Sidecar 部署 + WebUI 嵌入 + 灰度回滚"),
    ("en/content/Getting Started.md",
     "development/getting_started_analysis.md",
     "en", "三入口快速上手 (CLI/Server/SDK)"),
    ("en/content/Performance and Optimization.md",
     "development/performance_optimization.md",
     "en", "性能优化路径 (并行/字典/缓存)"),
    ("en/content/Plugin System/Built-in Plugin Catalog.md",
     "design/builtin_plugin_catalog.md",
     "en", "60+ 内置插件族目录"),
    # 知识模块 (zh/en 平衡)
    ("knowledge/zh/TokenSlim 多平台构建与发布体系/build_system.md",
     "development/build_release_system.md",
     "zh", "Cargo + Maturin + Gradle + npm 多生态构建/CI/发布"),
    ("knowledge/zh/TokenSlim 日志与可观测性系统/logging_system.md",
     "development/logging_system.md",
     "zh", "env_logger + tracing 混合日志架构"),
    ("knowledge/en/TokenSlim Configuration System/configuration_system.md",
     "development/configuration_system.md",
     "en", "5 层配置 (TOML/JSON/env) 加载顺序"),
    ("knowledge/en/Error Handling and Isolation Architecture/error_handling.md",
     "development/error_handling.md",
     "en", "thiserror + SafeExecutor + MetricsCollector"),
    ("knowledge/en/Multi-Ecosystem Dependency Management Strategy/dependency_management.md",
     "development/dependency_management.md",
     "en", "Rust/npm/Gradle/Python/Java/Node 多生态依赖"),
]

HEADER_TEMPLATE = """<!--
本文件由 AI 工具自动迁移自 .qoder/repowiki/, 用于补充 docs/ 的视角。
- 生成器: {generator}
- 生成日期: {generated_date}
- 原文件: .qoder/repowiki/{src_rel}
- 适用版本: TokenSlim v0.3.5 (扫描基线: C:\\git_work\\TokenSlim-publish2 当时快照)
- 维护说明: 内容已与项目 docs/ 现有文件去重; 若代码变更需人工校对。
-->

"""

FOOTER_TEMPLATE = """
---

<!--
来源: {src_rel}  |  生成器: {generator}  |  扫描基线: publish2 @ {generated_date}
-->
"""


def main() -> int:
    DST_BASE.mkdir(parents=True, exist_ok=True)
    migrated = 0
    skipped = []

    for src_rel, dst_rel, lang, summary in CANDIDATES:
        src = SRC_BASE / src_rel
        dst = DST_BASE / dst_rel

        if not src.exists():
            skipped.append(f"missing source: {src_rel}")
            continue

        if dst.exists():
            skipped.append(f"target exists, skip: {dst_rel}")
            continue

        content = src.read_text(encoding="utf-8", errors="replace")
        header = HEADER_TEMPLATE.format(
            generator=GENERATOR,
            generated_date=GENERATED_DATE,
            src_rel=src_rel,
        )
        footer = FOOTER_TEMPLATE.format(
            src_rel=src_rel,
            generator=GENERATOR,
            generated_date=GENERATED_DATE,
        )
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_text(header + content + footer, encoding="utf-8")
        migrated += 1
        print(f"  OK  {src_rel}  ->  {dst_rel}  ({lang}, {summary})")

    print(f"\nMigrated: {migrated}  Skipped: {len(skipped)}")
    for s in skipped:
        print(f"  SKIP  {s}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
