# 非 VCS 插件分族战术提示词

> 当前状态（2026-05-13）：下文保留的是历史战术处方与修复背景；当前权威总览见 `docs/audit/non_vcs_case_semantic_audit.md`，非 VCS 已收敛为 **484/484 all_pass，needs_fix=0，frozen=484**。

> 使用方式：本文档结合 TokenSlim 项目的 `CLAUDE.md`（Compression Protocol V1）一起使用。
> `CLAUDE.md` 是通用宪法，本文档是 31 个非 VCS 插件的族群战术补充。
> 执行任何修改前必须先读取 `CLAUDE.md`，再读取本文档的「通用部分」，最后读取本文档对应族群章节。

---

## 0. 定位：为什么需要非 VCS 战术提示词

Compression Protocol V1 最初针对 14 个 VCS 工具设计。同一套 4 闸门审计（G1_ROI / G2_ANSI_CLEAN / G3_NO_ERROR_LOSS / G4_NON_EMPTY）下：

- VCS 侧 318 个 case：**313 通过，5 需优化（98.4%）**
- 非 VCS 侧 344 个 case：**274 通过，70 需优化（79.7%）**

差距主要来自 V1 描述中未明确定义的语义在非 VCS 场景的歧义：

| V1 法则                    | VCS 原意                                                | 非 VCS 翻译                                                                                                                                                                                                 |
| -------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 法则 0 绝对锚点            | 第一行触发命令（`git log` 等）                          | **非 VCS 往往没有命令锚点**。翻译为「第一行诊断签名」：编译器头行（`error[E...]`）、异常类名（`Traceback (most recent call last)`）、协议版本（`HTTP/1.1`）等第一个可识别的类型签名。样本无签名时无锚点要求 |
| 法则 A 路径字典 + ROI 门控 | `dict_engine.add_path_layered` + `prefer_non_expanding` | **同样适用**。但非 VCS 插件大量违规：自造 `$XXX\|` IR 标签而不接 `prefer_non_expanding`，导致小样本（<100B）反而扩张                                                                                        |
| 法则 B 时间规范化          | `YYYY-MM-DD HH:MM:SS`                                   | **多格式**：ISO8601（`2024-01-15T10:30:45Z`）、Epoch、Unreal（`[2024.01.15-10.30.45:456]`）、syslog（`Jan 15 10:30:45`）。统一目标仍是 `YYYY-MM-DD HH:MM:SS`，但输入解析子规则因插件而异                    |
| 法则 C ANSI 净化           | 文本处理第一步剥离                                      | **完全适用**，无差异                                                                                                                                                                                        |
| 法则 D 防失忆              | Commit message / error / fatal                          | **扩展保留词**：`error`、`fatal`、`panic`、`TypeError`、`SyntaxError`、`NullPointerException`、`Uncaught Exception`、`Traceback` 等。**任何运行时错误/异常类关键词压缩后必须保留**（大小写不敏感）          |
| 法则 E 零噪音              | `hint:` / `Counting objects`                            | **按插件定义**：编译器进度行（`[  50%] Building`）、下载进度（`Downloading: 40%`）、路由器诊断（`BGP table version`）等                                                                                     |
| 法则 F Diff 爆栈           | `MAX_DIFF_LINES` 截断                                   | **仅对 git_diff 适用**；其他插件不涉及                                                                                                                                                                      |

---

## 1. 通用部分：所有非 VCS 插件必须遵守

### 1.1 四闸门机器化审计（硬约束，禁止绕过）

所有非 VCS 插件的 case 在冻结前必须通过 4 条机器化闸门：

- **G1_ROI**：`compression_pct >= 0`。数据源以 `scripts/audit_case_metrics.py` 生成的 snapshot JSON 为准，不要在 PowerShell 里重新读文件计算字节数（尾换行归一化会偏差）。
- **G2_ANSI_CLEAN**：compact 中必须无 `0x1B` ESC 字节。用 `[regex]::IsMatch($compact, '\x1b')` 检查，**不得用 `` `e `` 字面量**（PowerShell 5.1 对 ``-match "`e"`` 的展开不稳定）。
- **G3_NO_ERROR_LOSS**：若 original 含 `(?i)error|fatal|panic`，compact 必须保留其中至少一个字面量（大小写不敏感）。
- **G4_NON_EMPTY**：非空 original 必须产生非空 compact。

**工具**：`tmp/honest_audit_non_vcs.ps1` 已实现这 4 闸门。运行它得到每个 case 的 pass/fail 结论与 needs_fix 清单。

