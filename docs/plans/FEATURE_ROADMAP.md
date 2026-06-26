# TokenSlim 功能补全开发计划 (Feature Completion Roadmap)

> 基于对 TokenSlim、RTK、TOKF 三者的实际源码交叉验证制定
> 制定日期: 2026-05-10 | 最后更新: 2026-06-26 | 版本: v1.3

## 当前状态（2026-06-25）

本路线图最初用于补齐 RTK/TOKF 功能差距。根据当前代码和审计结果，P1-P4 核心功能已完成：

- ✅ P1: Token 追踪、Gain 报告、过滤器变体、安全检查
- ✅ P2: 命令重写、静态规则增强、过滤器发现
- ✅ P3: 树结构重组、模板渲染
- ✅ P4: NDJSON 流式解析、Server Sidecar、辅助函数与 JSON 提取
- ❌ P4-1 Lua 逻辑逃生舱不实现，已判定为不符合当前静态插件和审计体系的架构方向

### 第三阶段：扩展全功能化 + Chrome 无感压缩（2026-06-25 完成）

- ✅ Chrome 扩展 v0.3.0: 全平台无感压缩（ChatGPT/Claude/Gemini/通义千问/文心一言），输入拦截 + server API 通信 + Popup 控制面板 + esbuild 构建
- ✅ VS Code 扩展 v0.3.0: 双向能力（compress/decompress），终端输出自动压缩
- ✅ JetBrains 插件 v0.3.0: DecompressAction + Builder 模式 TokenSlimClient
- ✅ Java SDK: Builder 模式、compressFile、batchCompress、增强 JSON 序列化
- ✅ Node.js SDK: TypeScript 类型定义、compressStream、batchCompress、自动重连
- ✅ Python SDK: compress_file、batch_compress、AsyncTokenSlimClient (aiohttp)
- ✅ 全局审计自动化: 57 个审计插件、`1000/1000` case 通过并冻结，`p2_samples_gap_20260515` 门禁通过，并自动生成 `docs/audit/audit_review_prompt.md`、`docs/audit/plugin_capability_index.json` 与 `docs/reports/plugin_capability_matrix.md`
- ✅ run 路由可解释: `tokenslim run --explain-route <cmd>` 输出 route group、intent、匹配来源和插件链
- ✅ 插件选择实战解释: `tokenslim explain-plugin --explain-command "<cmd>"` / `--input <log>` 输出 selected plugin、why、alternatives、fallback_decision、retry_plugin、route/detector evidence 与能力索引覆盖证据，并支持 `--explain-replay-out` 生成误判回放模板
- ✅ 高频插件深挖: `cloud_log_plugin` 作为云厂商日志剥壳层，覆盖 AWS/GCP/Azure/阿里云/OCI/腾讯云/华为云/Cloudflare 的 tail/table/csv/jsonl/plain wrapper，并将脱壳后的内容交给 web/java/python/node/db/syslog 等传统日志插件链
- ✅ web_log_plugin v3/P3: 47/47 case 通过并冻结，覆盖 Nginx/Apache/Ingress/Uvicorn/Envoy/Istio/CloudFront/IIS W3C/Cloudflare/AWS/GCP/Azure/OCI wrapper、原生 ALB access log、DICT_IP/DICT_UA、ROUTINE、DIAG、SCAN/BURST/ANOMALY/SLOW 语义
- ✅ ci_log_plugin P2: 44/44 case 通过并冻结，覆盖 GitHub Actions/GitLab CI/Jenkins/Azure Pipelines/CircleCI/Buildkite/local act/TeamCity/Travis CI/组织 banner provider shell
- ✅ artifact_summary_plugin: 12/12 case 通过并冻结，在通用 JSON/XML 前处理 SARIF 与 JUnit XML 构建产物摘要
- ✅ CI/CD P1 内层插件深挖: Kubernetes/Docker、CTest/CMake、Gradle、Node、pytest 均已扩充 CI 场景并冻结
- ✅ 插件能力索引: `docs/audit/plugin_capability_index.json` 与 `docs/reports/plugin_capability_matrix.md`
- ✅ 全量 lib 测试: `786 passed`

