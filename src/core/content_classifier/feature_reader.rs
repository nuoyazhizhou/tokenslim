//! 运行期特征库读写与增量回填（计划 T-E 增量学习）
//!
//! # 职责
//!
//! 让分类器具备「运行期学习」能力：将误路由样本按正确类别回填到持久化特征库
//! （`classifier_features.json`），并在分类器初始化（`seed_model`）时合并这些增量，
//! 从而不断逼近真实语料的类别边界，降低后续误路由概率。
//!
//! # 特征库格式
//!
//! JSON 对象，键为类别名（[`Category::name`] 的取值：`cargo`/`gcc`/`test`/
//! `git_diff`/`generic_text`），值为「单词 → 权重」映射：
//!
//! ```json
//! { "gcc": { "abi": 1.4, "collect2": 3.0 }, "test": { "pytest": 2.5 } }
//! ```
//!
//! # 设计要点
//!
//! - **复用 T-B 分词/滤噪管线**：回填时对样本文本调用
//!   [`crate::core::content_classifier::corpus_tokens::tokenize`]，与编译期特征聚合器、
//!   运行期分类器共享同一套分词与噪声过滤规则，保证增量特征与语料特征口径一致。
//! - **纯 std + serde_json**：序列化直接用 `serde_json::Map`，不引入新依赖；
//!   以「类别名 → 词 → 权重」的扁平字符串键存储，规避为 [`Category`] 派生 serde。
//! - **Fail-Soft**：特征库缺失/损坏时加载为空表，绝不阻断分类器初始化。
//! - **去重计权**：同一样本内每词只计一次（去重后权重 = 词出现相对频次的增强证据），
//!   避免单行高频词过度放权。

use super::corpus_tokens::tokenize;
use super::model::Category;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 特征库文件名（位于配置目录下）。
pub const FEATURE_LIB_FILE: &str = "classifier_features.json";

/// 特征库错误。
#[derive(Debug, thiserror::Error)]
pub enum FeatureError {
    #[error("特征库 IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("特征库 JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("特征库含未知类别名 '{0}'")]
    UnknownCategory(String),
}

/// 默认特征库路径：优先取环境变量 `TS_CC_FEATURE_LIB`，其次取
/// 配置目录下的 `classifier_features.json`（与插件配置同一基准目录，
/// 见 `plugin_config_loader::PluginConfigLoader::resolve_config_dir`）。
pub fn default_feature_lib_path() -> PathBuf {
    if let Ok(p) = std::env::var("TS_CC_FEATURE_LIB") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    crate::core::plugin_config_loader::PluginConfigLoader::resolve_config_dir()
        .join(FEATURE_LIB_FILE)
}

/// 从特征库文件加载「类别名 → 词 → 权重」映射。
///
/// Fail-Soft：文件不存在或内容损坏时返回空表（不报错），保证分类器初始化不受阻断；
/// 仅当文件存在但结构非法（如未知类别名导致无法安全落位）时才返回 `Err`。
pub fn load(path: &Path) -> Result<HashMap<String, HashMap<String, f64>>, FeatureError> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let text = std::fs::read_to_string(path)?;
    let raw: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)?;
    let mut table: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for (cat_name, value) in raw {
        let obj = value.as_object().ok_or_else(|| {
            FeatureError::Json(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("类别 '{cat_name}' 的值不是对象"),
            )))
        })?;
        let mut words: HashMap<String, f64> = HashMap::new();
        for (word, w) in obj {
            // 兼容数值与数字字符串两种写法，其余类型忽略。
            let weight = if let Some(n) = w.as_f64() {
                n
            } else if let Some(s) = w.as_str() {
                s.parse::<f64>().unwrap_or(0.0)
            } else {
                0.0
            };
            if weight > 0.0 {
                words.insert(word.clone(), weight);
            }
        }
        table.insert(cat_name, words);
    }
    Ok(table)
}

