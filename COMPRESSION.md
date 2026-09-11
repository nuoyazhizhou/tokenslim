<!-- version: 2026-06-11 | 被 AGENTS.md 引用。改 VCS/插件 parser、rule 时先读本文件 -->

# Compression Protocol V1（跨 VCS 通用宪法）

> 身份、致命红线、规划概览见 `AGENTS.md`。本文件是完整执行宪法与压缩规则。

在执行任何 VCS 模块的压缩重构时，必须严格遵守以下法则。适用于全部 14 个 VCS 工具插件（Git/SVN/Hg/P4/CVS/Bzr/Fossil/Darcs/GH/GLab/AZ/Bitbucket/Repo/Gerrit），任何偏离都会导致重构失败。

## 法则 0：绝对锚点守卫 (Anchor Guard)

无论执行什么解析，**严禁**过滤或截断原始输入的第一行触发命令（如 `git log`、`p4 opened`、`svn status`）。必须强制提取并作为 IR 输出的绝对第一行。这是下游 LLM 的意图坐标系，丢失即崩塌。

## 压缩符号参考表

各工具保留其原生状态符号；对冗长动作动词（如 P4 的 `opened for edit`、SVN commit 的 `Sending`）压缩为单字母状态码。禁止自创新符号：

- 状态码：`M`=Modified, `A`=Added, `D`=Deleted, `R`=Renamed/Moved, `C`=Copied, `?`=Untracked, `U`=Unmerged/Conflict, `T`=Typechange, `I`=Ignored

- 活跃标记：`*` 前缀替代 `current` / `active` / `checked out`

- 异常标记：`!` 前缀替代 `Conflict` / `Error` / `Failed` / `Rejected` / `fatal`

- 流向符号：push/pull/merge 用 `->` 表示流向

## 脱敏占位符参考表（P3-209）

privacy 插件（及 sql_plugin `obfuscate_sensitive` 复用路径）输出**短占位符**：信息量为零
（不区分命中规则/出现次数、不携带原文），单向不可逆，**严禁**建立「占位符 → 原文」映射
或引入 `$` 字典命名空间的变体。占位符总数 ≥3 时输出尾部追加一行类型图例
（`[legend] [UR]=user-rule-redaction ... (irreversible)`，仅描述类型语义）：

- `[UR]`=用户自定义规则命中（`.tokenslim-redact.toml`；词表 `.tokenslim-redact-allowlist` 内的词整体命中时原样保留，内置规则不受词表影响，P3-210）
- `[SEC]`=通用凭证赋值（`key=/token=/password=` 及 `--flag` 形态）
- `[LLMKEY]`=LLM API Key
- `[AWSID]`/`[AWSKEY]`=AWS Access Key ID / Secret
- `[GHTOK]`=GitHub token；`[BEARER]`=Bearer 令牌；`[JWT]`=JWT
- `[DBCRED]`=连接串凭证（`scheme://[DBCRED]@host`）；`[PK]`=PEM 私钥块

## 法则 A：路径字典压缩与门控 (Path Dictionary & ROI)

- **路径识别防污染**：提取任何路径前必须用 `looks_like_vcs_path(path)` 过滤邮箱、URL、代码调用。**严禁在各 VCS 插件内部自行实现**，必须用 `crate::core` 共享版本。

- **字典替换闭环**：

  1. 调用 `dict_engine.add_path_layered(path)` 收集路径。
  2. 用 `rewrite_line_with_paths` 或 `compact_vcs_text_with_paths` 将正文路径替换为 `$P` Token。
  3. 必须调用 `append_inline_path_dictionary` 组装输出首部字典行（格式 `[paths] $Pn=prefix`），内部用 `apply_parent_prefix_aliases` 自动实现长前缀降维。

- **绝对 ROI 门控**：任何 `compact` 入口返回前必须用 `prefer_non_expanding(raw, compacted)` 包裹，确保文本体积不增反降。

## 法则 B：时间强规范化 (Time Normalization)

- 剥离星期（`Fri`）、月份英文（`Apr`）、微秒（`.000`）、时区（`+0800`）。

- 强制转为紧凑数字格式 `YYYY-MM-DD HH:MM:SS`（或 `HH:MM`）。

- 相对时间（`2 weeks ago`）保留原样（已是最紧凑形式）。

## 法则 C：ANSI 净化 (ANSI Stripping)

- 文本处理第一步彻底剥离所有 ANSI 逃逸序列和终端颜色代码。

## 法则 D：防失忆红线 (Anti-Amnesia)

- 压缩 `log` 类输出时**绝对不允许丢弃 Commit Message**，必须拼接到单行。

- **一维化拍扁**：松散多行结构必须彻底拍扁——每个 Commit/Record 及其所有字段紧凑在单行输出，禁止一个字段占一行。

- **绝对禁止**吞掉 error/fatal 输出——这是 LLM 决策的关键信号。

- **ROI 自适应形态**：字段标签（`CH:`/`OW:`/`DT:`/`CM:`）仅为可选压缩手段，不是硬性格式；必须服从 `prefer_non_expanding`，标签化导致膨胀时自动退回更短表达（如原生 `oneline`/无前缀紧凑行）。

## Commit Hash 统一原则

- 默认不输出 40 位全长 hash，优先用"**仓库内最短唯一前缀**"。

- 默认长度 10（或 12）；冲突时按 2 位步长加长（12→14→16）直到唯一。

