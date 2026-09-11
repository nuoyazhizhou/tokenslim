# 传统 VCS 工具专属战术提示词
# ② Bzr | ③ CVS | ④ Fossil | ⑤ Darcs

> **使用方式**：本文档结合 TokenSlim 项目的 `CLAUDE.md`（Compression Protocol V1）一起使用。  
> `CLAUDE.md` 是通用宪法，本文档是工具专属战术补充。  
> 执行任何修改前必须先读取 `CLAUDE.md`，再读取本文档对应章节。

---

# ② Bazaar (Bzr) 专属战术提示词

## 代码结构

```
src/plugins/vcs_bzr_plugin/
├── parser.rs    549 行  — 解析器（含 8 个 Parser + 内联辅助函数）
├── methods.rs   17KB    — 压缩逻辑（compact_bzr_* 函数）
├── showcase.rs  3.4KB   — 测试用例展示
└── tests.rs     4.8KB   — 测试用例
```

## Parser 覆盖列表

| Parser | 触发命令 | 当前状态 |
|--------|---------|---------|
| `BzrStatusParser` | `bzr status` / `bzr st` | ✅ 委托 generic |
| `BzrDiffParser` | `bzr diff` | ✅ 委托 generic |
| `BzrLogParser` | `bzr log` | ✅ 委托 generic |
| `BzrPullParser` | `bzr pull` | ✅ 专用：压缩为 `pull N/T revs` |
| `BzrPushParser` | `bzr push` | ✅ 专用：压缩为 `push target` |
| `BzrMergeParser` | `bzr merge` | ✅ 专用：压缩为 `merge source` |
| `BzrResolveParser` | `bzr resolve` | ✅ 专用：列出 `resolved: path` |
| `BzrBranchParser` | `bzr branch` | ✅ 专用：压缩为 `branch target N revs` |

## 已知问题与修改指令

### 问题1：三个重复的 `#[tracing::instrument]` 属性
**位置**：`parser.rs` 第 126-128 行附近（Flash 修复时可能引入）

**检查**：在 `fn looks_like_vcs_path` 前是否有多个相同的 `#[tracing::instrument(level = "debug", skip_all)]`。

**修复**：保留且只保留一个。

### 问题2：Bzr 日期格式未规范化

**Bzr log 的原生日期格式**：
```
timestamp: Fri 2024-01-15 10:30:00 +0800
```

**当前 `parse_generic_log_for_tool` 行为**：将 `timestamp:` 后的内容整体放到 `VcsRecord::Date()`，但 `compact_log_date_value` 是直接 pass-through，没有剥离星期和时区。

**修复目标**：在 `compact_log_date_value` 中（或在 Bzr 专用解析路径里）实现：
- 输入：`Fri 2024-01-15 10:30:00 +0800`
- 输出：`2024-01-15 10:30:00`（剥离星期 + 时区）

### 问题3：Bzr status 原生词语压缩缺失

**Bzr status 原生输出**（单词格式，不是单字母）：
```
bzr status
modified:
  src/main.rs
  src/lib.rs
added:
  src/new_feature.rs
unknown:
  temp.log
```

**当前行为**：`parse_status_word_and_path` 能处理 `modified: path`（冒号后跟路径）这种格式，但 Bzr 实际上是 section 标题 + 缩进路径的格式，这种情况会被当成 `Raw` 落底。

**审查项**：运行 `bzr status` 样本，确认压缩率是否正常。如果 `modified:` 被识别为 Section 而路径被正确提取为 File，则 OK。

### 问题4：`looks_like_vcs_path` 版本检查

读取 `parser.rs` 中的 `looks_like_vcs_path` 函数，确认它包含全部 6 道过滤屏障（参见通用 Delta ① 变更 B）。

## 验证
```bash
cargo test vcs_bzr
cargo check
```

---

# ③ CVS 专属战术提示词

## 代码结构