/// 将「类别名 → 词 → 权重」映射写回特征库文件（原子替换：先写临时文件再改名）。
///
/// 若父目录不存在会一并创建；仅写入权重为正（`> 0.0`）的词，避免空噪声落盘。
pub fn save(
    path: &Path,
    table: &HashMap<String, HashMap<String, f64>>,
) -> Result<(), FeatureError> {
    let mut root = serde_json::Map::new();
    // 按类别名排序写出，保证内容确定性、可复现。
    let mut cats: Vec<&String> = table.keys().collect();
    cats.sort();
    for cat_name in cats {
        let mut words = serde_json::Map::new();
        let mut ws: Vec<(String, f64)> = table[cat_name]
            .iter()
            .map(|(w, weight)| (w.clone(), *weight))
            .collect();
        ws.sort_by(|a, b| a.0.cmp(&b.0));
        for (w, weight) in ws {
            if weight > 0.0 {
                words.insert(w, serde_json::Value::from((weight * 100.0).round() / 100.0));
            }
        }
        root.insert(cat_name.clone(), serde_json::Value::Object(words));
    }
    let body = serde_json::Value::Object(root).to_string();

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 将单个误路由样本按指定类别回填到特征库（`table` 就地变更），返回据此生成的
/// 「词 → 权重」证据，供调用方折叠进分类器模型（`model.append_features`）。
///
/// # 加权策略
/// 复用 [`tokenize`] 对样本分词（已滤除噪声/停用词），对每个**唯一**词累加 1.0 证据：
/// - 样本中出现多次的词仍只计一次（去重），避免高频词单句放权；
/// - 回归样本越干净，注入的术语越聚焦，越能修正类别边界。
///
/// # 返回
/// 与注入到 `table` 相同的「词 → 权重」列表，便于调用方同步 `append_features`。
#[tracing::instrument(level = "debug", skip_all)]
pub fn append_sample(
    table: &mut HashMap<String, HashMap<String, f64>>,
    category: Category,
    sample: &str,
) -> Vec<(String, f64)> {
    // 复用 T-B 分词/滤噪管线；用 HashSet 去重，保证同一样本内每词只注入一份证据。
    let tokens = tokenize(sample);
    let mut seen = std::collections::HashSet::new();
    let mut terms: Vec<(String, f64)> = Vec::new();
    for w in tokens {
        if seen.insert(w.clone()) {
            terms.push((w, 1.0));
        }
    }

    let cat = table.entry(category.name().to_string()).or_default();
    for (w, weight) in &terms {
        *cat.entry(w.clone()).or_insert(0.0) += weight;
    }
    terms
}

/// 将特征库加载结果合并进分类器模型（供 `seed_model` 初始化时调用）。
///
/// 对每个有效类别调用 [`NaiveBayesClassifier::append_features`]，使增量特征参与后续
/// 平滑与 softmax；未知类别名直接跳过（不回退、不报错），保证 Fail-Fast 与容错兼顾。
#[tracing::instrument(level = "debug", skip_all)]
pub fn merge_into_model(
    model: &mut super::model::NaiveBayesClassifier,
    table: &HashMap<String, HashMap<String, f64>>,
) {
    for (cat_name, words) in table {
        let cat = Category::from_name(cat_name);
        // from_name 对未知名会回退 GenericText，此处仅回填已知类别以规避噪声污染。
        if cat.name() != cat_name {
            continue;
        }
        let terms: Vec<(String, f64)> = words
            .iter()
            .filter(|(_, &w)| w > 0.0)
            .map(|(w, &weight)| (w.clone(), weight))
            .collect();
        if !terms.is_empty() {
            model.append_features(cat, terms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::content_classifier::features::seed_model;

    /// save/load 往返一致：写入的表经序列化→反序列化后应完全还原（含词条与权重）。
    #[test]
    fn save_load_roundtrip() {
        let path =
            std::env::temp_dir().join(format!("ts_feature_roundtrip_{}.json", std::process::id()));
        let mut table: HashMap<String, HashMap<String, f64>> = HashMap::new();
        table.insert(
            "gcc".to_string(),
            HashMap::from([("collect2".to_string(), 3.0), ("abi".to_string(), 1.4)]),
        );
        table.insert(
            "test".to_string(),
            HashMap::from([("pytest".to_string(), 2.5)]),
        );
        save(&path, &table).expect("save 应成功");
        let loaded = load(&path).expect("load 应成功");
        assert_eq!(loaded, table, "往返后特征库应完全一致");
        std::fs::remove_file(&path).ok();
    }

    /// 缺失文件按空表处理（Fail-Soft），不报错。
    #[test]
    fn load_missing_is_empty() {
        let path = std::env::temp_dir().join("ts_feature_definitely_missing.json");
        let table = load(&path).expect("缺失文件应返回空表而非错误");
        assert!(table.is_empty());
    }

    /// 回填去重：同一词在样本中出现多次也只注入一份 1.0 证据；噪声词（数字/停用词）被滤除。
    #[test]
    fn append_sample_dedups_and_filters_noise() {
        let mut table: HashMap<String, HashMap<String, f64>> = HashMap::new();
        // "collect2" 重复出现 3 次，应只计 1 次；"the"(停用词) 与 "123"(纯数字) 应被剔除。
        let sample = "collect2 collect2 collect2 the 123 undefined reference";
        let terms = append_sample(&mut table, Category::Gcc, sample);
        let gcc = &table["gcc"];
        assert_eq!(gcc.get("collect2"), Some(&1.0), "重复词应仅注入一份");
        assert_eq!(gcc.get("the"), None, "停用词应被滤除");
        assert_eq!(gcc.get("123"), None, "纯数字应被滤除");
        assert!(gcc.get("undefined").is_some(), "有效特征词应保留");
        assert_eq!(terms.len(), 3, "应仅含 3 个去重有效词");
    }

    /// 增量回填能修正分类：注入某类别专属词后，含该词的文本应被正确归类。
    #[test]
    fn merge_into_model_shifts_classification() {
        let mut model = seed_model();
        // 注入前：该专属词不在任何类别词表，属 OOV，文本应归兜底 GenericText。
        let marker = "xyzwgccmarker";
        let base = model.classify(marker);
        assert_eq!(base.category, Category::GenericText, "注入前无信号应归兜底");

        // 注入 gcc 专属的高权重词（用与既有词无碰撞的合成词）。
        let mut table: HashMap<String, HashMap<String, f64>> = HashMap::new();
        table.insert(
            "gcc".to_string(),
            HashMap::from([(marker.to_string(), 8.0)]),
        );
        merge_into_model(&mut model, &table);

        let now = model.classify(marker);
        assert_eq!(now.category, Category::Gcc, "注入专属词后应归类为 gcc");
    }
}