### 1.2 冻结动作的硬约束

- **禁止** `-FreezeUnchanged`：这是「hash 不变就批量冻结」的快捷路径，绕过语义闸门。建议 5 的第一次执行因此被撤回。
- **允许** `-FreezeCase <case_id>`：对通过全部 4 闸门的单个 case 逐个冻结。
- **CLAUDE.md 引用**：「禁止后补识别」「只有在该 case 当轮回归为 improved/unchanged 且语义通过时，才允许执行冻结」。

### 1.3 插件实现的硬约束

- **必须**调用 `crate::core::prefer_non_expanding(raw, compacted)` 作为 `compress()` 的最外层包装。
- **必须**用 `crate::core::dict_engine::add_path_layered(path)` 把路径字典化，不要自造 `$XXX|file:line:col` 的长 IR 字符串。
- **严禁**在 `compress` 入口之外自行实现 `looks_like_vcs_path`/`looks_like_path`；使用 `crate::core` 共享版本。
- **必须**在第一步剥离 ANSI 逃逸序列（`crate::core::strip_ansi` 或等价工具）。
- **必须**在测试里用 `std::fs::read_to_string("samples/<p>/case_XXX.log")` 读真实样本，禁止 hardcode 日志字符串。
- **所有 Rust 注释必须是中文。**

### 1.4 审计流程范例

```powershell
# 1. 改完插件后先跑整体回归
tokenslim run cargo test --lib <plugin>_plugin::

# 2. 跑本插件的审计快照
tokenslim run python scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -FailOnRegression -FailOnFrozenChange

# 3. 导出单 case 前后文本
tokenslim run python scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -CaseId case_XXX
# 读 docs/audit/<plugin>/cases/case_XXX/original.txt 和 compact.txt

# 4. 单 case 语义通过后冻结
tokenslim run python scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -FreezeCase case_XXX

# 5. 整体闸门审计（覆盖 4 条闸门 + 导出 + 冻结通过的 case）
powershell -File tmp/honest_audit_non_vcs.ps1
```

### 1.5 族群索引

| 族群           | 插件数 | 成员                                                                               | 通过率 | 主要问题                             |
| -------------- | -----: | ---------------------------------------------------------------------------------- | -----: | ------------------------------------ |
| A 编译日志族   |      6 | rust_go、gcc_log、dotnet、android_gradle、maven、xcode_log                         |     中 | G1 小样本扩张 + IR 标签未 ROI 门控   |
| B 运行时错误族 |      5 | java_stack、node_error、python_traceback、smart_code、php_ruby                     |     低 | G3 防失忆违规（吞 error/TypeError）  |
| C 结构化日志族 |      6 | syslog、db_log、web_log、nodejs、spring_boot、kubernetes_docker                    |     低 | G1 系统性扩张（syslog/db_log 11/11） |
| D 序列化格式族 |      5 | json、yaml、xml_html、markdown、sql                                                |     高 | G1 小样本扩张                        |
| E 通用工具族   |      6 | ansi_cleaner、generic_text、noise_filter、smart_path、static_rule、template_driven |     高 | G1 小样本扩张 + G4 纯 ANSI 空输出    |
| F 特定场景族   |      3 | webpack_vite、unity_unreal、git_diff                                               |     高 | 单点问题                             |

**合计 31 个插件**。完整 70 个 needs_fix case 清单见 `docs/audit/non_vcs_case_semantic_audit.md`。

---

## 族群 A — 编译日志族

**成员**：`rust_go`、`gcc_log`、`dotnet`、`android_gradle`、`maven`、`xcode_log`
**共同特征**：编译器 / 构建工具输出。含 `文件:行:列` 格式的诊断头、工具链自动生成的进度与下载噪音、堆栈路径、路径前缀高度重复。
**核心战术**：诊断头保留 + 路径字典化 + 进度噪音过滤 + ROI 门控。

### A.1 共同压缩约束

