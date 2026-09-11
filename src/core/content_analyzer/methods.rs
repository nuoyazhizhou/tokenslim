//! content analyzer 方法实现
//!
//! # 方法概述
//!
//! 本模块实现了 content analyzer 模块的主要业务逻辑。
//! 包含所有公共 API 的实现，以及内部辅助函数。

use super::types::ContentAnalyzer;
use crate::core::text_slicer::Slice;
use crate::core::text_slicer::SliceType;

/// 配置类文本（TOML/INI）对应的结构化插件名，供 `candidate_plugins_for_slice` 前置命中。
const TOML_INI_PLUGIN: &str = "toml_ini";

/// 极廉价的行级「配置样」判定：统计前 `CONFIG_PROBE_LINES` 行里「段头 `[xxx]`」与
/// 「键值 `k=...` 」行的数量，满足「≥1 段头 且 ≥1 键值」或「键值行达下限」即视为配置类。
///
/// 与 `toml_ini_plugin` 的 `is_config_like` 语义一致（不依赖 toml 解析，纯行统计），保证
/// 「候选提升先于调度」和「插件 detect」两处判定一致，互不漂移。此处刻意轻量（无正则、零分配，
/// 每切片调用一次），仅用于把配置文本从 smart_path 兜底中提前认领出来；真正的归一化/压缩
/// 仍由 toml_ini 插件完成。
fn is_config_like_text(text: &str) -> bool {
    const CONFIG_PROBE_LINES: usize = 40;
    const MIN_KEYVAL: usize = 3;
    fn is_section_line(trimmed: &str) -> bool {
        // 段头：[ 开头、] 收尾，中括号内为 `字母/数字/_.-` 组合（可由尾注释 `#` 承接）
        let Some(body) = trimmed.strip_prefix('[') else {
            return false;
        };
        let Some((inner, after)) = body.split_once(']') else {
            return false;
        };
        let after_trim = after.trim_start();
        if !after_trim.is_empty() && !after_trim.starts_with('#') {
            return false;
        }
        !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
    }
    fn is_keyval_line(trimmed: &str) -> bool {
        // 键值行：以 `字母/数字/_.-` 开头，且其后紧邻 `=`（允许 `=` 前有空白）
        let mut chars = trimmed.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_alphanumeric() || first == '_' || first == '.' || first == '-') {
            return false;
        }
        trimmed.find('=').is_some_and(|i| {
            trimmed[..i].chars().all(|c| {
                c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' || c.is_whitespace()
            })
        })
    }
    let mut sections = 0usize;
    let mut keyvals = 0usize;
    for line in text.lines().take(CONFIG_PROBE_LINES) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if is_section_line(trimmed) {
            sections += 1;
        } else if is_keyval_line(trimmed) {
            keyvals += 1;
        }
    }
    (sections >= 1 && keyvals >= 1) || keyvals >= MIN_KEYVAL
}

/// 确定性摘要锚点：对「格式信号强、贝叶斯弱」的摘要文本做前置识别。
///
/// 复用 [`is_config_like_text`] 的「锚点前置」范式：在贝叶斯分类之前，用极廉价、
/// 无正则的行级判定直接认领高特异性的确定性摘要（如 dotnet/VSTest 测试运行头），
/// 避免这类短摘要落入贝叶斯弱判别（dotnet vs test 竞争）导致的漏认与误判。
///
/// `classify()` 本身保持纯贝叶斯、不含锚点（F-3 快照语义 QR 不变），锚点仅作为
/// 本分析器判定流程的前置加权层。
fn anchored_category(text: &str) -> Option<crate::core::content_classifier::Category> {
    if is_dotnet_anchor_text(text) {
        return Some(crate::core::content_classifier::Category::Dotnet);
    }
    // cargo 编译错误摘要：`error[E0xxx]` 是 Rust/Cargo 独有签名（gcc 为无编号 `error:`，
    // git_diff 无此格式），而这类错误行缺 Cargo 常见上下文词，贝叶斯会弱判到 gcc/git_diff。
    if is_cargo_error_anchor_text(text) {
        return Some(crate::core::content_classifier::Category::Cargo);
    }
    None
}

