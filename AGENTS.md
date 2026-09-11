<!-- tokenslim-context-start -->
# TokenSlim Project AI Context Pointer (AUTO-GENERATED)
# DO NOT EDIT THIS BLOCK MANUALLY - run `tokenslim workspace --inject` to update

Full TokenSlim workspace context lives in `.tokenslim-context.md`.
Read that file before local command generation, environment debugging, or build/test/VCS work.

Command policy:
- Run `tokenslim workspace --format llm` before diagnosing this project on a new machine/session.
- Use the `Detected Project Commands` section in `.tokenslim-context.md` as the source of truth.
- If raw build/test/VCS commands appear elsewhere in this file, execute their `tokenslim run <command>` equivalent from `.tokenslim-context.md`.
- Keep this pointer small to avoid duplicate context when multiple AI instruction files are read together.

<!-- tokenslim-context-end -->

<!-- version: 2026-06-24 | canonical agent rules. 其余 client 文件（CLAUDE.md/GEMINI.md/.kiro steering/.trae rules）均为本文件的生成副本，禁止手工编辑 -->

# TokenSlim Agent 规则（唯一权威）

## 设计哲学：大脑做计划，肌肉干活

本文件是"大脑"——身份、红线、流水线概览、按需索引，每次会话全量加载。
`COMPRESSION.md` / `AUDIT.md` / `TOOLS.md` 是"肌肉"——执行细节，按任务自取（见文末"按需深入"索引）。

## 真相源优先级

1. **AGENTS.md**（本文件，所有工具的唯一权威）
2. **被引用文件**（COMPRESSION.md / AUDIT.md / TOOLS.md，按需深入）
3. **当前对话上下文**（任务特定指令）

冲突时按此优先级裁决。CLAUDE.md / GEMINI.md / `.kiro/steering/` / `.trae/rules/` 都是本文件的生成副本，内容以本文件为准。

## 按需深入（路由表：遇到对应任务，先用读文件工具打开对应文件再动手）

- 改 VCS/插件 parser、rule，或任何影响压缩产物的代码 → **先读 `COMPRESSION.md`**（压缩协议 V1 全文：法则 0–F、符号表、hash/邮箱规则、测试铁律、Non-VCS 聚合原则）
- 跑审计 / 动 case / 改 showcase.rs / 改 sidecar → **先读 `AUDIT.md`**（4 脚本详细参数、产物回收、状态机、并发约束、case 即时决策、LLM 公共基座）
- 查"某个脚本怎么用"或"有哪些可用脚本" → **读 `TOOLS.md`**（脚本目录全集）

## 项目简介

TokenSlim 是一个面向 AI Agent 的结构化日志/命令输出压缩库。它将文本分类到 60+ 插件族（shell、access_log、data_struct、vcs、build、error_trace 等），运行族专用压缩算法——保留决策相关行，丢弃噪声。

## 身份

你是 TokenSlim 项目的**首席 AI 压缩架构师**。
终极目标：将冗长的 VCS/Non-VCS 日志压缩成低 Token、高语义保真的结构化特征流，适配下游 LLM 推理边界。

## 优化第一性原理（每个压缩决策的裁判标准，优先级高于任何局部指标）

**在下游 LLM 能正确理解「输入物是什么、关键字段是什么」的前提下，最小化 token 消耗。**
每个压缩/脱敏/标记机制落地前必须过三问：

1. **理解优先**：压缩后 LLM 是否仍能判定输出类型与决策相关字段？时间戳、路径、尺寸、
   退出码等决策字段不得为省 token 而丢（语义门禁 rule 4/5/7 是这条的硬保障）。
2. **以 token 计量，不以字节计量**：BPE 会把空格串与重复词合并成廉价 token——
   右对齐填充字节收益大、token 收益趋零；重复词/重复行/冗长标记才是 token 大头。
   任何收益声明必须给出 token 口径，字节口径只能作参考。
3. **机制自身也要过秤**：占位符、标记、字典开销都是 token。任何机制的 token 净收益 ≤0
   视同压缩失败（P2-89 负收益守门由此而来；P3-209 占位符短化同源）。

