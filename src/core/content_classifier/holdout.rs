//! content classifier holdout 盲测共享辅助
//!
//! # 职责
//!
//! 为「泛化（holdout）盲测」提供与压缩链路同源的评测原语，供两类调用方复用：
//!
//! - `src/bin/classifier_holdout.rs`：命令行盲测 runner（跑基线、写 `docs/audit` 报告）。
//! - `content_classifier/test.rs::holdout_gate`：泛化门禁测试（`Bayesian recall` /
//!   `structured detect` / `corpus isolation` 三断言）。
//!
//! # 分工语义（根因隔离）
//!
//! 盲测语料根 `classifier_holdout/` 与训练语料 `samples/` 相互隔离：
//! `build.rs` 只聚合 `samples/<plugin_dir>` 白名单，`sweep_*` 只自评 `samples/`。
//! 本模块的一切读取都指向 `classifier_holdout/` 的**物理文件**（读盘 `read_to_string`），
//! 严禁在此内联 mock 文本（AGENTS 红线）。
//!
//! # 两套正交评测面
//!
//! - **bayesian**：`bayesian/<category_name>/case_*.log`，期望标签=父目录名，
//!   用 `NaiveBayesClassifier::classify()` 评测语义路由的泛化能力。
//! - **structured**：`structured/<fmt>/case_*`，用结构化插件 `detect()`（0/1 布尔）评测；
//!   与贝叶斯正交，结构化识别不依赖分类器。
//!   `structured/gap_probes/**` 只记录命中、不判定（用于定位已知覆盖缺口，如 TOML/INI）。

use super::model::Category;
use super::NaiveBayesClassifier;
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::{Slice, SliceType};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// holdout 语料根目录（与 `samples/` 平级的独立盲测语料根）。
pub fn holdout_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("classifier_holdout")
}

/// 列出目录下所有普通文件（非递归），按路径排序，保证评测顺序稳定。
fn scan_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 列出目录下的直接子目录（非递归），按路径排序。
fn fs_read_subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 一个 bayesian 盲测样本条目：期望类别名（=父目录名）+ 物理文件路径。
#[derive(Debug, Clone)]
pub struct BayesCase {
    pub path: PathBuf,
    pub expected: String,
}

/// 递归扫描 `bayesian/<cat>/case_*.log`，期望标签取父目录名（`Category::name()`）。
pub fn load_bayesian_spec(root: &Path) -> Vec<BayesCase> {
    let bayes_dir = root.join("bayesian");
    let mut cases = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&bayes_dir) {
        for e in rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
            let expected = e
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            for file in scan_files(&e) {
                cases.push(BayesCase {
                    path: file,
                    expected: expected.clone(),
                });
            }
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    cases
}

/// 单个 bayesian 样本的预测结果。
#[derive(Debug, Clone)]
pub struct BayesResult {
    pub expected: String,
    pub actual: String,
    pub confidence: f32,
    pub margin: f32,
}

/// 用给定分类器对一个文本块做一次 `classify()`，包装成 [`BayesResult`]。
pub fn run_bayesian_case(
    classifier: &NaiveBayesClassifier,
    expected: String,
    text: &str,
) -> BayesResult {
    let r = classifier.classify(text);
    BayesResult {
        expected,
        actual: r.category.name().to_string(),
        confidence: r.confidence,
        margin: r.margin,
    }
}

/// bayesian 盲测聚合指标：类别顺序、混淆矩阵、逐类 recall、整体 top1 与平均 margin。
#[derive(Debug, Clone)]
pub struct BayesReport {
    pub cat_order: Vec<&'static str>,
    pub confusion: Vec<Vec<usize>>,
    pub recall: HashMap<String, f64>,
    pub total: usize,
    pub correct: usize,
    pub top1_acc: f64,
    pub mean_margin: f64,
}

/// 汇总一组 bayesian 预测结果：以期望类别为行、实际类别为列构造混淆矩阵，
/// 并计算逐类 recall、整体 top1 准确率与平均 margin。
pub fn bayesian_metrics(results: &[BayesResult]) -> BayesReport {
    let cat_order: Vec<&'static str> = Category::ALL.iter().map(|c| c.name()).collect();
    let n = cat_order.len();
    let idx: HashMap<&str, usize> = cat_order.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    // 兜底：未知类别名落到 GenericText 末尾格。
    let generic_idx = n - 1;
    let mut confusion = vec![vec![0usize; n]; n];
    for r in results {
        let ei = idx.get(r.expected.as_str()).copied().unwrap_or(generic_idx);
        let ai = idx.get(r.actual.as_str()).copied().unwrap_or(generic_idx);
        confusion[ei][ai] += 1;
    }
    let mut correct = 0usize;
    let mut margin_sum = 0.0f32;
    for r in results {
        if r.expected == r.actual {
            correct += 1;
        }
        margin_sum += r.margin;
    }
    let mut recall = HashMap::new();
    for (i, name) in cat_order.iter().enumerate() {
        let row_sum: usize = confusion[i].iter().sum();
        recall.insert(
            name.to_string(),
            if row_sum > 0 {
                confusion[i][i] as f64 / row_sum as f64
            } else {
                f64::NAN
            },
        );
    }
    let total = results.len();
    BayesReport {
        cat_order,
        confusion,
        recall,
        total,
        correct,
        top1_acc: if total > 0 {
            correct as f64 / total as f64
        } else {
            0.0
        },
        mean_margin: if total > 0 {
            margin_sum as f64 / total as f64
        } else {
            0.0
        },
    }
}

