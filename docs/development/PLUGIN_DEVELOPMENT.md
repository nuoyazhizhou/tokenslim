# TokenSlim 压缩协议 V1（公开版）

> 本文件面向**贡献者**：描述 TokenSlim 用来压缩各种 VCS / 构建 / 日志输出的规则集合。  
> 实际源代码实现见 `src/core/` 和 `src/plugins/<plugin>_plugin/`。  
> 改动任何 parser / rule 后，请遵循 `docs/development/TESTING.md` 的回归流程。

---

## 1. 绝对锚点守卫（Anchor Guard）

任何 parser 都不允许**过滤或截断**原始输入的第一行触发命令（如 `git log`、`p4 opened`、`svn status`）。它必须作为 IR 输出的**绝对第一行**保留——下游 LLM 用这一行作为意图坐标系，丢失即崩塌。

实现位置：每个 plugin 的 `parse` / `compact` 入口的最前面。

---

## 2. 压缩符号参考表

各工具保留其原生状态符号；对冗长动作动词（如 P4 的 `opened for edit`、SVN commit 的 `Sending`）压缩为单字母状态码。**禁止自创新符号**：

| 符号 | 含义 |
|---|---|
| `M` | Modified |
| `A` | Added |
| `D` | Deleted |
| `R` | Renamed / Moved |
| `C` | Copied |
| `?` | Untracked |
| `U` | Unmerged / Conflict |
| `T` | Typechange |
| `I` | Ignored |
| `*` | 前缀替代 `current` / `active` / `checked out` |
| `!` | 前缀替代 `Conflict` / `Error` / `Failed` / `Rejected` / `fatal` |
| `->` | 流向（push / pull / merge） |

---

## 3. 路径字典压缩与 ROI 门控

### 3.1 路径识别防污染

提取任何路径前必须用 `looks_like_vcs_path(path)` 过滤邮箱、URL、代码调用。**严禁在各 VCS 插件内部自行实现**——必须用 `crate::core` 共享版本。

### 3.2 字典替换闭环

1. 调用 `dict_engine.add_path_layered(path)` 收集路径。
2. 用 `rewrite_line_with_paths` 或 `compact_vcs_text_with_paths` 将正文路径替换为 `$P` Token。
3. 必须调用 `append_inline_path_dictionary` 组装输出首部字典行（格式 `[paths] $Pn=prefix`），内部用 `apply_parent_prefix_aliases` 自动实现长前缀降维。

### 3.3 绝对 ROI 门控

任何 `compact` 入口返回前必须用 `prefer_non_expanding(raw, compacted)` 包裹，确保文本体积**不增反降**。

---

## 4. 时间强规范化

- 剥离星期（`Fri`）、月份英文（`Apr`）、微秒（`.000`）、时区（`+0800`）。
- 强制转为紧凑数字格式 `YYYY-MM-DD HH:MM:SS`（或 `HH:MM`）。
- 相对时间（`2 weeks ago`）保留原样（已是最紧凑形式）。

---

## 5. ANSI 净化

文本处理第一步**彻底剥离**所有 ANSI 逃逸序列和终端颜色代码。

---

## 6. 防失忆红线（Anti-Amnesia）

- 压缩 `log` 类输出时**绝对不允许丢弃 Commit Message**，必须拼接到单行。
- **一维化拍扁**：松散多行结构必须彻底拍扁——每个 Commit/Record 及其所有字段紧凑在单行输出，禁止一个字段占一行。
- **绝对禁止**吞掉 error/fatal 输出——这是 LLM 决策的关键信号。
- **ROI 自适应形态**：字段标签（`CH:`/`OW:`/`DT:`/`CM:`）仅为可选压缩手段，不是硬性格式；必须服从 `prefer_non_expanding`，标签化导致膨胀时自动退回更短表达（如原生 `oneline` / 无前缀紧凑行）。

---

## 7. Commit Hash 统一原则

- 默认**不输出** 40 位全长 hash，优先用"**仓库内最短唯一前缀**"。
- 默认长度 10（或 12）；冲突时按 2 位步长加长（12 → 14 → 16）直到唯一。
- 仅机器回放、跨仓库引用、审计留痕等场景强制保留全长 hash。

---

## 8. 零容忍废话与空状态歧义

- 拦截进度输出：`Counting objects`、`Receiving objects:`、`Resolving deltas:`、`Compressing objects:`、`Transmitting...` 等。
- 碾碎视觉噪音：连续空格（` {2,}`） / Tab 压缩为单空格；去除重复 `---` / `+++`。
- **Diff 文件头降维**：各类混乱 Diff 分隔头（Git 的 `diff --git a/... b/...`、P4 的 `==== //depot/... ====`、SVN 的 `Index: ...`）统一拦截，降维替换为标准 `DIFF://<压缩后路径>`。
- **空状态防歧义**：清理废话后若仅剩命令锚点，必须显式追加 `ST:[CLEAN]`。

---

## 9. Diff 爆栈防御

- 识别并跳过二进制文件差异（`Binary files differ`）。
- 单文件超 `MAX_DIFF_LINES`（默认 100）行的 Diff，强制截断并追加 `<TRUNCATED>`。

---

## 10. 邮箱降维

提取作者时正则丢弃邮箱域名后缀，仅提取 `@` 前完整前缀（`alice.chen@domain.com` → `@alice.chen`）。保留完整前缀防大型项目重名碰撞。

---

## 11. 测试架构铁律

- **严禁 Hardcode**：禁止代码里手写长字符串 Mock 输入。
- **动态加载**：用 `std::fs::read_to_string("samples/<plugin>/xxx.log")` 或 `include_str!`。
- **禁止重复实现测试辅助函数**：`sample_dir()` 和 `read_case()` 严禁在各 VCS 插件内部重复实现，必须统一提取到 `crate::test_utils` 或对应插件的 `#[cfg(test)] mod helpers` 共享。
- **回归防线**：每个新增 Parser 至少 1 个 sample case + 1 个断言测试；修改已有 Parser 先确认现有测试全部通过。

---

## 12. Non-VCS 语义聚合原则

非 VCS 插件目标不是单纯缩短每一行，而是在 ROI 门禁下生成 LLM 可直接决策的语义结构。

- 高重复运行日志、Web access log、云日志脱壳后的内层日志，允许 `SUMMARY / TOP / ANOMALY / SAMPLE` 形态聚合，但必须保留错误、异常、4xx/5xx、panic、fatal、slow request 等关键锚点。
- Access log 聚合优先用三层漏斗：`DICT_IP / DICT_UA` 做重复维度字典，`ROUTINE` 折叠健康检查 / 静态资源 / 普通 2xx，`SCAN / BURST / SLOW / ANOMALY` 单独高亮。该能力归属 `web_log_plugin`，不得混入通用日志或云厂商剥壳插件。
- 聚合输出必须遵守防失忆红线：少数异常不得被多数健康请求、进度行或 2xx 流量淹没；至少输出异常类别、命中次数、关键维度、代表性样本。
- 云厂商日志按"剥壳优先"：外层包装可压缩；脱壳后内层日志交给对应语义插件复用，不在云插件里重复实现专用聚合。
- SARIF / JUnit XML 等构建产物不得直接依赖通用 JSON / XML 插件；先由 `artifact_summary_plugin` 提取决策信号。
- 新增聚合形态必须补 showcase case、断言测试、审计冻结。
- 真实边界样本扩充走定向补强：先依据 `tokenslim run --explain-route` / `tokenslim explain-plugin` / 审计镜像定位薄弱点，再补最小必要 case，不得按 case 数盲目堆样本。
