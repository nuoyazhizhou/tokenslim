# TokenSlim 项目架构文档

> 面向新开发者的项目架构总览，帮助快速理解模块划分、数据流和扩展方式。

---

## 一、项目概览

TokenSlim 是一款由 Rust 编写的高性能、插件化 AI 文本输入压缩引擎。核心使命是将极度冗长的版本控制系统（VCS）日志、编译器输出、构建流水线等文本，压缩成 LLM 可极速解析的紧凑格式。

### 关键指标

| 指标         | 数值                                                                      |
| ------------ | ------------------------------------------------------------------------- |
| 语言         | Rust 2021                                                                 |
| 插件目录总数 | 55（含 14 个专用 VCS 插件、1 个统一 VCS 插件、40 个 non-VCS 插件）        |
| VCS 插件     | 14（Git/SVN/Hg/P4/CVS/Bzr/Fossil/Darcs/GH/GLab/AZ/Bitbucket/Repo/Gerrit） |
| non-VCS 插件 | 40                                                                        |
| IDE 扩展     | 3（VS Code / JetBrains / Chrome）                                         |
| SDK          | 3（Python / Node.js / Java）                                              |
| 吞吐量       | ~400 MB/s                                                                 |

---

## 二、顶层目录结构

```
TokenSlim/
├── src/                          # Rust 源码
│   ├── main.rs                   # CLI 入口
│   ├── lib.rs                    # 库入口
│   ├── cli/                      # CLI 命令解析与路由
│   ├── core/                     # 核心引擎（30+ 模块）
│   ├── plugins/                  # 55 个插件目录
│   └── bin/                      # 独立二进制（server、bench 等）
│
├── crates/                       # 子 crate
│   └── plugin-interface/         # 动态插件 FFI 接口定义
│
├── vscode-extension/             # VS Code 扩展（TypeScript）
├── jetbrains-plugin/             # JetBrains IDE 插件（Kotlin）
├── chrome-extension/             # Chrome 浏览器扩展（TypeScript）
├── sdk/                          # 多语言 SDK
│   ├── python/tokenslim_sdk.py
│   ├── nodejs/tokenslim-sdk.js
│   └── java/TokenSlimClient.java
│
├── samples/                      # 测试样本（按插件分类）
├── config/                       # 插件配置文件
├── scripts/                      # 构建/审计/工具脚本
├── docs/                         # 项目文档
│   ├── development/              # 开发者文档
│   ├── design/                   # 设计文档
│   ├── guides/                   # 使用指南
│   ├── reports/                  # 完成报告
│   ├── audit/                    # 审计报告（自动生成）
│   ├── tasks/                    # 任务看板
│   ├── plans/                    # 计划文档
│   ├── prompts/                  # 提示词模板
│   └── archive/                  # 历史归档
│
└── other/                        # 参考项目（RTK/TOKF）
```

---

## 三、核心引擎架构（src/core/）

核心引擎采用**管道（Pipeline）架构**，文本从输入到输出经过一系列可组合的处理阶段。

### 3.1 数据流

```
输入文本
  │
  ▼
┌─────────────────┐
│  StreamReader    │  文件/流读取，自动选择 mmap 或内存读取
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  TextSlicer      │  将文本切分为可独立处理的切片（Slice）
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  ContentAnalyzer │  分析切片内容特征，识别文本类型
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  PluginDispatcher│  根据分析结果分派到对应插件处理
└────────┬────────┘
         │
    ┌────┴────┐
    ▼         ▼
┌───────┐ ┌───────┐
│Plugin1│ │Plugin2│  ...  各插件并行/串行处理
└───┬───┘ └───┬───┘
    └────┬────┘
         ▼
┌─────────────────┐
│  Compression     │  最终压缩组装（字典、去重、格式化）
│  Pipeline        │
└────────┬────────┘
         │
         ▼
      输出文本
```

### 3.2 核心模块清单

