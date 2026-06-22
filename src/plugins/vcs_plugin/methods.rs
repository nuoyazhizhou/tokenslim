//! vcs plugin — 旧版分发中心（Git/Hg/SVN/P4 已割接到独立微插件）

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::path_compressor::methods::{
    is_vcs_diff_header_line, replace_paths_in_text_scoped,
};
use crate::core::path_optimizer::methods::current_path_dictionary_options;
use crate::core::path_optimizer::token_boundary::{
    contains_path_token_boundary, is_path_token_boundary_next, replace_path_token_boundary,
};
use crate::core::plugin_config_loader::{
    load_run_route_capabilities, parse_vcs_command_words_from_line, RunRouteCapability,
};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
// 旧 Git/SVN/Hg/P4/CVS/Bzr/Fossil/Darcs 解析器已全部迁移到独立微插件
use crate::plugins::vcs_plugin::parser::VcsParser;
use crate::plugins::vcs_plugin::rule_engine::VcsRuleEngine;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

thread_local! {
    static VCS_AI_COMPACT_MODE: Cell<bool> = const { Cell::new(false) };
    static VCS_AI_PROFILE: Cell<u8> = const { Cell::new(0) };
}
static VCS_ROUTE_CAPABILITY: Lazy<Option<RunRouteCapability>> = Lazy::new(|| {
    let config_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("plugins");
    let caps = load_run_route_capabilities(Some(&config_dir));
    caps.into_iter()
        .find(|cap| cap.name == "vcs" || cap.route.route_group == "vcs")
});
static VCS_TOOL_ALIASES: Lazy<HashMap<String, VcsTool>> = Lazy::new(|| {
    // 从路由配置读取“命令关键词 -> 工具家族”映射，避免在代码中写死 alias 表。
    VCS_ROUTE_CAPABILITY
        .as_ref()
        .map(|cap| {
            cap.run_tool_aliases
                .iter()
                .filter_map(|(keyword, family)| {
                    VcsTool::from_key(family).map(|tool| (keyword.to_ascii_lowercase(), tool))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default()
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcsAiProfile {
    None,
    Status,
    Log,
    Diff,
    Other,
}

impl VcsAiProfile {
    fn as_u8(self) -> u8 {
        match self {
            VcsAiProfile::None => 0,
            VcsAiProfile::Status => 1,
            VcsAiProfile::Log => 2,
            VcsAiProfile::Diff => 3,
            VcsAiProfile::Other => 4,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => VcsAiProfile::Status,
            2 => VcsAiProfile::Log,
            3 => VcsAiProfile::Diff,
            4 => VcsAiProfile::Other,
            _ => VcsAiProfile::None,
        }
    }
}

pub fn set_vcs_ai_compact_mode(enabled: bool) {
    VCS_AI_COMPACT_MODE.with(|mode| mode.set(enabled));
}

pub fn run_with_vcs_ai_compact_mode<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
    run_with_vcs_ai_context(enabled, VcsAiProfile::None, f)
}

pub fn run_with_vcs_ai_context<T>(
    enabled: bool,
    profile: VcsAiProfile,
    f: impl FnOnce() -> T,
) -> T {
    let previous = VCS_AI_COMPACT_MODE.with(|mode| {
        let prev = mode.get();
        mode.set(enabled);
        prev
    });
    let prev_profile = VCS_AI_PROFILE.with(|state| {
        let prev = state.get();
        state.set(profile.as_u8());
        prev
    });

    struct ResetGuard(bool, u8);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            VCS_AI_COMPACT_MODE.with(|mode| mode.set(self.0));
            VCS_AI_PROFILE.with(|state| state.set(self.1));
        }
    }

    let _guard = ResetGuard(previous, prev_profile);
    f()
}

fn vcs_ai_compact_mode() -> bool {
    VCS_AI_COMPACT_MODE.with(Cell::get)
}

fn vcs_ai_profile() -> VcsAiProfile {
    VcsAiProfile::from_u8(VCS_AI_PROFILE.with(Cell::get))
}

static STATUS_PATH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:modified|new file|deleted|renamed|copied):\s+(?P<path>.+)$").unwrap()
});
static SHORT_STATUS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:[ MADRCU?!~I]{1,2})\s+(?P<path>.+)$").unwrap());
static P4_PATH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\.\.\.\s+(?P<path>//[^#\s]+)").unwrap());
static GENERIC_PATH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<path>(?:[a-zA-Z]:\\|//|[./]|[\w.-]+/)[\w./\\-]+(?:\.[\w-]+)?)").unwrap()
});
static PATH_TOKEN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\$P\d+").unwrap());
static MULTI_HWS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").unwrap());
static COMPACTABLE_DATE_TIME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?P<y>\d{4})[-/](?P<m>\d{2})[-/](?P<d>\d{2})[T\s]+(?P<h>\d{2}):(?P<mi>\d{2})(?::\d{2})?",
    )
    .unwrap()
});
static SIZE_BYTES_PHRASE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(?P<num>\d{1,3}(?:,\d{3})+|\d+)\s*(?:bytes?|b)\b").unwrap());
static SIZE_KEY_VALUE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?P<key>(?:file(?:\s|-)?size|filesize|size|content(?:\s|-)?length|payload(?:\s|-)?size|blob(?:\s|-)?size))\b(?P<sep>\s*[:=]\s*)(?P<num>\d{1,3}(?:,\d{3})+|\d+)\b",
    )
    .unwrap()
});
static SIZE_JSON_KEY_VALUE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?i)"(?P<key>(?:file(?:\s|-)?size|filesize|size|content(?:\s|-)?length|payload(?:\s|-)?size|blob(?:\s|-)?size))"(?P<sep>\s*:\s*)(?P<num>\d{1,3}(?:,\d{3})+|\d+)\b"#,
    )
    .unwrap()
});

static P4_DEPOT_PATH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"//[\w./\-]+(?:\.[\w-]+)?").unwrap());

impl Default for VcsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

include!("methods/core_logic.rs");

include!("methods/text_compact.rs");

use crate::plugins::vcs_git_plugin::methods as git_methods;
use crate::plugins::vcs_git_plugin::parser as git_parser;

use crate::plugins::vcs_hg_plugin::methods as hg_methods;
use crate::plugins::vcs_hg_plugin::parser as hg_parser;

use crate::plugins::vcs_svn_plugin::methods as svn_methods;
use crate::plugins::vcs_svn_plugin::parser as svn_parser;

use crate::plugins::vcs_cvs_plugin::methods as cvs_methods;
use crate::plugins::vcs_cvs_plugin::parser as cvs_parser;
use crate::plugins::vcs_p4_plugin::methods as p4_methods;

