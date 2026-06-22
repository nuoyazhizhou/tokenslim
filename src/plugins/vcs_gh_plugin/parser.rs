#![allow(dead_code)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsTool {
    Gh,
}
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsDocKind {
    Status,
    Log,
    Diff,
    Show,
}
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsRecord {
    Section(String),
    Branch(String),
    File { status: Option<char>, path: String },
    LabeledFile { label: String, path: String },
    DiffFile { left: String, right: String },
    Subject(String),
    Author(String),
    Date(String),
    Commit(String),
    Stat(String),
    Patch(String),
    Hunk(String),
    Raw(String),
}
impl std::fmt::Display for VcsRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VcsRecord::Section(s) => write!(f, "[{}]", s),
            VcsRecord::Branch(b) => write!(f, "branch: {}", b),
            VcsRecord::File { status, path } => {
                if let Some(st) = status {
                    write!(f, "{} {}", st, path)
                } else {
                    write!(f, "{}", path)
                }
            }
            VcsRecord::LabeledFile { label, path } => write!(f, "{}: {}", label, path),
            VcsRecord::DiffFile { left, right } => write!(f, "--- {}\n+++ {}", left, right),
            VcsRecord::Subject(s) => write!(f, "{}", s),
            VcsRecord::Author(a) => write!(f, "au:{}", a.trim_start_matches("Author:").trim()),
            VcsRecord::Date(d) => write!(f, "Date: {}", d),
            VcsRecord::Commit(c) => write!(f, "commit {}", c),
            VcsRecord::Stat(s) => write!(f, "{}", s),
            VcsRecord::Patch(p) => write!(f, "{}", p),
            VcsRecord::Hunk(p) => write!(f, "{}", p),
            VcsRecord::Raw(r) => write!(f, "{}", r),
        }
    }
}
pub struct VcsDocument {
    pub tool: VcsTool,
    pub kind: VcsDocKind,
    pub records: Vec<VcsRecord>,
}
pub trait VcsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument>;
}
pub struct GhPrListParser;
pub struct GhIssueListParser;
pub struct GhPrViewParser;
pub struct GhRunListParser;
pub struct GhApiParser;
pub struct GhAuthParser;
pub struct GhPrCreateParser;
pub struct GhPrMergeParser;
pub struct GhIssueCreateParser;
pub struct GhIssueViewParser;
pub struct GhRunViewParser;
pub struct GhRepoListParser;
pub struct GhRepoViewParser;
pub struct GhGistListParser;
pub struct GhGistViewParser;
pub struct GhActionsListParser;
pub struct GhActionsViewParser;
pub struct GhSecretListParser;
pub struct GhDeployListParser;
#[tracing::instrument(level = "debug", skip_all)]
fn to_doc_if_any(t: VcsTool, k: VcsDocKind, r: Vec<VcsRecord>) -> Option<VcsDocument> {
    if r.is_empty() {
        None
    } else {
        Some(VcsDocument {
            tool: t,
            kind: k,
            records: r,
        })
    }
}
// All parsers use raw pass-through
impl VcsParser for GhPrListParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut r = vec![];
        for l in raw.lines() {
            let t = l.trim_end_matches('\r').trim();
            if t.is_empty() {
                continue;
            }
            r.push(VcsRecord::Raw(t.to_string()))
        }
        to_doc_if_any(VcsTool::Gh, VcsDocKind::Log, r)
    }
}
impl VcsParser for GhIssueListParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut r = vec![];
        for l in raw.lines() {
            let t = l.trim_end_matches('\r').trim();
            if t.is_empty() {
                continue;
            }
            r.push(VcsRecord::Raw(t.to_string()))
        }
        to_doc_if_any(VcsTool::Gh, VcsDocKind::Log, r)
    }
}
impl VcsParser for GhPrViewParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut r = vec![];
        for l in raw.lines() {
            let t = l.trim_end_matches('\r').trim();
            if t.is_empty() {
                continue;
            }
            r.push(VcsRecord::Raw(t.to_string()))
        }
        to_doc_if_any(VcsTool::Gh, VcsDocKind::Log, r)
    }
}
impl VcsParser for GhRunListParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut r = vec![];
        for l in raw.lines() {
            let t = l.trim_end_matches('\r').trim();
            if t.is_empty() {
                continue;
            }
            r.push(VcsRecord::Raw(t.to_string()))
        }
        to_doc_if_any(VcsTool::Gh, VcsDocKind::Log, r)
    }
}
