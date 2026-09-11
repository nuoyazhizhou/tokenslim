//! VCS 中间表示（IR）类型定义：文档种类、文档根结构与单条记录。
use super::types::VcsTool;

/// VCS 中间文档（IR）的种类标签。
/// 用于区分一条压缩产物属于 status（状态）、log（日志）、diff（差异）还是 show（详情），
/// 渲染与归一化阶段据此选择不同的字段组织方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcsDocKind {
    Status,
    Log,
    Diff,
    Show,
}

/// VCS 解析后的中间表示（IR）根结构。
/// 主要字段：
/// - `tool`：来源 VCS 工具家族（git/svn/hg/p4…）。
/// - `kind`：文档种类（status/log/diff/show），见 `VcsDocKind`。
/// - `records`：已抽取的结构化记录列表，见 `VcsRecord`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsDocument {
    pub tool: VcsTool,
    pub kind: VcsDocKind,
    pub records: Vec<VcsRecord>,
}

/// VCS 文档中的单条结构化记录。
/// 变体含义概览：
/// - `Branch`：分支名；`Section`：分段标题（静态字符串）。
/// - `File`：文件条目，可选状态码 `status` 与路径 `path`。
/// - `LabeledFile`：带标签的文件条目（如 `Removing`/`Would remove`）。
/// - `Commit`/`Author`/`Date`/`Subject`：提交相关字段。
/// - `DiffFile`：差异文件左右两侧路径；`Hunk`/`Patch`/`Stat`/`Raw`：差异正文、补丁行、统计行与原样行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcsRecord {
    Branch(String),
    Section(&'static str),
    File { status: Option<char>, path: String },
    LabeledFile { label: String, path: String },
    Commit(String),
    Author(String),
    Date(String),
    Subject(String),
    DiffFile { left: String, right: String },
    Hunk(String),
    Patch(String),
    Stat(String),
    Raw(String),
}