use crate::plugins::vcs_bzr_plugin::methods as bzr_methods;
use crate::plugins::vcs_bzr_plugin::parser as bzr_parser;

use crate::plugins::vcs_fossil_plugin::methods as fossil_methods;
use crate::plugins::vcs_fossil_plugin::parser as fossil_parser;

use crate::plugins::vcs_darcs_plugin::methods as darcs_methods;
use crate::plugins::vcs_darcs_plugin::parser as darcs_parser;

// 全部 VCS 工具现在都通过独立微插件路由

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplicitIntent {
    Status,
    Diff,
    Log,
    Other,
}

#[derive(Debug, Clone, Default)]
struct ParsedCommand {
    tool_keyword: String,
    subcommand: String,
    args_after_subcommand: Vec<String>,
}

fn vcs_tool_from_keyword(keyword: &str) -> Option<VcsTool> {
    let normalized = keyword.to_ascii_lowercase();
    VCS_TOOL_ALIASES
        .get(&normalized)
        .copied()
        .or_else(|| VcsTool::from_key(&normalized))
}

fn normalize_command_keyword(raw: &str) -> String {
    let file = std::path::Path::new(raw.trim_matches('"'))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(raw)
        .to_ascii_lowercase();

    for suffix in [".exe", ".cmd", ".bat", ".com", ".ps1"] {
        if let Some(stripped) = file.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }

    file
}

