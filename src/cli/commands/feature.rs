//! `tokenslim feature` 子命令：分类器增量特征库治理（计划 T-E）审批通过后落地。
//!
//! 用于将「误路由样本」按正确类别回填到持久化特征库，训练朴素贝叶斯分类器
//! 修正类别边界，降低后续误路由概率。两条用法：
//!
//! - **回填**：`tokenslim feature --learn <category> --sample <file> [--feature-lib <path>]`
//!   把 `<file>` 中样本文本按 `<category>` 类别并入特征库并持久化。
//! - **查看**：`tokenslim feature [--feature-lib <path>]`
//!   无回填参数时打印特征库当前各类别词条数摘要。

use crate::cli::types::{CliArgs, CliError};
use crate::core::content_classifier::{feature_reader, Category};

/// 处理 `feature` 子命令的入口。
pub(crate) fn handle_feature_command(args: &CliArgs) -> Result<(), CliError> {
    let lib_path = args
        .feature_lib
        .clone()
        .unwrap_or_else(feature_reader::default_feature_lib_path);

    // 加载现有特征库（缺失/损坏按空表处理）
    let mut table = feature_reader::load(&lib_path).map_err(|e| CliError::Config(e.to_string()))?;

    match &args.feature_learn {
        Some(cat_name) if args.feature_sample.is_some() => {
            let sample_path = args
                .feature_sample
                .as_ref()
                .expect("已校验 feature_sample 存在");
            let sample = std::fs::read_to_string(sample_path)?;
            if sample.trim().is_empty() {
                return Err(CliError::InvalidArgs(format!(
                    "样本文件 '{}' 为空，无可回填特征",
                    sample_path.display()
                )));
            }
            let category = Category::from_name(cat_name);
            if category.name() != cat_name {
                return Err(CliError::InvalidArgs(format!(
                    "未知类别 '{}'（期望 cargo|gcc|test|git_diff|generic_text）",
                    cat_name
                )));
            }
            let terms = feature_reader::append_sample(&mut table, category, &sample);
            if terms.is_empty() {
                return Err(CliError::Config(format!(
                    "样本未提取到有效特征词（均为噪声/停用词），未落盘"
                )));
            }
            feature_reader::save(&lib_path, &table).map_err(|e| CliError::Config(e.to_string()))?;
            println!(
                "已回填 {} 个特征词到类别 '{}'，特征库已更新 => {}",
                terms.len(),
                category.name(),
                lib_path.display()
            );
            println!(
                "  特征词: {}",
                terms
                    .iter()
                    .map(|(w, _)| w.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        // 仅查看：打印当前特征库摘要
        _ => print_lib_summary(&lib_path, &table),
    }
    Ok(())
}

/// 打印特征库摘要：各类别词条数与特征库路径。
fn print_lib_summary(
    lib_path: &std::path::Path,
    table: &std::collections::HashMap<String, std::collections::HashMap<String, f64>>,
) {
    println!("特征库路径: {}", lib_path.display());
    if table.is_empty() {
        println!("（空特征库，尚无回填样本）");
        return;
    }
    for name in Category::ALL {
        let n = table.get(name.name()).map(|m| m.len()).unwrap_or(0);
        println!("  {:<14} {} 词条", name.name(), n);
    }
}