1. **诊断头必须保留**：`error[EXXXX]:` / `warning:` / `error CS\d+:` / `error: invalid conversion` 等第一字段是 LLM 定位错误类型的锚点，不得压缩或改写。
2. **路径字典化必须接 ROI**：`src/main.rs:5:9` 这种 `<file>:<line>:<col>` 三元组，直接用 `dict_engine.add_path_layered(file)` 替换为 `$Pn`，然后组合为 `$Pn:5:9`。**不要**插入 `$XXX\|R\|... --> \|$Pn\|5\|9` 这种带 IR 标签的格式——标签字符 + 竖线分隔符会比 `:` 三元组更长，必然触发法则 A ROI 违规。
3. **多行诊断主体尽量保留结构**：Rust 的 `--> file:line:col` + 下方带箭头的源码上下文是 LLM 理解错误点位的必要信号，不要激进折叠。
4. **进度 / 下载噪音必须过滤**（法则 E 零废话）：
   - Cargo：`   Compiling <pkg>`、`    Finished release`、`   Checking <pkg>`
   - Gradle：`> Task :xxxTask`、`BUILD SUCCESSFUL in Xms`、Gradle daemon startup
   - Maven：`[INFO] Scanning for projects...`、`[INFO] Building <module>`（这些是元信息头，不是错误）、`Downloading from central: <url>`
   - MSBuild：`Microsoft (R) Build Engine...`、`Copyright (C) Microsoft Corporation.`
   - Xcode：`Build description signature:`、编译器完整命令行
5. **ROI 门控**：`compress()` 最外层必须用 `prefer_non_expanding(raw, compacted)` 包裹。这是本族违规最严重的一类：`rust_go` 7/12 扩张、`gcc_log` 6/12 扩张、`maven` 10/11 扩张、`xcode_log` 2/11 扩张——根因都是 IR 标签 + 字段分隔符的总字节数超过了被替换掉的原文字节数。

### A.2 已知问题（真实 needs_fix case）

#### A.2.1 `rust_go`（7/12 扩张）

- **根因**：`compress_rust_compile_path()` 把 ` --> src/main.rs:5:9`（19B）改写为 `$RG|R| --> |src/main.rs|5|9`（27B），纯扩张 8 字节。
- **修复目标**：
  - 去掉 `$RG|R|` IR 标签。输出保持 ` --> $Pn:5:9` 即可（复用路径字典）。
  - 或者：保留 IR 标签但用 `prefer_non_expanding` 做最终兜底，扩张时回退到原文。
- **具体 case**：`case_001_rust_warning`、`case_003_rust_error`、`case_005_rust_noise`、`case_007_rust_long_line`、`case_009_rust_single`、`case_011_rust_no_compress`、`case_012_go_no_compress`。

#### A.2.2 `gcc_log`（6/12 扩张）

- **根因**：`compress_error_line()` / `compress_make_line()` 加 `$GCC`、`$MAKE` 标签后整体变长。
- **修复目标**：
  - 短样本（`<300B`）整体走 `prefer_non_expanding`；不足扩张时回退。
  - 保留状态码压缩（`make[2]: Entering directory` → `make[2] → $Pn`）但确保总长度减少。
- **具体 case**：`case_001_compile_success`（-1.6%）、`case_002_compile_error`（-3.6%）、`case_005_single_line`（-6.4%）、`case_007_multiple_warnings`（-2.7%）、`case_008_special_chars`（-1.6%）、`case_011_cpp_error`（-1.1%）。

#### A.2.3 `maven`（10/11 扩张）

- **根因**：`compact_maven_line()` 对每行 `[INFO] ...` 都调用字典化+重写，路径还没进字典时 `[INFO]` 前缀的处理只增加 1-2 字节但整体总叠加为 -0.1% 到 -0.6%。是系统性的微量扩张。
- **修复目标**：
  - 添加短样本 fast-path：整段 `<200B` 且字典命中率 <1 时直接 `prefer_non_expanding` 回退。
  - 或者：`[INFO] ` 前缀本身做去重（在段首出现一次后续不重复），但这会改变 compact_hash，需审慎评估下游影响。
- **具体 case**：`case_001_build_success`、`case_002_build_error`、`case_003_test_failure`、`case_004_clean`、`case_005_dependency_tree`、`case_006_noise`、`case_007_single_line`（-5%，最大）、`case_009_special_chars`、`case_010_mixed`、`case_012_long`。

#### A.2.4 `xcode_log`（2/11 扩张）

- **根因**：特殊字符和 no_compress 场景触发了字典化但字典头部本身比节省的字节多。
- **修复目标**：小样本 fast-path。
- **具体 case**：`case_010_special_chars`（-0.6%）、`case_012_no_compress`（-0.9%）。

#### A.2.5 `android_gradle`（1/11 扩张）

- `case_012_gradle_no_compress`（-1.8%）：同小样本 fast-path 问题。

### A.3 验证流程

```powershell
# 对单个插件回归
tokenslim run cargo test --lib <plugin>_plugin::

# 生成快照 + 4 闸门检查
powershell -File tmp/honest_audit_non_vcs.ps1
# 查看 docs/audit/non_vcs_case_semantic_audit.md 看 needs_fix 是否变化

# 单个 case 前后对比
tokenslim run python scripts/audit_case_metrics.py -Plugin <plugin> -Version v2 -CaseId case_XXX
```