fn parse_command_line_tokens(line: &str) -> Option<Vec<String>> {
    #[derive(Clone, Copy)]
    enum QuoteMode {
        None,
        Single,
        Double,
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut mode = QuoteMode::None;
    let mut escaped = false;

    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match mode {
            QuoteMode::None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else if ch == '"' {
                    mode = QuoteMode::Double;
                } else if ch == '\'' {
                    mode = QuoteMode::Single;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Single => {
                if ch == '\'' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Double => {
                if escaped {
                    current.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    // Windows 路径中的反斜杠不能被无条件吞掉，仅在转义引号/反斜杠时生效。
                    match chars.peek() {
                        Some('"') | Some('\\') => escaped = true,
                        _ => current.push(ch),
                    }
                } else if ch == '"' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
        }
    }

    match mode {
        QuoteMode::None => {}
        QuoteMode::Single | QuoteMode::Double => return None,
    }

    if escaped {
        current.push('\\');
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Some(tokens)
}

fn parse_command_from_raw(raw: &str) -> Option<ParsedCommand> {
    let line = first_non_empty_line(raw)
        .trim_start_matches('\u{feff}')
        .trim_end_matches('\r')
        .trim();
    if line.is_empty() {
        return None;
    }

    let (tool_keyword, words) = parse_vcs_command_words_from_line(line)?;
    let Some(subcommand) = words.first().cloned() else {
        return None;
    };
    let args_after_subcommand = words.into_iter().skip(1).collect();

    Some(ParsedCommand {
        tool_keyword,
        subcommand,
        args_after_subcommand,
    })
}

fn command_is(raw: &str, tool_keyword: &str, subcommand: &str) -> bool {
    parse_command_from_raw(raw)
        .is_some_and(|cmd| cmd.tool_keyword == tool_keyword && cmd.subcommand == subcommand)
}

fn command_first_arg_is(raw: &str, tool_keyword: &str, subcommand: &str, first_arg: &str) -> bool {
    parse_command_from_raw(raw).is_some_and(|cmd| {
        cmd.tool_keyword == tool_keyword
            && cmd.subcommand == subcommand
            && cmd
                .args_after_subcommand
                .first()
                .is_some_and(|arg| arg == first_arg)
    })
}

fn explicit_intent_from_command(_tool: Option<VcsTool>, raw: &str) -> Option<ExplicitIntent> {
    let Some(cmd) = parse_command_from_raw(raw) else {
        return None;
    };
    let cap = VCS_ROUTE_CAPABILITY.as_ref()?;
    let intents = cap.run_intents.get(&cmd.tool_keyword)?;
    let intent = intents
        .get(&cmd.subcommand)
        .map(String::as_str)
        .unwrap_or("other");

    if intent.eq_ignore_ascii_case("status") {
        Some(ExplicitIntent::Status)
    } else if intent.eq_ignore_ascii_case("diff") {
        Some(ExplicitIntent::Diff)
    } else if intent.eq_ignore_ascii_case("log") {
        Some(ExplicitIntent::Log)
    } else {
        Some(ExplicitIntent::Other)
    }
}

fn compact_status_for_tool(tool: Option<VcsTool>, raw: &str) -> String {
    match tool {
        Some(VcsTool::Git) => {
            if command_is(raw, "git", "checkout")
                || command_is(raw, "repo", "checkout")
                || command_is(raw, "gerrit", "checkout")
            {
                git_methods::compact_git_checkout_for_ai(raw)
            } else if command_is(raw, "git", "restore") {
                git_methods::process_parser(&git_parser::GitRestoreParser, raw)
            } else if command_is(raw, "git", "switch") {
                git_methods::process_parser(&git_parser::GitSwitchParser, raw)
            } else if command_is(raw, "git", "clean") {
                git_methods::process_parser(&git_parser::GitCleanParser, raw)
            } else {
                git_methods::compact_git_status_for_ai(raw)
            }
        }
        Some(VcsTool::Svn) => {
            if command_is(raw, "svn", "update") {
                svn_methods::process_parser(&svn_parser::SvnUpdateParser, raw)
            } else if command_is(raw, "svn", "switch") {
                svn_methods::process_parser(&svn_parser::SvnSwitchParser, raw)
            } else if command_is(raw, "svn", "relocate") {
                svn_methods::process_parser(&svn_parser::SvnRelocateParser, raw)
            } else if command_is(raw, "svn", "lock") {
                svn_methods::process_parser(&svn_parser::SvnLockParser, raw)
            } else if command_is(raw, "svn", "unlock") {
                svn_methods::process_parser(&svn_parser::SvnUnlockParser, raw)
            } else if command_is(raw, "svn", "revert") {
                svn_methods::process_parser(&svn_parser::SvnRevertParser, raw)
            } else if command_is(raw, "svn", "cleanup") {
                svn_methods::process_parser(&svn_parser::SvnCleanupParser, raw)
            } else if command_is(raw, "svn", "resolve") {
                svn_methods::process_parser(&svn_parser::SvnResolveParser, raw)
            } else if command_is(raw, "svn", "export") {
                svn_methods::process_parser(&svn_parser::SvnExportParser, raw)
            } else {
                svn_methods::process_parser(&svn_parser::SvnStatusParser, raw)
            }
        }
        Some(VcsTool::Hg) => {
            if command_is(raw, "hg", "clone") {
                hg_methods::process_parser(&hg_parser::HgCloneParser, raw)
            } else if command_is(raw, "hg", "update") {
                hg_methods::process_parser(&hg_parser::HgUpdateParser, raw)
            } else if command_is(raw, "hg", "merge") {
                hg_methods::process_parser(&hg_parser::HgMergeParser, raw)
            } else if command_is(raw, "hg", "rollback") {
                hg_methods::process_parser(&hg_parser::HgRollbackParser, raw)
            } else if command_is(raw, "hg", "backout") {
                hg_methods::process_parser(&hg_parser::HgBackoutParser, raw)
            } else if command_is(raw, "hg", "shelve") {
                hg_methods::process_parser(&hg_parser::HgShelveParser, raw)
            } else if command_is(raw, "hg", "phase") {
                hg_methods::process_parser(&hg_parser::HgPhaseParser, raw)
            } else if command_is(raw, "hg", "bookmarks") {
                hg_methods::process_parser(&hg_parser::HgBookmarksParser, raw)
            } else if command_is(raw, "hg", "tag") {
                hg_methods::process_parser(&hg_parser::HgTagParser, raw)
            } else {
                hg_methods::compact_hg_status_for_ai(raw)
            }
        }
        Some(VcsTool::P4) => p4_methods::compact_p4_status_for_ai(raw),
        Some(VcsTool::Cvs) => {
            if command_is(raw, "cvs", "update") {
                cvs_methods::process_parser(&cvs_parser::CvsUpdateParser, raw)
            } else if command_is(raw, "cvs", "edit") {
                cvs_methods::process_parser(&cvs_parser::CvsEditParser, raw)
            } else {
                cvs_methods::compact_cvs_status_for_ai(raw)
            }
        }
        Some(VcsTool::Bzr) => {
            if command_is(raw, "bzr", "resolve") {
                bzr_methods::process_parser(&bzr_parser::BzrResolveParser, raw)
            } else {
                bzr_methods::compact_bzr_status_for_ai(raw)
            }
        }
        Some(VcsTool::Fossil) => {
            if command_is(raw, "fossil", "changes") {
                fossil_methods::process_parser(&fossil_parser::FossilChangesParser, raw)
            } else {
                fossil_methods::compact_fossil_status_for_ai(raw)
            }
        }
        Some(VcsTool::Darcs) => {
            if command_is(raw, "darcs", "whatsnew") {
                darcs_methods::process_parser(&darcs_parser::DarcsWhatsnewParser, raw)
            } else {
                darcs_methods::compact_darcs_status_for_ai(raw)
            }
        }
        _ => raw.to_string(),
    }
}

fn compact_diff_for_tool(tool: Option<VcsTool>, raw: &str) -> String {
    match tool {
        Some(VcsTool::Git) => git_methods::compact_git_diff_for_ai(raw),
        Some(VcsTool::Svn) => svn_methods::compact_svn_diff_for_ai(raw),
        Some(VcsTool::Hg) => hg_methods::compact_hg_diff_for_ai(raw),
        Some(VcsTool::P4) => p4_methods::compact_p4_describe_for_ai(raw),
        Some(VcsTool::Cvs) => cvs_methods::compact_cvs_diff_for_ai(raw),
        Some(VcsTool::Bzr) => bzr_methods::compact_bzr_diff_for_ai(raw),
        Some(VcsTool::Fossil) => fossil_methods::compact_fossil_diff_for_ai(raw),
        Some(VcsTool::Darcs) => darcs_methods::compact_darcs_diff_for_ai(raw),
        _ => raw.to_string(),
    }
}

fn compact_log_for_tool(tool: Option<VcsTool>, raw: &str) -> String {
    match tool {
        Some(VcsTool::Git) => {
            if command_is(raw, "git", "revert") {
                git_methods::process_parser(&git_parser::GitRevertParser, raw)
            } else if command_is(raw, "git", "reset") {
                git_methods::process_parser(&git_parser::GitResetParser, raw)
            } else if command_is(raw, "git", "stash")
                && command_first_arg_is(raw, "git", "stash", "list")
            {
                git_methods::process_parser(&git_parser::GitStashParser, raw)
            } else if command_is(raw, "git", "bisect") {
                git_methods::process_parser(&git_parser::GitBisectParser, raw)
            } else if command_is(raw, "git", "merge") {
                git_methods::process_parser(&git_parser::GitMergeParser, raw)
            } else if command_is(raw, "git", "pull") {
                git_methods::process_parser(&git_parser::GitPullParser, raw)
            } else if command_is(raw, "git", "push") {
                git_methods::process_parser(&git_parser::GitPushParser, raw)
            } else if command_is(raw, "git", "submodule") {
                git_methods::process_parser(&git_parser::GitSubmoduleParser, raw)
            } else if command_is(raw, "git", "cherry-pick") {
                git_methods::process_parser(&git_parser::GitCherryPickParser, raw)
            } else if command_is(raw, "git", "rebase") {
                if command_first_arg_is(raw, "git", "rebase", "-i")
                    || command_first_arg_is(raw, "git", "rebase", "--interactive")
                {
                    git_methods::compact_git_log_for_ai(raw)
                } else {
                    git_methods::process_parser(&git_parser::GitRebaseParser, raw)
                }
            } else {
                git_methods::compact_git_log_for_ai(raw)
            }
        }
        Some(VcsTool::Svn) => {
            if command_is(raw, "svn", "commit") {
                svn_methods::compact_svn_commit_for_ai(raw)
            } else if command_is(raw, "svn", "switch") {
                svn_methods::process_parser(&svn_parser::SvnSwitchParser, raw)
            } else if command_is(raw, "svn", "relocate") {
                svn_methods::process_parser(&svn_parser::SvnRelocateParser, raw)
            } else if command_is(raw, "svn", "merge") {
                svn_methods::process_parser(&svn_parser::SvnMergeParser, raw)
            } else {
                svn_methods::compact_svn_log_for_ai(raw)
            }
        }
        Some(VcsTool::Hg) => {
            if command_is(raw, "hg", "clone") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgCloneParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "pull") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgPullParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "push") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgPushParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "commit") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgCommitParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "merge") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgMergeParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "rollback") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgRollbackParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "backout") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgBackoutParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "shelve") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgShelveParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "phase") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgPhaseParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "bookmarks") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgBookmarksParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "tag") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgTagParser,
                    raw,
                ))
            } else if command_is(raw, "hg", "branches") {
                compact_size_mentions_for_text(&hg_methods::process_parser(
                    &hg_parser::HgBranchesParser,
                    raw,
                ))
            } else {
                compact_size_mentions_for_text(&hg_methods::compact_hg_log_family_for_ai(raw))
            }
        }
        Some(VcsTool::P4) => p4_methods::compact_p4_log_family_for_ai(raw),
        Some(VcsTool::Cvs) => {
            if command_is(raw, "cvs", "commit") {
                cvs_methods::process_parser(&cvs_parser::CvsCommitParser, raw)
            } else if command_is(raw, "cvs", "tag") {
                cvs_methods::process_parser(&cvs_parser::CvsTagParser, raw)
            } else {
                cvs_methods::compact_cvs_log_family_for_ai(raw)
            }
        }
        Some(VcsTool::Bzr) => {
            if command_is(raw, "bzr", "pull") {
                bzr_methods::process_parser(&bzr_parser::BzrPullParser, raw)
            } else if command_is(raw, "bzr", "push") {
                bzr_methods::process_parser(&bzr_parser::BzrPushParser, raw)
            } else if command_is(raw, "bzr", "merge") {
                bzr_methods::process_parser(&bzr_parser::BzrMergeParser, raw)
            } else if command_is(raw, "bzr", "branch") {
                bzr_methods::process_parser(&bzr_parser::BzrBranchParser, raw)
            } else {
                bzr_methods::compact_bzr_log_family_for_ai(raw)
            }
        }
        Some(VcsTool::Fossil) => {
            if command_is(raw, "fossil", "timeline") {
                fossil_methods::process_parser(&fossil_parser::FossilTimelineParser, raw)
            } else if command_is(raw, "fossil", "undo") {
                fossil_methods::process_parser(&fossil_parser::FossilUndoParser, raw)
            } else if command_is(raw, "fossil", "stash") {
                fossil_methods::process_parser(&fossil_parser::FossilStashParser, raw)
            } else if command_is(raw, "fossil", "merge") {
                fossil_methods::process_parser(&fossil_parser::FossilMergeParser, raw)
            } else if command_is(raw, "fossil", "sync") {
                fossil_methods::process_parser(&fossil_parser::FossilSyncParser, raw)
            } else {
                fossil_methods::compact_fossil_log_family_for_ai(raw)
            }
        }
        Some(VcsTool::Darcs) => {
            if command_is(raw, "darcs", "record") {
                darcs_methods::process_parser(&darcs_parser::DarcsRecordParser, raw)
            } else if command_is(raw, "darcs", "amend") {
                darcs_methods::process_parser(&darcs_parser::DarcsAmendParser, raw)
            } else if command_is(raw, "darcs", "obliterate") {
                darcs_methods::process_parser(&darcs_parser::DarcsObliterateParser, raw)
            } else if command_is(raw, "darcs", "whatsnew") {
                darcs_methods::process_parser(&darcs_parser::DarcsWhatsnewParser, raw)
            } else {
                darcs_methods::compact_darcs_log_family_for_ai(raw)
            }
        }
        _ => compact_size_mentions_for_text(raw),
    }
}

