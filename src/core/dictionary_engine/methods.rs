//! dictionary engine 方法实现

use super::types::*;
use crate::core::dictionary_manager::DictionaryManager;
use std::collections::HashMap;
use std::sync::Arc;

impl Dictionary {
    pub fn new() -> Self {
        Dictionary {
            paths: HashMap::new(),
            packages: HashMap::new(),
            macros: HashMap::new(),
            files: HashMap::new(),
            directories: HashMap::new(),
            flags: HashMap::new(),
            custom: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn resolve_one_level(&self, token: &str) -> Option<String> {
        if !token.starts_with('$') {
            return None;
        }
        if token.starts_with("$P") {
            self.paths.get(token).cloned()
        } else if token.starts_with("$D") {
            self.directories.get(token).cloned()
        } else if token.starts_with("$M") {
            self.macros.get(token).cloned()
        } else if token.starts_with("$PK") {
            self.packages.get(token).cloned()
        } else if token.starts_with("$C") {
            self.flags.get(token).cloned()
        } else {
            None
        }
    }

    pub fn resolve(&self, token: &str) -> Option<String> {
        self.resolve_one_level(token).map(|v| {
            if v.contains('$') {
                self.resolve_recursive(&v)
            } else {
                v
            }
        })
    }

    pub fn resolve_or_self(&self, text: &str) -> String {
        if text.contains('$') {
            self.resolve_recursive(text)
        } else {
            self.resolve(text).unwrap_or_else(|| text.to_string())
        }
    }

    pub fn resolve_recursive(&self, text: &str) -> String {
        let mut current = text.to_string();
        let mut depth = 0;

        while depth < 10 && current.contains('$') {
            let mut next_str = String::with_capacity(current.len());
            let mut chars = current.char_indices().peekable();
            let mut changed = false;

            while let Some((i, c)) = chars.next() {
                if c == '$' {
                    let mut end = i + 1;
                    while let Some(&(j, ch)) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '_' {
                            end = j + ch.len_utf8();
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let token = &current[i..end];
                    if let Some(resolved) = self.resolve_one_level(token) {
                        next_str.push_str(&resolved);
                        changed = true;
                    } else {
                        next_str.push_str(token);
                    }
                } else {
                    next_str.push(c);
                }
            }
            if !changed {
                break;
            }
            current = next_str;
            depth += 1;
        }
        current
    }

    pub fn resolve_for_ai(&self, text: &str) -> String {
        let mut current = text.to_string();
        let mut depth = 0;
        let mut skipped_noise = 0;

        while depth < 10 && current.contains('$') {
            let mut next_str = String::with_capacity(current.len());
            let mut chars = current.char_indices().peekable();
            let mut changed = false;

            while let Some((i, c)) = chars.next() {
                if c == '$' {
                    let mut end = i + 1;
                    while let Some(&(j, ch)) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '_' {
                            end = j + ch.len_utf8();
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let token = &current[i..end];

                    if token.starts_with("$M") {
                        if let Some(val) = self.macros.get(token) {
                            let lower = val.to_lowercase();
                            let is_semantic = lower.contains("error")
                                || lower.contains("fail")
                                || lower.contains("exception")
                                || lower.contains("warning");
                            if is_semantic {
                                next_str.push_str(val);
                                changed = true;
                            } else {
                                skipped_noise += 1;
                                changed = true;
                            }
                        } else {
                            next_str.push_str(token);
                        }
                    } else if token.starts_with("$P") {
                        if let Some(val) = self.paths.get(token) {
                            next_str.push_str(val);
                            changed = true;
                        } else {
                            next_str.push_str(token);
                        }
                    } else if token.starts_with("$PK")
                        || token.starts_with("$C")
                        || token.starts_with("$FL")
                    {
                        if let Some(val) = self.resolve_one_level(token) {
                            next_str.push_str(&val);
                            changed = true;
                        } else {
                            next_str.push_str(token);
                        }
                    } else {
                        // $D 和其他保留
                        next_str.push_str(token);
                    }
                } else {
                    next_str.push(c);
                }
            }
            if !changed {
                break;
            }
            current = next_str;
            depth += 1;
        }

        if skipped_noise > 0 {
            current.push_str(&format!(
                "\n... [TokenSlim AI Mode: Skipped {} noise events] ...\n",
                skipped_noise
            ));
        }

        current
    }
}

impl DictionaryEngine {
    pub fn new() -> Self {
        let mut next_ids = HashMap::new();
        next_ids.insert(DictType::Path, 1);
        next_ids.insert(DictType::Package, 1);
        next_ids.insert(DictType::Macro, 1);
        next_ids.insert(DictType::File, 1);
        next_ids.insert(DictType::Directory, 1);
        next_ids.insert(DictType::Flag, 1);

        DictionaryEngine {
            paths: HashMap::new(),
            packages: HashMap::new(),
            macros: HashMap::new(),
            files: HashMap::new(),
            directories: HashMap::new(),
            flags: HashMap::new(),
            custom: HashMap::new(),
            custom_prefixes: HashMap::new(),
            next_ids,
            path_hierarchy_enabled: false,
            semantic_aliases: HashMap::new(),
            alias_rules: Vec::new(),
            manager: Some(Arc::new(DictionaryManager::new())),
        }
    }

    pub fn with_manager(manager: Arc<DictionaryManager>) -> Self {
        let mut engine = Self::new();
        engine.manager = Some(manager);
        engine
    }

    pub fn add_path_layered(&mut self, original: &str) -> String {
        if let Some(m) = &self.manager {
            return to_readable_path_token(m, original);
        }
        original.to_string()
    }

    pub fn add_macro(&mut self, original: &str) -> String {
        if let Some(m) = &self.manager {
            return m.get_or_add_macro(original);
        }
        original.to_string()
    }

    pub fn add_package(&mut self, original: &str) -> String {
        if let Some(m) = &self.manager {
            return m.get_or_add_package(original);
        }
        original.to_string()
    }

    pub fn snapshot(&self) -> Dictionary {
        if let Some(m) = &self.manager {
            return m.snapshot();
        }
        Dictionary::new()
    }

    pub fn skeletonize_path(&self, token: &str) -> String {
        if !token.starts_with("$P") {
            return token.to_string();
        }

        let path_val = if let Some(m) = &self.manager {
            m.get_path_by_token(token)
        } else {
            None
        };

        if let Some(full) = path_val {
            let normalized: String = full.replace('\\', "/").to_string();
            let parts: Vec<&str> = normalized.split('/').collect();
            if parts.len() > 2 {
                return format!("{}/.../{}", parts[0], parts.last().unwrap());
            }
        }
        token.to_string()
    }
}

fn to_readable_path_token(manager: &DictionaryManager, original: &str) -> String {
    if original.starts_with('$') {
        return original.to_string();
    }

    // Keep directory-only paths intact via normal tokenization.
    if original.ends_with('/') || original.ends_with('\\') {
        return manager.get_or_add_path(original);
    }

    let slash_idx = original.rfind('/');
    let backslash_idx = original.rfind('\\');
    let split_idx = match (slash_idx, backslash_idx) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let Some(idx) = split_idx else {
        return manager.get_or_add_path(original);
    };

    if idx == 0 || idx + 1 >= original.len() {
        return manager.get_or_add_path(original);
    }

    let prefix = &original[..idx];
    let leaf = &original[idx + 1..];
    let sep = &original[idx..idx + 1];

    if leaf.is_empty() {
        return manager.get_or_add_path(original);
    }

    let prefix_token = manager.get_or_add_path(prefix);
    format!("{}{}{}", prefix_token, sep, leaf)
}