每个 case 修复后：
1. 该 case 的 compression_pct 必须 ≥ 0（G1 通过）。
2. 其他已通过的 case 必须保持 improved / unchanged（`-FailOnRegression` 通过）。
3. 已冻结的 case 的 compact_hash 不得变化（`-FailOnFrozenChange` 通过）。
4. 所有 4 闸门通过后，用 `-FreezeCase` 冻结。

---

## 族群 B — 运行时错误族

**成员**：`java_stack`、`node_error`、`python_traceback`、`smart_code`、`php_ruby`
**共同特征**：运行时语言异常 / 堆栈追踪。信息密度高，每一行都是 LLM 排查的关键信号。
**核心战术**：**法则 D 防失忆红线**——任何与 `error|fatal|panic|exception|traceback|TypeError|SyntaxError|NullPointerException|Uncaught` 相关的关键词，压缩后必须可识别保留。

### B.1 共同压缩约束

1. **异常类名与消息不可丢**：`TypeError: Cannot read properties of undefined` → 压缩后至少保留 `TypeError` 或 `Cannot read properties`。把 `TypeError:` 替换为 `$PK1:` 的方案**违反法则 D**——字典映射对 LLM 是不透明的。
2. **堆栈帧可折叠但不可吞**：
   - 允许：重复 `at <framework>.xxx` 的堆栈连续段聚合成 `<N> more`
   - 允许：把 `at X.Y.method(File.java:42)` 字典化为 `$JST|Pn|method|42`
   - 禁止：丢失最底层（用户代码）与最顶层（异常起点）的完整帧
3. **原始异常类型必须保留可读**：
   - Java：`NullPointerException`、`IllegalArgumentException`、`RuntimeException`
   - Node.js：`TypeError`、`SyntaxError`、`ReferenceError`、`Error`
   - Python：`ValueError`、`KeyError`、`ImportError`、`AssertionError`
   - Ruby：`NoMethodError`、`NameError`、`ArgumentError`
   - PHP：`Fatal error:`、`Parse error:`、`Uncaught Error:`
4. **`Caused by:` / `During handling of the above exception:` / `Exception in thread "main":` 等链式异常前缀**必须保留，它们是多层异常嵌套的分界线。
5. **闸门 G3 为该族的硬约束**：`compress()` 必须在结尾处做自检：若 raw 含 `(?i)error|fatal|panic`，compacted 也必须含；否则触发兜底保留策略（例如把异常首行原样附加到 compact 末尾）。

### B.2 已知问题（真实 needs_fix case）

#### B.2.1 `node_error`（5/11 违反 G3）

- **根因**：`compress()` 的字典化策略把 `TypeError: Cannot read properties of undefined (reading 'x')` 整段替换为 `$PK1: Cannot read properties of undefined (reading 'x')`。**虽然 hash 稳定、压缩率正数（9.4% 到 23.2%），但 `TypeError`/`SyntaxError` 等异常类名字面量丢失**，LLM 只能看到 `$PK1` 无法判断异常类型。
- **修复目标**：
  - 异常类名白名单：`TypeError`/`SyntaxError`/`ReferenceError`/`RangeError`/`URIError`/`EvalError`/`Error` 这几个关键词**禁止**被字典化，保持字面量。
  - 其余「类名 + 方法 + 文件」仍然可以字典化。
- **具体 case**：`case_001_syntax_error`、`case_003_type_error`、`case_006_noise`、`case_007_single_line`、`case_009_special_chars`。

#### B.2.2 `python_traceback`（3/11 违反 G3）

- **根因**：`compress()` 把 `AssertionError: expected 42 got 41` 改写为 `$PK1: expected 42 got 41`，`KeyError: 'missing_key'` → `$PK1: 'missing_key'`，吞掉异常类名。
- **修复目标**：同 B.2.1，异常类名白名单保留。
- **具体 case**：`case_010_assertion`、`case_011_import_error`、`case_012_key_error`。

#### B.2.3 `smart_code`（2/11 违反 G3）

- **根因**：`smart_code` 是通用源码压缩插件，对 `try` / `catch` / `throw new Error(...)` / `Exception` 这些关键词也会字典化。
- **修复目标**：
  - 异常类名 + 错误抛出词（`throw`、`raise`、`panic!`）加入保留白名单。
  - 或者：在 `compress()` 的后置校验里做 G3 检查，命中时回退单行不字典化。