// ================= 结构化（detect）盲测原语 =================

/// 参与盲测的结构化插件名（与插件的 `name()` 一致）。
pub const STRUCTURED_PLUGINS: &[&str] =
    &["json", "xml_html", "yaml", "markdown", "ndjson", "toml_ini"];

/// 将 `structured/<fmt>` 子目录名映射为目标结构化插件名；非结构化格式返回 `None`。
pub fn fmt_to_plugin(fmt: &str) -> Option<&'static str> {
    match fmt {
        "json" => Some("json"),
        "xml" | "html" => Some("xml_html"),
        "yaml" => Some("yaml"),
        "markdown" => Some("markdown"),
        "ndjson" => Some("ndjson"),
        _ => None,
    }
}

/// 一个 structured 盲测样本条目。
///
/// `expected_plugin` 为期望命中插件名；`probe` 为 `true` 表示来自 `gap_probes/`
/// （只记录实际命中、不参与「必须 detect 命中」的强断言）。
#[derive(Debug, Clone)]
pub struct StructCase {
    pub path: PathBuf,
    pub expected_plugin: String,
    pub probe: bool,
}

/// 加载 `structured/<fmt>/*` 与 `structured/gap_probes/<sub>/*` 全部样本。
pub fn load_structured_spec(root: &Path) -> Vec<StructCase> {
    let base = root.join("structured");
    let mut cases = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
            let dir_name = e
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if dir_name == "gap_probes" {
                // 探针子目录（可按缺口类型再分子目录，如 toml/ini）：期望插件留空，
                // 只记录实际命中，不做「必须命中」强断言。
                for sub in fs_read_subdirs(&e) {
                    for file in scan_files(&sub) {
                        cases.push(StructCase {
                            path: file,
                            expected_plugin: String::new(),
                            probe: true,
                        });
                    }
                }
            } else if let Some(plugin) = fmt_to_plugin(&dir_name) {
                for file in scan_files(&e) {
                    cases.push(StructCase {
                        path: file,
                        expected_plugin: plugin.to_string(),
                        probe: false,
                    });
                }
            } else if dir_name == "mixed" {
                // 两层化×结构化协同样本：外壳里可能内嵌多个结构化块，无法用单一插件强断言，
                // 只记录实际 detect 命中分布，供协同效果评估。是否进入两层流水线由 runner 判定。
                for file in scan_files(&e) {
                    cases.push(StructCase {
                        path: file,
                        expected_plugin: String::new(),
                        probe: true,
                    });
                }
            }
            // 未知非探针格式子目录：跳过（不属于结构化盲测面）。
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    cases
}

/// 从文本构造仅供 `detect()` 用的轻量 `Slice`（沿用插件测试的构造范式）。
fn make_slice(text: &str) -> Slice<'_> {
    Slice {
        id: 0,
        text: Cow::Borrowed(text),
        slice_type: SliceType::LogBlock,
        offset: 0,
        line_start: 1,
        line_end: text.lines().count().max(1),
        file_metadata: None,
        flags: Default::default(),
    }
}

/// 对 `plugins_by_name[name]` 调一次 `detect()`，返回命中分数。
pub fn detect_with(
    plugins_by_name: &HashMap<String, &dyn Plugin>,
    plugin: &str,
    text: &str,
) -> Option<f32> {
    let p = plugins_by_name.get(plugin)?;
    let slice = make_slice(text);
    p.detect(&slice)
}

/// 依次对全部 [`STRUCTURED_PLUGINS`] 调 `detect()`，收集命中列表（插件名, 分数）。
pub fn struct_detects(
    plugins_by_name: &HashMap<String, &dyn Plugin>,
    text: &str,
) -> Vec<(String, f32)> {
    STRUCTURED_PLUGINS
        .iter()
        .filter_map(|name| detect_with(plugins_by_name, name, text).map(|s| (name.to_string(), s)))
        .collect()
}