```
src/plugins/vcs_cvs_plugin/
├── parser.rs    862 行  — 解析器（含 8 个 Parser + CVS 专用辅助函数）
├── methods.rs   15.9KB  — 压缩逻辑
├── showcase.rs  3.4KB   — 测试用例展示
└── tests.rs     4.3KB   — 测试用例
```

## Parser 覆盖列表

| Parser | 触发命令 | 当前状态 |
|--------|---------|---------|
| `CvsStatusParser` | `cvs status` / `cvs update` | ✅ 委托 generic |
| `CvsDiffParser` | `cvs diff` | ✅ 委托 generic |
| `CvsLogParser` | `cvs log` | ✅ 委托 generic |
| `CvsAnnotateParser` | `cvs annotate` | ✅ 专用：有连续相同行去重逻辑 |
| `CvsUpdateParser` | `cvs update` | ✅ 专用：识别 U/A/R/M/D/C/?/! 单字母+路径 |
| `CvsCommitParser` | `cvs commit` | ✅ 专用：`Checking in file.c; → 1.5` 格式 |
| `CvsTagParser` | `cvs tag` | ✅ 专用：`T path` → `tag(tagname): path` |
| `CvsEditParser` | `cvs edit` | ❓ 检查是否实现或为 pass-through |

## CVS 特殊概念

- **版本号格式**：CVS 使用 `1.2.3.4` 形式的修订号（不是哈希），由 `looks_like_cvs_revision_token()` 识别
- **`U` 状态码语义**：CVS `U` = "Updated from server"（服务器覆盖本地），不同于 Git 的 `U`（Unmerged）
- **Annotate 格式**：`*** 1.5 (alice:2024-01-15): code line` — 包含版本、作者、日期

## 已知问题与修改指令

### 问题1：`compact_log_date_value` 是 pass-through

**位置**：`parser.rs` 约第 455 行：
```rust
fn compact_log_date_value(value: &str) -> String {
    value.trim().to_string()  // ← 完全没有规范化！
}
```

**CVS log 原生日期格式**：
```
date: 2024/01/15 10:30:00;  author: alice;  state: Exp;
```

**修复目标**：
1. 截取 `date:` 后的日期部分，剥离 `;` 后面的 author/state 等字段
2. 将 `2024/01/15 10:30:00` 规范化为 `2024-01-15 10:30:00`（用 `-` 代替 `/`）

### 问题2：Annotate 缩进压缩算法审查

`compact_blame_code_indent` 函数将代码缩进减半（保留 `levels / 2` 层），审查这个压缩是否太激进：
- 4 层缩进 → 2 层：合理
- 1 层缩进 → 0 层：代码行与无缩进行混在一起，可能导致可读性问题

**建议**：最少保留 1 层缩进（`kept_levels.max(1)`，当 `levels > 0` 时）。

### 问题3：CVS commit 消息未被压缩

`CvsCommitParser` 当前流程：
```
"Checking in src/main.c;" → pending_path
"1.5"                     → pending_revision  
"Log message here"        → Subject(subject)
```

检查 `Subject` 字段是否在 `compact` 函数中被正确拼接到单行输出（法则 D：一维化拍扁）。

### 问题4：`CvsEditParser` 检查

读取 `methods.rs` 中 `compact_cvs_edit_for_ai` 函数（如存在），确认是否有实质压缩，还是 pass-through。

## 验证
```bash
cargo test vcs_cvs
cargo check
```

---

# ④ Fossil 专属战术提示词

## 代码结构

```
src/plugins/vcs_fossil_plugin/
├── parser.rs    382 行  — 解析器（9 个 Parser + 内联辅助函数）
├── methods.rs   14KB    — 压缩逻辑
├── showcase.rs  3.3KB   — 测试用例展示
└── tests.rs     4.5KB   — 测试用例
```

## Parser 覆盖列表

