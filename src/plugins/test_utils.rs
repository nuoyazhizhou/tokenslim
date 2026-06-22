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

/// VCS 插件专用：从 `samples/<plugin_dir>/<stem>.log` 读取测试用例。
///
/// 等价于 VCS tests.rs 中的 `read_case(name)` 函数。
pub fn vcs_read_case(plugin_dir: &str, stem: &str) -> String {
    let p = vcs_sample_dir(plugin_dir).join(format!("{stem}.log"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}