后续路线不再是补齐 P1-P4，也不再是 P0-P3 插件治理补洞；当前 P0 路由/能力索引、P1 cloud_log 剥壳、P2 db_log 诊断语义、P3 web_log 真实格式 case 均已闭环。下一轮应以插件选择能力实战化、交付基线维护和少量高价值真实样本为主。

可选增强项统一归档为 non-gating backlog，见：`docs/tasks/OPTIONAL_BACKLOG.md`。

### 第四阶段：CI Pipeline 建设 + 打包补全 + CLI/Server 能力扩展（2026-06-26 完成）

> 详细任务分解见：`.qoder/plans/CI_Pipeline_and_Packaging_56b9ce98.md`

**CI/CD 基础设施：**
- ✅ Task 1: 主 CI 流水线 `ci.yml`（Rust 核心 + lint-audit + Chrome 扩展 + MCP/VSCode 构建验证）
- ✅ Task 2: SDK 构建验证 `ci-sdks.yml`（Node.js/Python/Java SDK）
- ✅ Task 3: JetBrains 插件构建验证 `ci-jetbrains.yml`
- ✅ Task 4: npm 打包修复（补入 log_reorder + log_miner）
- ✅ Task 5: Cargo.toml 修复（log_miner 显式注册）
- ✅ Task 6: Python SDK PyPI 包结构（tokenslim-client）
- ✅ Task 7: 添加 PyPI SDK 发布 job

**CLI 能力扩展：**
- ✅ Task 8: CLI 流式压缩（`--stream` 管道模式）
- ✅ Task 10: CLI `serve-static` 静态文件服务命令
- ✅ Task 11: CLI `run` 通用命令行工具包装器
- 🚫 Task 15: CLI 命令扩展机制（评估后判定为 WON'T DO，不符合当前架构方向）
- ✅ Task 17: CLI 配置系统（`tokenslim config`）
- ✅ Task 18: 插件配置管理（enable/disable + 参数配置）
- 🚫 Task 19: CLI AI 辅助命令（`tokenslim ask`）（评估后判定为 WON'T DO）

**Docker 与分发：**
- ✅ Task 9: Docker 官方镜像（多阶段构建 + ghcr.io 发布 + 多架构）

**Server 能力扩展：**
- ✅ Task 12: Server 请求防护（Rate Limit + Body Size）
- ✅ Task 13: Server JWT 鉴权（static/jwt/none 三模式 + /auth/token + /auth/refresh）
- ✅ Task 14: Server WebSocket 双向压缩通道（/ws/compress，Binary/Text 帧 + 心跳 + 并发限制）

**技术债务清理：**
- ✅ Task 16: Embedding 智能路由（方案 B: 彻底移除，清理全部 ML 依赖和 cfg 宏）

**文档与记录：**
- ✅ Task 20: 更新 FEATURE_ROADMAP.md 记录（本任务，已完成第四阶段条目添加；剩余：随 Task 1-19 实施同步更新状态）

---

## 一、背景与目标

### 1.1 当前状态

TokenSlim 已完成 P1-P4 核心功能。以下原始分阶段内容保留为历史计划和验收依据：
- ✅ 高性能压缩管道（rayon 并行 + Bump 内存池，400 MB/s）
- ✅ 30+ 专用插件（14 种 VCS + 构建工具 + 结构化数据）
- ✅ 编码互操作治理（encoding + 回退链）
- ✅ 工作空间诊断（workspace/rule）
- ✅ Shell Hooks（bash/zsh/fish/powershell/cmd）
- ✅ SQLite 持久化 Token 追踪
- ✅ RTK/TOKF 核心差距已收敛

### 1.2 对标目标

补齐与 RTK/TOKF 的功能差距，同时保持 TokenSlim 在高性能压缩和编码治理方面的独特优势。

---

## 二、分阶段开发计划

### P1：核心体验闭环（2 周内）

#### P1-1: SQLite Token 追踪系统

