# TokenSlim Workspace Doctor 规范（面向 LLM 的本地工作区画像）

> 目标：让 LLM 在生成代码前，先知道“这个仓库到底是什么项目、该用什么工具链、有哪些环境约束”，从而减少错误生成与无效往返。

---

## 1. 设计目标

1. 提供 **决策型上下文**（而非冗长说明）。
2. 同时支持：
   - 人类可读（`text`）
   - 机器可读（`json`）
   - LLM 省 token（`llm` / `json-min`）
3. 与现有 `encoding` 协同，形成统一诊断体系。

---

## 2. 命令规划

1. `tokenslim workspace`
2. `tokenslim workspace --format text|json|llm|json-min`
3. `tokenslim workspace --strict`（可选：信息缺失时提高风险等级）

---

## 3. 最小输出模型（LLM 优先）

## 3.1 标准 JSON（给系统集成）

```json
{
  "risk": "warn",
  "os": "windows11",
  "shell": "powershell",
  "encoding": "cp936",
  "project": {
    "primary": "rust",
    "secondary": [],
    "build": "cargo",
    "test": "cargo test"
  },
  "tools": {
    "rust": "1.87.0",
    "node": "v24.14.0",
    "python": "3.14.3",
    "java": "17.0.10"
  },
  "ide": {
    "vscode": true,
    "idea": false,
    "visual_studio": false
  },
  "repo": {
    "git": true,
    "branch": "master",
    "dirty": true
  },
  "actions": [
    "set session utf-8",
    "prefer cargo commands"
  ]
}
```

## 3.2 LLM 紧凑格式（优先省 token）

```json
{"r":"warn","enc_risk":"warn","os":"win11","sh":"pwsh","enc":"cp936","proj":"rust","ide":["vscode"],"repo":{"v":"git","b":"master","d":true},"act":["utf8-session","cargo-first"]}
```

### 3.3 LLM 协议 v1（固定字段）

为保证 IDE 插件/Agent 稳定消费，`--format llm` 约定如下：

1. `r`: 工作区总体风险（`ok|warn|fail`）
2. `enc_risk`: 编码风险（`ok|warn|fail`）
3. `os`: 操作系统简述
4. `sh`: 当前 shell（`bash|cmd|powershell|...`）
5. `enc`: 编码摘要（如 `utf8` / `936`）
6. `proj`: 主项目类型（如 `rust`）
7. `ide`: IDE 数组（可多值）
8. `repo`: 紧凑仓库对象
   - `v`: `git|no-git`
   - `b`: branch
   - `d`: dirty 布尔（可为空）
9. `act`: 动作建议 code 列表

说明：
- 该 schema 为 **v1 基线**，新增字段需保持向后兼容。
- 调用方应容忍未知字段，并以 `r` / `enc_risk` 作为核心决策信号。

---

## 10. Prompt 注入模板（给 Agent/LLM）

以下模板用于把 `--format llm` 结果注入到系统提示词，减少误判与无效命令。

### 10.1 极简模板（推荐）

```text
[Workspace Context v1]
{LLM_JSON}

Rules:
1) Use project type in `proj` as primary implementation language.
2) Use environment constraints from `sh` and `enc_risk`.
3) Prefer repository-native build/test commands for the detected project type.
4) If `enc_risk=warn|fail`, avoid assumptions about UTF-8 and suggest explicit encoding settings.
5) If `repo.v=no-git`, avoid git operations.
```

### 10.2 强约束模板（代码生成场景）

```text
You are operating in this workspace context (schema v1):
{LLM_JSON}

Hard constraints:
- Primary language/framework must follow `proj`.
- Shell-specific commands must match `sh`.
- If `enc_risk != ok`, output explicit encoding-safe commands and avoid lossy assumptions.
- Keep actions aligned with `act` hints.
- Do not suggest unrelated toolchains.
```

### 10.3 解析建议（调用方）

