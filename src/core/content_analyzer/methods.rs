//! content analyzer 方法实现
//!
//! # 方法概述
//!
//! 本模块实现了 content analyzer 模块的主要业务逻辑。
//! 包含所有公共 API 的实现，以及内部辅助函数。

use super::types::*;
use crate::core::observability::{log_progress, ScopeProbe};
use crate::core::stream_reader::{CharsetEncoding, FileMetadata, SliceInput};
use crate::core::text_slicer::Slice;
use crate::core::text_slicer::SliceType;

impl ContentAnalyzer {
    /// 创建一个新的 ContentAnalyzer 实例。
    ///
    /// # 参数
    /// - `config`: 分析器配置（包含分类规则、置信度阈值等）。
    pub fn new(config: AnalyzerConfig) -> Result<Self, AnalyzerError> {
        let mut probe = ScopeProbe::new("content_analyzer", "new");
        probe.add_field("rule_count", config.rules.len());
        probe.add_field("enable_rules", config.enable_rules);

        // 验证配置：权重不能为负
        for rule in &config.rules {
            if rule.weight < 0.0 {
                return Err(AnalyzerError::InvalidConfig);
            }
        }

        Ok(ContentAnalyzer { config })
    }

    /// 快速识别（第一级）- 基于配置文件和词表
    ///
    /// # 功能说明
    /// 1. 使用文件扩展名快速匹配
    /// 2. 使用词表识别路径、包名、宏定义等
    /// 3. 使用配置文件中的规则进行模式匹配
    ///
    /// # 参数
    /// - `input`: 切片输入（包含元数据）
    ///
    /// # 返回值
    /// - QuickAnalysisResult: 快速识别结果
    pub fn quick_analyze<'a>(&self, input: &SliceInput<'a>) -> QuickAnalysisResult {
        let text = input.raw.as_ref();

        // 策略 1: 使用文件扩展名快速识别
        if let Some(metadata) = &input.file_metadata {
            if let Some(ext) = Self::extract_extension(metadata) {
                let detected_type = self.detect_by_extension(&ext);
                if detected_type != SliceType::Unknown {
                    return QuickAnalysisResult {
                        detected_type,
                        confidence: 0.9,
                        file_metadata: input.file_metadata.cloned(),
                    };
                }
            }
        }

        // 策略 2: 使用词表识别
        if self.config.enable_vocabulary {
            if let Some(vocab) = &self.config.vocabulary {
                let recognized = vocab.recognize(text.trim());
                if !recognized.is_empty() {
                    let detected_type = self.detect_by_vocabulary(&recognized);
                    if detected_type != SliceType::Unknown {
                        return QuickAnalysisResult {
                            detected_type,
                            confidence: 0.8,
                            file_metadata: input.file_metadata.cloned(),
                        };
                    }
                }
            }
        }

        // 策略 3: 使用配置文件规则识别
        let detected_type = self.detect_by_patterns(text);

        QuickAnalysisResult {
            detected_type,
            confidence: if detected_type != SliceType::Unknown {
                0.7
            } else {
                0.3
            },
            file_metadata: input.file_metadata.cloned(),
        }
    }

    /// 根据文件扩展名识别文本类型。
    fn detect_by_extension(&self, ext: &str) -> SliceType {
        match ext.to_lowercase().as_str() {
            ".java" | ".jsp" => SliceType::CodeBlock,
            ".groovy" | ".gradle" => SliceType::CodeBlock,
            ".kt" | ".kts" => SliceType::CodeBlock,
            ".py" | ".pyw" => SliceType::CodeBlock,
            ".js" | ".jsx" | ".ts" | ".tsx" => SliceType::CodeBlock,
            ".c" | ".cpp" | ".h" | ".hpp" => SliceType::CodeBlock,
            ".cs" | ".csx" => SliceType::CodeBlock,
            ".rs" => SliceType::CodeBlock,
            ".go" => SliceType::CodeBlock,
            ".sh" | ".bash" | ".zsh" => SliceType::CodeBlock,
            ".scala" | ".sbt" => SliceType::CodeBlock,
            ".yaml" | ".yml" => SliceType::CodeBlock,
            ".json" | ".jsonc" => SliceType::JsonBlock,
            ".xml" | ".config" => SliceType::XmlBlock,
            ".ini" | ".cfg" | ".conf" => SliceType::CodeBlock,
            ".properties" => SliceType::CodeBlock,
            ".toml" => SliceType::CodeBlock,
            ".vue" => SliceType::VueComponent,
            ".svelte" => SliceType::SvelteComponent,
            ".hbs" | ".handlebars" => SliceType::HandlebarsTemplate,
            ".erb" => SliceType::ERBTemplate,
            ".ftl" | ".ftl.html" => SliceType::FreemarkerTemplate,
            ".html" | ".htm" => SliceType::HtmlBlock,
            ".md" | ".markdown" => SliceType::Paragraph,
            ".txt" | ".log" => SliceType::LogBlock,
            _ => SliceType::Unknown,
        }
    }

    /// 辅助方法：从元数据中提取规范化的文件扩展名（带点）。
    fn extract_extension(metadata: &FileMetadata) -> Option<String> {
        metadata.path.as_ref().and_then(|p| {
            p.extension()
                .and_then(|ext| ext.to_str())
                .map(|s| format!(".{}", s))
        })
    }

    /// 根据词表匹配条目判断文本类型。
    fn detect_by_vocabulary(&self, entries: &[&VocabularyEntry]) -> SliceType {
        for entry in entries {
            match entry.category.as_str() {
                "java_package" | "maven_coord" => return SliceType::CodeBlock,
                "gradle_path" => return SliceType::CodeBlock,
                "macro_ref" => return SliceType::CodeBlock,
                "windows_path" | "unix_path" => return SliceType::CodeBlock,
                _ => {}
            }
        }
        SliceType::Unknown
    }

    /// 根据预定义的内容特征模式（Patterns）识别类型。使用全局缓存的框架配置。
    fn detect_by_patterns(&self, text: &str) -> SliceType {
        // Use the global cached configs instead of re-loading on every call
        for config in crate::core::text_slicer::get_framework_configs() {
            if self.match_config(text, config) {
                return config.slice_type;
            }
        }
        SliceType::Unknown
    }

    /// 内部逻辑：将文本与特定的特征配置进行匹配。
    fn match_config(
        &self,
        text: &str,
        config: &crate::core::text_slicer::FrameworkDetectionConfig,
    ) -> bool {
        fn match_pattern(text: &str, pattern: &crate::core::text_slicer::FeaturePattern) -> bool {
            match pattern.pattern_type {
                crate::core::text_slicer::FeaturePatternType::Contains => {
                    text.contains(&pattern.pattern)
                }
                crate::core::text_slicer::FeaturePatternType::StartsWith => {
                    text.trim_start().starts_with(&pattern.pattern)
                }
                crate::core::text_slicer::FeaturePatternType::EndsWith => {
                    text.trim_end().ends_with(&pattern.pattern)
                }
                crate::core::text_slicer::FeaturePatternType::PrefixChar => {
                    text.contains(&pattern.pattern) && {
                        let parts: Vec<&str> = text.split(&pattern.pattern).collect();
                        parts.len() > 1
                            && parts[1].chars().next().map_or(false, |c| c.is_alphabetic())
                    }
                }
                crate::core::text_slicer::FeaturePatternType::PairedDelimiters => {
                    text.contains(&pattern.pattern)
                        && pattern
                            .end_delimiter
                            .as_ref()
                            .map_or(true, |end| text.contains(end))
                }
                crate::core::text_slicer::FeaturePatternType::Regex => false,
            }
        }

        fn match_rule(text: &str, rule: &crate::core::text_slicer::DetectionRule) -> bool {
            match rule {
                crate::core::text_slicer::DetectionRule::Any(patterns) => {
                    patterns.iter().any(|p| match_pattern(text, p))
                }
                crate::core::text_slicer::DetectionRule::All(patterns) => {
                    patterns.iter().all(|p| match_pattern(text, p))
                }
                crate::core::text_slicer::DetectionRule::Combo { required, optional } => {
                    let required_match =
                        required.is_empty() || required.iter().all(|p| match_pattern(text, p));
                    let optional_match =
                        optional.is_empty() || optional.iter().any(|p| match_pattern(text, p));
                    required_match && optional_match
                }
            }
        }

        !config.rules.is_empty() && config.rules.iter().all(|rule| match_rule(text, rule))
    }

    /// 深度分析（第二级）- 对已切分的块进行详细分析
    pub fn analyze<'a>(&self, slice: &Slice<'a>) -> AnalysisResult {
        if !self.config.enable_rules {
            return AnalysisResult {
                slice_type: self.config.fallback_type,
                confidence: 0.0,
                details: Some("Rules disabled".to_string()),
            };
        }

        let mut type_scores: std::collections::HashMap<SliceType, f32> =
            std::collections::HashMap::new();

        // 1. 规则匹配分数
        for rule in &self.config.rules {
            if rule.pattern.is_match(slice.text.as_ref()) {
                *type_scores.entry(rule.type_on_match).or_insert(0.0) += rule.weight;
            }
        }

        // 2. 添加词表识别分数
        if self.config.enable_vocabulary {
            if let Some(vocab) = &self.config.vocabulary {
                let recognized = vocab.recognize(slice.text.as_ref());
                if !recognized.is_empty() {
                    *type_scores.entry(SliceType::CodeBlock).or_insert(0.0) += 0.5;
                }
            }
        }

        // 3. 添加脚本检测分数（多语言支持）
        if self.config.enable_script_detection {
            if let Some(metadata) = slice.file_metadata {
                let script_result = detect_scripts(&metadata.encoding, slice.text.as_ref());
                let text_len = slice.text.len();

                // 中日韩文字通常出现在代码中（变量名、字符串等）
                if matches!(
                    script_result.primary_script,
                    Script::Chinese
                        | Script::Japanese
                        | Script::Korean
                        | Script::Hiragana
                        | Script::Katakana
                        | Script::Hangul
                        | Script::Hanja
                ) {
                    // 根据文本长度调整权重：短文本权重更高
                    let weight = if text_len < 100 { 0.8 } else { 0.5 };
                    *type_scores.entry(SliceType::CodeBlock).or_insert(0.0) += weight;
                }

                // 混合脚本更可能是代码
                if script_result.mixed {
                    *type_scores.entry(SliceType::CodeBlock).or_insert(0.0) += 0.3;
                }
            }
        }

        // 5. 根据文本长度归一化分数
        let text_len = slice.text.len() as f32;
        let length_bonus = (text_len / 1000.0).min(1.0) * 0.2; // 最长文本多给 0.2 分

        let mut max_score = 0.0;
        let mut best_type = self.config.fallback_type;

        for (slice_type, score) in type_scores {
            let adjusted_score = score + length_bonus;
            if adjusted_score > max_score {
                max_score = adjusted_score;
                best_type = slice_type;
            }
        }

        let confidence = self.normalize_confidence(max_score);

        if confidence < self.config.confidence_threshold {
            AnalysisResult {
                slice_type: self.config.fallback_type,
                confidence: 0.0,
                details: Some(format!("Below threshold: {:.2}", confidence)),
            }
        } else {
            AnalysisResult {
                slice_type: best_type,
                confidence,
                details: Some(format!("Score: {:.2}", max_score)),
            }
        }
    }

    /// 将原始权重分数归一化为 0.0 到 1.0 之间的置信度。
    pub(crate) fn normalize_confidence(&self, raw_score: f32) -> f32 {
        raw_score.min(1.0).max(0.0)
    }

    /// 列出当前分析器中定义的所有识别规则。
    pub fn list_rules(&self) -> Vec<(&str, SliceType)> {
        log_progress(
            "content_analyzer",
            "list_rules",
            self.config.rules.len(),
            "listed",
        );
        self.config
            .rules
            .iter()
            .map(|r| (r.name.as_str(), r.type_on_match))
            .collect()
    }

    /// 动态更新分析器的识别规则。
    pub fn update_rules(&mut self, new_rules: Vec<Rule>) -> Result<(), AnalyzerError> {
        let mut probe = ScopeProbe::new("content_analyzer", "update_rules");
        probe.add_field("new_rule_count", new_rules.len());
        for rule in &new_rules {
            if rule.weight < 0.0 {
                return Err(AnalyzerError::InvalidConfig);
            }
        }
        self.config.rules = new_rules;
        log_progress(
            "content_analyzer",
            "update_rules.done",
            self.config.rules.len(),
            "updated",
        );
        Ok(())
    }
}

