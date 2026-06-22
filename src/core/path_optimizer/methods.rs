use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::path_optimizer::token_boundary::{
    contains_path_token_boundary, is_path_token_boundary_next, replace_path_token_boundary,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::RwLock;

static PATH_DICTIONARY_PRESET: AtomicU8 = AtomicU8::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathDictionaryPreset {
    Conservative,
    Balanced,
    Aggressive,
}

impl PathDictionaryPreset {
    fn as_u8(self) -> u8 {
        match self {
            PathDictionaryPreset::Conservative => 0,
            PathDictionaryPreset::Balanced => 1,
            PathDictionaryPreset::Aggressive => 2,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            0 => PathDictionaryPreset::Conservative,
            2 => PathDictionaryPreset::Aggressive,
            _ => PathDictionaryPreset::Balanced,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PathDictionaryOptions {
    pub enabled: bool,
    pub min_primary_occurrences: usize,
    pub min_nested_occurrences: usize,
    pub enable_nested_aliases: bool,
    pub path_parse_cost: isize,
    pub global_paths_block_cost: isize,
    pub nested_parse_cost: isize,
    pub min_footer_token_uses: usize,
}

impl Default for PathDictionaryOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            min_primary_occurrences: 2,
            min_nested_occurrences: 2,
            enable_nested_aliases: true,
            path_parse_cost: 0,
            global_paths_block_cost: 0,
            nested_parse_cost: 1,
            min_footer_token_uses: 2,
        }
    }
}

pub fn options_from_preset(preset: PathDictionaryPreset) -> PathDictionaryOptions {
    match preset {
        PathDictionaryPreset::Conservative => PathDictionaryOptions {
            enabled: true,
            min_primary_occurrences: 2,
            min_nested_occurrences: 2,
            enable_nested_aliases: false,
            path_parse_cost: 1,
            global_paths_block_cost: 1,
            nested_parse_cost: 2,
            min_footer_token_uses: 2,
        },
        PathDictionaryPreset::Balanced => PathDictionaryOptions {
            enabled: true,
            min_primary_occurrences: 2,
            min_nested_occurrences: 2,
            enable_nested_aliases: true,
            path_parse_cost: 0,
            global_paths_block_cost: 0,
            nested_parse_cost: 1,
            min_footer_token_uses: 2,
        },
        PathDictionaryPreset::Aggressive => PathDictionaryOptions {
            enabled: true,
            min_primary_occurrences: 2,
            min_nested_occurrences: 2,
            enable_nested_aliases: true,
            path_parse_cost: 0,
            global_paths_block_cost: 0,
            nested_parse_cost: 0,
            min_footer_token_uses: 2,
        },
    }
}

static PATH_DICTIONARY_OPTIONS: std::sync::LazyLock<RwLock<PathDictionaryOptions>> =
    std::sync::LazyLock::new(|| RwLock::new(options_from_preset(PathDictionaryPreset::Balanced)));

pub fn run_with_path_dictionary_preset<T>(
    preset: PathDictionaryPreset,
    f: impl FnOnce() -> T,
) -> T {
    let previous = PATH_DICTIONARY_PRESET.swap(preset.as_u8(), Ordering::SeqCst);
    let previous_options = current_path_dictionary_options();
    set_current_path_dictionary_options(options_from_preset(preset));

    struct ResetGuard(u8, PathDictionaryOptions);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            PATH_DICTIONARY_PRESET.store(self.0, Ordering::SeqCst);
            set_current_path_dictionary_options(self.1.clone());
        }
    }

    let _guard = ResetGuard(previous, previous_options);
    f()
}

pub fn current_path_dictionary_options() -> PathDictionaryOptions {
    PATH_DICTIONARY_OPTIONS
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|_| {
            let preset =
                PathDictionaryPreset::from_u8(PATH_DICTIONARY_PRESET.load(Ordering::Relaxed));
            options_from_preset(preset)
        })
}

pub fn set_current_path_dictionary_options(options: PathDictionaryOptions) {
    if let Ok(mut g) = PATH_DICTIONARY_OPTIONS.write() {
        *g = options;
    }
}