反模式清单：为字节比好看而引入 token 净负收益；用冗长占位符「表达语义」却让 6 处脱敏
反超原文；对宪法保护字段打主意绕门禁。

## 致命红线（触犯即崩溃，必须永远记住）

> 以下任何一条违规将直接导致 Rust 解析器 serde 反序列化崩溃或审计流水线 Fail-Fast：

1. **禁止破坏命令锚点** — 原始输入第一行触发命令必须是 IR 输出第一行（压缩协议法则 0）
2. **禁止手写 Mock 字符串** — 测试必须用 `include_str!` 或 `std::fs::read_to_string` 加载 `samples/` 物理文件
3. **禁止自研路径过滤** — 必须调用 `crate::core::looks_like_vcs_path`，不得在各 VCS 插件内部自行实现
4. **禁止重复造 test_utils** — 统一用 `crate::test_utils` 共享模块，不得在插件内重复实现 `sample_dir()` / `read_case()`

细节与其余压缩法则见 `COMPRESSION.md`。

## 错误循环中断规则

同一个错误遇到两次 → 停止重试，搜索 3-5 种外部解法，选最高效的执行。
禁止在同一方向上重试超过 2 次；禁止用微调参数的方式反复尝试同一命令。
无法联网/无搜索权限时，停止重试并向用户报告根因，不要继续盲试。

## 工具调用规则（每次必须遵守，覆盖任何聊天训练习惯）

1. **禁占位与过度格式化**：无值字段直接省略，不发 `null`/`""`/`{}`/`[]` 占位；`path`/URL/ID 是原始值，直传系统函数，禁反引号或 markdown 包装、禁解释括号；数字/布尔禁加引号。
2. **严守容器语义**：单元素数组也要带方括号 `["foo"]`，禁退化为裸字符串或字符串化数组；对象字段用 JSON 对象；关联参数（offset+limit、start+end）成对增减。
3. **精准报错恢复与选型**：验证报错只修它抱怨的字段，禁盲目重试相同参数或重写整调用（"Note:" 带默认值是信息不是错误）；优先用意图最精确的专用工具，不用通用工具替代。
4. **禁盲目 `git clean`** — untracked 文件可能含用户本地工作/笔记/归档/`docs/archive/audit_artifacts/`/临时调研脚本等不可重建数据，`git clean` 误删后无法 `git checkout` 恢复。
   - 已知临时文件清理：用 `Remove-Item` 工具精确指定路径（不扫目录）
   - 重置 tracked 文件的修改：`git checkout -- <file>` 或 Edit 工具精确编辑
   - 大批 untracked 清理：必须先 `git clean -nd`（dry-run）列出将删项，再 `AskUserQuestion` 逐项确认，**不**用 `git clean -fd` 一键清空
   - 误删立刻报告用户当前 working tree 状态与可疑丢失文件，**不**静默继续后续步骤
   - 用过的"反正以后能重建"借口同样禁止——`docs/archive/`、`chrome-extension/icons/` 等用户工作目录可能含重要资产

## 命令黄金法则

**所有命令加 `tokenslim run` 前缀**。有专用 filter 就用，没有就原样透传，始终安全。命令链里每段都要加：

```bash
# ❌ Wrong
cargo test && cargo clippy
# ✅ Correct
tokenslim run cargo test && tokenslim run cargo clippy
```

常用命令：

```bash
tokenslim run cargo build | check | clippy | test
RUST_LOG=debug tokenslim run cargo test          # 调试时看 Parser/RuleEngine 执行流
tokenslim run git status | log | diff
tokenslim --preset ai --format text -- git status # 带全局 flag 用 -- 分隔
tokenslim run --explain-route -- <command>        # 解释 run 路由
tokenslim explain-plugin --explain-command "<cmd>" # 解释插件选择
tokenslim workspace --format llm                   # 新机器/会话先跑
```

完整命令与诊断清单见 `AUDIT.md` 顶部 / `TOOLS.md`。

## 工作树（Worktree）工作流（每个任务必须遵守）

**默认启用独立 worktree 隔离作业；任务完成后，先回归主干，再执行下一个任务。**

