# ⑥ 云服务 CLI 统一战术提示词
# 适用于：GH / GLab / AZ / Bitbucket / Repo / Gerrit

> **使用方式**：本模板结合 `CLAUDE.md`（Compression Protocol V1）使用。  
> 以下 6 个插件共用本模板，因为它们有几乎相同的代码结构和相同的问题。  
> 执行时，将 `{TOOL}` 替换为具体工具名（gh/glab/az/bitbucket/repo/gerrit）。

---

## 插件现状一览

| 插件 | parser.rs | 当前 Parser 实现 | 压缩率 |
|------|-----------|----------------|--------|
| `vcs_gh_plugin` | 4.3KB，18个 Parser | ❌ **全部 pass-through** | 0% |
| `vcs_glab_plugin` | 2.1KB，N个 Parser | ❌ **全部 pass-through** | 0% |
| `vcs_az_plugin` | 2.0KB，N个 Parser | ❌ **全部 pass-through** | 0% |
| `vcs_bitbucket_plugin` | 2.1KB，N个 Parser | ❌ **全部 pass-through** | 0% |
| `vcs_repo_plugin` | 2.2KB，N个 Parser | ❌ **全部 pass-through** | 0% |
| `vcs_gerrit_plugin` | 2.0KB，N个 Parser | ❌ **全部 pass-through** | 0% |

所有 Parser 当前实现模式（GH 为例，其余雷同）：
```rust
impl VcsParser for GhPrListParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut r = vec![];
        for l in raw.lines() {
            let t = l.trim_end_matches('\r').trim();
            if t.is_empty() { continue; }
            r.push(VcsRecord::Raw(t.to_string()))  // ← 零压缩
        }
        to_doc_if_any(VcsTool::Gh, VcsDocKind::Log, r)
    }
}
```

---

## 云 CLI 工具的输出特征

这 6 个工具均不是传统 VCS，而是平台 API 的 CLI 封装。输出格式以**表格**为主：

### GH (GitHub CLI) 典型输出格式

```bash
gh pr list
#123  Fix login bug      open    alice   2024-01-15
#124  Update dependency  closed  bob     2024-01-14

gh run list
ID         STATUS     CONCLUSION  WORKFLOW      BRANCH  EVENT  ELAPSED
1234567890  completed  success     CI            main    push   45s
1234567891  in_progress  -         Build        feat    push   -

gh issue list
#45  Bug: crash on startup  open  alice  2024-01-15
#46  Feature request        closed  bob  2024-01-14
```

### GLab (GitLab CLI) 典型输出格式
```bash
glab mr list
!23  Fix login      opened  alice  2024-01-15  main
!24  Update deps    merged  bob    2024-01-14  feat

glab ci status
Pipeline #1234 - passed
Stage: build (passed) | Stage: test (passed) | Stage: deploy (passed)
```

### AZ (Azure DevOps CLI) 典型输出格式
```bash
az repos pr list
ID    Title          Status  Creator  Created
1234  Fix login bug  Active  alice    2024-01-15T10:30:00Z
1235  Update deps    Completed  bob   2024-01-14T09:00:00Z
```

### Repo (Android Repo) 典型输出格式
```bash
repo status
project platform/frameworks/base/
 -m     packages/apps/Settings/AndroidManifest.xml
 -m     packages/apps/Camera/Camera.java
```

### Gerrit 典型输出格式
```bash
ssh gerrit query --format=TEXT status:open
rowCount: 3
type: stats
---
...
change 12345
  subject: Fix login bug
  status: NEW
  owner: alice
```

---

## 压缩战术：三层处理

### 第一层：通用噪音过滤（所有 6 个工具都适用）

在 `compact_{tool}_*_for_ai` 方法中，在 pass-through 之前先做：

1. **去除 ANSI 颜色码**（CLAUDE.md 法则 C）
2. **合并连续空行** → 保留最多 1 个空行
3. **过滤纯横线分隔符** → `---`、`===` 等超过 3 字符的分隔线直接删除
4. **修剪行末空白**
5. **长行截断**（超过 200 字符的，截断并加 `...`）

