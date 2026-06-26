//! content analyzer 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 content analyzer 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、trait 等，用于表示该模块的数据结构和配置信息。

use crate::core::stream_reader::FileMetadata;
use crate::core::text_slicer::SliceType;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;

/// 脚本/文字系统枚举（用于多语言检测）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Script {
    Latin,      // 拉丁文（英文、西班牙文、法文、德文等）
    Chinese,    // 中文（简体/繁体）
    Japanese,   // 日文（平假名、片假名）
    Korean,     // 韩文
    Cyrillic,   // 西里尔文（俄文、乌克兰文等）
    Arabic,     // 阿拉伯文
    Hebrew,     // 希伯来文
    Greek,      // 希腊文
    Thai,       // 泰文
    Devanagari, // 梵文/印地文
    Hangul,     // 韩文字母
    Hiragana,   // 日文平假名
    Katakana,   // 日文片假名
    Hanja,      // 汉字（繁体中文、日文汉字）
    Unknown,
}

impl Script {
    pub fn family(&self) -> ScriptFamily {
        match self {
            Script::Latin => ScriptFamily::Latin,
            Script::Chinese | Script::Hanja => ScriptFamily::Cjk,
            Script::Japanese | Script::Hiragana | Script::Katakana => ScriptFamily::Cjk,
            Script::Korean | Script::Hangul => ScriptFamily::Cjk,
            Script::Cyrillic => ScriptFamily::Cyrillic,
            Script::Arabic => ScriptFamily::Arabic,
            Script::Hebrew => ScriptFamily::Hebrew,
            Script::Greek => ScriptFamily::Greek,
            Script::Thai => ScriptFamily::Thai,
            Script::Devanagari => ScriptFamily::Indic,
            Script::Unknown => ScriptFamily::Unknown,
        }
    }
}

/// 文字系统家族
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScriptFamily {
    Latin,    // 拉丁字母家族
    Cjk,      // 中日韩统一表意文字
    Cyrillic, // 西里尔字母家族
    Arabic,   // 阿拉伯字母家族
    Hebrew,   // 希伯来字母家族
    Greek,    // 希腊字母家族
    Thai,     // 泰文字母家族
    Indic,    // 印度文字家族
    Unknown,
}

/// 脚本检测结果
#[derive(Debug, Clone)]
pub struct ScriptDetectionResult {
    pub primary_script: Script,
    pub secondary_scripts: Vec<Script>,
    #[allow(dead_code)]
    pub script_families: Vec<ScriptFamily>,
    pub confidence: f32,
    pub mixed: bool, // 是否混合多种文字
}

/// 快速分析结果（用于第一级识别）
#[derive(Debug, Clone)]
pub struct QuickAnalysisResult {
    pub detected_type: SliceType,
    pub confidence: f32,
    pub file_metadata: Option<FileMetadata>,
}

/// 分析结果
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub slice_type: SliceType,
    pub confidence: f32,
    pub details: Option<String>,
}

/// 词表条目
#[derive(Debug, Clone)]
pub struct VocabularyEntry {
    pub term: String,
    pub category: String, // 如："path", "macro", "package", "location"
    pub frequency: u32,
}

/// 词表（用于快速识别名词、地名、路径、宏定义等）
#[derive(Debug, Clone)]
pub struct Vocabulary {
    pub entries: HashMap<String, VocabularyEntry>,
    pub patterns: HashMap<String, Arc<Regex>>, // 预编译的正则模式
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self::new()
    }
}