- **具体 case**：`case_002_code_error`、`case_011_stack_trace`。

#### B.2.4 `java_stack`（1/11 违反 G3）

- `case_003_long_stack`（27.6% 压缩率，但 G3 失败）：`Exception in thread "main" java.lang.NullPointerException: ...` 被压缩后 `NullPointerException` 字面量消失。
- **修复目标**：Java 异常类后缀白名单——类名以 `Exception` / `Error` 结尾的保留字面量，不字典化。

### B.3 验证流程

同族群 A。**额外要求**：修复后运行 `honest_audit_non_vcs.ps1` 确认该插件的 `g3_pass == cases`，才算族 B 合格。

---

## 族群 C — 结构化日志族

**成员**：`syslog`、`db_log`、`web_log`、`nodejs`、`spring_boot`、`kubernetes_docker`
**共同特征**：系统/应用运行日志。有显式的行级结构（时间戳 + 级别 + 进程/线程 + logger + 消息），重复度高。
**核心战术**：按行解析 → 关键字段保留 + 重复前缀字典化 → **严格 ROI 门控**。

### C.1 共同压缩约束

1. **时间戳强规范化**（法则 B）：
   - `syslog` 格式：`Jan 15 10:30:45 host ...` → `01-15 10:30:45 host ...` 或直接去掉年份外的冗余
   - `db_log` PG 格式：`2024-01-15 10:30:45.123 UTC [1234] LOG:` → `2024-01-15 10:30:45 [1234] LOG:`（去微秒 + 时区）
   - `web_log` common format：`[15/Jan/2024:10:30:45 +0800]` → `2024-01-15 10:30:45`
   - `spring_boot`：`2024-01-15 10:30:45.123 INFO 12345 --- [main] ...` → 保留时间 + 去微秒
   - `nodejs` / `kubernetes_docker`：依各自 format 适配
2. **重复前缀字典化**：
   - syslog 的 `hostname[pid]:` 前缀、PG 的 `[pid-N] LOG:` 前缀、spring_boot 的 `o.s.b.w.embedded.tomcat.TomcatWebServer` logger 名字——这些高频重复字段用 `dict_engine.add_macro` 或 `dict_engine.add_package` 字典化。
3. **日志级别保留原样字面量**：`INFO`/`WARN`/`ERROR`/`DEBUG`/`FATAL` 是 LLM 过滤关注点的关键词，**禁止字典化**。
4. **IP/MAC/UUID 等标识符**：可字典化为 `$Dn` Token，但同族内复用一致（IP 用 `$IPn`、UUID 用 `$Un`）。
5. **SQL 语句**（db_log 特有）：作为运行日志嵌入的 SQL 语句，保留关键字（`SELECT`/`UPDATE`/`INSERT`/`DELETE`/`WHERE`/`FROM`/`JOIN`）字面量，数据值可字典化。
6. **ROI 门控**：必须用 `prefer_non_expanding`。本族是问题重灾区——`syslog` 11/11 扩张、`db_log` 11/11 扩张。

### C.2 已知问题（真实 needs_fix case）

#### C.2.1 `syslog`（11/11 扩张 — 系统性缺陷）

- **根因**：`compact_syslog_line()` 对每行 `Jan 15 10:30:45 hostname authd[1234]: ...` 做字段级重写，把 `Jan 15` 规范化 + `hostname[1234]:` 字典化。但插件自造的 IR 字段分隔符（例如 `$SYS|`、`$SYSPFX|`）+ 字典 token 的总长度略大于原字段。每行扩张 0.5-2 字节，整段累积为 -1% 到 -7%。
- **修复目标**：
  - 检查 `compact_syslog_line` 是否调用 `prefer_non_expanding(raw_line, compact_line)`，没有就加上，单行扩张时回退到原文。
  - 字典化阈值：只有当原文件总字节 `>500B` 时才做 hostname/pid 字典化，小样本直接回退。
- **具体 case**：全部 11 个。

#### C.2.2 `db_log`（11/11 扩张 — 系统性缺陷）

- **根因**：同 syslog。`[2024-01-15 10:30:45.123 UTC] [pid-1234] LOG: query ...` 行级重写后平均每行扩张 1-3 字节。
- **修复目标**：
  - `compact_db_log_line` 加 `prefer_non_expanding`。
  - PostgreSQL 的 `[pid-N]` 和 MySQL 的 `[Note]`/`[Warning]` 前缀专项处理——这些是重复度最高的字段，应该在首次出现时建立字典 Token，后续出现时引用（如果整段 ROI 能正）；无法保证 ROI 时整段回退。