- **模块**: `src/core/tracking/` (新建)
- **参考**: RTK `other/rtk/src/core/tracking.rs` (约 1400 行), TOKF `other/tokf/crates/tokf-common/src/tracking/`
- **工作量**: 3-5 天
- **任务分解**:
  1. 添加 `rusqlite` 依赖到 `Cargo.toml`
  2. 创建 `src/core/tracking/mod.rs` — 模块入口
  3. 创建 `src/core/tracking/schema.rs` — 建表 DDL
     ```sql
     CREATE TABLE IF NOT EXISTS commands (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       timestamp TEXT NOT NULL,
       original_cmd TEXT NOT NULL,
       filter_name TEXT,
       input_bytes INTEGER NOT NULL,
       output_bytes INTEGER NOT NULL,
       input_tokens INTEGER NOT NULL,
       output_tokens INTEGER NOT NULL,
       exec_time_ms INTEGER NOT NULL DEFAULT 0,
       exit_code INTEGER NOT NULL DEFAULT 0,
       project TEXT NOT NULL DEFAULT ''
     );
     CREATE INDEX IF NOT EXISTS idx_timestamp ON commands(timestamp);
     CREATE INDEX IF NOT EXISTS idx_project ON commands(project);
     ```
  4. 创建 `src/core/tracking/tracker.rs` — `Tracker` 结构体
     - `Tracker::new(db_path) → Result<Self>`
     - `Tracker::record(cmd, filter, input_bytes, output_bytes, input_tokens, output_tokens, exec_time_ms, exit_code, project)`
     - `Tracker::get_summary() → GainSummary`
     - `Tracker::get_daily(days) → Vec<DailyGain>`
     - `Tracker::get_by_filter() → Vec<FilterGain>`
     - `Tracker::cleanup_older_than(days)` — 90 天自动清理
  5. 创建 `src/core/tracking/types.rs` — 数据类型
     - `GainSummary { total_commands, total_input_tokens, total_output_tokens, tokens_saved, savings_pct, ... }`
     - `DailyGain { date, commands, input_tokens, output_tokens, tokens_saved, savings_pct }`
     - `FilterGain { filter_name, commands, input_tokens, output_tokens, tokens_saved, savings_pct }`
  6. 迁移：保留 `stats/mod.rs` 的 JSON 读取兼容，后台线程写入 SQLite
  7. 单元测试：内存数据库测试 (rusqlite `Connection::open_in_memory()`)
- **验收标准**:
  - `tokenslim run git status` 后可在 SQLite 中查询到记录
  - `tokenslim gain` 可显示累计统计
  - 90 天前记录自动清理

#### P1-2: `tokenslim gain` 多维报告

- **模块**: `src/core/tracking/gain.rs` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-cli/src/gain.rs` + `gain_render/`
- **工作量**: 2-3 天
- **任务分解**:
  1. 实现 `tokenslim gain` (默认 summary)
  2. 实现 `tokenslim gain --daily` (按日统计)
  3. 实现 `tokenslim gain --by-filter` (按过滤器统计)
  4. 实现 `tokenslim gain --json` (JSON 输出)
  5. 实现 TTY 彩色渲染 (参考 TOKF `gain_render`)
     - `render_summary_tty()` — 带颜色的终端输出
     - `render_summary_plain()` — 纯文本输出
  6. 辅助函数：
     - `format_num(n: i64) → String` — 千分位分隔
     - `format_tokens(n: i64) → String` — K/M 后缀
     - `format_bytes(n: u64) → String` — B/KB/MB/GB
- **验收标准**:
  - `tokenslim gain` 显示累计统计
  - `tokenslim gain --daily` 显示按日明细
  - `tokenslim gain --by-filter` 显示按过滤器明细
  - `tokenslim gain --json` 输出有效 JSON

#### P1-3: 过滤器变体系统

- **模块**: `src/core/filter_variants/` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-cli/src/discover/`, `tokf-common/src/config/types.rs` (Variant)
- **工作量**: 3-5 天
- **任务分解**:
  1. 创建 `src/core/filter_variants/mod.rs` — 模块入口
  2. 创建 `src/core/filter_variants/types.rs` — 数据类型
     ```rust
     pub struct VariantConfig {
         pub name: String,
         pub detect: VariantDetect,
         pub filter: String,
     }
     pub enum VariantDetect {
         File { exists: String },
         ArgsPattern { pattern: String },
         OutputPattern { pattern: String },
     }
     ```
  3. 创建 `src/core/filter_variants/detector.rs` — 三层检测
     - `detect_file(cwd, pattern)` — 检查配置文件存在性
     - `detect_args(args, pattern)` — 正则匹配命令行参数
     - `detect_output(output, pattern)` — 正则匹配命令输出
  4. 创建 `src/core/filter_variants/router.rs` — 变体路由
     - `resolve_variant(config, cwd, args, output) → Option<String>`
  5. 集成到 `plugin_dispatcher` — 插件分发前先检查变体
  6. 为 `npm test` → jest/vitest/mocha 添加示例变体配置