/// 行级 dotnet/VSTest 测试运行摘要签名探测（无正则、零分配，探测前 `PROBE_LINES` 行的行首）。
///
/// 高特异性白名单：`dotnet test <dll>`（dotnet CLI）与 `Test run for <...>.dll`
/// （VSTest 运行头）。二者在其他类别的输出（测试失败详情、编译输出）中几乎不出现，
/// 命中即视为 dotnet 测试运行摘要。
fn is_dotnet_anchor_text(text: &str) -> bool {
    const PROBE_LINES: usize = 10;
    for line in text.lines().take(PROBE_LINES) {
        let t = line.trim_start();
        if t.starts_with("dotnet test ") || (t.starts_with("Test run for ") && t.contains(".dll")) {
            return true;
        }
    }
    false
}

/// 行级 cargo/Rust 编译错误签名探测（无正则、零分配，探测前 `PROBE_LINES` 行的行首）。
///
/// 仅匹配 `error[E0xxx]: ...` 形式（Rust 编译器类型错误编码）。gcc/clang 使用无编号的
/// `error:`、git_diff 不会产生 `[E…]:`，故该签名对 Cargo 高特异，不会误伤他类。
fn is_cargo_error_anchor_text(text: &str) -> bool {
    const PROBE_LINES: usize = 12;
    for line in text.lines().take(PROBE_LINES) {
        let t = line.trim_start();
        if t.starts_with("error[E") && t.contains("]:") {
            return true;
        }
    }
    false
}

impl ContentAnalyzer {
    /// 创建一个新的 ContentAnalyzer 实例。
    ///
    /// 分析器为无状态结构（见 [`types::ContentAnalyzer`]），构造不携带配置、不会失败。
    pub fn new() -> Self {
        ContentAnalyzer
    }

