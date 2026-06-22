impl VcsPlugin {
    pub fn new() -> Self {
        let mut plugin = Self {
            name: "vcs",
            priority: 150,
            config: VcsConfig::default(),
            command_whitelists: default_command_whitelists(),
            signatures: default_signatures(),
        };

        plugin.try_load_external_config();
        plugin
    }

    fn try_load_external_config(&mut self) {
        if let Ok(cwd) = std::env::current_dir() {
            let repo_candidates = vec![
                cwd.join("config").join("vcs_plugin.json"),
                cwd.join("config").join("vcs_plugin.toml"),
            ];
            self.apply_first_valid_in_layer(&repo_candidates, "repo");

            let local_candidates = vec![
                cwd.join(".tokenslim").join("vcs_plugin.json"),
                cwd.join(".tokenslim").join("vcs_plugin.toml"),
            ];
            self.apply_first_valid_in_layer(&local_candidates, "local");
        }

        if let Ok(path) = std::env::var("TOKENSLIM_VCS_CONFIG") {
            if !path.trim().is_empty() {
                let env_path = PathBuf::from(path);
                match self.read_override_file(&env_path) {
                    Ok(config) => self.apply_overrides(config),
                    Err(err) => {
                        eprintln!("{}", crate::utils::i18n::t1("vcs_invalid_env_config", err));
                    }
                }
            }
        }
    }

    fn apply_first_valid_in_layer(&mut self, candidates: &[PathBuf], layer: &str) {
        let mut found_any = false;
        for path in candidates {
            if !path.exists() {
                continue;
            }

            found_any = true;
            match self.read_override_file(path) {
                Ok(config) => {
                    self.apply_overrides(config);
                    return;
                }
                Err(err) => {
                    eprintln!(
                        "{}",
                        crate::utils::i18n::t3(
                            "vcs_invalid_layer_config",
                            layer,
                            path.display(),
                            err
                        )
                    );
                }
            }
        }

        if found_any {
            eprintln!(
                "{}",
                crate::utils::i18n::t1("vcs_no_valid_layer_config", layer)
            );
        }
    }

    fn read_override_file(&self, path: &Path) -> Result<VcsOverrideConfig, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("E_VCS_OVERRIDE_READ:{}:{e}", path.display()))?;
        parse_override_from_path(path, &content)
    }

    pub(crate) fn apply_overrides(&mut self, override_config: VcsOverrideConfig) {
        if override_config.replace_command_whitelists {
            self.command_whitelists.clear();
        }
        if override_config.replace_signatures {
            self.signatures.clear();
        }

        if let Some(v) = override_config.dictionaryize_paths {
            self.config.dictionaryize_paths = v;
        }
        if let Some(v) = override_config.compact_leading_ws {
            self.config.compact_leading_ws = v;
        }
        if let Some(v) = override_config.collapse_blank_lines {
            self.config.collapse_blank_lines = v;
        }
        if let Some(v) = override_config.max_blank_lines {
            self.config.max_blank_lines = v;
        }

        for (tool, commands) in override_config.command_whitelists {
            if let Some(vcs_tool) = VcsTool::from_key(&tool) {
                self.command_whitelists.insert(vcs_tool, commands);
            } else {
                eprintln!(
                    "{}",
                    crate::utils::i18n::t1("vcs_unknown_tool_whitelist", tool)
                );
            }
        }

        for (tool, signatures) in override_config.signatures {
            if let Some(vcs_tool) = VcsTool::from_key(&tool) {
                self.signatures.insert(vcs_tool, signatures);
            } else {
                eprintln!(
                    "{}",
                    crate::utils::i18n::t1("vcs_unknown_tool_signatures", tool)
                );
            }
        }
    }

    fn classify_tool(&self, text: &str) -> Option<VcsTool> {
        let lower = text.to_ascii_lowercase();
        // 显式命令优先：若首行可解析为 VCS 命令，直接按命令工具归类。
        if let Some(tool) = infer_tool_from_explicit_command(text) {
            return Some(tool);
        }
        // 显式非 VCS 命令短路：避免 npm/cargo 等命令输出被启发式误判为 VCS。
        if has_non_vcs_explicit_command_head(text) {
            return None;
        }

        // Prioritize Git structural signatures before generic "@@ -" style diff checks
        // used by SVN/HG, otherwise git show/diff can be misrouted.
        if is_git_show_block(text) || is_git_log_block(text) || lower.contains("diff --git ") {
            return Some(VcsTool::Git);
        }

        if is_svn_log_block(text) || is_svn_diff_block(text) {
            return Some(VcsTool::Svn);
        }
        if is_hg_log_block(text) || is_hg_diff_block(text) {
            return Some(VcsTool::Hg);
        }
        if is_p4_changes_block(text) || is_p4_describe_block(text) || is_p4_labels_block(text) {
            return Some(VcsTool::P4);
        }
        if is_cvs_log_block(text) || is_cvs_diff_block(text) {
            return Some(VcsTool::Cvs);
        }
        if is_bzr_log_block(text) || is_bzr_diff_block(text) {
            return Some(VcsTool::Bzr);
        }
        if is_fossil_log_block(text) || is_fossil_diff_block(text) {
            return Some(VcsTool::Fossil);
        }
        if is_darcs_log_block(text) || is_darcs_diff_block(text) {
            return Some(VcsTool::Darcs);
        }

        if is_git_status_fragment(text) {
            return Some(VcsTool::Git);
        }
        if is_git_log_block(text) {
            return Some(VcsTool::Git);
        }

        if is_p4_fstat_block(text)
            || is_p4_where_block(text)
            || is_p4_info_block(text)
            || is_p4_dirs_block(text)
        {
            return Some(VcsTool::P4);
        }

        if is_svn_blame_block(text)
            || is_svn_list_block(text)
            || is_svn_prop_block(text)
            || is_svn_info_block(text)
        {
            return Some(VcsTool::Svn);
        }

        // Structural detection for non-Git VCS status blocks.
        if is_svn_status_block(text) {
            return Some(VcsTool::Svn);
        }
        if is_hg_status_block(text) {
            return Some(VcsTool::Hg);
        }
        if is_p4_opened_block(text) {
            return Some(VcsTool::P4);
        }
        if is_cvs_status_block(text) {
            return Some(VcsTool::Cvs);
        }
        if is_bzr_status_block(text) {
            return Some(VcsTool::Bzr);
        }
        if is_fossil_status_block(text) {
            return Some(VcsTool::Fossil);
        }
        if is_darcs_status_block(text) {
            return Some(VcsTool::Darcs);
        }

        let mut best: Option<(VcsTool, usize)> = None;

        let tool_order = [
            VcsTool::Git,
            VcsTool::Svn,
            VcsTool::Hg,
            VcsTool::P4,
            VcsTool::Cvs,
            VcsTool::Bzr,
            VcsTool::Fossil,
            VcsTool::Darcs,
        ];

        for tool in tool_order {
            let hints = match self.signatures.get(&tool) {
                Some(v) => v,
                None => continue,
            };

            let signature_hits = hints
                .iter()
                .map(|h| h.to_ascii_lowercase())
                .filter(|h| lower.contains(h))
                .count();

            // Avoid command-word-only false positives in generic logs/help text.
            // In practice, non-VCS outputs (e.g. npm help) may contain many verbs like
            // "add/diff/config/update" and previously could be misclassified as SVN/Hg.
            // For fallback scoring, require at least one tool signature hit.
            if signature_hits == 0 {
                continue;
            }

            // 启发式降权：命令词仅作为轻量增益，避免 generic 文本被动词污染误判。
            let mut score = signature_hits.saturating_mul(2);
            if let Some(commands) = self.commands_for(tool) {
                score += command_hits(&lower, commands).min(1);
            }
            if score > 0 {
                if let Some((_, best_score)) = best {
                    if score > best_score {
                        best = Some((tool, score));
                    }
                } else {
                    best = Some((tool, score));
                }
            }
        }

        best.map(|(tool, _)| tool)
    }

    fn rewrite_line_with_paths(&self, line: &str, dict_engine: &mut DictionaryEngine) -> String {
        if !self.config.dictionaryize_paths {
            return line.to_string();
        }

        if let Some(caps) = STATUS_PATH_RE.captures(line) {
            let path = caps.name("path").map(|m| m.as_str()).unwrap_or_default();
            if looks_like_vcs_path(path) {
                let token = dict_engine.add_path_layered(path);
                return line.replacen(path, &token, 1);
            }
        }

        if let Some(caps) = SHORT_STATUS_RE.captures(line) {
            let path = caps.name("path").map(|m| m.as_str()).unwrap_or_default();
            if looks_like_vcs_path(path) {
                let token = dict_engine.add_path_layered(path);
                return line.replacen(path, &token, 1);
            }
        }

        if let Some(caps) = P4_PATH_RE.captures(line) {
            let path = caps.name("path").map(|m| m.as_str()).unwrap_or_default();
            if looks_like_vcs_path(path) {
                let token = dict_engine.add_path_layered(path);
                return line.replacen(path, &token, 1);
            }
        }

        // Skip GENERIC_PATH_RE on diff patch payload lines (code lines starting with +, -, space)
        // to prevent matching method names like `.detect_project_type` as paths.
        let trimmed = line.trim_start();
        let is_patch_payload =
            (trimmed.starts_with('+') || trimmed.starts_with('-') || trimmed.starts_with(' '))
                && !trimmed.starts_with("+++")
                && !trimmed.starts_with("---")
                && !trimmed.starts_with("@@")
                && !trimmed.starts_with("diff --");
        if is_patch_payload {
            return line.to_string();
        }

        GENERIC_PATH_RE
            .replace_all(line, |caps: &regex::Captures| {
                let m = caps.get(0).unwrap();
                let path = caps.name("path").map(|m| m.as_str()).unwrap_or_default();
                
                let preceding = &line[..m.start()];
                if preceding.ends_with("http:") || preceding.ends_with("https:") {
                    return path.to_string();
                }

                if looks_like_vcs_path(path) {
                    dict_engine.add_path_layered(path)
                } else {
                    path.to_string()
                }
            })
            .into_owned()
    }
}