fn compact_other_for_tool(tool: Option<VcsTool>, raw: &str) -> String {
    match tool {
        Some(VcsTool::Git) => {
            if command_is(raw, "git", "worktree") || command_is(raw, "git", "grep") {
                git_methods::compact_git_other_for_ai(raw)
            } else if command_is(raw, "git", "blame") {
                git_methods::compact_git_blame_for_ai(raw)
            } else {
                git_methods::compact_git_other_for_ai(raw)
            }
        }
        Some(VcsTool::Svn) => {
            if command_is(raw, "svn", "blame") {
                svn_methods::compact_svn_blame_for_ai(raw)
            } else if command_is(raw, "svn", "list") {
                svn_methods::compact_svn_list_for_ai(raw)
            } else if command_is(raw, "svn", "propget") || command_is(raw, "svn", "proplist") {
                svn_methods::compact_svn_prop_for_ai(raw)
            } else if command_is(raw, "svn", "info") {
                svn_methods::compact_svn_info_for_ai(raw)
            } else {
                svn_methods::compact_svn_other_for_ai(raw)
            }
        }
        Some(VcsTool::Hg) => {
            if command_is(raw, "hg", "copy") {
                hg_methods::process_parser(&hg_parser::HgCopyParser, raw)
            } else if command_is(raw, "hg", "move") {
                hg_methods::process_parser(&hg_parser::HgMoveParser, raw)
            } else if command_is(raw, "hg", "purge") {
                hg_methods::process_parser(&hg_parser::HgPurgeParser, raw)
            } else if command_is(raw, "hg", "archive") {
                hg_methods::process_parser(&hg_parser::HgArchiveParser, raw)
            } else if command_is(raw, "hg", "verify") {
                hg_methods::process_parser(&hg_parser::HgVerifyParser, raw)
            } else if command_is(raw, "hg", "identify") {
                hg_methods::process_parser(&hg_parser::HgIdentifyParser, raw)
            } else if command_is(raw, "hg", "paths") {
                hg_methods::process_parser(&hg_parser::HgPathsParser, raw)
            } else if command_is(raw, "hg", "config") {
                hg_methods::process_parser(&hg_parser::HgConfigParser, raw)
            } else if command_is(raw, "hg", "summarize") {
                hg_methods::process_parser(&hg_parser::HgSummarizeParser, raw)
            } else if command_is(raw, "hg", "transplant") {
                hg_methods::process_parser(&hg_parser::HgTransplantParser, raw)
            } else {
                hg_methods::compact_hg_other_for_ai(raw)
            }
        }
        Some(VcsTool::P4) => p4_methods::compact_p4_other_for_ai(raw),
        Some(VcsTool::Cvs) => cvs_methods::compact_cvs_other_for_ai(raw),
        Some(VcsTool::Bzr) => bzr_methods::compact_bzr_other_for_ai(raw),
        Some(VcsTool::Fossil) => fossil_methods::compact_fossil_other_for_ai(raw),
        Some(VcsTool::Darcs) => darcs_methods::compact_darcs_other_for_ai(raw),
        _ => raw.to_string(),
    }
}