pub fn run_with_path_dictionary_options<T>(
    options: PathDictionaryOptions,
    f: impl FnOnce() -> T,
) -> T {
    let previous = current_path_dictionary_options();

    struct ResetGuard(PathDictionaryOptions);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            set_current_path_dictionary_options(self.0.clone());
        }
    }

    set_current_path_dictionary_options(options);
    let _guard = ResetGuard(previous);
    f()
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TokenOptimizerTomlRoot {
    #[serde(default)]
    token_optimizer: Option<TokenOptimizerToml>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TokenOptimizerToml {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    paths: Option<PathDictionaryOptionsOverride>,
    #[serde(default)]
    scopes: Option<TokenOptimizerScopesToml>,
    #[serde(default)]
    presets: Option<TokenOptimizerPresetsToml>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TokenOptimizerScopesToml {
    #[serde(default)]
    paths: Option<PathDictionaryOptionsOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TokenOptimizerPresetsToml {
    #[serde(default)]
    fast: Option<TokenOptimizerPresetEntryToml>,
    #[serde(default)]
    balanced: Option<TokenOptimizerPresetEntryToml>,
    #[serde(default)]
    ai: Option<TokenOptimizerPresetEntryToml>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TokenOptimizerPresetEntryToml {
    #[serde(default)]
    paths: Option<PathDictionaryOptionsOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PathDictionaryOptionsOverride {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    min_primary_occurrences: Option<usize>,
    #[serde(default)]
    min_nested_occurrences: Option<usize>,
    #[serde(default)]
    enable_nested_aliases: Option<bool>,
    #[serde(default)]
    path_parse_cost: Option<isize>,
    #[serde(default)]
    global_paths_block_cost: Option<isize>,
    #[serde(default)]
    nested_parse_cost: Option<isize>,
    #[serde(default)]
    min_footer_token_uses: Option<usize>,
}

fn apply_path_override(base: &mut PathDictionaryOptions, ov: &PathDictionaryOptionsOverride) {
    if let Some(v) = ov.enabled {
        base.enabled = v;
    }
    if let Some(v) = ov.min_primary_occurrences {
        base.min_primary_occurrences = v;
    }
    if let Some(v) = ov.min_nested_occurrences {
        base.min_nested_occurrences = v;
    }
    if let Some(v) = ov.enable_nested_aliases {
        base.enable_nested_aliases = v;
    }
    if let Some(v) = ov.path_parse_cost {
        base.path_parse_cost = v;
    }
    if let Some(v) = ov.global_paths_block_cost {
        base.global_paths_block_cost = v;
    }
    if let Some(v) = ov.nested_parse_cost {
        base.nested_parse_cost = v;
    }
    if let Some(v) = ov.min_footer_token_uses {
        base.min_footer_token_uses = v;
    }
}

fn parse_token_optimizer_toml(content: &str) -> Option<TokenOptimizerToml> {
    toml::from_str::<TokenOptimizerTomlRoot>(content)
        .ok()
        .and_then(|root| root.token_optimizer)
}

fn preset_override<'a>(
    presets: &'a TokenOptimizerPresetsToml,
    preset: PathDictionaryPreset,
) -> Option<&'a PathDictionaryOptionsOverride> {
    match preset {
        PathDictionaryPreset::Conservative => presets.fast.as_ref().and_then(|e| e.paths.as_ref()),
        PathDictionaryPreset::Balanced => presets.balanced.as_ref().and_then(|e| e.paths.as_ref()),
        PathDictionaryPreset::Aggressive => presets.ai.as_ref().and_then(|e| e.paths.as_ref()),
    }
}

pub fn resolve_path_dictionary_options_from_files(
    preset: PathDictionaryPreset,
    explicit_config: Option<&Path>,
) -> PathDictionaryOptions {
    let mut options = options_from_preset(preset);

    let mut files: Vec<PathBuf> = vec![
        PathBuf::from("config/plugins.toml"),
        PathBuf::from(".tokenslim.toml"),
    ];
    if let Some(p) = explicit_config {
        files.push(p.to_path_buf());
    }

    for path in files {
        if !path.exists() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(cfg) = parse_token_optimizer_toml(&content) else {
            continue;
        };

        if let Some(enabled) = cfg.enabled {
            options.enabled = enabled;
        }
        if let Some(paths) = cfg.paths.as_ref() {
            apply_path_override(&mut options, paths);
        }
        if let Some(scopes) = cfg.scopes.as_ref().and_then(|s| s.paths.as_ref()) {
            apply_path_override(&mut options, scopes);
        }
        if let Some(presets) = cfg
            .presets
            .as_ref()
            .and_then(|p| preset_override(p, preset))
        {
            apply_path_override(&mut options, presets);
        }
    }

    options
}

#[derive(Clone)]
struct VcsPathOccurrence {
    line_index: usize,
    marker: String,
    path: String,
    suffix: String,
}

fn token_key_as_num(token: &str) -> usize {
    token
        .strip_prefix("$P")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
}

fn estimate_tokens(text: &str) -> isize {
    ((text.chars().count() + 3) / 4) as isize
}

fn replace_all_path_tokens(body: &str, mappings: &[(String, String)]) -> String {
    let mut out = body.to_string();
    let mut sorted: Vec<(String, String)> = mappings.to_vec();
    sorted.sort_by(|a, b| {
        b.0.len()
            .cmp(&a.0.len())
            .then_with(|| token_key_as_num(&b.0).cmp(&token_key_as_num(&a.0)))
    });
    for (token, path) in sorted {
        out = replace_path_token_boundary(&out, &token, &path);
    }
    out
}

fn parse_vcs_path_line(line: &str) -> Option<(String, String, String)> {
    if line.starts_with("?? ") {
        return split_path_with_suffix("?? ", &line[3..]);
    }

    let bytes = line.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b' ' {
        let marker = &line[..2];
        return split_path_with_suffix(marker, &line[2..]);
    }

    if bytes.len() >= 3 && bytes[2] == b' ' {
        let marker = &line[..3];
        return split_path_with_suffix(marker, &line[3..]);
    }

    if let Some(path) = parse_plain_path_line(line) {
        return Some((String::new(), path, String::new()));
    }

    None
}

fn parse_plain_path_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let is_header = trimmed.starts_with("commit ")
        || trimmed.starts_with("Author:")
        || trimmed.starts_with("Date:")
        || trimmed.starts_with("branch:")
        || trimmed.starts_with("changes:")
        || trimmed.starts_with("untracked:")
        || trimmed.starts_with("diff --")
        || trimmed.starts_with("@@")
        || trimmed.starts_with("+++")
        || trimmed.starts_with("---")
        || trimmed.starts_with("paths: ");
    if is_header {
        return None;
    }

    let looks_like_path =
        (trimmed.contains('/') || trimmed.contains('\\') || trimmed.starts_with('.'))
            && !trimmed.contains(' ');
    if looks_like_path {
        return Some(trimmed.to_string());
    }

    None
}

fn split_path_with_suffix(marker: &str, rest: &str) -> Option<(String, String, String)> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(idx) = trimmed.rfind(" (") {
        if trimmed.ends_with(')') {
            let path = trimmed[..idx].trim();
            let suffix = trimmed[idx..].to_string();
            if !path.is_empty() {
                return Some((marker.to_string(), path.to_string(), suffix));
            }
        }
    }

    Some((marker.to_string(), trimmed.to_string(), String::new()))
}

fn generate_prefix_candidates(path: &str) -> Vec<String> {
    if path.contains(" -> ") {
        return Vec::new();
    }

    let normalized = path.trim_end_matches('/');
    let leading_slashes = normalized.bytes().take_while(|b| *b == b'/').count();
    let parts: Vec<&str> = normalized[leading_slashes..]
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 2 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut current = "/".repeat(leading_slashes);
    for part in parts.iter().take(parts.len() - 1) {
        if !current.is_empty() && !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(part);
        out.push(current.clone());
    }
    out
}

fn simplify_dead_anchors(dictionary_entries: &mut Vec<(String, String)>, body: &str) {
    loop {
        let mut changed = false;

        for idx in 0..dictionary_entries.len() {
            let (token, value) = dictionary_entries[idx].clone();

            if contains_path_token_boundary(body, &token) {
                continue;
            }

            let mut refs: Vec<usize> = Vec::new();
            for (j, (_, other_value)) in dictionary_entries.iter().enumerate() {
                if j != idx && contains_path_token_boundary(other_value, &token) {
                    refs.push(j);
                }
            }

            if refs.len() == 1 {
                let owner = refs[0];
                let replaced =
                    replace_path_token_boundary(&dictionary_entries[owner].1, &token, &value);
                dictionary_entries[owner].1 = replaced;
                dictionary_entries.remove(idx);
                changed = true;
                break;
            }
        }

        if !changed {
            break;
        }
    }
}

fn collect_path_token_counts(text: &str) -> HashMap<String, usize> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut counts: HashMap<String, usize> = HashMap::new();

    while i + 2 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'P' && bytes[i + 2].is_ascii_digit() {
            let mut end = i + 3;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            let next = bytes.get(end).copied();
            if is_path_token_boundary_next(next) {
                let token = text[i..end].to_string();
                *counts.entry(token).or_insert(0) += 1;
            }
            i = end;
            continue;
        }
        i += 1;
    }

    counts
}

