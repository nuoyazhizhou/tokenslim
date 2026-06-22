// filter_discover/mod.rs
// 过滤器发现模块 - 扫描 session 文件并识别缺失的过滤器

pub mod aggregator;
pub mod classifier;
pub mod parser;
pub mod types;

pub use aggregator::*;
pub use classifier::*;
pub use types::*;

use crate::core::tracking::Tracker;
use std::path::Path;

/// 发现缺失的过滤器
///
/// # 参数
/// - `session_files`: session 文件路径列表
/// - `tracker`: Token 追踪器（用于加载历史 savings_pct）
///
/// # 返回
/// - `DiscoverResult`: 发现结果
#[tracing::instrument(level = "debug", skip_all)]
pub fn discover_filters(
    session_files: &[impl AsRef<Path>],
    tracker: &Tracker,
) -> Result<DiscoverResult, String> {
    // 1. 解析所有 session 文件
    let mut all_commands = Vec::new();
    for file in session_files {
        let commands = parser::parse_session_file(file.as_ref())?;
        all_commands.extend(commands);
    }

    // 2. 分类命令
    let classified = classify_commands(&all_commands)?;

    // 3. 聚合并估算
    let result = aggregate_and_estimate(&classified, tracker)?;

    Ok(result)
}
