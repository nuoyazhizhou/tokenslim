use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VcsTool {
    Git,
    Svn,
    Hg,
    P4,
    Cvs,
    Bzr,
    Fossil,
    Darcs,
}

impl VcsTool {
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "git" => Some(VcsTool::Git),
            "svn" | "subversion" => Some(VcsTool::Svn),
            "hg" | "mercurial" => Some(VcsTool::Hg),
            "p4" | "perforce" => Some(VcsTool::P4),
            "cvs" => Some(VcsTool::Cvs),
            "bzr" | "bazaar" => Some(VcsTool::Bzr),
            "fossil" => Some(VcsTool::Fossil),
            "darcs" => Some(VcsTool::Darcs),
            _ => None,
        }
    }
}

impl VcsTool {
    pub fn as_str(&self) -> &'static str {
        match self {
            VcsTool::Git => "git",
            VcsTool::Svn => "svn",
            VcsTool::Hg => "hg",
            VcsTool::P4 => "p4",
            VcsTool::Cvs => "cvs",
            VcsTool::Bzr => "bzr",
            VcsTool::Fossil => "fossil",
            VcsTool::Darcs => "darcs",
        }
    }
}

pub const GIT_COMMANDS: &[&str] = &[
    "status",
    "diff",
    "log",
    "show",
    "branch",
    "checkout",
    "switch",
    "merge",
    "rebase",
    "reset",
    "stash",
    "fetch",
    "pull",
    "push",
    "remote",
    "tag",
    "cherry-pick",
    "revert",
    "blame",
    "bisect",
    "restore",
    "clean",
    "submodule",
];

pub const SVN_COMMANDS: &[&str] = &[
    "status", "diff", "log", "info", "add", "delete", "move", "copy", "commit", "update",
    "checkout", "revert", "merge", "switch", "resolve", "cleanup", "blame", "propget", "proplist",
    "list", "cat", "mkdir", "export", "import", "lock", "unlock", "relocate", "propset",
];

pub const HG_COMMANDS: &[&str] = &[
    "status",
    "diff",
    "log",
    "summary",
    "add",
    "remove",
    "rename",
    "commit",
    "update",
    "branch",
    "pull",
    "push",
    "annotate",
    "graft",
    "rebase",
    "shelve",
    "unshelve",
    "revert",
    "cat",
    "heads",
    "outgoing",
    "incoming",
    "backout",
    "uncommit",
    "forget",
    "parents",
    "tip",
    "tags",
    "clone",
    "branches",
    "merge",
    "rollback",
    "phase",
    "bookmarks",
    "copy",
    "move",
    "purge",
    "archive",
    "verify",
    "identify",
    "paths",
    "config",
    "summarize",
    "transplant",
];

pub const P4_COMMANDS: &[&str] = &[
    "opened",
    "changes",
    "describe",
    "diff",
    "submit",
    "sync",
    "edit",
    "add",
    "delete",
    "revert",
    "integrate",
    "resolve",
    "reconcile",
    "shelve",
    "unshelve",
    "files",
    "client",
    "fstat",
    "print",
    "where",
    "info",
    "have",
    "label",
    "labels",
    "dirs",
    "users",
    "move",
    "copy",
    "branches",
    "tag",
    "passwd",
    "protect",
    "triggers",
    "depot",
    "diff2",
];

pub const CVS_COMMANDS: &[&str] = &[
    "status", "diff", "log", "add", "remove", "commit", "update", "checkout", "tag", "annotate",
    "edit", "unedit", "release", "history",
];

pub const BZR_COMMANDS: &[&str] = &[
    "status", "diff", "log", "add", "remove", "commit", "update", "branch", "pull", "push",
    "merge", "resolve", "missing", "revert",
];

pub const FOSSIL_COMMANDS: &[&str] = &[
    "status", "diff", "timeline", "changes", "add", "rm", "commit", "update", "sync", "checkout",
    "merge", "stash", "undo", "tag",
];

pub const DARCS_COMMANDS: &[&str] = &[
    "whatsnew",
    "status",
    "diff",
    "log",
    "changes",
    "record",
    "pull",
    "push",
    "rebase",
    "add",
    "remove",
    "revert",
    "tag",
    "amend",
    "amend-record",
    "obliterate",
];