// ========== 代码特征检测函数 ==========

// ========== 脚本检测函数 ==========

/// 根据给定的字符集编码推断可能的文本脚本（Script）。
pub fn detect_scripts_by_encoding(encoding: &CharsetEncoding) -> Vec<Script> {
    match encoding {
        CharsetEncoding::Utf8 | CharsetEncoding::Utf16Le | CharsetEncoding::Utf16Be => {
            vec![
                Script::Latin,
                Script::Chinese,
                Script::Japanese,
                Script::Korean,
            ]
        }
        CharsetEncoding::Gb2312 | CharsetEncoding::Gbk => vec![Script::Chinese],
        CharsetEncoding::Big5 => vec![Script::Chinese, Script::Hanja],
        CharsetEncoding::ShiftJis => vec![Script::Japanese, Script::Katakana, Script::Hiragana],
        CharsetEncoding::EucKr | CharsetEncoding::Cp949 => vec![Script::Korean, Script::Hangul],
        CharsetEncoding::Windows1251 | CharsetEncoding::Koi8R | CharsetEncoding::Iso8859_5 => {
            vec![Script::Cyrillic]
        }
        CharsetEncoding::Windows1256 | CharsetEncoding::Iso8859_6 => vec![Script::Arabic],
        CharsetEncoding::Windows1255 | CharsetEncoding::Iso8859_8 => vec![Script::Hebrew],
        CharsetEncoding::Latin1 | CharsetEncoding::Windows1252 => vec![Script::Latin],
        CharsetEncoding::Utf32Le | CharsetEncoding::Utf32Be => {
            vec![
                Script::Latin,
                Script::Chinese,
                Script::Japanese,
                Script::Korean,
            ]
        }
        CharsetEncoding::Unknown => vec![Script::Unknown],
    }
}