fn compact_size_mentions_for_text(input: &str) -> String {
    let bytes_compacted = SIZE_BYTES_PHRASE_RE
        .replace_all(input, |caps: &regex::Captures| {
            compact_human_size_text(&caps["num"]).unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned();

    let key_value_compacted = SIZE_KEY_VALUE_RE
        .replace_all(&bytes_compacted, |caps: &regex::Captures| {
            let compact =
                compact_human_size_value(&caps["num"]).unwrap_or_else(|| caps["num"].to_string());
            format!("{}{}{}", &caps["key"], &caps["sep"], compact)
        })
        .into_owned();

    SIZE_JSON_KEY_VALUE_RE
        .replace_all(&key_value_compacted, |caps: &regex::Captures| {
            let compact =
                compact_human_size_value(&caps["num"]).unwrap_or_else(|| caps["num"].to_string());
            format!("\"{}\"{}{}", &caps["key"], &caps["sep"], compact)
        })
        .into_owned()
}

fn compact_human_size_text(num_token: &str) -> Option<String> {
    let digits = num_token.replace(',', "");
    let bytes = digits.parse::<u64>().ok()?;
    if bytes < 1024 {
        return Some(format!("{bytes}B"));
    }

    const UNITS: [&str; 4] = ["K", "M", "G", "T"];
    let mut value = bytes as f64 / 1024.0;
    let mut unit_idx = 0usize;
    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }

    if (value - value.floor()).abs() < 0.05 {
        Some(format!("{:.0}{}", value, UNITS[unit_idx]))
    } else {
        Some(format!("{:.1}{}", value, UNITS[unit_idx]))
    }
}

fn compact_human_size_value(num_token: &str) -> Option<String> {
    let compact = compact_human_size_text(num_token)?;
    if compact.len() <= num_token.len() {
        Some(compact)
    } else {
        None
    }
}

fn prefer_non_expanding(raw: &str, compacted: String) -> String {
    let raw_len = raw.trim_end_matches('\n').trim_end_matches('\r').len();
    let comp_len = compacted
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .len();
    if comp_len <= raw_len {
        compacted
    } else {
        raw.to_string()
    }
}

fn align_trailing_newline_with_raw(raw: &str, mut rendered: String) -> String {
    if raw.ends_with('\n') {
        if !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        return rendered;
    }

    while rendered.ends_with('\n') {
        rendered.pop();
    }
    if rendered.ends_with('\r') {
        rendered.pop();
    }
    rendered
}

fn normalize_compact_vcs_conventions(mut rendered: String) -> String {
    if rendered.is_empty() {
        return rendered;
    }

    // Normalize inline path dictionary footer to explicit bracketed form.
    rendered = rendered.replace("\n[paths] ", "\n[paths] ");
    if let Some(rest) = rendered.strip_prefix("[paths] ") {
        rendered = format!("[paths] {rest}");
    }

    // Normalize numeric date-time tokens to compact form: YYYYMMDD HH:MM.
    rendered = COMPACTABLE_DATE_TIME_RE
        .replace_all(&rendered, |caps: &regex::Captures| {
            format!(
                "{}{}{} {}:{}",
                &caps["y"], &caps["m"], &caps["d"], &caps["h"], &caps["mi"]
            )
        })
        .into_owned();

    rendered
}

fn append_inline_path_dictionary(rendered: String, dict_engine: &DictionaryEngine) -> String {
    let options = current_path_dictionary_options();
    if !options.enabled {
        return rendered;
    }

    let mut token_counts: HashMap<String, usize> = HashMap::new();
    for m in PATH_TOKEN_RE.find_iter(&rendered) {
        let next = rendered.as_bytes().get(m.end()).copied();
        if !is_path_token_boundary_next(next) {
            continue;
        }
        let token = m.as_str().to_string();
        *token_counts.entry(token).or_insert(0) += 1;
    }

    // Phase 1: Filter tokens by the combined body/footer gate.
    // Tokens that will not be explained by the inline footer mapping must be reverted
    // in the body text, otherwise users see body-only $P tokens with no `[paths]` entry.
    let dict = dict_engine.snapshot();
    let mut out = rendered;
    for (token, count) in &token_counts {
        if *count < options.min_primary_occurrences || *count < options.min_footer_token_uses {
            if let Some(raw_path) = dict.paths.get(token) {
                out = replace_path_token_boundary(&out, token, raw_path);
            }
        }
    }

    // Phase 2: Collect qualifying tokens for footer (also check min_footer_token_uses).
    let mut path_tokens: Vec<String> = token_counts
        .into_iter()
        .filter_map(|(token, count)| {
            if count >= options.min_primary_occurrences && count >= options.min_footer_token_uses {
                Some(token)
            } else {
                None
            }
        })
        .collect();
    if path_tokens.is_empty() {
        return out;
    }
    path_tokens.sort_by_key(|token| {
        token
            .strip_prefix("$P")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(usize::MAX)
    });

    let mut mappings: Vec<(String, String)> = Vec::new();
    for token in path_tokens {
        if let Some(raw_path) = dict.paths.get(&token) {
            let resolved = dict.resolve_or_self(raw_path);
            mappings.push((token, resolved));
        }
    }

    if mappings.is_empty() {
        return out;
    }

    simplify_dead_anchor_mappings(&mut mappings, &out);

    // Phase 3: Parent-prefix reuse.
    // If a kept token maps to a high-value directory prefix (e.g. //depot/main),
    // reuse it inside longer raw paths so trie node frequency is reflected in body text.
    out = apply_parent_prefix_aliases(out, &mappings);

    let parts: Vec<String> = mappings
        .iter()
        .map(|(token, path)| format!("{}={}", token, path))
        .collect();

    let dict_line = format!("[paths] {}", parts.join("; "));
    let body = out.trim_start_matches(|c| c == '\n' || c == '\r');

    let render_with_dict = |body: &str, dict_line: &str| -> String {
        // Keep command-first ergonomics (`<cmd>` then `[paths]`) to avoid duplicate
        // command insertion in ensure_explicit_command_header while still front-loading dictionary.
        if first_explicit_command_line(body).is_some() {
            if let Some(pos) = body.find('\n') {
                let mut prefixed = String::new();
                prefixed.push_str(&body[..=pos]);
                prefixed.push_str(dict_line);
                prefixed.push('\n');
                prefixed.push_str(&body[(pos + 1)..]);
                return prefixed;
            }
        }

        let mut prefixed = String::new();
        prefixed.push_str(dict_line);
        prefixed.push('\n');
        prefixed.push_str(body);
        prefixed
    };

    let dict_version = render_with_dict(body, &dict_line);

    // Final ROI gate: if `[paths]` metadata does not produce a net size win,
    // fall back to expanded literal paths.
    let mut expanded = out;
    for (token, raw_path) in &mappings {
        expanded = replace_path_token_boundary(&expanded, token, raw_path);
    }
    let expanded = expanded
        .trim_start_matches(|c| c == '\n' || c == '\r')
        .to_string();

    if dict_version.len() < expanded.len() {
        dict_version
    } else {
        expanded
    }
}

fn apply_parent_prefix_aliases(mut body: String, mappings: &[(String, String)]) -> String {
    let mut pairs: Vec<(&str, &str)> = mappings
        .iter()
        .map(|(token, path)| (token.as_str(), path.as_str()))
        .collect();
    // Prefer longer prefixes first to avoid shorter prefix stealing.
    pairs.sort_by_key(|(_, path)| std::cmp::Reverse(path.len()));

    for (token, prefix) in pairs {
        if !prefix.starts_with("//") && !prefix.contains('/') {
            continue;
        }

        body = P4_DEPOT_PATH_RE
            .replace_all(&body, |caps: &regex::Captures| {
                let full = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
                if full == prefix {
                    return token.to_string();
                }
                if let Some(rest) = full.strip_prefix(prefix) {
                    if rest.starts_with('/') || rest.starts_with('\\') {
                        return format!("{}{}", token, rest);
                    }
                }
                full.to_string()
            })
            .into_owned();
    }

    body
}

fn simplify_dead_anchor_mappings(mappings: &mut Vec<(String, String)>, body: &str) {
    loop {
        let mut changed = false;

        for idx in 0..mappings.len() {
            let (token, value) = mappings[idx].clone();
            if contains_path_token_boundary(body, &token) {
                continue;
            }

            let refs: Vec<usize> = mappings
                .iter()
                .enumerate()
                .filter_map(|(j, (_, other_value))| {
                    if j != idx && contains_path_token_boundary(other_value, &token) {
                        Some(j)
                    } else {
                        None
                    }
                })
                .collect();

            if refs.len() == 1 {
                let owner = refs[0];
                mappings[owner].1 = replace_path_token_boundary(&mappings[owner].1, &token, &value);
                mappings.remove(idx);
                changed = true;
                break;
            }
        }

        if !changed {
            break;
        }
    }
}

#[derive(Default)]
struct WsCompactionContext {
    current_file_is_python: bool,
    in_diff_block: bool,
}

fn update_ws_context_from_line(line: &str, ctx: &mut WsCompactionContext) {
    let trimmed = line.trim();

    if trimmed.starts_with("diff --git")
        || trimmed.starts_with("Index: ")
        || line.starts_with("+++")
        || line.starts_with("---")
    {
        ctx.in_diff_block = true;
    }

    let update_from_path = |path: &str, ctx: &mut WsCompactionContext| {
        let p = path.trim_matches('"').to_ascii_lowercase();
        ctx.current_file_is_python = p.ends_with(".py") || p.ends_with(".pyi");
    };

    if let Some(rest) = trimmed.strip_prefix("+++") {
        let path = rest
            .trim()
            .trim_start_matches("b/")
            .trim_start_matches("a/");
        if !path.is_empty() && path != "/dev/null" {
            update_from_path(path, ctx);
        }
        return;
    }

    if let Some(rest) = trimmed.strip_prefix("diff --git ") {
        if let Some(right) = parse_git_diff_header_right_path(rest) {
            let path = right.trim_start_matches("b/");
            if !path.is_empty() {
                update_from_path(path, ctx);
            }
        }
    }
}

fn parse_git_diff_header_right_path(rest: &str) -> Option<String> {
    let (_, consumed_left) = parse_git_path_token(rest)?;
    let remaining = rest.get(consumed_left..)?.trim_start();
    let (right, _) = parse_git_path_token(remaining)?;
    Some(right)
}

fn parse_git_path_token(input: &str) -> Option<(String, usize)> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    if bytes[0] == b'"' {
        let mut out = String::new();
        let mut i = 1usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => {
                    out.push(bytes[i + 1] as char);
                    i += 2;
                }
                b'"' => return Some((out, i + 1)),
                c => {
                    out.push(c as char);
                    i += 1;
                }
            }
        }
        None
    } else {
        let end = input.find(char::is_whitespace).unwrap_or(input.len());
        if end == 0 {
            None
        } else {
            Some((input[..end].to_string(), end))
        }
    }
}

