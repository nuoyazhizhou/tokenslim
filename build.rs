//! 特征聚合器（feature_builder，计划 T-B）
//!
//! # 职责
//!
//! 编译期（`cargo build`）扫描 `samples/` 下的代表性插件**语料**，对其中
//! 真实命令输出做与运行期分类器完全一致的分词与词频统计，再经「跨类别可区分性」
//! 过滤，生成一组用于增强朴素贝叶斯分类器词表的特征数据，写入
//! `OUT_DIR/features_generated.rs`，由
//! [`crate::core::content_classifier::features`] 以 `include!` 方式合入种子特征表，
//! 从而让分类器学识真实语料中出现、但手工词表未收录的高区分度词。
//!
//! # 设计要点
//!
//! - **纯 std**：不引入 build-dependencies，分词/计数/权重全部基于标准库。
//! - **Fail-Soft**：任何 IO/解析异常都不让编译失败——退化为「生成空表 + 可用性标志
//!   = false」，运行期回到纯种子特征。保证增强可有可无，绝不引入编译硬故障。
//! - **跨类别可区分性（IDF 思想）**：仅采纳「在单一类别中占绝对多数」的词，自动丢弃
//!   error/warning/source/target 等跨类别通用词，避免污染各语义类别。
//! - **Cargo 类别刻意不聚合**：其唯一候选插件 `rust_go_plugin` 同时混有 Rust 输出与
//!   Go panic/goroutine 语料，自动聚合会把 Go 词灌入 Cargo 特征并导致路由回退，
//!   故保留手工种子特征；仅聚合 gcc / test / git_diff 三个类别纯净的样本目录。

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// 参与聚合的「类别名 →（语义类别枚举变体, 采样插件目录）」映射。
/// 只收录语料纯净、与类别一一对应的插件目录；`None` 表示该类别不聚合。
///
/// # 训练/测试同源隔离红线（holdout 盲测）
///
/// 本映射**仅**聚合白名单内的 `samples/<plugin_dir>` 语料；`classifier_holdout/`
/// 是与 `samples/` 平级的**独立盲测语料根**（见 `classifier_holdout/README.md`），
/// 用于评测分类器对**未知输出**的泛化能力。**严禁**把 `classifier_holdout/**` 的任何
/// 文件加入本清单或迁入 `samples/`，否则会重新引入「训练/测试同源泄漏」——届时
/// `sweep_*` 自评、holdout 泛化门禁都将失准。此红线仅约束语料来源，不改变聚合逻辑。
const CATEGORY_SOURCES: &[(&str, &str)] = &[
    ("Gcc", "gcc_log_plugin"),
    ("Test", "pytest_plugin"),
    ("GitDiff", "git_diff_plugin"),
    ("DockerK8s", "kubernetes_docker_plugin"),
    ("Node", "nodejs_plugin"),
    ("Node", "node_error_plugin"),
    ("Web", "webpack_vite_plugin"),
    ("Java", "java_stack_plugin"),
    ("SpringBoot", "spring_boot_plugin"),
    ("Maven", "maven_plugin"),
    ("PhpRuby", "php_ruby_plugin"),
    ("Dotnet", "dotnet_plugin"),
    ("Helm", "helm_plugin"),
    ("Terraform", "terraform_plugin"),
    ("WebLog", "web_log_plugin"),
    ("PythonTraceback", "python_traceback_plugin"),
    ("Bazel", "bazel_plugin"),
    ("Gradle", "android_gradle_plugin"),
    ("Xcode", "xcode_log_plugin"),
    ("Ansible", "ansible_plugin"),
    ("Pulumi", "pulumi_plugin"),
    ("CloudFormation", "cloudformation_plugin"),
    ("Sql", "sql_plugin"),
    ("DbLog", "db_log_plugin"),
    ("UnityUnreal", "unity_unreal_plugin"),
    ("Syslog", "syslog_plugin"),
    // CiLog / CloudLog 同为「剥皮」类别，刻意不聚合语料（故不入本表）——其语料是「包装 + 内嵌第三方
    // 输出」混合体（CiLog 内嵌 npm/test/gradle/docker；CloudLog 内嵌 HTTP access→web_log、
    // java/python/node、syslog/db），聚合会把内层词灌进包装桶，经 SHARE_THRESHOLD 全局改权殃及
    // node/maven/docker/web_log 等类别边界（实测 node 召回跌穿）。两者仅靠种子词判别，内嵌内容
    // 按语义由内层插件接管属合法。CloudLog 皮检率天然低于 CiLog，见 features.rs 注释。
];