- **验收标准**:
  - `npm test` 在存在 `vitest.config.ts` 时自动路由到 vitest 过滤器
  - 无匹配变体时回退到默认过滤器
  - TOML 配置中 `[[variant]]` 块可正确解析

#### P1-4: 安全检查模块

- **模块**: `src/core/safety_check/` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-common/src/safety/mod.rs` + `checks.rs`
- **工作量**: 2-3 天
- **任务分解**:
  1. 创建 `src/core/safety_check/mod.rs` — 模块入口 + `SafetyCheck` trait
     ```rust
     pub trait SafetyCheck: Send + Sync {
         fn name(&self) -> &'static str;
         fn check_config(&self, config: &RuleSection) -> Vec<SafetyWarning>;
         fn check_output(&self, raw: &str, filtered: &str) -> Vec<SafetyWarning>;
     }
     ```
  2. 创建 `src/core/safety_check/prompt_injection.rs`
     - 检测 "ignore previous instructions"、"disregard"、"you are now" 等注入模式
     - 检测模板中的注入特征
  3. 创建 `src/core/safety_check/shell_injection.rs`
     - 检测 `|`、`>`、`<`、`;`、`&&`、`||`、反引号等 shell 元字符
     - 检测 `$(...)` 命令替换
  4. 创建 `src/core/safety_check/hidden_unicode.rs`
     - 检测零宽空格 `\u{200B}`、`\u{200C}`、`\u{200D}`、`\u{FEFF}`
     - 检测 RTL 覆盖 `\u{202E}`
     - 检测其他不可见 Unicode 控制字符
  5. 注册表模式：`ALL_CHECKS: &[&dyn SafetyCheck]`
  6. CLI 集成：`tokenslim verify --safety`
- **验收标准**:
  - 包含注入模式的配置被检测并报告
  - Shell 元字符在 `run` 字段中被检测
  - 隐藏 Unicode 字符被识别
  - `--safety` 标志可在 CLI 中使用

---

### P2：过滤器管理体验（1 个月内）

#### P2-1: 命令重写引擎

- **模块**: `src/core/rewrite/` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-cli/src/rewrite/` (约 2000 行)
- **工作量**: 5-7 天
- **任务分解**:
  1. 创建 `src/core/rewrite/mod.rs` — 模块入口
  2. 创建 `src/core/rewrite/bash_ast.rs` — bash AST 拆分
     - `split_compound(cmd: &str) → Vec<(String, String)>` — 拆分 `&&` / `||` / `;` 复合命令
     - `strip_env_prefix(cmd: &str) → Option<(String, String)>` — 剥离 `KEY=value` 前缀
  3. 创建 `src/core/rewrite/rules.rs` — 规则应用
     - `apply_rules(rules: &[RewriteRule], command: &str) → String`
     - 内置包装器：`make SHELL=tokenslim`、`just --shell tokenslim`
  4. 创建 `src/core/rewrite/user_config.rs` — 用户配置
     - 加载 `rewrites.toml` 中的 `[[rewrite]]` 规则
     - 支持 `skip.patterns` 跳过列表
  5. 创建 `src/core/rewrite/transparent.rs` — 透明命令
     - ssh/mysql/psql 等不透明命令不参与重写
  6. CLI 集成：`tokenslim rewrite <command>`