fn compact_leading_ws_with_language_guard(line: &str, ctx: &WsCompactionContext) -> String {
    if ctx.current_file_is_python {
        return line.to_string();
    }

    let trimmed = line.trim_start();
    let is_hash_range = trimmed.contains("..")
        && trimmed
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_alphanumeric());

    if is_hash_range {
        return trimmed.to_string();
    }

    let patch_header = line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with("@@")
        || line.starts_with("diff --");
    if patch_header {
        return line.to_string();
    }

    if let Some(marker) = line.chars().next() {
        if matches!(marker, '+' | '-' | ' ') && line.len() > 1 {
            let body = &line[1..];
            let trimmed = body.trim_start_matches(' ');
            let leading = body.len().saturating_sub(trimmed.len());
            if leading >= 2 {
                let unit = if leading % 4 == 0 {
                    4
                } else if leading % 2 == 0 {
                    2
                } else {
                    1
                };
                if unit > 1 {
                    return format!("{}{}{}", marker, " ".repeat(leading / unit), trimmed);
                }
            }
            return line.to_string();
        }
    }

    let trimmed = line.trim_start();
    let ws = line.len().saturating_sub(trimmed.len());

    if ws > 1 {
        return format!(" {}", trimmed);
    }

    line.to_string()
}

fn compact_inline_alignment_ws(line: &str) -> String {
    if !line.contains('\t') && !line.contains("  ") {
        return line.to_string();
    }

    let patch_header = line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with("@@")
        || line.starts_with("diff --");
    if patch_header {
        return line.to_string();
    }

    // Protect tables: if there is at least one segment of multiple spaces and it contains common headers,
    // or if it has multiple segments of multiple spaces, it's likely a table.
    let trimmed = line.trim_start();
    let internal_multi_ws = MULTI_HWS_RE.find_iter(trimmed).count();
    let lower = line.to_ascii_lowercase();
    if internal_multi_ws >= 1
        && (lower.contains("id")
            || lower.contains("status")
            || lower.contains("title")
            || lower.contains("workflow")
            || internal_multi_ws >= 2)
    {
        if !trimmed.starts_with("//") {
            let normalized_tabs = line.replace('\t', "  ");
            return MULTI_HWS_RE.replace_all(&normalized_tabs, "  ").to_string();
        }
    }

    // Protect P4 View mappings: "//depot/path //client/path"
    if line.matches("//").count() >= 2 {
        return line.to_string();
    }

    let normalized_tabs = line.replace('\t', " ");
    MULTI_HWS_RE.replace_all(&normalized_tabs, " ").to_string()
}