pub fn append_optimized_inline_path_dictionary(
    rendered: &str,
    dict_engine: &DictionaryEngine,
) -> String {
    append_optimized_inline_path_dictionary_with_options(
        rendered,
        dict_engine,
        &PathDictionaryOptions::default(),
    )
}

pub fn append_optimized_inline_path_dictionary_with_options(
    rendered: &str,
    dict_engine: &DictionaryEngine,
    options: &PathDictionaryOptions,
) -> String {
    if !options.enabled {
        return rendered.to_string();
    }

    let token_counts = collect_path_token_counts(rendered);
    if token_counts.is_empty() {
        return rendered.to_string();
    }

    let mut path_tokens: Vec<String> = token_counts
        .iter()
        .filter_map(|(token, count)| {
            if *count >= options.min_footer_token_uses {
                Some(token.clone())
            } else {
                None
            }
        })
        .collect();
    path_tokens.sort_by_key(|token| token_key_as_num(token));

    let dict = dict_engine.snapshot();
    let mut parts = Vec::new();
    for token in path_tokens {
        if let Some(raw_path) = dict.paths.get(&token) {
            let resolved = dict.resolve_or_self(raw_path);
            parts.push(format!("{}={}", token, resolved));
        }
    }

    if parts.is_empty() {
        return rendered.to_string();
    }

    let mut out = rendered.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("paths: ");
    out.push_str(&parts.join("; "));
    out.push('\n');

    optimize_path_dictionary_blocks_with_options(&out, options)
}