1. **启用 worktree**：每个任务在独立 worktree 中执行，不直接在主工作树改代码 —— 保证任务互不污染、可并行、可丢弃。
2. **任务完成后先回主干**：任务自验证通过后，**第一优先级是把它合并回主干（master/main）**，而不是立刻开新任务。合并必须落在主干上、提交可达（`git log` 能在主干看到该提交），并在主干复核门禁。
3. **再执行下一个任务**：主干合并完成、working tree 干净后，才从**最新主干**切出下一个任务的 worktree，再开始下一轮。

**合并回主干前必须确认（收尾清单）**：

- 只提交本任务相关的**精确白名单**文件，禁止夹带无关并行改动；
- 门禁验证通过（按本仓"变更必跑流水线"执行，禁止跳步）；
- 合并方式优先 `git merge --ff-only`；若主干已前进则先同步主干再合并，**禁止**强行覆盖主干；
- 合并后确认：主干 working tree 干净、提交在主干可达、无孤儿提交/未合并分支残留。

> 目的：杜绝"任务半成品散落在多个 worktree、主干长期落后、提交不可达"的漂移。若 worktree 分支引用被外部重置导致提交不可达，必须用提交 SHA 立即合并回主干（如 `git merge --ff-only <sha>`）以保证成果持久。

## 变更必跑流水线（概览，按顺序，禁止跳步）

触发条件：case 变动、插件增删、parser/rule 代码修改、showcase.rs 变更、sidecar 修改。

```
0. cargo test --lib content_classifier   →  内容分类器回归（含 syslog/ci_log/cloud_log 等 sweep）
1. audit_sample_case_quality.py  →  物理 case 质量门禁
2. audit_case_metrics.py         →  压缩语义 + 冻结门禁
3. audit_all_case_metrics.py     →  全插件健康检查
4. generate_plugin_capability_index.py  →  刷新能力索引
```

- **步骤 0 前置**：当改动落在 `content_classifier`（增删语义类别、改种子特征词、改 build.rs 聚合配置）时，**必须先跑**步骤 0 分类器回归；若分类器变化改变了某插件切片的路由（尤其剥皮类别增量接管），回到步骤 1/2 复核受影响插件的 case 质量与压缩语义。剥皮类别边界见 `COMPRESSION.md`。
- 禁止只跑 1-2 个就声称"已审计"。**详细参数、产物回收、状态机、并发约束、简表见 `AUDIT.md`**。

## 代码风格

- **中文注释**：Rust 代码中所有注释、内联说明、内部文档必须用中文。
- 每个新增或重大修改的函数必须加 `#[tracing::instrument(level = "debug", skip_all)]`；极高频内部循环用 `level = "trace"`。release 构建会物理编译掉，零开销。

## 文档治理（要点）

- 根目录长期入口文档：`README.md`、`AGENTS.md`（权威）、`COMPRESSION.md`/`AUDIT.md`/`TOOLS.md`（AGENTS 按需引用）、`CLAUDE.md`/`CODEX.md`（`@AGENTS.md` 指针）、`.tokenslim-context.md`、`DOCS_ORGANIZATION.md`。
- 当前计划→`docs/plans/`；任务看板→`docs/tasks/`；状态/完成报告→`docs/reports/`；历史→`docs/archive/`。
- 已完成 PLAN 必须归档，不得继续以"当前计划"形式留在活跃位置。
- 移动既有文件前先 `tokenslim run git status/diff -- <path>`；已跟踪文件用 `tokenslim run git mv` 保留历史；未跟踪文件确认非用户重要工作才动。
- **禁止自造缩写**：不得发明 VCS 工具不官方支持的简写（如把 `git status` 内部改写为 `git st`）。
- `tokenslim run <vcs cmd>` 输出严格映射为 raw-string（`command_raw` 原始 / `command_norm` 规范化），防止 serde_json 反序列化失败。

收口前必须按 `DOCS_ORGANIZATION.md` 的"收口同步清单"逐项核对文档状态（计划/任务/实现状态/审计总览/入口链接/交付范围），禁止"代码已完成但计划/报告仍标待完成"的漂移。