| 模块                      | 路径                          | 职责                                        |
| ------------------------- | ----------------------------- | ------------------------------------------- |
| **stream_reader**         | `core/stream_reader/`         | 文件读取，自动选择 mmap/内存策略            |
| **text_slicer**           | `core/text_slicer/`           | 文本切片，支持行级/块级/正则切片            |
| **content_analyzer**      | `core/content_analyzer/`      | 内容特征分析，识别日志/代码/配置类型        |
| **plugin_dispatcher**     | `core/plugin_dispatcher/`     | 插件路由与调度，管理插件生命周期            |
| **compression_pipeline**  | `core/compression_pipeline/`  | 压缩管线编排，串行/并行模式切换             |
| **compression**           | `core/compression/`           | 压缩核心逻辑                                |
| **dictionary_engine**     | `core/dictionary_engine/`     | 字典引擎，路径/宏/模式提取与替换            |
| **dictionary_manager**    | `core/dictionary_manager/`    | 字典生命周期管理                            |
| **dedup_engine**          | `core/dedup_engine/`          | 全局去重引擎，行级/帧级去重                 |
| **path_optimizer**        | `core/path_optimizer/`        | 路径优化，Radix Trie 前缀提取               |
| **path_analyzer**         | `core/path_analyzer/`         | 路径分析与分类                              |
| **path_compressor**       | `core/path_compressor/`       | 路径压缩，$P/$D 替换                        |
| **rehydration_pipeline**  | `core/rehydration_pipeline/`  | 解压还原管线                                |
| **log_reorderer**         | `core/log_reorderer/`         | 确定性日志重排（解决 make -jN 乱序）        |
| **timestamp_converter**   | `core/timestamp_converter/`   | 时间戳规范化                                |
| **encoding_fallback**     | `core/encoding_fallback/`     | 编码回退链（UTF-8→GBK→...）                 |
| **doctor_encoding**       | `core/doctor_encoding/`       | 编码环境诊断                                |
| **doctor_workspace**      | `core/doctor_workspace/`      | 工作区诊断（项目类型/框架/IDE）             |
| **rule_diagnosis**        | `core/rule_diagnosis/`        | 规则冲突/有效性诊断                         |
| **init_command**          | `core/init_command/`          | 项目初始化（.tokenslim.toml + Shell Hooks） |
| **tracking**              | `core/tracking/`              | SQLite Token 追踪与统计                     |
| **filter_variants**       | `core/filter_variants/`       | 过滤器变体系统                              |
| **filter_discover**       | `core/filter_discover/`       | 过滤器自动发现                              |
| **rewrite**               | `core/rewrite/`               | 命令重写引擎                                |
| **safety_check**          | `core/safety_check/`          | 安全检查（注入/隐藏字符）                   |
| **template_render**       | `core/template_render/`       | 模板渲染引擎                                |
| **tree_restructure**      | `core/tree_restructure/`      | 目录树结构重组                              |
| **error_isolation**       | `core/error_isolation/`       | 错误隔离与安全执行                          |
| **embedding_engine**      | `core/embedding_engine/`      | 向量嵌入引擎（预留）                        |
| **metrics**               | `core/metrics/`               | 压缩指标收集                                |
| **sys_env**               | `core/sys_env/`               | 系统环境信息收集                            |
| **plugin_config_loader**  | `core/plugin_config_loader/`  | 插件配置加载                                |
| **dynamic_plugin_loader** | `core/dynamic_plugin_loader/` | 动态插件加载（FFI）                         |
| **json_extractor**        | `core/json_extractor/`        | JSON 提取工具                               |
| **observability**         | `core/observability.rs`       | 可观测性（tracing 初始化）                  |
| **utils**                 | `core/utils/`                 | 通用工具（ROI 门控、JSON 工具）             |

---

## 四、插件系统（src/plugins/）

### 4.1 插件架构

每个插件遵循统一结构：

