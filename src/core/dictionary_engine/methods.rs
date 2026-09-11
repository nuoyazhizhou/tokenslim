//! dictionary engine 方法实现

use super::types::*;
use crate::core::dictionary_manager::DictionaryManager;
use std::collections::HashMap;
use std::sync::Arc;

/// 统一的宏「语义 / 噪声」判定谓词（P2-11）。
///
/// 命中 `error` / `fail` / `exception` / `warning` 之一即视为语义宏（保留进 AI 上下文），
/// 否则视为噪声宏（跳过后提示）。登记侧 [`DictionaryManager::get_or_add_macro`] 设定
/// `DictCategory` 与解析侧 [`Dictionary::resolve_for_ai`] 的跳噪决策共用本谓词，避免两侧
/// 各写一套关键字清单导致口径漂移。
pub(crate) fn is_semantic_macro(m: &str) -> bool {
    let lower = m.to_lowercase();
    lower.contains("error")
        || lower.contains("fail")
        || lower.contains("exception")
        || lower.contains("warning")
}

impl Dictionary {
    /// 构造空字典结构：各类型词表初始化为空 `HashMap`，等待 `DictionaryManager` 填充。
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

    /// 单级 token 解析：依据前缀（$P 路径 / $D 目录 / $M 宏 / $PK 包 / $C 标志）在对应映射表中查找并返回替换文本；非 `$` 开头的串或查不到时返回 `None`。
    pub fn resolve_one_level(&self, token: &str) -> Option<String> {
        if !token.starts_with('$') {
            return None;
        }
        // 长前缀优先：$PK 必须在 $P 之前判断，否则 $PK... 会被 $P 分支截获而永远查不到包（Q442）
        if token.starts_with("$PK") {
            self.packages.get(token).cloned()
        } else if token.starts_with("$P") {
            self.paths.get(token).cloned()
        } else if token.starts_with("$D") {
            self.directories.get(token).cloned()
        } else if token.starts_with("$M") {
            self.macros.get(token).cloned()
        } else if token.starts_with("$C") {
            self.flags.get(token).cloned()
        } else {
            None
        }
    }

    /// 解析单个 token：先做单级解析，若结果为含 `$` 的串则继续递归展开，直到不再包含可解析占位符或达到递归上限。
    pub fn resolve(&self, token: &str) -> Option<String> {
        self.resolve_one_level(token).map(|v| {
            if v.contains('$') {
                self.resolve_recursive(&v)
            } else {
                v
            }
        })
    }

    /// 解析文本中的首个 token：若文本整体含 `$` 则按完整字符串递归展开，否则按单个 token 解析；查不到时原样返回，保证调用方总能拿到一个字符串。
    pub fn resolve_or_self(&self, text: &str) -> String {
        if text.contains('$') {
            self.resolve_recursive(text)
        } else {
            self.resolve(text).unwrap_or_else(|| text.to_string())
        }
    }

    /// 递归展开文本中所有 `$` 占位符：逐字符扫描，遇到 `$` 切出其后字母数字/下划线组成的 token 并单级解析；循环至多 10 层直到无变化或达到深度上限，防止环形引用死循环。
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

    /// 面向 AI 上下文的解析：在递归展开基础上，对 `$M` 宏按语义（error/fail/exception/warning）选择性保留或跳过噪声，并把 `$P/$PK/$C/$FL` 等展开为可读值；末尾附带跳过的噪声计数提示。
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
                            // P2-11：跳噪/保留决策与登记侧共用同一谓词，不再本地重写关键字清单。
                            if is_semantic_macro(val) {
                                next_str.push_str(val);
                                changed = true;
                            } else {
                                skipped_noise += 1;
                                changed = true;
                            }
                        } else {
                            next_str.push_str(token);
                        }
                    } else if token.starts_with("$PK") || token.starts_with("$C") {
                        // $PK/$C 必须先于 $P 判断：$PK 以 $P 开头，若 $P 分支在前则 $PK 永不解析（Q442）。
                        // 注：flags 前缀约定为 $C；$FL 非合法前缀（resolve_one_level 无该分支、flags 表以 $C 键登记），
                        // 故不列入，避免「声称可解析实则永不展开」的误导（Q444 处置）。
                        if let Some(val) = self.resolve_one_level(token) {
                            next_str.push_str(&val);
                            changed = true;
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
    /// 构造 `DictionaryEngine`：仅持有一个新的 `DictionaryManager` 引用，
    /// 所有词表登记/查询/snapshot 一律委托给 manager（P3-01/P3-86：原 12 个本地字段均为死字段，已删除）。
    pub fn new() -> Self {
        DictionaryEngine {
            manager: Some(Arc::new(DictionaryManager::new())),
        }
    }

    /// 以指定 `DictionaryManager` 构建引擎：先调用 `new()` 取得默认结构，再替换其内部的 manager 引用，便于注入共享或受控的字典管理器。
    pub fn with_manager(manager: Arc<DictionaryManager>) -> Self {
        let mut engine = Self::new();
        engine.manager = Some(manager);
        engine
    }

    /// 登记一个路径并取回其 token：优先委托 manager 生成可读路径 token；无 manager 时直接原样返回原始路径。
    pub fn add_path_layered(&mut self, original: &str) -> String {
        if let Some(m) = &self.manager {
            return to_readable_path_token(m, original);
        }
        original.to_string()
    }

    /// 登记一个宏定义并取回其 token：委托 manager 去重登记；无 manager 时原样返回。
    pub fn add_macro(&mut self, original: &str) -> String {
        if let Some(m) = &self.manager {
            return m.get_or_add_macro(original);
        }
        original.to_string()
    }

    /// 登记一个包路径并取回其 token：委托 manager 去重登记；无 manager 时原样返回。
    pub fn add_package(&mut self, original: &str) -> String {
        if let Some(m) = &self.manager {
            return m.get_or_add_package(original);
        }
        original.to_string()
    }

    /// 生成当前字典快照：委托 manager 导出一份 `Dictionary`；无 manager 时返回空字典，用于压缩前后的对照与回放。
    pub fn snapshot(&self) -> Dictionary {
        if let Some(m) = &self.manager {
            return m.snapshot();
        }
        Dictionary::new()
    }

    /// 将路径 token 简化为骨架：仅对 `$P` 路径保留「首段/.../末段」形态以缩短长度；解析失败或层级不足时回退为原 token。
    pub fn skeletonize_path(&self, token: &str) -> String {
        // $PK 是包 token，不是路径，排除以免误走路径骨架化（Q442）
        if !token.starts_with("$P") || token.starts_with("$PK") {
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

/// 把原始路径转换为「前缀 token + 分隔符 + 叶子」形式：目录型路径整体登记，文件型路径只把目录前缀令牌化而保留文件名可读，避免路径过长淹没语义。
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