- **验收标准**:
  - `make test` 重写为 `make SHELL=tokenslim test`
  - 用户 `rewrites.toml` 规则生效
  - 复合命令 (`cmd1 && cmd2`) 各段独立重写
  - ssh 等透明命令不被重写

#### P2-2: 静态规则插件增强

- **模块**: `src/plugins/static_rule_plugin/` (修改)
- **参考**: TOKF `other/tokf/crates/tokf-filter/src/filter/section.rs`
- **工作量**: 2-3 天
- **任务分解**:
  1. 在 `RuleSection` 中新增字段：
     - `exit: Option<String>` — 退出状态的正则模式
     - `match_pattern: Option<String>` — 收集时的行过滤
     - `split_on: Option<String>` — 将收集行按分隔符切割为 blocks
  2. 修改状态机逻辑 (`methods.rs`)：
     - 从 `IDLE → ACTIVE` (单状态) 改为 `IDLE ⇄ ACTIVE` (双状态切换)
     - ACTIVE 状态下匹配到 `exit` 时停止收集（不收集 exit 行本身）
     - ACTIVE 状态下通过 `match_pattern` 过滤收集行
  3. 实现 `split_into_blocks()` 函数
  4. 更新 TOML 解析 (`types.rs`)：
     ```toml
     [[sections]]
     name = "test_failures"
     enter = "^FAILURES$"
     exit = "^$"
     match = "^  (test_|FAIL:)"
     split_on = "^---$"
     ```
- **验收标准**:
  - exit 模式正确停止收集
  - match 过滤只收集匹配行
  - split_on 正确分块
  - 向后兼容：不设置 exit 时行为不变

#### P2-3: 过滤器发现