```
src/plugins/<plugin_name>/
├── mod.rs          # 模块入口，注册插件
├── types.rs        # 数据结构定义
├── methods.rs      # 核心压缩逻辑
├── test.rs         # 单元测试
└── showcase.rs     # 展示报告生成
```

### 4.2 全部插件清单

#### VCS 插件（14 个）

| 插件                 | 目录                    | 覆盖命令                                   |
| -------------------- | ----------------------- | ------------------------------------------ |
| vcs_git_plugin       | `vcs_git_plugin/`       | git status/log/diff/branch/reflog/shortlog |
| vcs_svn_plugin       | `vcs_svn_plugin/`       | svn status/log/info/diff                   |
| vcs_hg_plugin        | `vcs_hg_plugin/`        | hg status/log/summary/diff                 |
| vcs_p4_plugin        | `vcs_p4_plugin/`        | p4 opened/changes/describe/diff            |
| vcs_cvs_plugin       | `vcs_cvs_plugin/`       | cvs status/log/diff                        |
| vcs_bzr_plugin       | `vcs_bzr_plugin/`       | bzr status/log/diff                        |
| vcs_fossil_plugin    | `vcs_fossil_plugin/`    | fossil status/timeline/diff                |
| vcs_darcs_plugin     | `vcs_darcs_plugin/`     | darcs whatsnew/log/diff                    |
| vcs_gh_plugin        | `vcs_gh_plugin/`        | gh pr/issue/repo                           |
| vcs_glab_plugin      | `vcs_glab_plugin/`      | glab mr/issue/repo                         |
| vcs_az_plugin        | `vcs_az_plugin/`        | az repos/pr                                |
| vcs_bitbucket_plugin | `vcs_bitbucket_plugin/` | bb pr/repo                                 |
| vcs_repo_plugin      | `vcs_repo_plugin/`      | repo status/sync/diff                      |
| vcs_gerrit_plugin    | `vcs_gerrit_plugin/`    | gerrit review/query                        |

#### 构建工具插件（8 个）

| 插件                  | 目录                     | 覆盖场景                    |
| --------------------- | ------------------------ | --------------------------- |
| gcc_log_plugin        | `gcc_log_plugin/`        | GCC/Clang 编译输出          |
| rust_go_plugin        | `rust_go_plugin/`        | Cargo/Go 构建与测试         |
| maven_plugin          | `maven_plugin/`          | Maven 构建输出              |
| nodejs_plugin         | `nodejs_plugin/`         | npm/tsc/eslint/webpack/jest |
| android_gradle_plugin | `android_gradle_plugin/` | Android Gradle 构建         |
| dotnet_plugin         | `dotnet_plugin/`         | .NET/MSBuild 输出           |
| bazel_plugin          | `bazel_plugin/`          | Bazel 构建输出              |
| webpack_vite_plugin   | `webpack_vite_plugin/`   | Webpack/Vite 构建输出       |

#### 异常/堆栈插件（3 个）

| 插件                    | 目录                       | 覆盖场景         |
| ----------------------- | -------------------------- | ---------------- |
| java_stack_plugin       | `java_stack_plugin/`       | Java 异常堆栈    |
| python_traceback_plugin | `python_traceback_plugin/` | Python Traceback |
| node_error_plugin       | `node_error_plugin/`       | Node.js 错误输出 |

#### 结构化数据插件（7 个）

| 插件            | 目录               | 覆盖场景          |
| --------------- | ------------------ | ----------------- |
| json_plugin     | `json_plugin/`     | JSON 压缩         |
| yaml_plugin     | `yaml_plugin/`     | YAML 压缩         |
| xml_html_plugin | `xml_html_plugin/` | XML/HTML 压缩     |
| sql_plugin      | `sql_plugin/`      | SQL 语句压缩      |
| markdown_plugin | `markdown_plugin/` | Markdown 压缩     |
| ndjson_plugin   | `ndjson_plugin/`   | NDJSON 流式解析   |
| protobuf_plugin | `protobuf_plugin/` | Protobuf 定义压缩 |

