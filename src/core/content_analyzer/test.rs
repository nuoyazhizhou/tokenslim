//! content analyzer 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 content analyzer 模块的单元测试和集成测试。
//! 测试覆盖了主要功能和边界情况。

#[cfg(test)]
mod tests {
    use crate::core::content_analyzer::{AnalyzerConfig, ContentAnalyzer, Rule};
    use crate::core::text_slicer::{Slice, SliceType};
    use std::borrow::Cow;

    #[test]
    fn test_new() {
        let rules = vec![];
        let config = AnalyzerConfig {
            enable_rules: true,
            rules,
            fallback_type: SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let analyzer = ContentAnalyzer::new(config).unwrap();
        assert!(analyzer.config.enable_rules);
        assert!(analyzer.config.rules.is_empty());
    }

    #[test]
    fn test_analyze_with_rules() {
        // 创建测试规则
        let rules = vec![
            Rule {
                name: "gcc_error".to_string(),
                pattern: std::sync::Arc::new(regex::Regex::new(r"error:").unwrap()),
                type_on_match: SliceType::LogBlock,
                weight: 0.8,
            },
            Rule {
                name: "gcc_warning".to_string(),
                pattern: std::sync::Arc::new(regex::Regex::new(r"warning:").unwrap()),
                type_on_match: SliceType::LogBlock,
                weight: 0.6,
            },
        ];

        let config = AnalyzerConfig {
            enable_rules: true,
            rules,
            fallback_type: SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let analyzer = ContentAnalyzer::new(config).unwrap();

        // 测试匹配规则的情况
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("error: undefined reference to `main'"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,

            flags: Default::default(),
        };

        let result = analyzer.analyze(&slice);
        assert_eq!(result.slice_type, SliceType::LogBlock);
        assert!(result.confidence >= 0.8);

        // 测试不匹配规则的情况
        let slice2 = Slice {
            id: 2,
            text: Cow::Borrowed("Hello world"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,

            flags: Default::default(),
        };

        let result2 = analyzer.analyze(&slice2);
        assert_eq!(result2.slice_type, SliceType::Unknown);
        assert_eq!(result2.confidence, 0.0);
    }

    #[test]
    fn test_analyze_below_threshold() {
        let rules = vec![Rule {
            name: "low_confidence".to_string(),
            pattern: std::sync::Arc::new(regex::Regex::new(r"test").unwrap()),
            type_on_match: SliceType::LogBlock,
            weight: 0.3,
        }];

        let config = AnalyzerConfig {
            enable_rules: true,
            rules,
            fallback_type: SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let analyzer = ContentAnalyzer::new(config).unwrap();

        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("This is a test"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,

            flags: Default::default(),
        };

        let result = analyzer.analyze(&slice);
        assert_eq!(result.slice_type, SliceType::Unknown);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_analyze_rules_disabled() {
        let rules = vec![Rule {
            name: "test_rule".to_string(),
            pattern: std::sync::Arc::new(regex::Regex::new(r"test").unwrap()),
            type_on_match: SliceType::LogBlock,
            weight: 0.8,
        }];

        let config = AnalyzerConfig {
            enable_rules: false,
            rules,
            fallback_type: SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let analyzer = ContentAnalyzer::new(config).unwrap();

        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("This is a test"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,

            flags: Default::default(),
        };

        let result = analyzer.analyze(&slice);
        assert_eq!(result.slice_type, SliceType::Unknown);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_normalize_confidence() {
        let rules = vec![];
        let config = AnalyzerConfig {
            enable_rules: true,
            rules,
            fallback_type: SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let analyzer = ContentAnalyzer::new(config).unwrap();

        // 测试正常范围
        assert_eq!(analyzer.normalize_confidence(0.5), 0.5);

        // 测试超出上限
        assert_eq!(analyzer.normalize_confidence(1.5), 1.0);

        // 测试低于下限
        assert_eq!(analyzer.normalize_confidence(-0.5), 0.0);
    }
}