/// 根据文本中字符的 Unicode 范围进行深度脚本检测。
pub fn detect_scripts_by_unicode(text: &str) -> ScriptDetectionResult {
    let mut script_counts: std::collections::HashMap<Script, usize> =
        std::collections::HashMap::new();

    for c in text.chars() {
        let script = match c {
            // Latin (ASCII + 西欧字符)
            '\u{0000}'..='\u{007F}' => Script::Latin,
            '\u{00C0}'..='\u{024F}' => Script::Latin, // Latin Extended
            // CJK Unified Ideographs
            '\u{4E00}'..='\u{9FFF}' => Script::Chinese,
            // CJK Extension A
            '\u{3400}'..='\u{4DBF}' => Script::Chinese,
            // CJK Extension B-E
            '\u{20000}'..='\u{2EBEF}' => Script::Chinese,
            // Hiragana (平假名)
            '\u{3040}'..='\u{309F}' => Script::Hiragana,
            // Katakana (片假名)
            '\u{30A0}'..='\u{30FF}' => Script::Katakana,
            // Katakana Extension
            '\u{31F0}'..='\u{31FF}' => Script::Katakana,
            // Hangul Jamo (韩文字母)
            '\u{1100}'..='\u{11FF}' => Script::Hangul,
            '\u{3130}'..='\u{318F}' => Script::Hangul,
            '\u{A960}'..='\u{A97F}' => Script::Hangul,
            // Hangul Syllables (完整的韩文字)
            '\u{AC00}'..='\u{D7AF}' => Script::Korean,
            // Hanja (繁体汉字)
            '\u{F900}'..='\u{FAFF}' => Script::Hanja,
            '\u{2F800}'..='\u{2FA1F}' => Script::Hanja,
            // Cyrillic (西里尔文)
            '\u{0400}'..='\u{04FF}' => Script::Cyrillic,
            '\u{0500}'..='\u{052F}' => Script::Cyrillic,
            '\u{2DE0}'..='\u{2DFF}' => Script::Cyrillic,
            '\u{A640}'..='\u{A69F}' => Script::Cyrillic,
            // Arabic (阿拉伯文)
            '\u{0600}'..='\u{06FF}' => Script::Arabic,
            '\u{0750}'..='\u{077F}' => Script::Arabic,
            '\u{08A0}'..='\u{08FF}' => Script::Arabic,
            '\u{FB50}'..='\u{FDFF}' => Script::Arabic,
            '\u{FE70}'..='\u{FEFF}' => Script::Arabic,
            // Hebrew (希伯来文)
            '\u{0590}'..='\u{05FF}' => Script::Hebrew,
            '\u{FB00}'..='\u{FB4F}' => Script::Hebrew,
            // Greek (希腊文)
            '\u{0370}'..='\u{03FF}' => Script::Greek,
            '\u{1F00}'..='\u{1FFF}' => Script::Greek,
            // Thai (泰文)
            '\u{0E00}'..='\u{0E7F}' => Script::Thai,
            // Devanagari (梵文/印地文)
            '\u{0900}'..='\u{097F}' => Script::Devanagari,
            '\u{A8E0}'..='\u{A8FF}' => Script::Devanagari,
            _ => Script::Latin, // 默认假设为拉丁文（ASCII 范围内外的其他字符）
        };

        *script_counts.entry(script).or_insert(0) += 1;
    }

    // 排序并获取主要脚本
    let mut sorted: Vec<_> = script_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let primary_script = sorted
        .first()
        .map(|(s, _)| s.clone())
        .unwrap_or(Script::Unknown);
    let secondary_scripts: Vec<_> = sorted.iter().skip(1).map(|(s, _)| s.clone()).collect();

    // 计算脚本家族
    let mut family_set: std::collections::HashSet<ScriptFamily> = std::collections::HashSet::new();
    for script in &secondary_scripts {
        family_set.insert(script.family());
    }
    if !primary_script.family().eq(&ScriptFamily::Unknown) {
        family_set.insert(primary_script.family());
    }
    let script_families: Vec<_> = family_set.into_iter().collect();

    // 判断是否混合
    let mixed =
        sorted.len() > 1 && sorted[0].1 < (sorted.iter().map(|(_, c)| c).sum::<usize>() / 2);

    // 计算置信度
    let total: usize = sorted.iter().map(|(_, c)| c).sum();
    let confidence = if total > 0 {
        sorted
            .first()
            .map(|(_, c)| *c as f32 / total as f32)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    ScriptDetectionResult {
        primary_script,
        secondary_scripts,
        script_families,
        confidence,
        mixed,
    }
}

/// 执行多维度脚本检测。结合了文件编码推断和文本 Unicode 扫描。
pub fn detect_scripts(encoding: &CharsetEncoding, text: &str) -> ScriptDetectionResult {
    // 首先尝试从文本内容检测
    let content_result = detect_scripts_by_unicode(text);

    // 如果文本检测置信度高，直接返回
    if content_result.confidence > 0.7 {
        return content_result;
    }

    // 否则结合编码信息
    let encoding_scripts = detect_scripts_by_encoding(encoding);

    // 合并结果
    let mut all_scripts: Vec<_> = encoding_scripts;
    for script in &content_result.secondary_scripts {
        if !all_scripts.contains(script) {
            all_scripts.push(script.clone());
        }
    }

    let primary_script = all_scripts.first().cloned().unwrap_or(Script::Unknown);

    let secondary_scripts: Vec<_> = all_scripts.into_iter().skip(1).collect();
    let primary_family = primary_script.family();

    ScriptDetectionResult {
        primary_script,
        secondary_scripts,
        script_families: vec![primary_family],
        confidence: 0.8,
        mixed: content_result.mixed,
    }
}