fn is_patch_payload_line(line: &str, ctx: &WsCompactionContext) -> bool {
    if !ctx.in_diff_block {
        return false;
    }

    (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
        && !line.starts_with("+++")
        && !line.starts_with("---")
        && !line.starts_with("@@")
        && !line.starts_with("diff --")
}

pub(crate) fn parse_override_from_path(
    path: &Path,
    content: &str,
) -> Result<VcsOverrideConfig, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    parse_override_from_str(content, &ext)
}

pub(crate) fn parse_override_from_str(
    content: &str,
    extension: &str,
) -> Result<VcsOverrideConfig, String> {
    match extension {
        "json" => serde_json::from_str(content).map_err(|e| e.to_string()),
        "toml" => toml::from_str(content).map_err(|e| e.to_string()),
        _ => Err("Unsupported VCS config format".to_string()),
    }
}

pub(crate) fn looks_like_vcs_path(path: &str) -> bool {
    let trimmed = path.trim_matches('"');
    // Reject email addresses, URLs
    if trimmed.contains('@') || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return false;
    }
    // Reject code-like patterns (method calls, function names, operators)
    if trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.contains(';')
        || trimmed.contains('=')
        || trimmed.contains('<')
        || trimmed.contains('>')
    {
        return false;
    }
    // Reject method-name-like patterns: `.methodName` without path separators
    // e.g. `.detect_project_type`, `.trim_end_matches`, `.starts_with`
    if trimmed.starts_with('.')
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed
            .chars()
            .skip(1)
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return false;
    }
    // Reject common TLD-only matches (e.g. `.com` from email domains matched by GENERIC_PATH_RE)
    let common_tlds = [
        ".com", ".org", ".net", ".io", ".edu", ".gov", ".mil", ".int", ".ma", ".uk", ".de", ".fr",
        ".jp", ".cn", ".au", ".br", ".ca", ".ru", ".in",
    ];
    if common_tlds
        .iter()
        .any(|tld| trimmed.eq_ignore_ascii_case(tld))
    {
        return false;
    }
    // Reject bare domains like "example.com" but allow known filenames like "Cargo.toml"
    let is_bare_domain = trimmed.contains('.')
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed.split('.').count() == 2;
    let known_file = matches!(
        trimmed,
        "Makefile" | "Dockerfile" | "Cargo.toml" | "package.json"
    );
    if is_bare_domain && !known_file {
        return false;
    }
    trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.ends_with(".rs")
        || trimmed.ends_with(".md")
        || trimmed.ends_with(".lock")
        || (!trimmed.contains(' ')
            && trimmed.contains('.')
            && trimmed.chars().any(|c| c.is_ascii_alphabetic()))
        || known_file
        || trimmed.starts_with('.')
}