| Parser | 触发命令 | 当前状态 |
|--------|---------|---------|
| `FossilStatusParser` | `fossil status` | ✅ 委托 generic |
| `FossilChangesParser` | `fossil changes` | ⚠️ 与 StatusParser 完全相同 |
| `FossilDiffParser` | `fossil diff` / `gdiff` | ✅ 委托 generic |
| `FossilLogParser` | `fossil log` | ✅ 委托 generic |
| `FossilTimelineParser` | `fossil timeline` | ⚠️ 与 LogParser 完全相同 |
| `FossilUndoParser` | `fossil undo` | ✅ 委托 generic（undo 输出很简单） |
| `FossilStashParser` | `fossil stash` | ✅ 委托 generic |
| `FossilMergeParser` | `fossil merge` | ✅ 委托 generic |
| `FossilSyncParser` | `fossil sync` | ✅ 委托 generic |

## Fossil 特殊概念

- **检入哈希**：Fossil 使用 40 位 SHA1（与 Git 格式相同），但命令是 `fossil info`
- **状态词格式**（关键差异）：Fossil status 输出的是**全大写单词**（不是单字母）：
  ```
  EDITED     src/main.rs
  ADDED      src/new_feature.rs  
  DELETED    src/old.rs
  MISSING    docs/readme.md
  UNCHANGED  src/lib.rs
  ```
- **Timeline 格式**（独特）：
  ```
  2024-01-15 10:30:00 [abc1234def5] branch: main  commit message here
  ```

## 已知问题与修改指令

### 问题1：Fossil 状态词未能识别（关键 Bug）

**当前解析路径**：`parse_generic_status_for_tool` → `parse_simple_status_path` 期待 `M path` 格式 → **完全无法识别 `EDITED src/main.rs`**！

**修复**：在 `parse_status_word_and_path` 的候选词列表中，添加 Fossil 的大写状态词：

```rust
("EDITED ", 'M'),
("ADDED ", 'A'),
("DELETED ", 'D'),
("MISSING ", '!'),
("RENAMED ", 'R'),
// UNCHANGED → 可以直接 skip（不输出）
```

同时在 `parse_generic_status_for_tool` 中，添加对 `UNCHANGED` 行的过滤（节省 token，干净状态不需要输出）：
```rust
if trimmed == "UNCHANGED" || lower.starts_with("unchanged ") {
    continue; // 跳过未修改文件
}
```

### 问题2：Timeline 格式未专门解析

**当前行为**：`FossilTimelineParser` 委托 `parse_generic_log_for_tool`，而该函数无法识别 Timeline 格式（`2024-01-15 10:30:00 [hash] branch: main  message`），整行被当成 `Raw` 落底。

**修复**：添加 Fossil Timeline 专用解析函数：
```rust
// 检测格式：YYYY-MM-DD HH:MM:SS [hash8-40] anything
fn parse_fossil_timeline_line(line: &str) -> Option<VcsRecord> {
    // 提取日期（前19字符）、哈希（方括号内）、消息（剩余部分去掉branch:前缀）
}
```

期望压缩输出：
```
2024-01-15 10:30 [abc1234] fix login bug
```

### 问题3：`FossilChangesParser` 与 `FossilStatusParser` 重复

这两个 Parser 的实现完全一样。如果 `fossil changes` 的实际输出与 `fossil status` 不同（changes 只显示修改文件），保持两个是合理的（都委托 generic 问题不大）。但如果输出格式完全相同，可以考虑合并。**暂不强制修改，留作审查项。**

## 验证
```bash
cargo test vcs_fossil
cargo check
```

---

# ⑤ Darcs 专属战术提示词

## 代码结构

```
src/plugins/vcs_darcs_plugin/
├── parser.rs    412 行  — 解析器（7 个 Parser + 内联辅助函数）
├── methods.rs   18.3KB  — 压缩逻辑
├── showcase.rs  3.3KB   — 测试用例展示
└── tests.rs     5.6KB   — 测试用例
```

## Parser 覆盖列表