- **具体 case**：全部 11 个。

#### C.2.3 `web_log`（1/11 扩张）

- `case_011_no_compress`（-1.1%）：非标准日志格式样本，单行 fast-path 问题。
- **修复目标**：短样本（`<150B`）或非命中主解析路径的样本直接 `prefer_non_expanding` 回退。

#### C.2.4 `nodejs`、`spring_boot`、`kubernetes_docker`（0 需优化）

- 已全过。`spring_boot` 建议 2 的 `(?m)` 正则修复让 detect 和 compress 都工作正常；`kubernetes_docker` 的弱特征补强让三种真实样本（pod-hash / table / cloudwatch）都正确进入字典化路径。

### C.3 验证流程

同族群 A。**额外要求**：本族必须在插件源码里显式调用 `crate::core::prefer_non_expanding`；不调用的插件必须加上。

---

## 族群 D — 序列化格式族

**成员**：`json`、`yaml`、`xml_html`、`markdown`、`sql`
**共同特征**：结构化数据 / 标记语言。压缩后必须能无损还原成结构等价的原始文档。
**核心战术**：结构等价保证 + 重复键 / 路径字典化 + **短样本兜底透传**。

### D.1 共同压缩约束

1. **解压往返结构等价**：
   - JSON：`serde_json::from_str(decompress(compress(raw))) == serde_json::from_str(raw)` 必须成立。
   - YAML：同，用 `serde_yaml::Value` 比较。
   - XML / HTML：tag 结构 + 属性等价。
   - Markdown：渲染后结构等价（允许空白顺序差异）。
2. **重复键 / 标签字典化**：
   - JSON：`"timestamp_iso"`、`"requestId"` 等高频键名用 `dict_engine.add_macro`。
   - YAML：同。
   - XML：重复 tag 名字可字典化。
3. **链接 / URL / 长字符串值字典化**：
   - Markdown：`[text](https://example.com/very/long/path)` 把 URL 部分走 `add_path_layered`。
   - JSON：长 URL 字符串值用 `add_path_layered`。
4. **SQL 特殊**：关键字保留字面量（同族 C.1.5），字符串字面量和数字值**不字典化**（保真数据）。
5. **短样本兜底**（法则 A 的 ROI 门控在短样本上尤其重要）：
   - JSON `{}` 只有 2 字节，加 `$JSON|` 前缀就是 6 字节，扩张 200%。
   - YAML 单行 `key: value` 同理。
   - **强制规则**：`raw.len() < MIN_COMPRESSION_SAMPLE_BYTES`（建议 50）时直接原文返回，不进压缩流程。

### D.2 已知问题（真实 needs_fix case）

#### D.2.1 `json`（2/12 扩张）

- `case_007_single_line`（18B → 23B，-37.5%）：单行 JSON 被加 `$JSON|` 前缀扩张。
- `case_008_empty`（4B → 10B，-350%）：空 JSON `{}` 或 `[]` 扩张极严重。
- **修复目标**：`compress()` 入口检测 `raw.len() < 50` 直接返回 `CompressResult { tokens: vec![Token::Text(Cow::Borrowed(raw))], .. }`，不进字典化流程。

#### D.2.2 `yaml`（3/11 扩张）

- `case_001_simple_yaml`（61B → 68B，-11.7%）：短样本 `$YAML|\n` 前缀扩张。
- `case_003_config`（178B → 181B，-1.7%）：轻微。
- `case_006_single_line`（12B → 19B，**-80%**）：单行 YAML 同 json 单行问题。
- **修复目标**：同 D.2.1。`raw.len() < 80` 直接透传。

#### D.2.3 `xml_html` / `markdown` / `sql`（0 需优化）

- 已全过。

### D.3 验证流程

同族群 A。**额外要求**：本族必须在 test.rs 里维持「解压等价」断言，防止 fast-path 兜底绕过压缩时破坏往返语义。

---

## 族群 E — 通用工具族

**成员**：`ansi_cleaner`、`generic_text`、`noise_filter`、`smart_path`、`static_rule`、`template_driven`
**共同特征**：非针对特定格式的兜底 / 清理工具。作为其他插件的前置或后置 chain 节点。
**核心战术**：最小副作用 + 保真兜底。

### E.1 共同压缩约束