#[tracing::instrument(level = "debug", skip_all)]
#[allow(dead_code)]
pub(crate) fn replace_vcs_paths_in_text_scoped<'a>(
    text: &'a str,
    dict_engine: &mut DictionaryEngine,
    arena: Option<&'a Bump>,
) -> Cow<'a, str> {
    if !text.contains('/') && !text.contains('\\') {
        return Cow::Borrowed(text);
    }

    let mut changed = false;
    let mut out = String::with_capacity(text.len());

    for chunk in text.split_inclusive('\n') {
        let (line_with_cr, has_newline) = if let Some(line) = chunk.strip_suffix('\n') {
            (line, true)
        } else {
            (chunk, false)
        };
        let (line, has_cr) = if let Some(line) = line_with_cr.strip_suffix('\r') {
            (line, true)
        } else {
            (line_with_cr, false)
        };

        if is_vcs_diff_header_line(line) {
            out.push_str(line);
        } else {
            let replaced = replace_paths_in_text_scoped(line, dict_engine, None);
            if replaced.as_ref() != line {
                changed = true;
            }
            out.push_str(replaced.as_ref());
        }

        if has_cr {
            out.push('\r');
        }
        if has_newline {
            out.push('\n');
        }
    }

    if !changed {
        return Cow::Borrowed(text);
    }

    if let Some(a) = arena {
        Cow::Borrowed(a.alloc_str(&out))
    } else {
        Cow::Owned(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_inline_path_dictionary, command_first_arg_is, command_is,
        explicit_intent_from_command, simplify_dead_anchor_mappings, vcs_tool_from_keyword,
        ExplicitIntent, VcsTool,
    };
    use crate::core::dictionary_engine::DictionaryEngine;

    #[test]
    fn simplify_dead_anchor_mappings_flattens_single_use_intermediate_anchor() {
        let body = "M $P2/issues.md\nM $P2/learnings.md\n";
        let mut mappings = vec![
            ("$P1".to_string(), ".sisyphus/notepads".to_string()),
            ("$P2".to_string(), "$P1/REFACTORING_PLAN_V6.2".to_string()),
        ];

        simplify_dead_anchor_mappings(&mut mappings, body);

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].0, "$P2");
        assert_eq!(mappings[0].1, ".sisyphus/notepads/REFACTORING_PLAN_V6.2");
    }

    #[test]
    fn append_inline_path_dictionary_emits_flat_status_footer_mapping() {
        let mut dict = DictionaryEngine::new();
        let plan = dict.add_path_layered(".sisyphus/notepads/REFACTORING_PLAN_V6.2");
        // 使用 3 次出现以满足 min_primary_occurrences=2 和 min_footer_token_uses=2 的阈值
        let rendered = format!(
            "M {}/issues.md\nM {}/learnings.md\nM {}/notes.md\n",
            plan, plan, plan
        );

        let out = append_inline_path_dictionary(rendered, &dict);

        assert_eq!(out.matches("[paths]").count(), 1);
        assert!(!out.contains("=$P"));
        assert!(out.contains("REFACTORING_PLAN_V6.2"));
    }

    #[test]
    fn simplify_dead_anchor_mappings_keeps_literal_token_like_segments() {
        let body = "M $P2/issues.md\n";
        let mut mappings = vec![
            ("$P1".to_string(), "docs/design".to_string()),
            (
                "$P2".to_string(),
                "$P1-notes/REFACTORING_PLAN.md".to_string(),
            ),
        ];

        simplify_dead_anchor_mappings(&mut mappings, body);

        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings[1].1, "$P1-notes/REFACTORING_PLAN.md");
    }

    #[test]
    fn append_inline_path_dictionary_ignores_literal_token_like_text_in_body() {
        let mut dict = DictionaryEngine::new();
        let _token = dict.add_path_layered("docs/design/vcs_plugin.md");
        let rendered = "note: docs/$P1-notes/readme.md\n".to_string();

        let out = append_inline_path_dictionary(rendered.clone(), &dict);

        assert_eq!(out, rendered);
    }

    #[test]
    fn append_inline_path_dictionary_reverts_body_only_token_without_footer_entry() {
        let mut dict = DictionaryEngine::new();
        let token = dict.add_path_layered("//depot/main/src/core/doctor_workspace");
        let rendered = format!("depotFile: {}/methods.rs\n", token);

        let out = append_inline_path_dictionary(rendered, &dict);

        assert!(!out.contains(&token));
        assert!(out.contains("//depot/main/src/core/doctor_workspace/methods.rs"));
        assert!(!out.contains("[paths]"));
    }

    #[test]
    fn append_inline_path_dictionary_reuses_parent_token_for_longer_p4_path() {
        let mut dict = DictionaryEngine::new();
        let root = dict.add_path_layered("//depot/main");
        let plugin_root = dict.add_path_layered("//depot/main/src/plugins/vcs_plugin");

        let rendered = format!(
            "M //depot/main/src/core/doctor_workspace/methods.rs\nA {}/parser.rs\nM {}/methods.rs\nM {}/Cargo.toml\nM {}/Cargo.lock\n",
            plugin_root, plugin_root, root, root
        );

        let out = append_inline_path_dictionary(rendered, &dict);

        assert!(
            out.contains(&format!("M {}/src/core/doctor_workspace/methods.rs", root)),
            "out={} root={} plugin_root={}",
            out,
            root,
            plugin_root
        );
    }

    #[test]
    fn append_inline_path_dictionary_skips_when_metadata_overhead_not_profitable() {
        let mut dict = DictionaryEngine::new();
        let token = dict.add_path_layered("src/plugins/vcs_plugin");
        let rendered = format!(
            "cvs status\nM {}/methods.rs\nA samples/vcs/case_31_git_checkout_file.log\nR src/legacy/old_cvs_adapter.rs\n? tmp/debug-notes.txt\nC {}/parser.rs\n",
            token, token
        );

        let out = append_inline_path_dictionary(rendered, &dict);

        assert!(!out.contains("[paths]"), "out={}", out);
        assert!(!out.contains(&token), "out={}", out);
        assert!(
            out.contains("src/plugins/vcs_plugin/methods.rs"),
            "out={}",
            out
        );
        assert!(
            out.contains("src/plugins/vcs_plugin/parser.rs"),
            "out={}",
            out
        );
    }

    #[test]
    fn append_inline_path_dictionary_does_not_replace_token_prefix_inside_longer_token() {
        let mut dict = DictionaryEngine::new();
        let short = dict.add_path_layered("src/legacy/old_bzr_adapter.rs");
        let short_token = short.split('/').next().unwrap_or_default().to_string();
        assert!(short_token.starts_with("$P"), "short={}", short);

        for i in 0..110 {
            let _ = dict.add_path_layered(&format!("tmp/path_overlap_{i}/file.rs"));
        }
        let long = dict.add_path_layered("samples/vcs/case_33_bzr_status.log");
        let long_token = long.split('/').next().unwrap_or_default().to_string();
        assert!(
            long_token.starts_with(&short_token) && long_token != short_token,
            "short_token={} long_token={} long={}",
            short_token,
            long_token,
            long
        );

        let rendered = format!(
            "bzr status\nA {}/case_33_bzr_status.log\nA {}/case_33_bzr_status.log\nR {}/old_bzr_adapter.rs\n",
            long_token, long_token, short_token
        );
        let out = append_inline_path_dictionary(rendered, &dict);

        assert!(!out.contains("src/legacy00/"), "out={}", out);
        assert!(out.contains("case_33_bzr_status.log"), "out={}", out);
    }

    #[test]
    fn compact_darcs_log_summary_flattens_to_one_line_per_commit() {
        // 已迁移到 vcs_darcs_plugin/tests.rs
    }

    #[test]
    fn compact_darcs_status_strips_dot_slash_prefixes() {
        // 已迁移到 vcs_darcs_plugin/tests.rs
    }

    #[test]
    fn vcs_tool_from_keyword_uses_route_aliases_for_cloud_vcs_commands() {
        assert_eq!(vcs_tool_from_keyword("git"), Some(VcsTool::Git));
        assert_eq!(vcs_tool_from_keyword("gh"), Some(VcsTool::Git));
        assert_eq!(vcs_tool_from_keyword("glab"), Some(VcsTool::Git));
        assert_eq!(vcs_tool_from_keyword("az"), Some(VcsTool::Git));
        assert_eq!(vcs_tool_from_keyword("bitbucket"), Some(VcsTool::Git));
        assert_eq!(vcs_tool_from_keyword("repo"), Some(VcsTool::Git));
        assert_eq!(vcs_tool_from_keyword("gerrit"), Some(VcsTool::Git));
        assert_eq!(vcs_tool_from_keyword("svn"), Some(VcsTool::Svn));
    }

    #[test]
    fn command_is_uses_route_parser_for_svn_global_options() {
        let raw = "svn --config-dir C:\\cfg --username bot update\nUpdated to revision 123.\n";
        assert!(command_is(raw, "svn", "update"));
    }

    #[test]
    fn command_is_uses_route_parser_for_hg_global_options() {
        let raw = "hg --repository C:\\repo --config ui.username=bot pull\nsearching for changes\n";
        assert!(command_is(raw, "hg", "pull"));
    }

    #[test]
    fn command_first_arg_is_uses_route_parser_for_git_global_options() {
        let raw = "git -C C:\\repo --git-dir=.git stash list\nstash@{0}: WIP on main\n";
        assert!(command_is(raw, "git", "stash"));
        assert!(command_first_arg_is(raw, "git", "stash", "list"));
    }

    #[test]
    fn explicit_intent_uses_route_config_for_non_git_tools() {
        let raw = "svn --config-dir C:\\cfg log --limit 3\nr1 | alice | 2026-01-01\n";
        assert_eq!(
            explicit_intent_from_command(Some(VcsTool::Svn), raw),
            Some(ExplicitIntent::Log)
        );
    }

    #[test]
    fn explicit_intent_uses_route_config_for_cloud_vcs_tools() {
        let raw = "gh --repo owner/repo pr list\n#12 open Fix build\n";
        assert_eq!(
            explicit_intent_from_command(Some(VcsTool::Git), raw),
            Some(ExplicitIntent::Other)
        );
    }
}