pub const VCS_COMMAND_WHITELISTS: &[(VcsTool, &[&str])] = &[
    (VcsTool::Git, GIT_COMMANDS),
    (VcsTool::Svn, SVN_COMMANDS),
    (VcsTool::Hg, HG_COMMANDS),
    (VcsTool::P4, P4_COMMANDS),
    (VcsTool::Cvs, CVS_COMMANDS),
    (VcsTool::Bzr, BZR_COMMANDS),
    (VcsTool::Fossil, FOSSIL_COMMANDS),
    (VcsTool::Darcs, DARCS_COMMANDS),
];

pub const GIT_SIGNATURES: &[&str] = &["on branch", "not a git repository", "diff --git"];
pub const SVN_SIGNATURES: &[&str] = &["svn:", "checked out revision", "subversion"];
pub const HG_SIGNATURES: &[&str] = &["mercurial", "changeset:", "abort: no repository found"];
pub const P4_SIGNATURES: &[&str] = &["perforce", "client error:", "... //"];
pub const CVS_SIGNATURES: &[&str] = &["cvs [", "cvs checkout", "cvs update"];
pub const BZR_SIGNATURES: &[&str] = &["bzr:", "bazaar", "no working tree"];
pub const FOSSIL_SIGNATURES: &[&str] = &["fossil", "project-name:", "checkout:"];
pub const DARCS_SIGNATURES: &[&str] = &["darcs", "no repository present", "whatsnew"];

pub const VCS_SIGNATURES: &[(VcsTool, &[&str])] = &[
    (VcsTool::Git, GIT_SIGNATURES),
    (VcsTool::Svn, SVN_SIGNATURES),
    (VcsTool::Hg, HG_SIGNATURES),
    (VcsTool::P4, P4_SIGNATURES),
    (VcsTool::Cvs, CVS_SIGNATURES),
    (VcsTool::Bzr, BZR_SIGNATURES),
    (VcsTool::Fossil, FOSSIL_SIGNATURES),
    (VcsTool::Darcs, DARCS_SIGNATURES),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcsConfig {
    #[serde(default = "default_true")]
    pub dictionaryize_paths: bool,
    #[serde(default = "default_true")]
    pub compact_leading_ws: bool,
    #[serde(default = "default_true")]
    pub collapse_blank_lines: bool,
    #[serde(default = "default_max_blank_lines")]
    pub max_blank_lines: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_blank_lines() -> usize {
    1
}

impl Default for VcsConfig {
    fn default() -> Self {
        Self {
            dictionaryize_paths: true,
            compact_leading_ws: true,
            collapse_blank_lines: true,
            max_blank_lines: default_max_blank_lines(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VcsOverrideConfig {
    #[serde(default)]
    pub dictionaryize_paths: Option<bool>,
    #[serde(default)]
    pub compact_leading_ws: Option<bool>,
    #[serde(default)]
    pub collapse_blank_lines: Option<bool>,
    #[serde(default)]
    pub max_blank_lines: Option<usize>,
    #[serde(default)]
    pub command_whitelists: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub signatures: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub replace_command_whitelists: bool,
    #[serde(default)]
    pub replace_signatures: bool,
}

pub fn default_command_whitelists() -> HashMap<VcsTool, Vec<String>> {
    VCS_COMMAND_WHITELISTS
        .iter()
        .map(|(tool, commands)| {
            (
                *tool,
                commands.iter().map(|cmd| (*cmd).to_string()).collect(),
            )
        })
        .collect()
}

pub fn default_signatures() -> HashMap<VcsTool, Vec<String>> {
    VCS_SIGNATURES
        .iter()
        .map(|(tool, signatures)| (*tool, signatures.iter().map(|s| (*s).to_string()).collect()))
        .collect()
}

pub struct VcsPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: VcsConfig,
    pub command_whitelists: HashMap<VcsTool, Vec<String>>,
    pub signatures: HashMap<VcsTool, Vec<String>>,
}