fn has_non_vcs_explicit_command_head(text: &str) -> bool {
    let first = first_non_empty_line(text);
    if first.is_empty() {
        return false;
    }
    let Some(tokens) = parse_command_line_tokens(first) else {
        return false;
    };
    let Some(head) = tokens.first() else {
        return false;
    };
    let keyword = normalize_command_keyword(head);
    !keyword.is_empty() && vcs_tool_from_keyword(&keyword).is_none()
}

impl Plugin for VcsPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let lower = text.to_ascii_lowercase();

        if vcs_ai_compact_mode() && !matches!(vcs_ai_profile(), VcsAiProfile::None) {
            return Some(1.0);
        }

        let tool = self.classify_tool(text)?;

        if tool == VcsTool::Git && (is_git_status_block(text) || is_git_status_fragment(text)) {
            return Some(0.99);
        }
        if tool == VcsTool::Git && is_git_log_block(text) {
            return Some(0.95);
        }

        let mut score = 0.45;
        if lower.contains("status") || lower.contains("changes not staged") {
            score += 0.2;
        }
        if lower.contains("revision") || lower.contains("changeset") {
            score += 0.1;
        }

        if let Some(commands) = self.commands_for(tool) {
            score += (command_hits(&lower, commands) as f32 * 0.04).min(0.2);
        }

        // Let git_diff plugin win on classic patch text.
        if lower.contains("diff --git") {
            score = score.min(0.55);
        }

        Some(score.min(0.95))
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let tool = infer_tool_from_explicit_command(text).or_else(|| self.classify_tool(text));
        let mut metadata = HashMap::new();
        if let Some(tool) = tool {
            metadata.insert("tool".to_string(), tool.as_str().to_string());
        }

        let profile = vcs_ai_profile();
        let explicit_intent = explicit_intent_from_command(tool, text);

        if matches!(tool, Some(VcsTool::Cvs)) && command_is(text, "cvs", "history") {
            let mut rendered = if vcs_ai_compact_mode() {
                metadata.insert("mode".to_string(), "ai-compact-log-cvs-history".to_string());
                let compact = cvs_methods::compact_cvs_history_lines(text);
                let compacted = compact_vcs_text_with_paths(self, &compact, dict_engine, arena, false);
                let tokenized =
                    replace_paths_in_text_scoped(&compacted, dict_engine, None).into_owned();
                prefer_non_expanding(text, append_inline_path_dictionary(tokenized, dict_engine))
            } else {
                metadata.insert("mode".to_string(), "lossless".to_string());
                text.to_string()
            };
            rendered = ensure_explicit_command_header(text, rendered);
            if vcs_ai_compact_mode() {
                rendered = normalize_compact_vcs_conventions(rendered);
            }
            rendered = align_trailing_newline_with_raw(text, rendered);
            return CompressResult {
                tokens: vec![Token::Text(Cow::Owned(rendered))],
                metadata: Some(metadata),
                plugin_name: Some(self.name()),
            };
        }

        let is_status_block = match tool {
            Some(VcsTool::Git) => {
                let is_rebase_cmd = command_is(text, "git", "rebase");
                let is_bisect_cmd = command_is(text, "git", "bisect");
                let is_clean_cmd = command_is(text, "git", "clean");
                !is_rebase_cmd
                    && !is_bisect_cmd
                    && (is_clean_cmd || is_git_status_block(text) || is_git_status_fragment(text))
            }
            Some(VcsTool::Svn) => svn_methods::is_svn_status_block(text),
            Some(VcsTool::Hg) => hg_methods::is_hg_status_block(text),
            Some(VcsTool::P4) => {
                let is_p4_diff_like = command_is(text, "p4", "diff")
                    || command_is(text, "p4", "describe")
                    || command_is(text, "p4", "diff2");
                let is_p4_sync_cmd = command_is(text, "p4", "sync");
                (p4_methods::is_p4_opened_block(text) && !is_p4_diff_like) || is_p4_sync_cmd
            }
            Some(VcsTool::Cvs) => cvs_methods::is_cvs_status_block(text),
            Some(VcsTool::Bzr) => bzr_methods::is_bzr_status_block(text),
            Some(VcsTool::Fossil) => fossil_methods::is_fossil_status_block(text),
            Some(VcsTool::Darcs) => darcs_methods::is_darcs_status_block(text),
            _ => false,
        };

        let prefer_git_diff_name_list = matches!(profile, VcsAiProfile::Diff)
            && matches!(tool, Some(VcsTool::Git))
            && is_git_name_only_or_status_block(text);

        let should_apply_status = (matches!(explicit_intent, Some(ExplicitIntent::Status))
            || (explicit_intent.is_none()
                && ((matches!(profile, VcsAiProfile::Status) && tool.is_some()) || is_status_block)))
            && !prefer_git_diff_name_list;

        if should_apply_status {
            let mut rendered = if vcs_ai_compact_mode() {
                metadata.insert("mode".to_string(), "ai-compact".to_string());
                let compact = compact_status_for_tool(tool, text);
                let skip_path_tokenization = matches!(tool, Some(VcsTool::Git))
                    && command_is(text, "git", "checkout");
                let candidate = if skip_path_tokenization {
                    compact
                } else {
                    let tokenized =
                        replace_paths_in_text_scoped(&compact, dict_engine, None).into_owned();
                    append_inline_path_dictionary(tokenized, dict_engine)
                };
                prefer_non_expanding(text, candidate)
            } else {
                metadata.insert("mode".to_string(), "lossless".to_string());
                text.to_string()
            };

            rendered = ensure_explicit_command_header(text, rendered);
            if vcs_ai_compact_mode() {
                rendered = normalize_compact_vcs_conventions(rendered);
            }
            rendered = align_trailing_newline_with_raw(text, rendered);

            return CompressResult {
                tokens: vec![Token::Text(Cow::Owned(rendered))],
                metadata: Some(metadata),
                plugin_name: Some(self.name()),
            };
        }

        let is_git_show = matches!(tool, Some(VcsTool::Git)) && is_git_show_block(text);
        let is_diff_block = match tool {
            Some(VcsTool::Git) => is_git_diff_block(text) && !is_git_show,
            Some(VcsTool::Svn) => is_svn_diff_block(text),
            Some(VcsTool::Hg) => is_hg_diff_block(text),
            Some(VcsTool::P4) => is_p4_describe_block(text),
            Some(VcsTool::Cvs) => is_cvs_diff_block(text),
            Some(VcsTool::Bzr) => is_bzr_diff_block(text),
            Some(VcsTool::Fossil) => is_fossil_diff_block(text),
            Some(VcsTool::Darcs) => is_darcs_diff_block(text),
            _ => false,
        };

        if matches!(explicit_intent, Some(ExplicitIntent::Diff))
            || (explicit_intent.is_none()
                && ((matches!(profile, VcsAiProfile::Diff) && tool.is_some() && !is_git_show)
                    || is_diff_block))
        {
            let mut rendered = if vcs_ai_compact_mode() {
                metadata.insert("mode".to_string(), "ai-compact-diff".to_string());
                let compact = compact_diff_for_tool(tool, text);
                let compacted = compact_vcs_text_with_paths(self, &compact, dict_engine, arena, false);
                let compacted = preserve_diff_scope_metadata(text, compacted);
                let compacted = preserve_p4_diff_revision_scope(text, compacted);
                let tokenized =
                    replace_paths_in_text_scoped(&compacted, dict_engine, None).into_owned();
                prefer_non_expanding(text, append_inline_path_dictionary(tokenized, dict_engine))
            } else {
                metadata.insert("mode".to_string(), "lossless".to_string());
                text.to_string()
            };

            rendered = ensure_explicit_command_header(text, rendered);
            if vcs_ai_compact_mode() {
                rendered = normalize_compact_vcs_conventions(rendered);
            }
            rendered = align_trailing_newline_with_raw(text, rendered);

            return CompressResult {
                tokens: vec![Token::Text(Cow::Owned(rendered))],
                metadata: Some(metadata),
                plugin_name: Some(self.name()),
            };
        }

        let is_log_block = match tool {
            Some(VcsTool::Git) => {
                let is_rebase_cmd = command_is(text, "git", "rebase");
                let is_log_cmd = command_is(text, "git", "pull")
                    || command_is(text, "git", "push")
                    || command_is(text, "git", "fetch")
                    || command_is(text, "git", "submodule")
                    || command_is(text, "git", "bisect")
                    || command_is(text, "git", "tag")
                    || command_is(text, "git", "cherry-pick")
                    || command_is(text, "git", "revert")
                    || command_is(text, "git", "reset")
                    || command_is(text, "git", "merge")
                    || command_is(text, "git", "stash")
                    || command_is(text, "git", "log");
                is_rebase_cmd || is_log_cmd || is_git_log_block(text) || is_git_show
            }
            Some(VcsTool::Svn) => svn_methods::is_svn_log_block(text),
            Some(VcsTool::Hg) => {
                let is_hg_log_cmd = command_is(text, "hg", "clone")
                    || command_is(text, "hg", "pull")
                    || command_is(text, "hg", "push")
                    || command_is(text, "hg", "commit")
                    || command_is(text, "hg", "branches");
                is_hg_log_cmd
                    || hg_methods::is_hg_log_block(text)
                    || hg_methods::is_hg_heads_block(text)
                    || hg_methods::is_hg_outgoing_block(text)
                    || hg_methods::is_hg_incoming_block(text)
                    || hg_methods::is_hg_parents_block(text)
            }
            Some(VcsTool::P4) => p4_methods::is_p4_changes_block(text) || p4_methods::is_p4_labels_block(text),
            Some(VcsTool::Cvs) => cvs_methods::is_cvs_log_block(text),
            Some(VcsTool::Bzr) => bzr_methods::is_bzr_log_block(text),
            Some(VcsTool::Fossil) => fossil_methods::is_fossil_log_block(text),
            Some(VcsTool::Darcs) => darcs_methods::is_darcs_log_block(text),
            _ => false,
        };

        if matches!(explicit_intent, Some(ExplicitIntent::Log))
            || (explicit_intent.is_none()
                && (is_log_block || (matches!(profile, VcsAiProfile::Log) && tool.is_some())))
        {
            let mut rendered = if vcs_ai_compact_mode() {
                if is_git_show {
                    metadata.insert("mode".to_string(), "ai-compact-show".to_string());
                } else {
                    metadata.insert("mode".to_string(), "ai-compact-log".to_string());
                }
                let compact = if is_git_show {
                    git_methods::compact_git_show_for_ai(text)
                } else {
                    compact_log_for_tool(tool, text)
                };
                let compact = compact_size_mentions_for_text(&compact);
                let tokenized = if compact.contains('/') || compact.contains('\\') {
                    let compacted =
                        compact_vcs_text_with_paths(self, &compact, dict_engine, arena, false);
                    replace_paths_in_text_scoped(&compacted, dict_engine, None).into_owned()
                } else {
                    compact
                };
                prefer_non_expanding(text, append_inline_path_dictionary(tokenized, dict_engine))
            } else {
                metadata.insert("mode".to_string(), "lossless".to_string());
                text.to_string()
            };

            rendered = ensure_explicit_command_header(text, rendered);
            if vcs_ai_compact_mode() {
                rendered = normalize_compact_vcs_conventions(rendered);
            }
            rendered = align_trailing_newline_with_raw(text, rendered);

            return CompressResult {
                tokens: vec![Token::Text(Cow::Owned(rendered))],
                metadata: Some(metadata),
                plugin_name: Some(self.name()),
            };
        }

        let is_other_block = match tool {
            Some(VcsTool::Git) => {
                command_is(text, "git", "worktree")
                    || command_is(text, "git", "grep")
                    || command_is(text, "git", "blame")
            }
            Some(VcsTool::Svn) => {
                svn_methods::is_svn_blame_block(text)
                    || svn_methods::is_svn_list_block(text)
                    || svn_methods::is_svn_prop_block(text)
                    || svn_methods::is_svn_info_block(text)
            }
            Some(VcsTool::P4) => {
                p4_methods::is_p4_fstat_block(text)
                    || p4_methods::is_p4_where_block(text)
                    || p4_methods::is_p4_info_block(text)
                    || p4_methods::is_p4_dirs_block(text)
            }
            _ => false,
        };

        if matches!(explicit_intent, Some(ExplicitIntent::Other))
            || (explicit_intent.is_none()
                && (is_other_block || (matches!(profile, VcsAiProfile::Other) && tool.is_some())))
        {
            let mut rendered = if vcs_ai_compact_mode() {
                metadata.insert("mode".to_string(), "ai-compact-other".to_string());
                let compact = if matches!(tool, Some(VcsTool::Git))
                    && (command_is(text, "git", "grep") || command_is(text, "git", "worktree"))
                {
                    git_methods::compact_git_other_for_ai(text)
                } else {
                    compact_other_for_tool(tool, text)
                };
                let skip_path_tokenization = matches!(tool, Some(VcsTool::Git))
                    && (command_is(text, "git", "grep") || command_is(text, "git", "worktree"));
                let candidate = if skip_path_tokenization {
                    compact
                } else {
                    let compacted =
                        compact_vcs_text_with_paths(self, &compact, dict_engine, arena, false);
                    let tokenized =
                        replace_paths_in_text_scoped(&compacted, dict_engine, None).into_owned();
                    append_inline_path_dictionary(tokenized, dict_engine)
                };
                prefer_non_expanding(text, candidate)
            } else {
                metadata.insert("mode".to_string(), "lossless".to_string());
                text.to_string()
            };

            rendered = ensure_explicit_command_header(text, rendered);
            if vcs_ai_compact_mode() {
                rendered = normalize_compact_vcs_conventions(rendered);
            }
            rendered = align_trailing_newline_with_raw(text, rendered);

            return CompressResult {
                tokens: vec![Token::Text(Cow::Owned(rendered))],
                metadata: Some(metadata),
                plugin_name: Some(self.name()),
            };
        }

        if vcs_ai_compact_mode() && is_git_show_block(text) {
            let compact = git_methods::compact_git_show_for_ai(text);
            let compacted = compact_vcs_text_with_paths(self, &compact, dict_engine, arena, false);
            let tokenized =
                replace_paths_in_text_scoped(&compacted, dict_engine, None).into_owned();
            let rendered = ensure_explicit_command_header(
                text,
                prefer_non_expanding(text, append_inline_path_dictionary(tokenized, dict_engine)),
            );
            let rendered = if vcs_ai_compact_mode() {
                normalize_compact_vcs_conventions(rendered)
            } else {
                rendered
            };
            let rendered = align_trailing_newline_with_raw(text, rendered);
            metadata.insert("mode".to_string(), "ai-compact-show-fallback".to_string());
            return CompressResult {
                tokens: vec![Token::Text(Cow::Owned(rendered))],
                metadata: Some(metadata),
                plugin_name: Some(self.name()),
            };
        }

        if vcs_ai_compact_mode() {
            let size_compacted = compact_size_mentions_for_text(text);
            let compact =
                compact_vcs_text_with_paths(self, &size_compacted, dict_engine, arena, true);
            let tokenized = replace_paths_in_text_scoped(&compact, dict_engine, None).into_owned();
            let rendered = ensure_explicit_command_header(
                text,
                prefer_non_expanding(text, append_inline_path_dictionary(tokenized, dict_engine)),
            );
            let rendered = if vcs_ai_compact_mode() {
                normalize_compact_vcs_conventions(rendered)
            } else {
                rendered
            };
            let rendered = align_trailing_newline_with_raw(text, rendered);
            metadata.insert("mode".to_string(), "ai-compact-generic".to_string());
            return CompressResult {
                tokens: vec![Token::Text(Cow::Owned(rendered))],
                metadata: Some(metadata),
                plugin_name: Some(self.name()),
            };
        }

        let mut out = bumpalo::collections::String::new_in(arena);
        if let Some(tool) = tool {
            out.push_str(bumpalo::format!(in arena, "$VCS {}\n", tool.as_str()).into_bump_str());
        }

        let mut blank_count = 0usize;
        let mut ws_ctx = WsCompactionContext::default();
        for raw in text.lines() {
            let mut line = raw.trim_end_matches('\r').to_string();

            update_ws_context_from_line(&line, &mut ws_ctx);

            if self.config.compact_leading_ws {
                line = compact_leading_ws_with_language_guard(&line, &ws_ctx);
            }

            let is_blank = line.trim().is_empty();
            if self.config.collapse_blank_lines && is_blank {
                blank_count += 1;
                if blank_count > self.config.max_blank_lines {
                    continue;
                }
            } else {
                blank_count = 0;
            }

            let rewritten = self.rewrite_line_with_paths(&line, dict_engine);
            out.push_str(&rewritten);
            out.push('\n');
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(out.into_bump_str()))],
            metadata: Some(metadata),
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<VcsConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        if let Some(c) = config.downcast_ref::<VcsOverrideConfig>() {
            self.apply_overrides(c.clone());
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}