/// 只在类别内占绝对多数（≥ 此比例）的词才被采纳为该类别特征。
/// 低于该阈值说明词在多类别中通用，不具备区分度，予以剔除。
const SHARE_THRESHOLD: f64 = 0.55;
/// 词在类别内出现的最小次数（低于则视为噪音，忽略）。
const MIN_COUNT: usize = 3;
/// 每类别最多保留的特征词数量（控制生成文件规模与可读性）。
const MAX_WORDS_PER_CATEGORY: usize = 200;
/// 生成权重的「原始」上下限（未衰减前）。
const WEIGHT_MIN: f64 = 1.5;
const WEIGHT_MAX: f64 = 8.0;
/// 生成权重衰减系数：语料词频天然偏高，统一乘以此系数再夹取，
/// 使聚合特征以「补充证据」身份并入种子，避免喧宾夺主打乱手工调优的类别边界。
const WEIGHT_DECAY: f64 = 0.5;
/// 衰减后最小可采纳权重：低于此则丢弃（太弱的证据不纳入）。
const WEIGHT_FLOOR: f64 = 0.9;

fn main() {
    // 必须：让 cargo 在 build.rs 或 samples 语料变化时重跑本脚本。
    println!("cargo:rerun-if-changed=build.rs");
    let samples_root =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()))
            .join("samples");
    println!("cargo:rerun-if-changed={}", samples_root.display());
    // 分词/噪声/复合标记逻辑与运行期共享（include! 内联），其变化须触发重新聚合。
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()))
            .join("src/core/content_classifier/corpus_tokens.rs")
            .display()
    );
    // 同时监控脚本自身所依赖的分词/停用词定义，保证逻辑变更可重入。
    println!("cargo:rerun-if-env-changed=TS_CC_FEATURE_BUILDER_DISABLE");

    // 出口目录：生成文件写到这里，交给 features.rs include!。
    let out_dir = match env::var("OUT_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => {
            emit_stub("OUT_DIR 未定义，跳过特征聚合");
            return;
        }
    };

    // 支持显式关闭（CI 或某些打包场景），保证能完全退回种子特征。
    if env::var("TS_CC_FEATURE_BUILDER_DISABLE")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        emit_stub("TS_CC_FEATURE_BUILDER_DISABLE=1，跳过特征聚合");
        return;
    }

    match build_features(&samples_root) {
        Ok(rows) if !rows.is_empty() => write_feature_rows(&out_dir, rows),
        Ok(_) => emit_stub("未在任何采样目录发现可用语料"),
        Err(e) => emit_stub(&format!("聚合失败（已降级为纯种子）：{e}")),
    }
}

/// 扫描语料并生成（类别, 词, 权重）三元组列表。
/// 返回聚合出的特征列表；无可用语料时返回空 `Ok(vec![])`，异常返回 `Err` 说明。
fn build_features(samples_root: &Path) -> Result<Vec<(String, &str, f64)>, String> {
    // 1) 采集每类别词频。按「类别名 variant」聚合：同一类别可挂多个采样目录
    //    （如 Node 聚合 nodejs_plugin + node_error_plugin），它们的词频合入同一桶，
    //    再做跨类别可区分性判定，避免同类别两目录间的通用词被误当多类别噪声过滤。
    let mut per_cat_counts: HashMap<&str, HashMap<String, usize>> = HashMap::new();
    let mut any_corpus = false;
    for &(variant, plugin_dir) in CATEGORY_SOURCES {
        let dir = samples_root.join(plugin_dir);
        if !dir.is_dir() {
            continue;
        }
        let counts = per_cat_counts.entry(variant).or_default();
        for entry in fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| format!("dir entry {}: {e}", dir.display()))?;
            let path = entry.path();
            // 只聚合命令输出的 `.log` 样本；跳过 scenario.yaml 等元数据与子目录。
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("log") || is_metadata_file(&path) {
                continue;
            }
            let text =
                fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            for tok in tokenize(&text) {
                *counts.entry(tok).or_insert(0) += 1;
            }
            any_corpus = true;
        }
    }
    if !any_corpus {
        return Ok(Vec::new());
    }

    // 2) 计算每词跨类别总频次，用于可区分性（share）判定。
    let mut total_by_word: HashMap<&str, usize> = HashMap::new();
    for counts in per_cat_counts.values() {
        for (w, c) in counts {
            *total_by_word.entry(w.as_str()).or_insert(0) += c;
        }
    }

    // 3) 逐类别按可区分性 + 词频生成加权词表。按固定类别顺序迭代保证输出确定。
    let mut result: Vec<(String, &str, f64)> = Vec::new();
    const CATEGORY_ORDER: [&str; 27] = [
        "Gcc",
        "Test",
        "GitDiff",
        "DockerK8s",
        "Node",
        "Web",
        "Java",
        "SpringBoot",
        "Maven",
        "PhpRuby",
        "Dotnet",
        "Helm",
        "Terraform",
        "WebLog",
        "PythonTraceback",
        "Bazel",
        "Gradle",
        "Xcode",
        "Ansible",
        "Pulumi",
        "CloudFormation",
        "Sql",
        "DbLog",
        "UnityUnreal",
        "Syslog",
        "CiLog",
        "CloudLog",
    ];
    for variant in CATEGORY_ORDER {
        let Some(counts) = per_cat_counts.get(variant) else {
            continue;
        };
        // 收集该类别候选词：仅采纳在类别内占绝对多数的词。
        let mut candidates: Vec<(String, f64)> = Vec::new();
        for (word, count) in counts {
            if *count < MIN_COUNT {
                continue;
            }
            let total = *total_by_word.get(word.as_str()).unwrap_or(&0) as f64;
            let share = if total > 0.0 {
                *count as f64 / total
            } else {
                1.0
            };
            if share < SHARE_THRESHOLD {
                continue;
            }
            // 原始权重 = log2(count+1) * share，夹取后按衰减系数折半，作为补充证据。
            let raw = (*count as f64 + 1.0).log2() * share;
            let w = (raw.clamp(WEIGHT_MIN, WEIGHT_MAX) * WEIGHT_DECAY).max(WEIGHT_FLOOR);
            candidates.push((word.clone(), w));
        }
        // 只保留权重最高的前 N 个，保证输出确定且有界。
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(MAX_WORDS_PER_CATEGORY);
        for (word, w) in candidates {
            result.push((word, variant, w));
        }
    }

    // 4) 按（类别, 词）排序，保证生成文件内容确定性、可复现。
    result.sort_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(&b.0)));

    Ok(result)
}