pub fn optimize_path_dictionary_blocks(formatted: &str) -> String {
    optimize_path_dictionary_blocks_with_options(formatted, &PathDictionaryOptions::default())
}

pub fn optimize_path_dictionary_blocks_with_options(
    formatted: &str,
    options: &PathDictionaryOptions,
) -> String {
    if !options.enabled {
        return formatted.to_string();
    }

    let mut body_lines: Vec<String> = Vec::new();
    let mut mappings: Vec<(String, String)> = Vec::new();

    for line in formatted.lines() {
        if let Some(rest) = line.strip_prefix("paths: ") {
            for part in rest.split(';') {
                let entry = part.trim();
                if entry.is_empty() {
                    continue;
                }
                if let Some(eq) = entry.find('=') {
                    let token = entry[..eq].trim().to_string();
                    let path = entry[eq + 1..].trim().to_string();
                    if token.starts_with("$P") && !path.is_empty() {
                        mappings.push((token, path));
                    }
                }
            }
        } else {
            body_lines.push(line.to_string());
        }
    }

    if mappings.is_empty() {
        return formatted.to_string();
    }

    mappings.sort_by(|a, b| token_key_as_num(&a.0).cmp(&token_key_as_num(&b.0)));
    mappings.dedup_by(|a, b| a.0 == b.0);

    let expanded_body = replace_all_path_tokens(&body_lines.join("\n"), &mappings);
    let mut expanded_lines: Vec<String> = expanded_body.lines().map(|s| s.to_string()).collect();

    let mut occurrences: Vec<VcsPathOccurrence> = Vec::new();
    for (idx, line) in expanded_lines.iter().enumerate() {
        if let Some((marker, path, suffix)) = parse_vcs_path_line(line) {
            if path.contains('/') {
                occurrences.push(VcsPathOccurrence {
                    line_index: idx,
                    marker,
                    path,
                    suffix,
                });
            }
        }
    }

    if occurrences.is_empty() {
        return expanded_lines.join("\n");
    }

    let mut candidate_map: HashMap<String, Vec<usize>> = HashMap::new();
    for (occ_idx, occ) in occurrences.iter().enumerate() {
        for prefix in generate_prefix_candidates(&occ.path) {
            if occ.path.starts_with(&(prefix.clone() + "/")) {
                candidate_map.entry(prefix).or_default().push(occ_idx);
            }
        }
    }

    let mut assigned: Vec<Option<String>> = vec![None; occurrences.len()];
    let mut selected: Vec<(String, isize)> = Vec::new();
    let ref_cost = estimate_tokens("$P1");

    loop {
        let mut best: Option<(String, isize)> = None;

        for (prefix, occs) in &candidate_map {
            let uncovered = occs.iter().filter(|idx| assigned[**idx].is_none()).count() as isize;
            if uncovered < options.min_primary_occurrences as isize {
                continue;
            }

            let lit = estimate_tokens(prefix);
            let replacement_savings = uncovered * (lit - ref_cost);
            let declaration_cost = lit + 1;
            let gain = replacement_savings - (declaration_cost + options.path_parse_cost);
            if gain >= 0 {
                if let Some((_, best_gain)) = &best {
                    if gain > *best_gain {
                        best = Some((prefix.clone(), gain));
                    }
                } else {
                    best = Some((prefix.clone(), gain));
                }
            }
        }

        let Some((chosen_prefix, chosen_gain)) = best else {
            break;
        };

        if let Some(covered) = candidate_map.get(&chosen_prefix) {
            for occ_idx in covered {
                if assigned[*occ_idx].is_none() {
                    assigned[*occ_idx] = Some(chosen_prefix.clone());
                }
            }
        }

        selected.push((chosen_prefix, chosen_gain));
    }

    if selected.is_empty() {
        return expanded_lines.join("\n");
    }

    let total_gain: isize = selected.iter().map(|(_, g)| *g).sum();
    if total_gain < options.global_paths_block_cost {
        return expanded_lines.join("\n");
    }

    let mut ordered_prefixes: Vec<String> = selected.into_iter().map(|(p, _)| p).collect();
    ordered_prefixes.sort_by(|a, b| {
        let a_first = occurrences
            .iter()
            .filter(|o| o.path.starts_with(&(a.clone() + "/")))
            .map(|o| o.line_index)
            .min()
            .unwrap_or(usize::MAX);
        let b_first = occurrences
            .iter()
            .filter(|o| o.path.starts_with(&(b.clone() + "/")))
            .map(|o| o.line_index)
            .min()
            .unwrap_or(usize::MAX);
        a_first
            .cmp(&b_first)
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });
    ordered_prefixes.dedup();

    let mut prefix_to_token: HashMap<String, String> = HashMap::new();
    for (i, prefix) in ordered_prefixes.iter().enumerate() {
        prefix_to_token.insert(prefix.clone(), format!("$P{}", i + 1));
    }

    for (occ_idx, occ) in occurrences.iter().enumerate() {
        if let Some(prefix) = &assigned[occ_idx] {
            if let Some(token) = prefix_to_token.get(prefix) {
                if let Some(suffix) = occ.path.strip_prefix(&(prefix.clone() + "/")) {
                    expanded_lines[occ.line_index] =
                        format!("{}{}{}{}{}", occ.marker, token, "/", suffix, occ.suffix);
                }
            }
        }
    }

    let mut second_occurrences: Vec<VcsPathOccurrence> = Vec::new();
    for (idx, line) in expanded_lines.iter().enumerate() {
        if let Some((marker, path, suffix)) = parse_vcs_path_line(line) {
            if path.starts_with("$P") && path.contains('/') {
                second_occurrences.push(VcsPathOccurrence {
                    line_index: idx,
                    marker,
                    path,
                    suffix,
                });
            }
        }
    }

    let mut dictionary_entries: Vec<(String, String)> = prefix_to_token
        .iter()
        .map(|(prefix, token)| (token.clone(), prefix.clone()))
        .collect();

    if options.enable_nested_aliases && !second_occurrences.is_empty() {
        let mut second_candidates: HashMap<String, Vec<usize>> = HashMap::new();
        for (occ_idx, occ) in second_occurrences.iter().enumerate() {
            for prefix in generate_prefix_candidates(&occ.path) {
                if prefix.starts_with("$P")
                    && prefix.contains('/')
                    && occ.path.starts_with(&(prefix.clone() + "/"))
                {
                    second_candidates.entry(prefix).or_default().push(occ_idx);
                }
            }
        }

        let mut second_assigned: Vec<Option<String>> = vec![None; second_occurrences.len()];
        let mut selected_second: Vec<(String, isize)> = Vec::new();
        let ref_cost = estimate_tokens("$P1");

        loop {
            let mut best: Option<(String, isize)> = None;
            for (prefix, occs) in &second_candidates {
                let uncovered = occs
                    .iter()
                    .filter(|idx| second_assigned[**idx].is_none())
                    .count() as isize;
                if uncovered < options.min_nested_occurrences as isize {
                    continue;
                }

                let lit = estimate_tokens(prefix);
                let replacement_savings = uncovered * (lit - ref_cost);
                let declaration_cost = lit + 1;
                let parse_cost = uncovered * options.nested_parse_cost;
                let gain = replacement_savings - (declaration_cost + parse_cost);
                if gain >= 0 {
                    if let Some((_, best_gain)) = &best {
                        if gain > *best_gain {
                            best = Some((prefix.clone(), gain));
                        }
                    } else {
                        best = Some((prefix.clone(), gain));
                    }
                }
            }

            let Some((chosen_prefix, chosen_gain)) = best else {
                break;
            };

            if let Some(covered) = second_candidates.get(&chosen_prefix) {
                for occ_idx in covered {
                    if second_assigned[*occ_idx].is_none() {
                        second_assigned[*occ_idx] = Some(chosen_prefix.clone());
                    }
                }
            }
            selected_second.push((chosen_prefix, chosen_gain));
        }

        if !selected_second.is_empty() {
            let mut ordered_second: Vec<String> =
                selected_second.into_iter().map(|(p, _)| p).collect();
            ordered_second.sort_by(|a, b| {
                let a_first = second_occurrences
                    .iter()
                    .filter(|o| o.path.starts_with(&(a.clone() + "/")))
                    .map(|o| o.line_index)
                    .min()
                    .unwrap_or(usize::MAX);
                let b_first = second_occurrences
                    .iter()
                    .filter(|o| o.path.starts_with(&(b.clone() + "/")))
                    .map(|o| o.line_index)
                    .min()
                    .unwrap_or(usize::MAX);
                a_first
                    .cmp(&b_first)
                    .then_with(|| a.len().cmp(&b.len()))
                    .then_with(|| a.cmp(b))
            });
            ordered_second.dedup();

            let mut second_prefix_to_token: HashMap<String, String> = HashMap::new();
            let mut next_id = prefix_to_token.len() + 1;
            for prefix in ordered_second {
                let token = format!("$P{}", next_id);
                next_id += 1;
                second_prefix_to_token.insert(prefix.clone(), token.clone());
                dictionary_entries.push((token, prefix));
            }

            for (occ_idx, occ) in second_occurrences.iter().enumerate() {
                if let Some(prefix) = &second_assigned[occ_idx] {
                    if let Some(token) = second_prefix_to_token.get(prefix) {
                        if let Some(suffix) = occ.path.strip_prefix(&(prefix.clone() + "/")) {
                            expanded_lines[occ.line_index] =
                                format!("{}{}{}{}{}", occ.marker, token, "/", suffix, occ.suffix);
                        }
                    }
                }
            }
        }
    }

    let mut out = expanded_lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }

    simplify_dead_anchors(&mut dictionary_entries, &out);

    let mut parts: Vec<String> = dictionary_entries
        .into_iter()
        .map(|(token, value)| format!("{}={}", token, value))
        .collect();
    parts.sort_by(|a, b| {
        let ta = a.split('=').next().unwrap_or_default();
        let tb = b.split('=').next().unwrap_or_default();
        token_key_as_num(ta).cmp(&token_key_as_num(tb))
    });
    out.push_str("paths: ");
    out.push_str(&parts.join("; "));
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::{
        append_optimized_inline_path_dictionary,
        append_optimized_inline_path_dictionary_with_options, current_path_dictionary_options,
        generate_prefix_candidates, optimize_path_dictionary_blocks,
        optimize_path_dictionary_blocks_with_options, resolve_path_dictionary_options_from_files,
        run_with_path_dictionary_preset, PathDictionaryOptions, PathDictionaryPreset,
    };
    use crate::core::dictionary_engine::DictionaryEngine;
    use std::fs;

    #[test]
    fn merges_and_filters_unprofitable_tokens() {
        let input = "changes:\nM $P1/issues.md\nM $P1/learnings.md\nM $P1/notes.md\nM $P1/plan.md\nM $P10/single.md\npaths: $P1=.sisyphus/notepads/REFACTORING_PLAN_V6.2\npaths: $P10=.tokenslim-context.md\n";
        let out = optimize_path_dictionary_blocks(input);
        assert_eq!(out.matches("paths:").count(), 1);
        assert!(out.contains("/issues.md"));
        assert!(out.contains("single.md"));
        assert!(!out.contains("$P10/single.md"));
    }

    #[test]
    fn expands_nested_source_tokens_before_reoptimization() {
        let input =
            "changes:\nM $P2/issues.md\npaths: $P1=.sisyphus/notepads; $P2=$P1/REFACTORING_PLAN\n";
        let out = optimize_path_dictionary_blocks(input);
        assert!(!out.contains("$P1=$P1/"));
        assert!(out.contains("REFACTORING_PLAN"));
    }

    #[test]
    fn merges_paths_blocks_across_sections() {
        let input = "branch: master\nchanges:\nM $P1/issues.md\nM $P1/learnings.md\npaths: $P1=.sisyphus/notepads/REFACTORING_PLAN_V6.2\nuntracked:\n?? $P2/path_optimizer/\n?? $P2/init_command/\npaths: $P2=src/core\n";

        let out = optimize_path_dictionary_blocks(input);
        assert_eq!(out.matches("paths:").count(), 1);
        assert!(out.contains("REFACTORING_PLAN_V6.2"));
        assert!(out.contains("path_optimizer/"));
        assert!(out.contains("init_command/"));
    }

    #[test]
    fn supports_nested_alias_for_plain_log_path_lines() {
        let input = "18f4bb5 feat: test\n$P1/doctor_workspace/methods.rs\n$P1/doctor_workspace/mod.rs\n$P1/doctor_workspace/types.rs\npaths: $P1=src/core\n";
        let options = PathDictionaryOptions {
            enable_nested_aliases: true,
            min_nested_occurrences: 2,
            nested_parse_cost: 0,
            ..PathDictionaryOptions::default()
        };
        let out = optimize_path_dictionary_blocks_with_options(input, &options);

        assert_eq!(out.matches("paths:").count(), 1);
        assert!(out.contains("$P"));
        assert!(
            out.contains("=$P") || out.contains("=src/core/doctor_workspace"),
            "out={}",
            out
        );
    }

    #[test]
    fn flattens_single_use_dead_anchor_in_footer() {
        let input = "changes:\nM $P2/issues.md\nM $P2/learnings.md\npaths: $P1=.sisyphus/notepads; $P2=$P1/REFACTORING_PLAN_V6.2\n";
        let out = optimize_path_dictionary_blocks(input);

        // 断言嵌套锚点已被扁平化，不再保留 $P2=$P1/... 这种死锚点结构。
        assert!(!out.contains("$P2=$P1/"));
        assert!(out.contains("REFACTORING_PLAN_V6.2"));
    }

    #[test]
    fn does_not_treat_literal_token_like_segments_as_aliases() {
        let input =
            "changes:\nM $P2/issues.md\npaths: $P1=docs/design; $P2=$P1-notes/REFACTORING_PLAN\n";
        let out = optimize_path_dictionary_blocks(input);

        assert!(out.contains("$P1-notes/REFACTORING_PLAN") || out.contains("$P1-notes"));
        assert!(!out.contains("docs/design-notes/REFACTORING_PLAN"));
    }

    #[test]
    fn keeps_break_even_prefix() {
        let input = "changes:\nM $P1/a.md\nM $P1/b.md\npaths: $P1=docs/design\n";
        let out = optimize_path_dictionary_blocks(input);
        assert!(out.contains("paths: $P1=docs/design"));
    }

    #[test]
    fn generate_prefix_candidates_preserves_absolute_roots() {
        assert_eq!(
            generate_prefix_candidates("/usr/local/bin"),
            vec!["/usr".to_string(), "/usr/local".to_string()]
        );
        assert_eq!(
            generate_prefix_candidates("//depot/main/src"),
            vec!["//depot".to_string(), "//depot/main".to_string()]
        );
    }

    #[test]
    fn keeps_profitable_deep_absolute_prefix() {
        let input = "changes:\nM $P1/a.rs\nM $P1/b.rs\nM $P1/c.rs\npaths: $P1=//depot/main/src/plugins/vcs_plugin\n";
        let out = optimize_path_dictionary_blocks(input);

        assert!(
            out.contains("paths: $P1=//depot/main/src/plugins/vcs_plugin"),
            "out={}",
            out
        );
        assert!(out.contains("$P1/a.rs"), "out={}", out);
        assert!(
            !out.contains("$P1/src/plugins/vcs_plugin/a.rs"),
            "out={}",
            out
        );
    }

    #[test]
    fn groups_absolute_p4_paths_under_shared_root() {
        let input = "changes:\nM $P1/main.rs\nM $P2/helper.rs\nM $P3/app.json\npaths: $P1=//depot/main/src; $P2=//depot/main/src/utils; $P3=//depot/main/config\n";
        let options = PathDictionaryOptions {
            enable_nested_aliases: false,
            ..PathDictionaryOptions::default()
        };
        let out = optimize_path_dictionary_blocks_with_options(input, &options);

        assert!(out.contains("paths: $P1=//depot/main"), "out={}", out);
        assert!(out.contains("M $P1/src/main.rs"), "out={}", out);
        assert!(out.contains("M $P1/src/utils/helper.rs"), "out={}", out);
        assert!(out.contains("M $P1/config/app.json"), "out={}", out);
    }

    #[test]
    fn appends_single_paths_footer_and_optimizes() {
        let mut dict = DictionaryEngine::new();
        let p1 = dict.add_path_layered("docs/design/vcs_plugin.md");
        let p2 = dict.add_path_layered("docs/design/vcs_coverage_matrix.md");

        let rendered = format!("M {}\nA {}\n", p1, p2);
        let out = append_optimized_inline_path_dictionary(&rendered, &dict);

        assert_eq!(out.matches("paths:").count(), 1);
        assert!(out.contains("$P"));
        assert!(out.contains("vcs_plugin.md"));
    }

    #[test]
    fn footer_respects_min_token_occurrences() {
        let mut dict = DictionaryEngine::new();
        let p1 = dict.add_path_layered("docs/design/vcs_plugin.md");
        let p2 = dict.add_path_layered("docs/design/vcs_coverage_matrix.md");

        let rendered = format!("M {}\nA {}\n", p1, p2);
        let options = PathDictionaryOptions {
            min_footer_token_uses: 3,
            ..PathDictionaryOptions::default()
        };
        let out = append_optimized_inline_path_dictionary_with_options(&rendered, &dict, &options);

        assert_eq!(out.matches("paths:").count(), 0);
    }

    #[test]
    fn can_disable_nested_aliases() {
        let input = "changes:\nM $P1/a.md\nM $P1/b.md\nM $P1/c.md\npaths: $P1=docs/design\n";
        let options = PathDictionaryOptions {
            enable_nested_aliases: false,
            ..PathDictionaryOptions::default()
        };
        let out = optimize_path_dictionary_blocks_with_options(input, &options);
        assert_eq!(out.matches("paths:").count(), 1);
    }

    #[test]
    fn shared_preset_scope_overrides_and_restores() {
        let default_opts = current_path_dictionary_options();
        assert_eq!(default_opts.min_footer_token_uses, 2);

        let scoped = run_with_path_dictionary_preset(PathDictionaryPreset::Conservative, || {
            current_path_dictionary_options()
        });
        assert_eq!(scoped.min_footer_token_uses, 2);
        assert!(!scoped.enable_nested_aliases);

        let restored = current_path_dictionary_options();
        assert_eq!(restored.min_footer_token_uses, 2);
    }

    #[test]
    fn explicit_token_optimizer_config_overrides_preset() {
        let mut cfg_path = std::env::temp_dir();
        cfg_path.push(format!(
            "tokenslim-token-optimizer-{}.toml",
            std::process::id()
        ));

        let content = r#"
[token_optimizer]
enabled = true

[token_optimizer.scopes.paths]
min_footer_token_uses = 9

[token_optimizer.presets.ai.paths]
enable_nested_aliases = false
nested_parse_cost = 7
"#;

        fs::write(&cfg_path, content).expect("write temp config");
        let resolved = resolve_path_dictionary_options_from_files(
            PathDictionaryPreset::Aggressive,
            Some(&cfg_path),
        );
        let _ = fs::remove_file(&cfg_path);

        assert_eq!(resolved.min_footer_token_uses, 9);
        assert!(!resolved.enable_nested_aliases);
        assert_eq!(resolved.nested_parse_cost, 7);
    }
}