1. **Utility 插件必须可幂等叠加**：被 chain 多次调用时输出稳定。
2. **不得引入新 IR 标签**：`ansi_cleaner` 剥完 ANSI 直接输出清洁文本，不加 `$ANSI|` 前缀；`smart_path` 替换路径为 `$Pn` 属于例外（字典共享）。
3. **静态规则 / 模板**：空配置时等价于透传。
4. **短样本兜底**：同族 D.1.5。

### E.2 已知问题（真实 needs_fix case）

#### E.2.1 `ansi_cleaner`（3/11 需优化）

- `case_005_no_ansi`（-1%）：原文无 ANSI，剥离步骤加了一个末尾换行或空白差异，扩张 1 字节。
  - **修复目标**：`strip_ansi` 入口检查 `text.contains(0x1B) == false` 时直接返回原文，不走替换流程。
- `case_009_ansi_escape_only`（100% 压缩率，但违反 G4）：输入全是 ANSI 控制码（98B），剥离后 `compact` 为空（仅 2 字节换行）。
  - **修复目标**：输出纯空时附加标记行 `[stripped: N ANSI bytes]` 让 LLM 知道被处理过；或者在剥离后输出用 `compact.trim().is_empty()` 检查，触发时保留一个语义标记。
- `case_012_ansi_no_compress`（-1.2%）：同 case_005。

#### E.2.2 `generic_text` / `noise_filter` / `smart_path` / `static_rule` / `template_driven`（0 需优化）

- 已全过。保持现有实现。

### E.3 验证流程

同族群 A。**额外要求**：本族作为 chain 节点时，需确认下游插件（如 `smart_path` 后常接 `rust_go`）输出的 compact_hash 不受副作用影响。

---

## 族群 F — 特定场景族

**成员**：`webpack_vite`、`unity_unreal`、`git_diff`
**共同特征**：场景高度特殊，既没有运行时错误的强 G3 保留需求，也没有结构化日志的批量行级重写。战术以「识别专有结构 + 针对性折叠」为主。
**核心战术**：专有结构识别 + 重复段聚合 + 常规 ROI 门控。

### F.1 专有战术

#### F.1.1 `webpack_vite`

- **专有结构**：asset 表格 `dist/... N KiB [emitted]` 和资源大小列。
- **压缩策略**：连续 asset 行（≥3 行）聚合为 `[assets: N files, total M KiB]`。
- **noise**：`Version: webpack 5.x.x`、`Time: NNNms`、`Built at: YYYY-MM-DD HH:MM:SS` 可折叠。
- **已知问题**：`case_006_single_line`（-1.1%）短样本加 fast-path。

#### F.1.2 `unity_unreal`

- **专有结构**：
  - Unreal log：`[YYYY.MM.DD-HH.MM.SS:mmm][frame]LogCategory: Level: message`
  - Unity log：`Unloading N unused assets`、`Building AssetBundle for platform X`
- **压缩策略**：
  - 时间戳规范化同族 C。
  - Unreal 的 LogCategory 高频复用，用 `add_macro` 字典化。
  - Unity 的 `Unloading` 多行合并。
- **已知问题**：无 needs_fix。

#### F.1.3 `git_diff`

- **专有结构**：Git diff 头（`diff --git`、`--- a/`、`+++ b/`、`@@ -X,Y +X,Y @@`）。
- **压缩策略**：
  - **头部必须原样保留**（参考 VCS `git_tactical_prompt.md`），不字典化路径前缀 `a/` 和 `b/`。
  - Hunk 内容可字典化但保持行级结构。
  - 法则 F 爆栈防御：单文件 diff 超 `MAX_DIFF_LINES`（默认 100）截断并附加 `<TRUNCATED>`。
- **已知问题**：`case_009_single_line_change`（-0.8%）—— 单行 diff 加头部上下文扩张。
  - **修复目标**：diff 总行数 ≤ 3 时直接 `prefer_non_expanding` 回退原文。

### F.2 验证流程

同族群 A。

---

## 2. 交付要求

1. **每次改动说明影响到的 case**（case_id 级精细度）。
2. **每次改动附回归结果**：
   - `cargo test --lib` 通过
   - `tmp/honest_audit_non_vcs.ps1` 该插件的 `g1_pass / g2_pass / g3_pass / g4_pass / all_pass` 各项值
   - `needs_fix` 数量相对之前的变化
3. **出现回归先修回归**再继续新优化。
4. **已冻结且未变化的 case 不重复消耗审计时间**。
5. **所有 Rust 注释必须中文**。
6. **交付 PR 描述模板**：
   ```
   族群: <A-F>
   插件: <plugin_name>
   修复 case 数: <before needs_fix> → <after needs_fix>
   修复策略: <简短描述>
   回归: cargo test OK / honest_audit G1-G4 全通过
   冻结: 新增冻结 case: case_XXX, case_YYY, ...
   ```

