//! git diff plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static DIFF_HEADER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^diff --git a/(?P<old>.*) b/(?P<new>.*)").unwrap());
static HUNK_HEADER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^@@ -(?P<os>\d+),(?P<ol>\d+) \+(?P<ns>\d+),(?P<nl>\d+) @@").unwrap());

impl GitDiffPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        GitDiffPlugin {
            name: "git_diff",
            priority: 160,
            config: GitDiffConfig::default(),
        }
    }
}

impl Plugin for GitDiffPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let mut score: f32 = 0.0;

        if text.contains("diff --git") {
            score += 0.6;
        }
        if text.contains("--- a/") {
            score += 0.2;
        }
        if text.contains("+++ b/") {
            score += 0.2;
        }
        if HUNK_HEADER_RE.is_match(text) {
            score += 0.5;
        }

        if score > 0.4 {
            Some(score.min(1.0f32))
        } else {
            None
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let mut result = bumpalo::collections::String::new_in(arena);

        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            if let Some(caps) = DIFF_HEADER_RE.captures(line) {
                let old_path = &caps["old"];
                let new_path = &caps["new"];
                result.push_str(
                    bumpalo::format!(in arena, "diff --git a/{} b/{}\n", old_path, new_path)
                        .into_bump_str(),
                );
                i += 1;
                while i < lines.len() && lines[i].starts_with("index ") {
                    result.push_str(lines[i]);
                    result.push('\n');
                    i += 1;
                }
                while i < lines.len()
                    && (lines[i].starts_with("--- ") || lines[i].starts_with("+++ "))
                {
                    result.push_str(lines[i]);
                    result.push('\n');
                    i += 1;
                }
                continue;
            }

            if let Some(caps) = HUNK_HEADER_RE.captures(line) {
                result.push_str(bumpalo::format!(in arena, "@@ -{},{} +{},{} @@\n", &caps["os"], &caps["ol"], &caps["ns"], &caps["nl"]).into_bump_str());
                i += 1;

                let mut context_queue = std::collections::VecDeque::new();
                let mut after_context_count = 0;
                let mut skipped_count = 0;

                while i < lines.len()
                    && !lines[i].starts_with("diff --git")
                    && !HUNK_HEADER_RE.is_match(lines[i])
                {
                    let hunk_line = lines[i];
                    if hunk_line.starts_with('+') || hunk_line.starts_with('-') {
                        if skipped_count > 0 {
                            result.push_str(bumpalo::format!(in arena, "  ... {} context lines suppressed ...\n", skipped_count).into_bump_str());
                            skipped_count = 0;
                        }
                        while let Some(ctx) = context_queue.pop_front() {
                            result.push_str("  ");
                            result.push_str(ctx);
                            result.push('\n');
                        }
                        result.push_str(hunk_line);
                        result.push('\n');
                        after_context_count = self.config.context_lines;
                    } else if hunk_line.starts_with(' ') {
                        let content = hunk_line.strip_prefix(' ').unwrap_or(hunk_line);
                        if after_context_count > 0 {
                            result.push_str("  ");
                            result.push_str(content);
                            result.push('\n');
                            after_context_count -= 1;
                        } else {
                            context_queue.push_back(content);
                            if context_queue.len() > self.config.context_lines {
                                context_queue.pop_front();
                                skipped_count += 1;
                            }
                        }
                    } else {
                        result.push_str(hunk_line);
                        result.push('\n');
                    }
                    i += 1;
                }

                if skipped_count > 0 {
                    result.push_str(bumpalo::format!(in arena, "  ... {} context lines suppressed ...\n", skipped_count).into_bump_str());
                }
                while let Some(ctx) = context_queue.pop_front() {
                    result.push_str("  ");
                    result.push_str(ctx);
                    result.push('\n');
                }
                continue;
            }

            result.push_str(line);
            result.push('\n');
            i += 1;
        }

        // 法则 A ROI 门控：短 diff（单行变更 / 极简 hunk）加一层「context 折叠」
        // 注释行反而扩张。参考 `docs/prompts/non_vcs_classical_prompts.md` § F.1.3。
        let compacted = result.into_bump_str();
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted.to_string());
        let final_in_arena = arena.alloc_str(&final_text);

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(final_in_arena))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<GitDiffConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}