1. 先判定：`r`、`enc_risk`
2. 再选路径：`proj` + `sh`
3. 再收敛动作：`act`
4. 若缺字段：降级到保守策略（不做 destructive 操作）

---

## 4. 检测维度与优先级

## 4.1 项目类型识别（核心）

按文件特征判定（可多命中，取 primary + secondary）：

1. Rust：`Cargo.toml`
2. Java：`pom.xml` / `build.gradle` / `build.gradle.kts`
3. C#：`*.sln` / `*.csproj`
4. Node：`package.json`
5. Python：`pyproject.toml` / `requirements.txt` / `setup.py`
6. C/C++：`CMakeLists.txt` / `Makefile`

优先级建议：
- 若 `Cargo.toml` 存在且 `src/main.rs|lib.rs` 存在 => primary=rust
- 多语言仓库保留 secondary 列表

## 4.2 构建/测试命令识别

从项目类型映射默认命令：

1. Rust：`cargo build` / `cargo test`
2. Java-Maven：`mvn -q test`
3. Java-Gradle：`./gradlew test`
4. C#：`dotnet build` / `dotnet test`
5. Node：`npm test`（或读取 scripts.test）
6. CMake：`cmake --build` + ctest

## 4.3 IDE/编辑器生态识别

1. VSCode：`.vscode/`
2. IDEA：`.idea/` 或 `*.iml`
3. Visual Studio：`*.sln`, `.vs/`
4. Cursor/Trae/Kiro 等：可按特征目录补充

## 4.4 环境与编码

复用 `encoding`：

1. os/shell/codepage
2. python/node/java 版本与编码信号
3. 风险汇总（OK/WARN/FAIL）

---

## 5. 风险评估规则（workspace 层）

1. `FAIL`
   - 项目类型无法识别 + 工具链关键命令不可用
   - 编码高风险且主构建链路不可执行

2. `WARN`
   - 项目可识别，但构建/测试命令不明确
   - 多语言冲突明显（例如 Rust 项目但 agent 常误用 npm）
   - 编码存在冲突（如 Windows cp936 + Python gbk）

3. `OK`
   - 项目类型明确
   - 主工具链可用
   - 编码一致性健康

---

## 6. 输出降噪策略（token 优化）

1. `llm` 默认只输出“决策字段”：
   - project.primary
   - build/test command
   - shell/encoding risk
   - actions

2. 不输出长解释文本；建议句改为短 action code：
   - `utf8-session`
   - `cargo-first`
   - `java-encoding-utf8`

3. `json-min` 去掉 null/false 默认项。

---

## 7. 实现拆解（最小可落地）

1. `src/core/doctor_workspace/types.rs`
   - 定义标准模型 + 紧凑模型

2. `src/core/doctor_workspace/methods.rs`
   - 文件特征扫描
   - 工具链探测
   - 风险判定
   - text/json/llm 渲染

3. `src/core/doctor_workspace/mod.rs`
   - 模块导出

4. `src/core/mod.rs`
   - 注册 `pub mod doctor_workspace;`

5. `src/cli/types.rs` / `src/cli/methods.rs`
   - 增加 `workspace` 子命令分支与 format 支持

6. `tests/doctor_workspace.rs`
   - 项目类型识别
   - 命令映射
   - llm 输出字段稳定性

---

## 8. 验收标准

1. 能识别主项目类型（至少 Rust/Java/C#/Node/Python/C++）。
2. 能给出主构建/测试命令建议。
3. 能识别 IDE 生态信号。
4. `--format llm` 明显短于 text/json（目标节省 50%+ token）。
5. 在本仓库输出应明确：`primary=rust`、`build=cargo build`、`test=cargo test`。

---

## 9. 一句话总结

`workspace` 是把“本地开发事实”压缩成 LLM 可直接消费的决策上下文，目标是 **少猜测、少返工、少 token**。