#### IaC/基础设施插件（7 个）

| 插件                     | 目录                        | 覆盖场景                           |
| ------------------------ | --------------------------- | ---------------------------------- |
| terraform_plugin         | `terraform_plugin/`         | Terraform 输出                     |
| ansible_plugin           | `ansible_plugin/`           | Ansible 输出                       |
| pulumi_plugin            | `pulumi_plugin/`            | Pulumi 输出                        |
| cloudformation_plugin    | `cloudformation_plugin/`    | CloudFormation 输出                |
| helm_plugin              | `helm_plugin/`              | Helm 输出                          |
| kubernetes_docker_plugin | `kubernetes_docker_plugin/` | K8s/Docker 输出                    |
| db_log_plugin            | `db_log_plugin/`            | 数据库日志（MySQL/PG/Mongo/Redis） |

#### 其他插件（13 个）

| 插件                   | 目录                      | 覆盖场景          |
| ---------------------- | ------------------------- | ----------------- |
| ansi_cleaner_plugin    | `ansi_cleaner_plugin/`    | ANSI 转义序列净化 |
| noise_filter_plugin    | `noise_filter_plugin/`    | 噪音过滤          |
| smart_code_plugin      | `smart_code_plugin/`      | 代码智能压缩      |
| smart_path_plugin      | `smart_path_plugin/`      | 智能路径压缩      |
| static_rule_plugin     | `static_rule_plugin/`     | 静态规则引擎      |
| template_driven_plugin | `template_driven_plugin/` | 模板驱动压缩      |
| git_diff_plugin        | `git_diff_plugin/`        | Git Diff 专用压缩 |
| generic_text_plugin    | `generic_text_plugin/`    | 通用文本压缩      |
| syslog_plugin          | `syslog_plugin/`          | Syslog 日志压缩   |
| spring_boot_plugin     | `spring_boot_plugin/`     | Spring Boot 日志  |
| php_ruby_plugin        | `php_ruby_plugin/`        | PHP/Ruby 输出     |
| unity_unreal_plugin    | `unity_unreal_plugin/`    | Unity/Unreal 日志 |
| pytest_plugin          | `pytest_plugin/`          | Pytest 输出       |

---

## 五、CLI 层（src/cli/）

CLI 层负责命令解析、参数验证和路由到核心引擎。

| 文件         | 职责                                                    |
| ------------ | ------------------------------------------------------- |
| `mod.rs`     | 模块入口，命令路由                                      |
| `types.rs`   | CLI 参数类型定义（CompressArgs/DecompressArgs/RunArgs） |
| `methods.rs` | CLI 命令实现（compress/decompress/run/doctor/init）     |
| `test.rs`    | CLI 集成测试                                            |

### 支持的命令

```
tokenslim compress -i <input> -o <output>    # 压缩
tokenslim decompress -i <input> -o <output>  # 解压
tokenslim run <command> [args...]            # 包装外部命令
tokenslim encoding [--format json]           # 编码诊断
tokenslim workspace [--format llm]           # 工作区诊断
tokenslim init [--no-hooks]                  # 项目初始化
tokenslim gain [--daily|--by-filter|--json]  # Token 节省统计
tokenslim --rewrite "make test"              # 命令重写预览
tokenslim --discover <session-dir>           # 过滤器发现
tokenslim --preset ai -- <command>           # AI 预设模式
```

---

## 六、IDE 扩展

### 6.1 VS Code 扩展（vscode-extension/）

- **语言**：TypeScript
- **入口**：`src/extension.ts`
- **模式**：REST API 模式，通过 HTTP 与 tokenslim-server 通信
- **命令**：Compress Current File / Compress Selection / Restart Server

### 6.2 JetBrains 插件（jetbrains-plugin/）

