//! 插件测试公共工具模块
//!
//! 本模块提取了所有插件 test.rs / tests.rs 中重复的三件套辅助函数，
//! 消除约 1000 行跨 45 个文件的重复代码。
//!
//! # 使用方式
//!
//! 在插件的 `test.rs` 或 `tests.rs` 中：
//!
//! ```rust,ignore
//! use crate::plugins::test_utils::{read_sample_file, make_test_slice, compress_to_string};
//! ```
//!
//! # 设计约束
//!
//! - 本模块仅在 `#[cfg(test)]` 下编译，不进入生产二进制。
//! - 严禁在本模块中 hardcode 任何日志字符串（测试架构铁律）。
//! - 所有函数均为纯函数，无副作用，可在并行测试中安全调用。

use crate::core::compression::Token;
use crate::core::dedup_engine::{DedupConfig, DedupEngine};
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::{Slice, SliceType};
use std::borrow::Cow;
use std::path::PathBuf;

/// 从 `samples/<plugin_dir>/<file_name>` 读取测试样本文件。
///
/// # 参数
/// - `plugin_dir`：插件样本目录名，例如 `"yaml_plugin"`。
/// - `file_name`：文件名（含扩展名），例如 `"case_001_simple_yaml.log"` 或 `"case_002.json"`。
///
/// # Panic
/// 文件不存在或读取失败时 panic，并打印完整路径，便于 CI 定位。
pub fn read_sample_file(plugin_dir: &str, file_name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(plugin_dir)
        .join(file_name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}

/// 从 `samples/<plugin_dir>/<stem>.log` 读取测试样本文件（自动补 `.log` 后缀）。
///
/// 适用于大多数插件的 `case_XXX_name` 命名约定。
/// 若样本文件使用非 `.log` 扩展名（如 `.json`、`.yaml`、`.md`），请改用 [`read_sample_file`]。
pub fn read_sample_log(plugin_dir: &str, stem: &str) -> String {
    read_sample_file(plugin_dir, &format!("{stem}.log"))
}

/// 构造一个用于测试的 `Slice`，绑定到给定文本。
///
/// # 参数
/// - `text`：切片文本内容（借用）。
/// - `slice_type`：切片类型，默认推荐 `SliceType::LogBlock`；
///   对于代码/JSON/YAML 等结构化内容可传 `SliceType::Unknown`。
pub fn make_test_slice<'a>(text: &'a str, slice_type: SliceType) -> Slice<'a> {
    Slice {
        id: 1,
        text: Cow::Borrowed(text),
        slice_type,
        offset: 0,
        line_start: 1,
        line_end: text.lines().count().max(1),
        file_metadata: None,
        flags: Default::default(),
    }
}

/// 构造 `SliceType::LogBlock` 类型的测试切片（最常用的快捷版本）。
pub fn make_log_slice(text: &str) -> Slice<'_> {
    make_test_slice(text, SliceType::LogBlock)
}

/// 对给定插件和文本执行压缩，返回 compact 字符串。
///
/// 内部使用无 dictionary manager 的默认 `DictionaryEngine`（与 showcase 测试链路一致）。
/// 若需要访问压缩后的字典（例如做 decompress 往返验证），请直接调用 `plugin.compress()`。
pub fn compress_to_string<P: Plugin>(plugin: &P, text: &str, slice_type: SliceType) -> String {
    let slice = make_test_slice(text, slice_type);
    let mut dict = DictionaryEngine::new();
    let mut dedup = DedupEngine::new(DedupConfig::default());
    let arena = bumpalo::Bump::new();
    let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
    result
        .tokens
        .iter()
        .filter_map(|t| match t {
            Token::Text(s) => Some(s.as_ref()),
            _ => None,
        })
        .collect::<String>()
}