impl Vocabulary {
    pub fn new() -> Self {
        let mut vocab = Vocabulary {
            entries: HashMap::new(),
            patterns: HashMap::new(),
        };

        // 初始化常见模式
        vocab.add_pattern("windows_path", r"^[A-Za-z]:\\[^\\]*");
        vocab.add_pattern("unix_path", r"^/[^/]+/");
        vocab.add_pattern("java_package", r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+");
        vocab.add_pattern("maven_coord", r"^[a-z][a-z0-9_]*:[a-z][a-z0-9_-]*:[0-9]");
        vocab.add_pattern("macro_ref", r"\$[A-Z_][A-Z0-9_]*");
        vocab.add_pattern("gradle_path", r"^:[a-z][a-z0-9_-]*(:[a-z][a-z0-9_-]*)*");

        vocab
    }

    pub fn add_pattern(&mut self, name: &str, pattern: &str) {
        if let Ok(regex) = Regex::new(pattern) {
            self.patterns.insert(name.to_string(), Arc::new(regex));
        }
    }

    pub fn add_entry(&mut self, term: String, category: String) {
        let entry = VocabularyEntry {
            term: term.clone(),
            category,
            frequency: 1,
        };
        self.entries.insert(term, entry);
    }

    /// 识别文本中的词表项
    pub fn recognize(&self, text: &str) -> Vec<&VocabularyEntry> {
        let mut results = Vec::new();

        // 使用模式匹配
        for (_name, pattern) in &self.patterns {
            if pattern.is_match(text) {
                if let Some(entry) = self.entries.get(text) {
                    results.push(entry);
                }
            }
        }

        results
    }
}

/// 单个规则的定义
#[derive(Clone)]
pub struct Rule {
    pub name: String,
    pub pattern: Arc<Regex>,
    pub type_on_match: SliceType,
    pub weight: f32,
}

/// 内容分析器配置
#[derive(Clone)]
pub struct AnalyzerConfig {
    pub enable_rules: bool,
    pub enable_vocabulary: bool,       // 启用词表识别
    pub enable_script_detection: bool, // 启用脚本/多语言检测
    pub rules: Vec<Rule>,
    pub fallback_type: SliceType,
    pub confidence_threshold: f32,
    pub vocabulary: Option<Vocabulary>, // 词表
}

/// 内容分析器错误类型
#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error("E_ANALYZER_REGEX:{0}")]
    Regex(#[from] regex::Error),
    #[error("E_ANALYZER_INVALID_CONFIG")]
    InvalidConfig,
}

/// 内容分析器主结构
pub struct ContentAnalyzer {
    pub(crate) config: AnalyzerConfig,
}
impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            rules: default_rules(),
            fallback_type: crate::core::text_slicer::SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_rules: true,
            enable_vocabulary: true,
            enable_script_detection: true,
            vocabulary: Some(Vocabulary::new()),
        }
    }
}

fn default_rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "stack_trace_python".to_string(),
            pattern: Arc::new(Regex::new(r"(?m)^Traceback \(most recent call last\):").unwrap()),
            type_on_match: SliceType::StackTrace,
            weight: 0.95,
        },
        Rule {
            name: "stack_trace_java".to_string(),
            pattern: Arc::new(
                Regex::new(r"(?m)^\s*at\s+[A-Za-z0-9_$.]+\([A-Za-z0-9_$.]+:\d+\)").unwrap(),
            ),
            type_on_match: SliceType::StackTrace,
            weight: 0.9,
        },
        Rule {
            name: "stack_trace_node".to_string(),
            pattern: Arc::new(Regex::new(r"(?m)^\s*at\s+.+\(.+:\d+:\d+\)").unwrap()),
            type_on_match: SliceType::StackTrace,
            weight: 0.85,
        },
        Rule {
            name: "json_object_or_array".to_string(),
            pattern: Arc::new(Regex::new(r"(?s)^\s*(\{.*\}|\[.*\])\s*$").unwrap()),
            type_on_match: SliceType::JsonBlock,
            weight: 0.85,
        },
        Rule {
            name: "xml_or_html_tag".to_string(),
            pattern: Arc::new(Regex::new(r"(?s)<[A-Za-z][^>]*>.*</[A-Za-z][^>]*>|<\?xml").unwrap()),
            type_on_match: SliceType::XmlBlock,
            weight: 0.8,
        },
        Rule {
            name: "log_header_timestamp".to_string(),
            pattern: Arc::new(
                Regex::new(r"(?m)^\[?\d{4}[-/]\d{2}[-/]\d{2}[T\s]\d{2}:\d{2}:\d{2}").unwrap(),
            ),
            type_on_match: SliceType::LogBlock,
            weight: 0.75,
        },
        Rule {
            name: "code_keywords".to_string(),
            pattern: Arc::new(
                Regex::new(r"\b(fn|class|def|function|public|private|return|if|for|while)\b")
                    .unwrap(),
            ),
            type_on_match: SliceType::CodeBlock,
            weight: 0.6,
        },
        Rule {
            name: "sql_keywords".to_string(),
            pattern: Arc::new(
                Regex::new(r"(?i)\b(select|insert|update|delete|from|where|join)\b").unwrap(),
            ),
            type_on_match: SliceType::SqlBlock,
            weight: 0.7,
        },
    ]
}