    /// 贝叶斯分类器兜底：将 `Category` 映射为本分析器可用的 `SliceType` 与候选插件名。
    /// 置信度低于阈值或归入通用文本时返回 `None`（不采纳）。
    ///
    /// 阈值取 0.40：允许 cargo/gcc/test/git_diff 等具有明确语义特征的输出通过，
    /// 同时排除无信号的通用文本（其置信度在均匀分布下 ≈ 0.2）。
    #[tracing::instrument(level = "trace", skip_all)]
    fn bayesian_fallback(&self, text: &str) -> Option<(SliceType, &'static [&'static str], f32)> {
        const MIN_CONFIDENCE: f32 = 0.40;
        // 锚点前置：格式强摘要不依赖贝叶斯弱判别，命中直接认领对应类别与候选插件。
        if let Some(anchor) = anchored_category(text) {
            let slice_type = match anchor {
                crate::core::content_classifier::Category::Dotnet => SliceType::LogBlock,
                _ => SliceType::LogBlock, // 未来新增锚点类别时在此显式映射覆盖
            };
            return Some((slice_type, anchor.candidate_plugins(), 1.0));
        }
        let classifier = crate::core::content_classifier::classifier();
        let result = classifier.classify(text);
        if result.confidence < MIN_CONFIDENCE {
            return None;
        }
        // 仅对具有专用插件/结构的类别生效；GenericText 视为无信号，不覆盖检测结果。
        let (slice_type, candidate_plugins) = match result.category {
            crate::core::content_classifier::Category::Cargo => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Gcc => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Test => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::GitDiff => {
                (SliceType::GitDiffBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::DockerK8s => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Node => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Web => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Java => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::SpringBoot => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Maven => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::PhpRuby => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Dotnet => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Helm => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Terraform => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Golang => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::WebLog => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::PythonTraceback => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Bazel => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Gradle => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Xcode => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Ansible => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Pulumi => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::CloudFormation => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Sql => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::DbLog => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::UnityUnreal => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::Syslog => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::CiLog => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::CloudLog => {
                (SliceType::LogBlock, result.category.candidate_plugins())
            }
            crate::core::content_classifier::Category::GenericText => return None,
        };
        Some((slice_type, candidate_plugins, result.confidence))
    }

    /// 返回给定切片由贝叶斯分类器推断出的「建议候选插件名」。
    ///
    /// 复用 [`bayesian_fallback`](Self::bayesian_fallback) 的分类逻辑：仅当文本被
    /// 归入 cargo/gcc/test/git_diff 等具语义信号类别且置信度达标时返回对应候选插件，
    /// 否则返回空数组（交由插件调度走全量 detect）。用于在 dispatch 阶段提升
    /// 专用插件的优先级，避免 cargo/gcc 输出落到 generic_text / smart_path 兜底。
    ///
    /// **结构化前置**（4c 协同修复）：配置文件（TOML/INI）是「行结构信号强、贝叶斯弱信号」
    /// 的文本——贝叶斯大概率归 GenericText 返回空候选，导致调度阶段 toml_ini（detect 0.85）
    /// 输给置信更高的 smart_path（对路径 token 天然高分）而永不执行。这里在贝叶斯兜底前
    /// 先做一次极廉价的「配置样」行级判定；命中即返回 `toml_ini` 候选，使 dispatch 的
    /// 候选提升把它排到 smart_path 之前按结构压缩，而非仅做路径字典化兜底。
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn candidate_plugins_for_slice<'a>(&self, slice: &Slice<'a>) -> &'static [&'static str] {
        // 结构化前置：配置类文本优先交给 toml_ini（不依赖贝叶斯，纯行级结构判定）
        if is_config_like_text(slice.text.as_ref()) {
            return &[TOML_INI_PLUGIN];
        }
        match self.bayesian_fallback(slice.text.as_ref()) {
            Some((_, plugins, _)) => plugins,
            None => &[],
        }
    }

    /// 文档级语义识别（通用版）：对整段文本做一次贝叶斯分类，置信度达标且非
    /// 通用文本时返回该类别，否则返回 `None`。
    ///
    /// 与 [`document_skin`](Self::document_skin) 的区别：不限于剥皮类别（syslog/ci/cloud），
    /// 任意语义类别（cargo/gcc/test/…）都可作为文档级先验。供两层化管线对大输入路径
    /// 做「文档级定性 → 逐块定向」的 sticky 种子，让无皮文档也走「识别→定向」而非
    /// 纯段落竞争。阈值复用 [`bayesian_fallback`](Self::bayesian_fallback) 的
    /// `MIN_CONFIDENCE` 约定（0.40）。
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn document_category(
        &self,
        text: &str,
    ) -> Option<crate::core::content_classifier::Category> {
        const MIN_CONFIDENCE: f32 = 0.40;
        // 锚点前置：格式强摘要不依赖贝叶斯弱判别，作为文档级 sticky 种子直接定类。
        if let Some(anchor) = anchored_category(text) {
            return Some(anchor);
        }
        let classifier = crate::core::content_classifier::classifier();
        let result = classifier.classify(text);
        if result.confidence < MIN_CONFIDENCE {
            return None;
        }
        match result.category {
            crate::core::content_classifier::Category::GenericText => None,
            cat => Some(cat),
        }
    }

    /// 文档级皮识别：对整段文本做一次贝叶斯分类，命中剥皮类别（syslog/ci/cloud）
    /// 且置信度达标时返回该类别，否则返回 `None`。
    ///
    /// 供两层化管线在切片前先做「整文档定性」：有皮则交由对应剥皮插件剥离外壳，
    /// 无皮则回退现状分段路径。复用 [`document_category`](Self::document_category) 的
    /// 置信度门槛（决策：复用 `classify().confidence`，不单独设阈值），仅额外收窄到剥皮类别。
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn document_skin(&self, text: &str) -> Option<crate::core::content_classifier::Category> {
        let cat = self.document_category(text)?;
        match cat {
            crate::core::content_classifier::Category::Syslog
            | crate::core::content_classifier::Category::CiLog
            | crate::core::content_classifier::Category::CloudLog => Some(cat),
            _ => None,
        }
    }
}