这一层预期给所有 6 个工具带来 **5-15% 的基础压缩**，且零语义损失。

### 第二层：表格列剪裁（重点优化）

这类工具的最大 token 浪费来自**冗余列和过宽对齐**。

**GH pr list 示例**：
```
# 原始（含对齐空格，每列人工拉宽）
#123  Fix login bug                           open    alice   about 2 hours ago
#124  Update dependency to latest version     closed  bob     yesterday

# 压缩后（保留关键列，去除过宽对齐）
#123 Fix login bug open alice 2h
#124 Update dependency closed bob 1d
```

**通用剪裁规则**：
- 将多个空格压缩为单个空格（`Regex::new(r" {2,}") → " "`）
- 相对时间 `about 2 hours ago` → `2h`，`yesterday` → `1d`，`3 days ago` → `3d`
- 状态词：`completed` → `✓`，`failed` → `✗`，`in_progress` → `...`（可选，如果 LLM 能识别简写）

### 第三层：工具专属字段提取（高优先级，按需实现）

#### GH 专属

**gh run list** 压缩：
```
# 原始
1234567890  completed  success     CI  main  push  45s
# 压缩
✓ #1234567… CI main push 45s
```

**gh pr list** 压缩：
```
# 原始  
#123  Fix login bug  OPEN  alice  about 2 hours ago
# 压缩
#123 open fix-login alice 2h
```

#### Repo 专属（Android）

**repo status 压缩**（已有类似路径格式）：
```
# 原始
project platform/frameworks/base/
 -m     packages/apps/Settings/AndroidManifest.xml
# 压缩：识别 project 名 + -m/-d/-a 状态行
base/ M Settings/AndroidManifest.xml
```

#### Gerrit 专属

**gerrit query 压缩**：
```
# 原始（多行键值对格式）
change 12345
  subject: Fix login bug
  status: NEW
  owner: alice
# 压缩（拍扁为单行）
#12345 NEW fix-login-bug alice
```

---

## 代码修改指令

### 步骤1：为每个工具添加通用噪音过滤

在 `methods.rs` 的某个位置（或新建辅助函数 `clean_cloud_cli_output`），实现以下过滤：

```rust
fn clean_cloud_cli_output(raw: &str) -> String {
    // 1. ANSI 净化（参考 vcs_plugin 的 strip_ansi_codes）
    // 2. 合并空行、过滤分隔线
    // 3. 多空格 → 单空格
    // 4. 相对时间规范化
    raw.lines()
        .map(|line| {
            let t = line.trim_end_matches('\r').trim();
            // ... 净化逻辑
            t
        })
        .filter(|t| !t.is_empty() && !t.chars().all(|c| c == '-' || c == '='))
        .collect::<Vec<_>>()
        .join("\n")
}
```

### 步骤2：替换 pass-through 的 parse 方法

将所有 Parser 中的：
```rust
r.push(VcsRecord::Raw(t.to_string()))
```
替换为经过第一层过滤的版本：
```rust
let cleaned = collapse_inline_whitespace(t);
if !cleaned.is_empty() {
    r.push(VcsRecord::Raw(cleaned))
}
```

### 步骤3：在 methods.rs 中添加表格列压缩

对于有固定列格式的命令（如 `gh pr list`、`gh run list`），在 `compact_{tool}_{cmd}_for_ai` 函数里应用第三层专属逻辑。

### 优先级建议

1. **先实现第一层**（通用过滤）：代码量小，所有 6 个工具一起实现，立即有效果
2. **GH 优先做第三层**：GH 是最常用的，`gh pr list` 和 `gh run list` 的压缩价值最高
3. **Repo 和 Gerrit** 因格式较独特，可后续单独处理

---

## 验证

修改每个工具后运行：
```bash
cargo test vcs_{tool}  # 替换 {tool} 为 gh/glab/az/bitbucket/repo/gerrit
cargo check
```

确认测试中的 showcase 报告中压缩率不为 0%，且输出语义完整（能识别 PR 编号、状态、作者）。