/// 将聚合结果写入 `OUT_DIR/features_generated.rs`（可用版本）。
fn write_feature_rows(out_dir: &Path, rows: Vec<(String, &str, f64)>) {
    let path = out_dir.join("features_generated.rs");
    let mut body = String::new();
    body.push_str("// auto-generated by build.rs feature_builder (计划 T-B)。手工改动会被覆盖。\n");
    body.push_str("// 来源：扫描 samples/ 各类别纯样样板 .log 语料做分词 + 跨类别可区分性加权。\n");
    body.push_str("pub(crate) const CORPUS_FEATURES_AVAILABLE: bool = true;\n");
    body.push_str(
        "#[allow(clippy::type_complexity)]\n\
         pub(crate) fn corpus_generated_features() -> Vec<(crate::core::content_classifier::model::Category, &'static str, f64)> {\n",
    );
    body.push_str("\tvec![\n");
    for (word, variant, weight) in &rows {
        // {:?} 输出合法 Rust 字符串字面量，避免转义隐患（词为字母数字，本不致歧义，此处保险起见）。
        body.push_str(&format!(
            "\t\t(crate::core::content_classifier::model::Category::{variant}, {word:?}, {weight:.2}),\n"
        ));
    }
    body.push_str("\t]\n}\n");
    if let Err(e) = fs::write(&path, body) {
        eprintln!(
            "[feature_builder] 写入 {} 失败（忽略，使用纯种子特征）：{e}",
            path.display()
        );
    }
}

/// 生成「不可用」桩：`CORPUS_FEATURES_AVAILABLE = false`，空特征表。
fn emit_stub(reason: &str) {
    let out_dir = match env::var("OUT_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => return,
    };
    let path = out_dir.join("features_generated.rs");
    let body = format!(
        "// auto-generated by build.rs feature_builder (计划 T-B)，当前为不可用桩。\n\
         // 原因：{reason}\n\
         pub(crate) const CORPUS_FEATURES_AVAILABLE: bool = false;\n\
         #[allow(clippy::type_complexity, unused)]\n\
         pub(crate) fn corpus_generated_features() -> Vec<(crate::core::content_classifier::model::Category, &'static str, f64)> {{ vec![] }}\n",
    );
    if let Err(e) = fs::write(&path, body) {
        eprintln!("[feature_builder] 写入降级桩失败（忽略）：{e}");
    }
}

/// 判断是否为 scenario 元数据 / README 等非命令输出文件。
fn is_metadata_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.contains(".scenario.") || name.eq_ignore_ascii_case("README.md")
}

// 分词与噪声过滤与运行期分类器共用同一实现（唯一权威见 corpus_tokens.rs）。
// 本文件是独立编译单元，无法引 crate，故用 include! 原样内联共享模块，
// 保证 build.rs 与运行期 feature_reader 的分词/滤噪逻辑永远一致，杜绝双份定义漂移。
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/core/content_classifier/corpus_tokens.rs"
));