- **模块**: `src/core/filter_discover/` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-cli/src/discover/mod.rs` (约 400 行)
- **工作量**: 3-5 天
- **任务分解**:
  1. 创建 `src/core/filter_discover/mod.rs` — 模块入口
  2. 创建 `src/core/filter_discover/parser.rs` — Session 文件解析
     - 解析 DeepSeek/Claude session JSON 文件
     - 提取所有命令执行记录
  3. 创建 `src/core/filter_discover/classifier.rs` — 命令分类
     - `AlreadyFiltered` — 已被 tokenslim 包装
     - `Filterable` — 存在匹配的过滤器
     - `NoFilter` — 无匹配过滤器
  4. 创建 `src/core/filter_discover/aggregator.rs` — 聚合与估算
     - 按命令分组键聚合 (如 `git status`, `cargo test`)
     - 从 tracking.db 加载历史 savings_pct
     - 估算缺失过滤器的潜在节省
  5. CLI 集成：`tokenslim discover [session_files...]`
- **验收标准**:
  - 扫描 session 文件后输出分类结果
  - 估算的潜在节省合理
  - 缺失过滤器的命令组被列出

---

### P3：压缩能力增强（2 个月内）

#### P3-1: 树结构重组

- **模块**: `src/core/tree_restructure/` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-filter/src/filter/tree.rs` (约 350 行), `tokf-common/src/config/tree.rs`
- **工作量**: 3-5 天
- **任务分解**:
  1. 创建 `src/core/tree_restructure/mod.rs` — 模块入口
  2. 实现 `TreeConfig` 配置结构体
  3. 实现 Trie 构建算法：
     - `parse_line(re, line) → Option<MatchedEntry>` — 解析每一行
     - `insert_path(root, components, decoration, tail)` — 构建 Trie
     - `shared_depth(matched) → usize` — 计算共享深度
  4. 实现折叠优化：
     - `collapse_single_child(node)` — 单孩子目录折叠
     - `sort_node(node)` — 字母排序
  5. 实现渲染引擎：
     - Unicode 框线风格 (`├─ │  └─`)
     - ASCII 风格 (`|- |  `- `)
     - 纯缩进风格
  6. 实现门控逻辑：
     - `min_files` (默认 4) — 最少匹配行数
     - `min_shared_depth` (默认 1) — 最少共享深度
  7. 集成到 `git status` 和 `git diff --name-only` 的 VCS 插件
- **验收标准**:
  - `git status` 文件列表输出为目录树结构
  - 单孩子目录正确折叠
  - 三种渲染风格可切换
  - 门控逻辑正确（不满足条件时返回原始输出）

#### P3-2: 模板渲染系统

- **模块**: `src/core/template_render/` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-filter/src/filter/template/`
- **工作量**: 2-3 天
- **任务分解**:
  1. 创建 `src/core/template_render/mod.rs` — 模块入口
  2. 实现变量替换引擎：
     - `{{var}}` — 简单变量
     - `{{section_name}}` — section 内容
     - `{{section_name.count}}` — section 行数/block 数
     - `{{section_name.items}}` — section 条目列表
  3. 支持条件渲染：
     - `{{#if var}}...{{/if}}` — 变量非空时渲染
     - `{{#unless var}}...{{/unless}}` — 变量为空时渲染
  4. 与 section/aggregate 变量集成
- **验收标准**:
  - `{{output}}` 变量正确替换
  - `{{section.count}}` 正确显示
  - 条件块正确渲染
  - 模板语法错误时有明确报错

---

### P4：生态扩展（后续评估）

#### P4-1: Lua 逻辑逃生舱

- **模块**: `src/core/lua_engine/` (新建)
- **参考**: TOKF `other/tokf/crates/tokf-filter/src/filter/lua.rs` (约 250 行)
- **工作量**: 3-5 天（含安全审计）
- **前置评估**:
  - 用户需求调查：是否有无法用正则表达的逻辑需求
  - 安全审计：mlua Luau 沙箱的隔离性
  - 维护成本：额外的依赖和测试负担
- **任务分解**:
  1. 添加 `mlua` (features=["luau"]) 依赖
  2. 实现沙箱执行：指令限制 1M，内存限制 16MB
  3. 实现中断处理器：每 ~1000 条指令检查
  4. 全局变量注入：`output`、`exit_code`、`args`
  5. 安全测试：os/io/package 禁用验证
  6. 无限循环/内存炸弹防御测试
- **验收标准**:
  - Lua 脚本可访问 output/exit_code/args
  - os.execute/io.read 被沙箱阻止
  - 无限循环在指令限制内终止
  - 存在明确的用户需求支撑

#### P4-2: NDJSON 流式解析 ✅ 完成

- **模块**: `src/plugins/ndjson_plugin/` (已实现)
- **参考**: RTK `other/rtk/src/parser/`
- **工作量**: 2-3 天（已完成）
- **实现内容**:
  1. ✅ 逐行 JSON 解析
  2. ✅ 交错包事件处理（多包测试输出混合）
  3. ✅ 聚合结果：`"N packages, M failures (pkg::TestName, ...)"`
  4. ✅ 9 个测试全部通过
  5. ✅ 80-90% 压缩率
- **验收标准**: ✅ `go test -json` 输出正确聚合
- **文档**: `P4-2_NDJSON_PLUGIN_IMPLEMENTATION.md`, `P4-2_COMPLETION_SUMMARY.md`

#### P4-3: Server 模式 ✅ 完成

- **模块**: `src/bin/tokenslim-server.rs` (已实现)
- **参考**: TOKF `other/tokf/crates/tokf-server/`
- **工作量**: 已完成
- **实现内容**:
  1. ✅ axum REST API (9 个端点): `GET /health`, `GET /metrics`, `GET /metrics/detail`, `GET /stats/aggregate`, `GET /stats/daily`, `GET /stats/by-filter`, `POST /compress`, `POST /decompress`, `POST /reload`
  2. ✅ Bearer Token 认证 (`TOKENSLIM_API_KEY`)
  3. ✅ CORS 支持
  4. ✅ 过滤器热加载（notify watcher）
  5. ✅ 远程统计聚合（基于 Tracker）
  6. ✅ Prometheus 格式 Metrics
  7. ✅ AI Export 模式
  8. ✅ 日志重排支持
- **验收标准**: ✅ Sidecar 模式可正常服务
- **文档**: `P4-3_SERVER_ENHANCEMENT_SUMMARY.md`

#### P4-4: 辅助函数补齐 + JSON 提取 ✅ 完成

- **模块**: `src/core/json_extractor/` + `src/core/tracking/gain.rs` (已实现)
- **工作量**: 已完成
- **实现内容**:
  1. ✅ `format_tokens(n)` — K/M 后缀
  2. ✅ `format_bytes(n)` — B/KB/MB/GB
  3. ✅ `format_num(n)` — 千分位分隔
  4. ✅ `extract_json_object(text)` — 括号平衡提取
  5. ✅ `extract_all_json_objects(text)` — 批量提取
  6. ✅ 16 个单元测试全部通过
- **验收标准**: ✅ 所有辅助函数有单元测试，JSON 提取正确

---

## 三、工作量汇总

| 阶段     | 功能              | 工作量       | 累计     |
| -------- | ----------------- | ------------ | -------- |
| P1-1     | SQLite Token 追踪 | 3-5 天       | 3-5 天   |
| P1-2     | Gain 多维报告     | 2-3 天       | 5-8 天   |
| P1-3     | 过滤器变体系统    | 3-5 天       | 8-13 天  |
| P1-4     | 安全检查模块      | 2-3 天       | 10-16 天 |
| P2-1     | 命令重写引擎      | 5-7 天       | 15-23 天 |
| P2-2     | 静态规则增强      | 2-3 天       | 17-26 天 |
| P2-3     | 过滤器发现        | 3-5 天       | 20-31 天 |
| P3-1     | 树结构重组        | 3-5 天       | 23-36 天 |
| P3-2     | 模板渲染系统      | 2-3 天       | 25-39 天 |
| P4-1     | Lua 引擎          | 3-5 天       | 28-44 天 |
| P4-2     | NDJSON 解析       | 2-3 天       | 30-47 天 |
| P4-3     | Server 模式       | 10-15 天     | 40-62 天 |
| P4-4     | 辅助函数 + JSON   | 2-3 天       | 42-65 天 |
| **总计** |                   | **42-65 天** |          |

> 注：P4 阶段功能取决于前置评估结果，可能裁剪或推迟。

---

## 四、风险与依赖

| 风险                | 影响      | 缓解措施                                                 |
| ------------------- | --------- | -------------------------------------------------------- |
| rusqlite 交叉编译   | P1-1/P1-2 | 使用 bundled feature 编译 SQLite                         |
| Lua 安全沙箱绕过    | P4-1      | 参考 TOKF 已有测试用例，执行负向安全测试                 |
| bash AST 解析复杂度 | P2-1      | 仅实现 split_compound + strip_env_prefix，不追求完整 AST |
| Server 模式维护成本 | P4-3      | 作为独立可选 crate，不影响核心 CLI                       |
| 现有 API 兼容性     | 全部      | 新增功能作为可选模块，不影响现有 CLI 行为                |

---

## 五、验收总清单

- [x] `tokenslim gain` 可显示 SQLite 驱动的多维统计
- [x] `tokenslim gain --daily --json` 输出正确
- [x] 过滤器变体自动路由 `npm test` → vitest/jest/mocha
- [x] `tokenslim verify --safety` 检测注入模式
- [x] `tokenslim rewrite "make test"` 输出 `make SHELL=tokenslim test`
- [x] 静态规则支持 exit/match/split_on
- [x] `tokenslim discover` 扫描 session 并输出缺失过滤器
- [x] `git status` 输出目录树结构
- [x] `{{section.count}}` 模板变量正确渲染
- [x] Lua 沙箱不实现决策已落地（架构反模式，不进入验收范围）
- [x] NDJSON 流正确聚合
- [x] Server 模式 `/compress` 端点可用