- **语言**：Kotlin
- **核心类**：`CompressAction`、`TokenSlimServerManager`、`TokenSlimClient`
- **模式**：通过 HTTP 与 tokenslim-server 通信

### 6.3 Chrome 扩展（chrome-extension/）

- **语言**：TypeScript
- **核心文件**：`src/content.ts`（内容注入）、`src/rehydrator.ts`（解压还原）
- **功能**：自动检测网页内容，对剪贴板 Token 进行实时膨胀/还原

---

## 七、SDK（sdk/）

提供 Python、Node.js、Java 三种语言的 SDK，封装对 TokenSlim Server 的 HTTP 调用。

| SDK     | 文件                            | 核心类/函数       |
| ------- | ------------------------------- | ----------------- |
| Python  | `sdk/python/tokenslim_sdk.py`   | `TokenSlimClient` |
| Node.js | `sdk/nodejs/tokenslim-sdk.js`   | `TokenSlimClient` |
| Java    | `sdk/java/TokenSlimClient.java` | `TokenSlimClient` |

---

## 八、如何添加新插件

### 8.1 静态插件（推荐）

1. 在 `src/plugins/` 下创建 `<plugin_name>/` 目录
2. 创建以下文件：
   - `mod.rs` — 注册插件，实现 `Plugin` trait
   - `types.rs` — 定义插件配置和数据结构
   - `methods.rs` — 实现 `detect()` 和 `compress()` 核心逻辑
   - `test.rs` — 单元测试
   - `showcase.rs` — 展示报告
3. 在 `src/plugins/mod.rs` 中注册新插件
4. 在 `samples/<plugin_name>/` 下添加测试样本
5. 运行 `cargo test` 验证

### 8.2 关键约束

- **ROI 门控**：压缩入口必须用 `prefer_non_expanding(raw, compacted)` 包裹
- **路径处理**：必须使用 `crate::core` 中的共享 `looks_like_vcs_path()` 函数
- **测试**：禁止 hardcode 长字符串，使用 `include_str!` 或 `fs::read_to_string`
- **注释**：所有代码注释使用中文

---

## 九、关键设计决策

| 决策                | 说明                                                   |
| ------------------- | ------------------------------------------------------ |
| 静态插件优先        | 核心插件编译进主程序，零加载开销；边缘插件支持动态加载 |
| Radix Trie 路径提取 | 全量扫描后构建前缀树，在高权重分支提取目录字典         |
| 确定性重排          | 解决 `make -jN` 并发构建的乱序日志问题                 |
| 编码回退链          | UTF-8 优先 + codepage 候选，确保多语言环境可靠性       |
| Bump 内存池         | 高性能内存分配，减少 alloc 开销                        |
| rayon 并行          | 大规模文本的并行块处理                                 |

---

## 十、相关文档

| 文档                    | 位置                                           | 说明               |
| ----------------------- | ---------------------------------------------- | ------------------ |
| 使用手册                | `../guides/USER_GUIDE.md`                      | 常用命令与审计入口 |
| 开发手册                | `DEVELOPER_GUIDE.md`                           | 接手流程与验证命令 |
| Compression Protocol V1 | `../../CLAUDE.md`                              | 压缩协议宪法       |
| 插件开发指南            | `../design/PLUGIN_DEVELOPMENT_GUIDE.md`        | 详细的插件开发教程 |
| 插件架构分析            | `../design/PLUGIN_ARCHITECTURE_ANALYSIS.md`    | 插件分工原则       |
| 文档组织规范            | `../../DOCS_ORGANIZATION.md`                   | 文档管理规则       |
| 功能路线图              | `../plans/FEATURE_ROADMAP.md`                  | 功能开发计划       |
| 当前实现状态            | `../reports/IMPLEMENTATION_STATUS.md`          | P1-P4 状态         |

---

**维护者**：TokenSlim 开发团队  
**最后更新**：2026-05-13