## 3. web_log v2 语义聚合补充

`web_log_plugin` 从 v2 开始不再只做逐行 access log 缩写，而是对 nginx、Apache、Ingress、ALB/Cloudflare/GCP/Azure/OCI 包装后的 Web access log 进行语义聚合。该补充受 `CLAUDE.md` 的 Non-VCS 语义聚合原则约束。

### 4.1 必须保留的统计维度

- `SUMMARY`：总记录数、时间窗口、`2xx/3xx/4xx/5xx/other` 分布、唯一 IP、唯一 URL、唯一 User-Agent、总 bytes、来源摘要。
- `TOP_URL`：按 `METHOD + normalized route` 统计。query string 可剥离，长数字 ID、UUID、长 hex segment 应归一成 `:id`。
- `TOP_IP`：高频访问 IP 必须可见，用于识别扫描、攻击源、健康检查源和流量热点。
- `TOP_UA`：User-Agent 必须可见，用于区分浏览器、健康检查、bot、curl、sqlmap 等流量。
- `STATUS` / `METHOD`：状态码和 HTTP 方法分布必须可见。
- `TOP_REF`：若原始日志包含 referer，需输出高频 referer。

### 4.2 异常与样本红线

- 任意 `4xx/5xx` 必须以 `!$W|ANOMALY` 或等价异常行突出，不能只藏在 summary 计数里。
- 慢请求字段（如 `request_time`、`upstream_response_time`、GCP `latency`）命中阈值时必须输出 `!$W|SLOW`。
- 异常行至少包含状态码、方法、归一化 URL、命中次数、IP 摘要、原因和代表性 sample。
- 对短样本或聚合后更长的样本必须服从 `prefer_non_expanding`，允许回退原文，但 showcase 中新增的 v2 case 应使用足够真实的批量规模，避免只测试回退路径。

### 4.3 case 覆盖矩阵

web_log v2 新增或修改 case 时必须覆盖：

- nginx combined/common：健康检查聚合、404 扫描、5xx 爆发、慢请求、静态资源、route id 归一化。
- Apache common/combined：缺 UA 的 common 格式与含 UA/referer 的 combined 格式。
- Ingress / Kubernetes Nginx：HTTP/2、pod 内网 IP、`kube-probe`。
- 云包装：AWS logs tail、CloudWatch table、AWS CSV、GCP `httpRequest` JSON、GCP JSON message、Azure CSV message、OCI JSON message、Cloudflare JSON/CSV。
- 每个新增 case 必须进入 `showcase.rs`，并至少有一个测试断言覆盖统计字段、异常保留、ROI 或格式识别。

### 4.4 web_log v3 Access Log 三层漏斗

`web_log_plugin` v3 将 access log 从“逐行缩短”升级为“统计聚合优先”的 LLM 决策层。压缩目标不是只省 token，而是把 LLM 不擅长的计数、分组、异常隔离提前在 Rust 层完成。

- 第一层 `DICT_IP` / `DICT_UA`：把高频 IP 和 User-Agent 映射为稳定别名，并标注 Internal、Health、Bot/Script、Browser 等类别。
- 第二层 `ROUTINE`：折叠健康检查、静态资源、普通 2xx/3xx 流量，输出 count、IP/UA 集合和 avg_ms。短样本或输出变长时仍必须服从 ROI 回退。
- 第三层 `SCAN` / `BURST` / `SLOW` / `ANOMALY`：404/403 敏感路径扫描、5xx 集中爆发和慢请求必须单独高亮，不允许被 routine 平均掉。
- `web_log_plugin` 负责 access-log 语义聚合；`cloud_log_plugin` 只做云厂商 wrapper 剥壳和通用云日志摘要，不重复实现 access-log 专属统计。
- v3 case 最少覆盖健康检查降噪、静态资源、404 敏感扫描、5xx burst、bot/UA 分类、慢请求、route id 泛化、ALB 原生日志和云包装脱壳后的 access log。

当前 v3 冻结基线：`web_log_plugin` 44/44 case 通过并冻结，新增 case_037 至 case_044 覆盖三层漏斗。

## 4. 回应口令

"首席架构师，我已接管非 VCS 插件审计上下文。通用宪法、4 闸门审计规则、族群 <X> 战术约束已加载。当前 needs_fix case 数 = <N>，目标 = <M>。请下达任务。"