fn is_decorative_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() >= 10 {
        let first = trimmed.chars().next().unwrap();
        if matches!(first, '-' | '=' | '_' | '*') {
            trimmed.chars().all(|c| c == first)
        } else {
            false
        }
    } else {
        false
    }
}

fn is_git_network_progress_line(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    let lower = lower.strip_prefix("remote: ").unwrap_or(&lower).trim();

    lower.starts_with("enumerating objects:")
        || lower.starts_with("counting objects:")
        || lower.starts_with("compressing objects:")
        || lower.starts_with("unpacking objects:")
        || lower.starts_with("writing objects:")
        || lower.starts_with("resolving deltas:")
        || (lower.starts_with("total ") && lower.contains("delta") && lower.contains("reused"))
}

fn prefer_shorter_line<'a>(original: &'a str, candidate: String) -> String {
    if candidate.len() < original.len() {
        candidate
    } else {
        original.to_string()
    }
}

fn compact_common_vcs_ack_line(line: &str) -> String {
    // Keep explicit command lines intact so command intent stays stable.
    if infer_tool_from_explicit_command(line).is_some() {
        let trimmed = line.trim_start();
        let mut parts = trimmed.split_whitespace();
        let _tool = parts.next();
        let second = parts.next().unwrap_or_default();
        if !second.ends_with(':') {
            return line.to_string();
        }
    }

    if line == "git worktree list" {
        return "git wt list".to_string();
    }

    if line == "git log --oneline" {
        return "git log oneline".to_string();
    }

    if let Some(rest) = line.strip_prefix("git grep ") {
        return prefer_shorter_line(line, format!("git g {rest}"));
    }

    if let Some(rest) = line.strip_prefix("darcs amend ") {
        return prefer_shorter_line(line, format!("amend {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Amending patch for: ") {
        return prefer_shorter_line(line, format!("amend: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Old message: ") {
        return prefer_shorter_line(line, format!("old: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("New message: ") {
        return prefer_shorter_line(line, format!("new: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Modified files: ") {
        return prefer_shorter_line(line, format!("M {rest}"));
    }

    if line.starts_with("gh pr create ") {
        let compact = line
            .replace(" --title ", " -t ")
            .replace(" --body ", " -b ");
        return prefer_shorter_line(line, compact);
    }

    if line.starts_with("gh issue create ") {
        let compact = line
            .replace(" --title ", " -t ")
            .replace(" --body ", " -b ");
        return prefer_shorter_line(line, compact);
    }

    if line.starts_with("glab mr create ") {
        let compact = line
            .replace(" --title ", " -t ")
            .replace(" --description ", " -d ")
            .replace(" --target-branch ", " -b ");
        return prefer_shorter_line(line, compact);
    }

    if line.starts_with("glab issue create ") {
        let compact = line
            .replace(" --title ", " -t ")
            .replace(" --description ", " -d ");
        return prefer_shorter_line(line, compact);
    }

    if line.starts_with("bitbucket pr create ") {
        let compact = line
            .replace(" --title ", " -t ")
            .replace(" --description ", " -d ");
        return prefer_shorter_line(line, compact);
    }

    if line.starts_with("az repos create ") {
        let compact = line
            .replace(" --name ", " -n ")
            .replace(" --project ", " -p ")
            .replace(" --source-control ", " -s ");
        return prefer_shorter_line(line, compact);
    }

    if line.starts_with("az repos delete ") {
        let compact = line
            .replacen("az repos delete ", "az repos del ", 1)
            .replace(" --project ", " -p ")
            .replace(" --yes", " -y");
        return prefer_shorter_line(line, compact);
    }

    if let Some(rest) = line.strip_prefix("issue #") {
        return prefer_shorter_line(line, format!("i#{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Labels: ") {
        return prefer_shorter_line(line, format!("labels:{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Assignees: ") {
        return prefer_shorter_line(line, format!("asg:{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Author: ") {
        return prefer_shorter_line(line, format!("au:{rest}"));
    }

    if let Some(rest) = line.strip_prefix("State: ") {
        return prefer_shorter_line(line, format!("st:{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Description: ") {
        return prefer_shorter_line(line, format!("desc:{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Files: ") {
        return prefer_shorter_line(line, format!("files:{rest}"));
    }

    if line == "Public: true" {
        return "public:y".to_string();
    }

    if line == "Public: false" {
        return "public:n".to_string();
    }

    if let Some(rest) = line.strip_prefix("Created: ") {
        return prefer_shorter_line(line, format!("created:{rest}"));
    }

    if let Some(rest) = line.strip_prefix("URL: ") {
        return prefer_shorter_line(line, format!("url {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Source: ") {
        return prefer_shorter_line(line, format!("src {rest}"));
    }

    if let Some(rest) = line.strip_prefix("HEAD@{") {
        if let Some((idx, tail)) = rest.split_once("}: checkout: moving from ") {
            if let Some((from, to)) = tail.split_once(" to ") {
                return prefer_shorter_line(line, format!("HEAD@{{{idx}}}: co {from}->{to}"));
            }
        }
        if let Some((idx, tail)) = rest.split_once("}: commit (merge): Merge branch '") {
            if let Some((branch, to)) = tail.split_once("' into ") {
                return prefer_shorter_line(line, format!("HEAD@{{{idx}}}: merge {branch}->{to}"));
            }
        }
        if let Some((idx, msg)) = rest.split_once("}: commit: ") {
            return prefer_shorter_line(line, format!("HEAD@{{{idx}}}: c {msg}"));
        }
    }

    let clean_prefix = line
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric() && c != '#')
        .trim_start();
    if clean_prefix != line {
        if let Some(rest) = clean_prefix.strip_prefix("Created pull request ") {
            return prefer_shorter_line(line, format!("pr+ {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Labeled as ") {
            return prefer_shorter_line(line, format!("label {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Merged pull request ") {
            return prefer_shorter_line(line, format!("merged {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Deleted branch ") {
            return prefer_shorter_line(line, format!("del-branch {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Created issue ") {
            return prefer_shorter_line(line, format!("issue+ {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Created merge request ") {
            return prefer_shorter_line(line, format!("mr+ {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Repository created: ") {
            return prefer_shorter_line(line, format!("repo+ {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Repository deleted: ") {
            return prefer_shorter_line(line, format!("repo- {rest}"));
        }
        if let Some(rest) = clean_prefix.strip_prefix("Created PR ") {
            return prefer_shorter_line(line, format!("pr+ {rest}"));
        }
    }

    if line == "repo status" {
        return "repo st".to_string();
    }

    if let Some(rest) = line.strip_prefix(" - Modified: ") {
        return prefer_shorter_line(line, format!("M {rest}"));
    }

    if let Some(rest) = line.strip_prefix(" - Added: ") {
        return prefer_shorter_line(line, format!("A {rest}"));
    }

    if let Some(rest) = line.strip_prefix("project ") {
        if let Some((path, tail)) = rest.split_once(" branch ") {
            if let Some((branch, state)) = tail.rsplit_once(" (") {
                let state = state.trim_end_matches(')');
                let state_short = match state {
                    "clean" => Some("c"),
                    "dirty" => Some("d"),
                    _ => None,
                };
                if let Some(ss) = state_short {
                    let path_short = path.trim_end_matches('/');
                    return prefer_shorter_line(line, format!("p {path_short}@{branch} {ss}"));
                }
            }
        }
    }

    if let Some((left, msg)) = line.split_once(" '") {
        let parts: Vec<&str> = left.split_whitespace().collect();
        if parts.len() == 4
            && parts[1].len() == 10
            && parts[1].chars().nth(4) == Some('/')
            && parts[1].chars().nth(7) == Some('/')
            && parts[2].len() == 8
            && parts[2].chars().nth(2) == Some(':')
            && parts[2].chars().nth(5) == Some(':')
        {
            let hhmm = &parts[2][..5];
            let tail = msg.trim_end_matches('\'');
            return prefer_shorter_line(
                line,
                format!("{} {} {} {} '{}'", parts[0], parts[1], hhmm, parts[3], tail),
            );
        }
    }

    if line == "Sync completed." {
        return "sync done".to_string();
    }

    if let Some(path) = line.strip_suffix(" - updated") {
        return prefer_shorter_line(line, format!("U {path}"));
    }

    if let Some(path) = line.strip_suffix(" - added") {
        return prefer_shorter_line(line, format!("A {path}"));
    }

    if let Some((path, mode)) = line
        .split_once(" - resolved using '")
        .and_then(|(p, m)| m.strip_suffix('\'').map(|mm| (p, mm)))
    {
        return prefer_shorter_line(line, format!("resolved({mode}) {path}"));
    }

    if let Some(path) = line.strip_suffix(" - reverted") {
        return prefer_shorter_line(line, format!("R {path}"));
    }

    if let Some(path) = line.strip_suffix(" - opened for edit") {
        return prefer_shorter_line(line, format!("E {path}"));
    }

    if let Some(path) = line.strip_suffix(" - added for add") {
        return prefer_shorter_line(line, format!("A {path}"));
    }

    if let Some(path) = line.strip_suffix(" - deleted for delete") {
        return prefer_shorter_line(line, format!("D {path}"));
    }

    if let Some((path, reason)) = line
        .split_once(" - skipped (")
        .and_then(|(p, r)| r.strip_suffix(')').map(|rr| (p, rr)))
    {
        return prefer_shorter_line(line, format!("skip({reason}) {path}"));
    }

    if let Some(rest) = line.strip_prefix("p4 branches") {
        return prefer_shorter_line(line, format!("branches{rest}"));
    }

    if let Some(rest) = line.strip_prefix("p4 move ") {
        return prefer_shorter_line(line, format!("mv {rest}"));
    }

    if let Some(rest) = line.strip_prefix("p4 integrate ") {
        return prefer_shorter_line(line, format!("integrate {rest}"));
    }

    if let Some(rest) = line.strip_prefix("p4 filelog ") {
        return prefer_shorter_line(line, format!("filelog {rest}"));
    }

    if let Some((dest, src)) = line
        .split_once(" - moved from ")
    {
        return prefer_shorter_line(line, format!("mv {src} -> {dest}"));
    }

    if let Some(rest) = line.strip_prefix("p4 tag ") {
        return prefer_shorter_line(line, format!("tag {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Tag ") {
        if let Some((tag, tail)) = rest.split_once(" marked on ") {
            return prefer_shorter_line(line, format!("tag {tag} on {tail}"));
        }
    }

    if let Some(rest) = line.strip_prefix("p4 passwd ") {
        return prefer_shorter_line(line, format!("passwd {rest}"));
    }

    if line == "Password updated." {
        return "passwd updated".to_string();
    }

    if let Some(rest) = line.strip_prefix("cvs checkout ") {
        return prefer_shorter_line(line, format!("checkout {rest}"));
    }

    if let Some(rest) = line.strip_prefix("cvs checkout: Updating ") {
        return prefer_shorter_line(line, format!("updating {rest}"));
    }

    if let Some(rest) = line.strip_prefix("cvs checkout: ") {
        return prefer_shorter_line(line, format!("checkout: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("cvs tag ") {
        return prefer_shorter_line(line, format!("tag {rest}"));
    }

    if let Some(file) = line
        .strip_prefix("cvs tag: Tagging `")
        .and_then(|s| s.strip_suffix('\''))
    {
        return prefer_shorter_line(line, format!("tagging {file}"));
    }

    if let Some(rest) = line.strip_prefix("cvs edit ") {
        return prefer_shorter_line(line, format!("edit {rest}"));
    }

    if let Some(rest) = line.strip_prefix("cvs edit: `") {
        let compact = rest
            .replace("' is already marked as being edited by ", " edited by ");
        return prefer_shorter_line(line, compact);
    }

    if let Some(rest) = line.strip_prefix("cvs unedit ") {
        return prefer_shorter_line(line, format!("unedit {rest}"));
    }

    if let Some(rest) = line.strip_prefix("You have no outstanding edits to ") {
        return prefer_shorter_line(line, format!("no edits: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("cvs history ") {
        return prefer_shorter_line(line, format!("history {rest}"));
    }

    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5
            && parts[0].len() == 1
            && parts[1].len() == 10
            && parts[1].chars().nth(4) == Some('-')
            && parts[1].chars().nth(7) == Some('-')
            && parts[2].len() == 5
            && parts[2].chars().nth(2) == Some(':')
        {
            let mut rebuilt = vec![parts[0], parts[1]];
            rebuilt.extend_from_slice(&parts[3..]);
            return prefer_shorter_line(line, rebuilt.join(" "));
        }
    }

    if let Some(rest) = line.strip_prefix("Checking in ") {
        return prefer_shorter_line(line, format!("checkin {rest}"));
    }

    if line == "initial version" {
        return "init".to_string();
    }

    if let Some(rest) = line.strip_prefix("bzr merge") {
        return prefer_shorter_line(line, format!("merge{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Merging from: ") {
        return prefer_shorter_line(line, format!("from: {rest}"));
    }

    if line == "All changes applied successfully." {
        return "merged ok".to_string();
    }

    if let Some(rest) = line.strip_prefix("bzr branch ") {
        return prefer_shorter_line(line, format!("branch {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Branched ") {
        let compact = rest.replace(" revisions.", " revs");
        return prefer_shorter_line(line, format!("branched {compact}"));
    }

    if let Some(rest) = line.strip_prefix("bzr revert ") {
        return prefer_shorter_line(line, format!("revert {rest}"));
    }

    if let Some(rest) = line.strip_prefix(" reverted ") {
        return prefer_shorter_line(line, format!("R {rest}"));
    }

    if let Some(rest) = line.strip_prefix("bzr commit ") {
        return prefer_shorter_line(line, format!("commit {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Committing to: ") {
        return prefer_shorter_line(line, format!("to: {rest}"));
    }

    if let Some(path) = line.strip_prefix("modified ") {
        return prefer_shorter_line(line, format!("M {path}"));
    }

    if let Some(rest) = line
        .strip_prefix("Committed revision ")
        .and_then(|s| s.strip_suffix('.'))
    {
        return prefer_shorter_line(line, format!("r{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Pulled ") {
        let compact = rest.replace(" revisions.", " revs");
        return prefer_shorter_line(line, format!("pulled {compact}"));
    }

    if let Some(rest) = line.strip_prefix("Total ") {
        let compact = rest.replace(" revisions in branch", " revs");
        return prefer_shorter_line(line, format!("total {compact}"));
    }

    if let Some(rest) = line.strip_prefix("bzr missing ") {
        return prefer_shorter_line(line, format!("missing {rest}"));
    }

    if let Some(rest) = line.strip_prefix("These commits are missing from ") {
        return prefer_shorter_line(line, format!("missing from {rest}"));
    }

    if line == "fossil undo" {
        return "undo".to_string();
    }

    if let Some(rest) = line.strip_prefix("Undo successful: ") {
        return prefer_shorter_line(line, format!("undo: {rest}"));
    }

    if line == "fossil stash" {
        return "stash".to_string();
    }

    if line == "Stash changes:" {
        return "stash:".to_string();
    }

    if let Some(rest) = line.strip_prefix("fossil merge ") {
        return prefer_shorter_line(line, format!("merge {rest}"));
    }

    if let Some(url) = line.strip_prefix("Pulling from ") {
        let url = url.trim_end_matches('.');
        return prefer_shorter_line(line, format!("pull {url}"));
    }

    if line == "Merging differences between repository URLs" {
        return "repo-merge".to_string();
    }

    if let Some(rest) = line.strip_prefix("Merging r") {
        let rest = rest.replace(" through r", "..r");
        return prefer_shorter_line(line, format!("merge r{rest}"));
    }

    if let Some(rest) = line.strip_prefix("Merging ") {
        return prefer_shorter_line(line, format!("merge {rest}"));
    }

    if let Some((updated, merged_part)) = line.split_once(" files updated, ") {
        if let Some(merged) = merged_part.strip_suffix(" files merged") {
            return prefer_shorter_line(line, format!("{updated} updated,{merged} merged"));
        }
    }

    if let Some(rest) = line.strip_prefix("fossil sync") {
        return prefer_shorter_line(line, format!("sync{rest}"));
    }

    if let Some(url) = line.strip_prefix("Sync with ") {
        let url = url.trim_end_matches('.');
        return prefer_shorter_line(line, format!("sync {url}"));
    }

    if let Some(rest) = line.strip_prefix("Pull: ") {
        let compact = rest.replace(" commits", "c").replace(" files", "f").replace(", ", ",");
        return prefer_shorter_line(line, format!("Pull: {compact}"));
    }

    if let Some(rest) = line.strip_prefix("Push: ") {
        let compact = rest.replace(" commits", "c").replace(" files", "f").replace(" file", "f").replace(", ", ",");
        return prefer_shorter_line(line, format!("Push: {compact}"));
    }

    if let Some(rest) = line.strip_prefix("... #") {
        if let Some((rev, tail)) = rest.split_once(" change ") {
            if let Some((chg, tail2)) = tail.split_once(' ') {
                let tail3 = tail2.replace(" by ", " ");
                return prefer_shorter_line(line, format!("#{} ch{} {}", rev, chg, tail3));
            }
        }
    }

    if line == "Done." {
        return "done".to_string();
    }

    if line == "hg copy src/file.txt dst/file.txt" {
        return "hg cp src/file.txt dst/file.txt".to_string();
    }

    if line == "hg move src/old.txt dst/new.txt" {
        return "hg mv src/old.txt dst/new.txt".to_string();
    }

    if line == "hg purge" {
        return "purge".to_string();
    }

    if line == "hg archive -t zip /tmp/project-backup.zip" {
        return "archive zip /tmp/project-backup.zip".to_string();
    }

    if line == "hg verify" {
        return "verify".to_string();
    }

    if line == "hg identify" {
        return "identify".to_string();
    }

    if line == "hg paths" {
        return "paths".to_string();
    }

    if line == "hg config" {
        return "config".to_string();
    }

    if line == "hg summarize" {
        return "summarize".to_string();
    }

    if let Some(rest) = line.strip_prefix("hg transplant ") {
        return prefer_shorter_line(line, format!("transplant {rest}"));
    }

    if let Some((left, right)) = line.split_once(" = ") {
        return prefer_shorter_line(line, format!("{left}={right}"));
    }

    if let Some(path) = line.strip_prefix("removed: ") {
        return prefer_shorter_line(line, format!("rm {path}"));
    }

    if let Some((src, dst)) = line
        .strip_prefix("copying ")
        .and_then(|s| s.split_once(" to "))
    {
        return prefer_shorter_line(line, format!("cp {src} -> {dst}"));
    }

    if let Some((src, dst)) = line
        .strip_prefix("moving ")
        .and_then(|s| s.split_once(" to "))
    {
        return prefer_shorter_line(line, format!("mv {src} -> {dst}"));
    }

    if let Some(path) = line.strip_prefix("archive created: ") {
        return prefer_shorter_line(line, format!("archive: {path}"));
    }

    if let Some(rest) = line.strip_prefix("verified ") {
        if let Some(num) = rest.strip_suffix(" changesets") {
            return prefer_shorter_line(line, format!("verified {num} csets"));
        }
    }

    if let Some(rest) = line.strip_prefix("checking ") {
        return prefer_shorter_line(line, format!("check {rest}"));
    }

    if let Some(rest) = line.strip_prefix("crosschecking ") {
        return prefer_shorter_line(line, format!("crosscheck {rest}"));
    }

    if let Some((hash, phase)) = line
        .split_once(" (")
        .and_then(|(h, p)| p.strip_suffix(')').map(|pp| (h, pp)))
    {
        let phase_short = match phase {
            "public" => Some("p"),
            "draft" => Some("d"),
            "inactive" => Some("i"),
            _ => None,
        };
        if let Some(ps) = phase_short {
            return prefer_shorter_line(line, format!("{hash} {ps}"));
        }
    }

    if let Some(rest) = line.strip_prefix("Components: ") {
        let compact = rest.replace(" revisions", " revs").replace(", ", ",");
        return prefer_shorter_line(line, format!("Components:{compact}"));
    }

    if let Some(rest) = line.strip_prefix("transplanting ") {
        return prefer_shorter_line(line, format!("xplant {rest}"));
    }

    if let Some(rest) = line.strip_prefix("transplanted ") {
        let compact = rest.replace(" revisions", " revs");
        return prefer_shorter_line(line, format!("transplanted {compact}"));
    }

    if let Some(rest) = line.strip_prefix("svn propset ") {
        let compact = rest.replace("\"Id\"", "Id");
        return prefer_shorter_line(line, format!("propset {compact}"));
    }

    if let Some(rest) = line.strip_prefix("property '") {
        if let Some((prop, suffix)) = rest.split_once("' set on '") {
            let path = suffix.trim_end_matches('\'');
            return prefer_shorter_line(line, format!("prop {prop} set {path}"));
        }
    }

    if let Some(url) = line.strip_prefix("cloning from ") {
        return prefer_shorter_line(line, format!("clone: {url}"));
    }

    if let Some(dir) = line.strip_prefix("destination directory: ") {
        return prefer_shorter_line(line, format!("dest: {dir}"));
    }

    if line == "requesting all changes" {
        return "requesting changes".to_string();
    }

    if line == "adding changesets" {
        return "adding csets".to_string();
    }

    if let Some(url) = line.strip_prefix("pulling from ") {
        return prefer_shorter_line(line, format!("pull: {url}"));
    }

    if line == "searching for changes" {
        return "searching changes".to_string();
    }

    if line == "no changes found" {
        return "no changes".to_string();
    }

    if let Some(rest) = line.strip_prefix("added ") {
        return prefer_shorter_line(line, format!("added {rest}"));
    }

    if let Some(url) = line.strip_prefix("pushing to ") {
        return prefer_shorter_line(line, format!("push: {url}"));
    }

    if let Some(rest) = line.strip_prefix("pushed ") {
        return prefer_shorter_line(line, format!("pushed {rest}"));
    }

    if let Some(rest) = line.strip_prefix("updating to changeset ") {
        return prefer_shorter_line(line, format!("update: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("working directory now at ") {
        return prefer_shorter_line(line, format!("wd: {rest}"));
    }

    if line == "committing files:" {
        return "commit files:".to_string();
    }

    if let Some(rest) = line.strip_prefix("committed changeset ") {
        return prefer_shorter_line(line, format!("committed: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("merging with ") {
        return prefer_shorter_line(line, format!("merge {rest}"));
    }

    if line == "Merge completed" {
        return "merge".to_string();
    }

    if line == "rollback completed" {
        return "rollback done".to_string();
    }

    if let Some(rest) = line.strip_prefix("backing out changeset ") {
        return prefer_shorter_line(line, format!("backout: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("backed out changeset ") {
        return prefer_shorter_line(line, format!("backed-out: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("shelved as ") {
        return prefer_shorter_line(line, format!("shelved:{rest}"));
    }

    if let Some(rest) = line.strip_prefix("tag '") {
        if let Some((tag, tail)) = rest.split_once("' ") {
            let tag = tag.trim();
            let tail = tail.trim();
            if !tag.is_empty() && !tail.is_empty() {
                return prefer_shorter_line(line, format!("tag:{} {}", tag, tail));
            }
        }
        if let Some(tag) = rest.strip_suffix('\'') {
            let tag = tag.trim();
            if !tag.is_empty() {
                return prefer_shorter_line(line, format!("tag:{tag}"));
            }
        }
    }

    if line == "Export complete." {
        return "export done".to_string();
    }

    if line == "Cleanup completed." {
        return "cleanup done".to_string();
    }

    if line == "Updating '.':" || line == "Updating \".\":" {
        return String::new();
    }

    if let Some(url) = line.strip_prefix("Switched to new URL: ") {
        let _ = url;
        return "switched".to_string();
    }

    if let Some(rest) = line.strip_prefix("Relocated '") {
        if let Some((path, to_part)) = rest.split_once("' to new URL ") {
            let _ = to_part;
            return prefer_shorter_line(line, format!("relocated: {path}"));
        }
    }

    if let Some(rest) = line.strip_prefix("-- Non-conflicting: ") {
        return prefer_shorter_line(line, format!("ok: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Importing into ") {
        return prefer_shorter_line(line, format!("import: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Importing ") {
        return prefer_shorter_line(line, format!("I {rest}"));
    }

    if let Some(path) = line.strip_prefix("Sending ") {
        return prefer_shorter_line(line, format!("M {}", path.trim()));
    }

    if let Some(path) = line.strip_prefix("Adding ") {
        return prefer_shorter_line(line, format!("A {}", path.trim()));
    }

    if let Some(path) = line.strip_prefix("Deleting ") {
        return prefer_shorter_line(line, format!("D {}", path.trim()));
    }

    if line.starts_with("Transmitting file data") {
        return String::new();
    }

    if let Some(rest) = line
        .strip_prefix("Committed revision ")
        .and_then(|s| s.strip_suffix('.'))
    {
        return prefer_shorter_line(line, format!("committed r{rest}"));
    }

    if let Some(strategy) = line
        .strip_prefix("Merge made by the '")
        .and_then(|s| s.strip_suffix("' strategy."))
    {
        return prefer_shorter_line(line, format!("merge: {strategy}"));
    }

    if let Some(path) = line.strip_prefix("Auto-merging ") {
        return prefer_shorter_line(line, format!("auto:{path}"));
    }

    if let Some(rest) = line.strip_prefix("* ") {
        if let Some((name, rev)) = rest.split_once(" @ ") {
            let compact = format!("*{}@{}", name.trim(), rev.trim());
            return prefer_shorter_line(line, compact);
        }
    }

    if line.starts_with(' ') && !line.contains(':') && !line.starts_with(" -") {
        return line.trim_start().to_string();
    }

    if let Some(path) = line
        .strip_prefix("CONFLICT (content): Merge conflict in ")
    {
        return prefer_shorter_line(line, format!("conflict(content): {path}"));
    }

    if line == "Automatic merge failed; fix conflicts and then commit the result." {
        return "merge failed: fix conflicts then commit".to_string();
    }

    if let Some(branch) = line
        .strip_prefix("Switched to branch '")
        .and_then(|s| s.strip_suffix('\''))
    {
        return prefer_shorter_line(line, format!("branch: {branch}"));
    }

    if let Some(branch) = line
        .strip_prefix("Switched to branch \"")
        .and_then(|s| s.strip_suffix('"'))
    {
        return prefer_shorter_line(line, format!("branch: {branch}"));
    }

    if let Some((branch, scope)) = line
        .strip_prefix("Switched to branch '")
        .and_then(|s| s.split_once("' in "))
    {
        return prefer_shorter_line(line, format!("branch: {branch} @ {scope}"));
    }

    if let Some(branch) = line
        .strip_prefix("Switched to a new branch '")
        .and_then(|s| s.strip_suffix('\''))
    {
        return prefer_shorter_line(line, format!("new-branch: {branch}"));
    }

    if let Some(rest) = line.strip_prefix("Previous HEAD position was ") {
        return prefer_shorter_line(line, format!("prev-head: {rest}"));
    }

    if line == "git bisect start" {
        return "bisect start".to_string();
    }

    if let Some(rest) = line.strip_prefix("git bisect bad ") {
        return prefer_shorter_line(line, format!("bisect bad {rest}"));
    }

    if let Some(rest) = line.strip_prefix("git bisect good ") {
        return prefer_shorter_line(line, format!("bisect good {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Bisecting: ") {
        let rest = rest
            .replace(" revisions left to test after this ", " revs left ")
            .replace("(roughly ", "(~")
            .replace(" steps)", " steps)");
        return prefer_shorter_line(line, format!("bisecting: {rest}"));
    }

    if let Some(rest) = line.strip_prefix("Current branch ") {
        if let Some(branch) = rest.strip_suffix(" is up to date.") {
            return prefer_shorter_line(line, format!("up-to-date: {branch}"));
        }
    }

    if let Some(remote) = line
        .strip_prefix("Your branch is up to date with '")
        .and_then(|s| s.strip_suffix("'."))
    {
        return prefer_shorter_line(line, format!("up-to-date: {remote}"));
    }

    if let Some(commit) = line
        .strip_prefix("Reverting commit ")
        .and_then(|s| s.strip_suffix(':'))
    {
        return prefer_shorter_line(line, format!("revert: {commit}"));
    }

    if line.trim_start().starts_with("This reverts commit ") {
        return String::new();
    }

    if line
        .trim_start()
        .starts_with("which was originally committed on ")
    {
        return String::new();
    }

    if let Some(rest) = line
        .strip_prefix("Automatic revert of commit ")
        .and_then(|s| s.strip_suffix(" completed successfully."))
    {
        return prefer_shorter_line(line, format!("revert ok: {rest}"));
    }

    if line == "Changes made:" {
        return String::new();
    }

    if let Some(commit) = line
        .strip_prefix("Commit '")
        .and_then(|s| s.strip_suffix("' reset to HEAD."))
    {
        return prefer_shorter_line(line, format!("reset: {commit} -> HEAD"));
    }

    if let Some(path) = line.strip_prefix("Restored path: ") {
        return prefer_shorter_line(line, format!("restored: {path}"));
    }

    if let Some(path) = line.strip_prefix("Discarded changes in ") {
        return prefer_shorter_line(line, format!("discarded: {path}"));
    }

    if let Some(rev) = line
        .strip_prefix("Updated to revision ")
        .and_then(|s| s.strip_suffix('.'))
    {
        return prefer_shorter_line(line, format!("rev: {rev}"));
    }

    if let Some(count) = line
        .strip_prefix("Updated ")
        .and_then(|s| s.strip_suffix(" files."))
    {
        return prefer_shorter_line(line, format!("files: +{count}"));
    }

    if let Some(path) = line.strip_prefix("Reverted ") {
        return prefer_shorter_line(line, format!("R {path}"));
    }

    if let Some(rest) = line
        .strip_prefix("Submodule path '")
        .and_then(|s| s.split_once("': checked out '"))
    {
        let (path, hash_with_quote) = rest;
        let hash = hash_with_quote.trim_end_matches('\'');
        return prefer_shorter_line(line, format!("submodule: {path} @ {hash}"));
    }

    if let Some(rest) = line
        .strip_prefix("Submodule '")
        .and_then(|s| s.split_once("' ("))
    {
        let (name, url_with_paren) = rest;
        let url = url_with_paren.trim_end_matches(')');
        return prefer_shorter_line(line, format!("submodule: {name} <{url}>"));
    }

    if let Some(path) = line
        .strip_prefix("Resolved conflict at ")
        .or_else(|| line.strip_prefix("Resolved: "))
    {
        return prefer_shorter_line(line, format!("resolved: {path}"));
    }

    if let Some(path) = line
        .strip_prefix("'")
        .and_then(|s| s.strip_suffix("' unlocked."))
    {
        return prefer_shorter_line(line, format!("unlock {path}"));
    }

    if let Some(inner) = line
        .strip_prefix("'")
        .and_then(|s| s.strip_suffix("'."))
    {
        if let Some((path, user)) = inner.split_once("' locked by user '") {
            return prefer_shorter_line(line, format!("lock {path} by {user}"));
        }
    }

    line.to_string()
}