/// 对给定插件和文本执行压缩，返回 `(compact_string, dict_engine)`。
///
/// 适用于需要做 decompress 往返验证的测试（如 json / yaml 插件）。
/// 返回的字符串仅拼接 Token::Text；非文本令牌不会被串行化到该字符串中。
/// 需要语义完整的往返断言时，调用方必须结合返回的 DictionaryEngine 进行解压。
pub fn compress_with_dict<P: Plugin>(
    plugin: &P,
    text: &str,
    slice_type: SliceType,
) -> (String, DictionaryEngine) {
    let slice = make_test_slice(text, slice_type);
    let mut dict = DictionaryEngine::new();
    let mut dedup = DedupEngine::new(DedupConfig::default());
    let arena = bumpalo::Bump::new();
    let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
    let out = result
        .tokens
        .iter()
        .filter_map(|t| match t {
            Token::Text(s) => Some(s.as_ref()),
            _ => None,
        })
        .collect::<String>();
    (out, dict)
}

/// VCS 插件专用：返回 `samples/<plugin_dir>/` 目录的 `PathBuf`。
///
/// 供 VCS tests.rs 中的 `sample_dir()` 函数替换使用。
pub fn vcs_sample_dir(plugin_dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(plugin_dir)
}

/// 将字典引擎快照扁平化为 `token -> 原文` 的 JSON 字符串，供审计 dictside 侧通道携带。
///
/// 覆盖全部可逆 token 前缀（`$PK/$P/$D/$M/$C/$FL`）对应的 packages/paths/directories/
/// macros/flags/files 映射；**仅保留 `compact` 中实际引用的 token**（用 `$<前缀><数字>` 匹配），
/// 避免把压缩器内部生成但未落入 compact 的中间 token（如 web_log 的 $M UA 宏）泄漏成噪声字典。
/// 用 `BTreeMap` 保证序列化顺序稳定、审计报告可复现；无引用 token 时返回空串。
pub fn full_dict_json(engine: &DictionaryEngine, compact: &str) -> String {
    use std::collections::{BTreeMap, HashMap};

    let snap = engine.snapshot();
    let mut merged: HashMap<String, String> = HashMap::new();
    merged.extend(snap.paths.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged.extend(snap.directories.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged.extend(snap.packages.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged.extend(snap.macros.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged.extend(snap.flags.iter().map(|(k, v)| (k.clone(), v.clone())));
    merged.extend(snap.files.iter().map(|(k, v)| (k.clone(), v.clone())));
    if merged.is_empty() {
        return String::new();
    }
    // 从 compact 中提取所有 `$<前缀><数字>` token，只保留被引用的字典项。
    let referenced = collect_tokens(compact);
    if referenced.is_empty() {
        return String::new();
    }
    let mut kept = BTreeMap::new();
    for tok in referenced {
        if let Some(v) = merged.get(&tok) {
            kept.insert(tok, v.clone());
        }
    }
    if kept.is_empty() {
        return String::new();
    }
    serde_json::to_string(&kept).unwrap_or_default()
}

/// 扫描文本中的 `$PKn/$Pn/$Dn/$Mn/$Cn/$FLn` 可逆 token（长前缀 `$PK` 优先判），返回去重的 token 集合。
fn collect_tokens(text: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != '$' {
            i += 1;
            continue;
        }
        let rest: String = bytes[i + 1..].iter().collect();
        // 长前缀优先（$PK 必须比 $P 先判）。匹配后直接推进，避免 `$X5` 之类误匹配。
        for p in ["PK", "P", "D", "M", "C", "FL"] {
            if rest.starts_with(p) {
                let mut j = p.len();
                let mut digit_ok = false;
                while j < rest.len() && rest.as_bytes()[j].is_ascii_digit() {
                    j += 1;
                    digit_ok = true;
                }
                if digit_ok {
                    out.insert(format!("${}{}", p, &rest[p.len()..j]));
                    i += 1 + p.len() + (j - p.len());
                    break;
                }
            }
        }
        i += 1;
    }
    out
}

/// VCS 插件专用：从 `samples/<plugin_dir>/<stem>.log` 读取测试用例。
///
/// 等价于 VCS tests.rs 中的 `read_case(name)` 函数。
pub fn vcs_read_case(plugin_dir: &str, stem: &str) -> String {
    let p = vcs_sample_dir(plugin_dir).join(format!("{stem}.log"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}