- 仅机器回放、跨仓库引用、审计留痕等场景强制保留全长 hash。

## 法则 E：零容忍废话与空状态歧义

- 拦截进度输出：`Counting objects`、`Receiving objects:`、`Resolving deltas:`、`Compressing objects:`、`Transmitting...` 等。

- 碾碎视觉噪音：连续空格（   ` {2,}`）/Tab 压缩为单空格；去除重复 `---`/`+++`。

- **Diff 文件头降维**：各类混乱 Diff 分隔头（Git 的 `diff --git a/... b/...`、P4 的 `==== //depot/... ====`、SVN 的 `Index: ...`）统一拦截，降维替换为标准 `DIFF://<压缩后路径>`。

- **空状态防歧义**：清理废话后若仅剩命令锚点，必须显式追加 `ST:[CLEAN]`。

## 法则 F：Diff 爆栈防御 (Anti-Explosion)

- 识别并跳过二进制文件差异（`Binary files differ`）。

- 单文件超 `MAX_DIFF_LINES`（默认 100）行的 Diff，强制截断并追加 `<TRUNCATED>`。

## 邮箱降维规则

- 提取作者时正则丢弃邮箱域名后缀，仅提取 `@` 前完整前缀（`alice.chen@domain.com` → `@alice.chen`）。保留完整前缀防大型项目重名碰撞。

## 测试架构铁律

- **严禁 Hardcode**：禁止代码里手写长字符串 Mock 输入。

- **动态加载**：用 `std::fs::read_to_string("samples/<plugin>/xxx.log")` 或 `include_str!`。

- **禁止重复实现测试辅助函数**：`sample_dir()` 和 `read_case()` 严禁在各 VCS 插件内部重复实现，必须统一提取到 `crate::test_utils` 或对应插件的 `#[cfg(test)] mod helpers` 共享。

- **回归防线**：每个新增 Parser 至少 1 个 sample case + 1 个断言测试；修改已有 Parser 先确认现有测试全部通过。

## Non-VCS 语义聚合原则

- 非 VCS 插件目标不是单纯缩短每一行，而是在 ROI 门禁下生成 LLM 可直接决策的语义结构。

- 高重复运行日志、Web access log、云日志脱壳后的内层日志，允许 `SUMMARY / TOP / ANOMALY / SAMPLE` 形态聚合，但必须保留错误、异常、4xx/5xx、panic、fatal、slow request 等关键锚点。

- Access log 聚合优先用三层漏斗：`DICT_IP/DICT_UA` 做重复维度字典，`ROUTINE` 折叠健康检查/静态资源/普通 2xx，`SCAN/BURST/SLOW/ANOMALY` 单独高亮。该能力归属 `web_log_plugin`，不得混入通用日志或云厂商剥壳插件。

- 聚合输出必须遵守防失忆红线：少数异常不得被多数健康请求、进度行或 2xx 流量淹没；至少输出异常类别、命中次数、关键维度、代表性样本。

- 云厂商日志按"剥壳优先"：外层包装可压缩；脱壳后内层日志交给对应语义插件复用，不在云插件里重复实现专用聚合。

- SARIF/JUnit XML 等构建产物不得直接依赖通用 JSON/XML 插件；先由 `artifact_summary_plugin` 提取决策信号。

- 新增聚合形态必须补 showcase case、断言测试、审计冻结；审计时除 G1-G4 外还要人工确认聚合统计没吞掉异常信号。

- 真实边界样本扩充走定向补强：先依据 route replay / explain-plugin / 审计镜像定位薄弱点，再补最小必要 case，不得按 case 数盲目堆样本。

## 内容分类器：剥皮类别边界（Syslog / CiLog / CloudLog）

- **剥皮类别定义**：Syslog（系统守护）、CiLog（CI/CD 编排）、CloudLog（云平台包装）是"剥皮/脱壳"语义类别——内容分类器命中后路由到对应插件剥外层包装，内嵌的第三方输出（HTTP access、npm/test、java/python/node 栈、syslog/db 守护）由内层语义类别插件接管压缩。包装插件**不重复实现**内层聚合（隔绝于上文 web\_log 归属边界）。

- **语料聚合红线**：剥皮类别的语料**刻意不聚合**——不入 build.rs 的 `CATEGORY_SOURCES`，仅靠种子特征词判别。其语料是「包装 + 内嵌第三方输出」混合体，聚合会把内层词灌进包装桶，经 `SHARE_THRESHOLD` 全局改权殃及内层类别边界（实证：node 召回跌穿 24/37）。

- **皮检率预期差异**：剥皮类别召回天然不同，取决于"皮"的可检性——CLI/守护进程名专属且密集则皮好剥（Syslog 高、CiLog 26/44），皮字段折叠后过弱则被内层淹没（CloudLog 7/52）。召回门槛仅覆盖稳定的"皮阳性"；内嵌样本按语义被内层类别接管，**不属于漏检**。

- **新增剥皮类别的可分性判据**：只要皮可辨（皮检率 > 0 且稳定），即使低于内层类别，也应作为独立剥皮类别保留（保证与 CiLog/CloudLog 一致对待）；但前提是 seed-only 不聚合。**绝不**因"皮检低"为凑召回而聚合语料。

- 分类器候选插件映射与路由分支见 `model.rs` / `content_analyzer::bayesian_fallback`。

> Case 审计的即时决策/冻结纪律属于审计域，见 `AUDIT.md`。

