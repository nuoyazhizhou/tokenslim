use super::types::VcsTool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcsDocKind {
    Status,
    Log,
    Diff,
    Show,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsDocument {
    pub tool: VcsTool,
    pub kind: VcsDocKind,
    pub records: Vec<VcsRecord>,
}

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