| Parser | 触发命令 | 当前状态 |
|--------|---------|---------|
| `DarcsStatusParser` | `darcs status` / `darcs whatsnew` | ✅ 委托 generic |
| `DarcsDiffParser` | `darcs diff` | ✅ 委托 generic |
| `DarcsLogParser` | `darcs log` / `darcs changes` | ⚠️ 委托 generic（无法处理 Darcs 独特格式） |
| `DarcsRecordParser` | `darcs record` | ⚠️ 委托 log（不匹配） |
| `DarcsAmendParser` | `darcs amend` | ⚠️ 委托 log（不匹配） |
| `DarcsObliterateParser` | `darcs obliterate` | ⚠️ 委托 log（不匹配） |
| `DarcsWhatsnewParser` | `darcs whatsnew` | ✅ 委托 generic status |

## Darcs 特殊概念

- **Patch-based**：Darcs 没有 "commit hash"，用 Patch 的原始作者和时间作为唯一标识
- **Log 格式**（关键差异）：
  ```
  patch be4862f...（可选的哈希前缀）
  Author: Alice Example <alice@example.com>
  Date: Mon Jan 15 10:30:00 UTC 2024
    * Fix login bug
    This is a longer description.
  ```
  日期前缀是 `Date:` 而非 `timestamp:`，格式是完整英文月份
- **whatsnew 格式**：
  ```
  hunk ./src/main.rs 42
  ...patch content...
  ```
  `hunk path/to/file LINE` 是 Darcs 的 "changed file" 标记
- **record/amend 输出**：实际只输出 "What's the patch name?" 等交互提示或最终 "Finished recording patch 'name'"

## 已知问题与修改指令

### 问题1：Darcs log 日期格式未规范化（关键）

**原生格式**：`Date: Mon Jan 15 10:30:00 UTC 2024`

**当前行为**：`parse_generic_log_for_tool` 会匹配 `lower.starts_with("date:")` 并调用 `split_once(':').map(|(_, v)| v.trim())`，输出：`Mon Jan 15 10:30:00 UTC 2024`（未规范化）。

**修复**：在 Darcs 专用路径或 `compact_log_date_value` 中处理此格式：
- 输入：`Mon Jan 15 10:30:00 UTC 2024`
- 输出：`2024-01-15 10:30:00`

### 问题2：DarcsLogParser 无法解析 Darcs patch 格式

**Darcs log 真实格式**：
```
patch be4862f...
Author: Alice <alice@example.com>
Date: Mon Jan 15 10:30:00 UTC 2024
  * Fix login bug
```

**当前行为**：`parse_generic_log_for_tool` 只匹配 `commit` 前缀的 hash，会错过 `patch hash` 行。

**修复**：在 `DarcsLogParser` 的 parse 方法中（或添加专用辅助函数），识别 `patch ` 前缀的行，提取 hash 放入 `VcsRecord::Commit`：
```rust
if let Some(hash) = trimmed.strip_prefix("patch ") {
    records.push(VcsRecord::Commit(hash.trim()[..8.min(hash.trim().len())].to_string()));
    continue;
}
```

### 问题3：DarcsRecordParser、DarcsAmendParser 委托错误

`darcs record` 成功后输出：
```
Finished recording patch 'Fix login bug'
```

`darcs amend` 成功后输出：
```
Finished amending patch 'Fix login bug'
```

当前这两个 Parser 委托 `parse_generic_log_for_tool`，会把类似 log 格式的内容当作日志解析，但 record/amend 输出其实是单行摘要。

**建议修复**：专用 Parser 识别 `Finished recording` / `Finished amending` → `VcsRecord::Raw("recorded: 'patch name'")`

### 问题4：`parse_darcs_hunk_record` 路径提取精度

**`hunk ./src/main.rs 42`** → 当前提取 `./src/main.rs` 作为路径（去掉行号）。

**审查项**：确认 `./` 前缀是否被正确处理（应该去掉 `./` 保留 `src/main.rs`）。

## 验证
```bash
cargo test vcs_darcs
cargo check
```
