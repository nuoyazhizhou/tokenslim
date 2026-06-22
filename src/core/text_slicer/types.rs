//! text slicer 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 text slicer 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

use crate::core::stream_reader::FileMetadata;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// 切片唯一标识，全局递增
pub type SliceId = u64;

/// 切片类型（增强版 - 支持 20 种主流语言及其生态）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SliceType {
    Line,          // 单行
    Paragraph,     // 段落
    CodeBlock,     // 代码块
    HtmlBlock,     // HTML/XML 块（包含所有模板语言）
    JsonBlock,     // JSON 块
    XmlBlock,      // XML 块（与 HTML 区分）
    SqlBlock,      // SQL 块（包含 ORM 模板）
    StackTrace,    // 堆栈跟踪
    LogBlock,      // 日志块
    TemplateBlock, // 模板语言块（通用）
    // 特定框架类型
    VueComponent,     // Vue 组件
    ReactComponent,   // React/JSX 组件
    AngularComponent, // Angular 组件
    SvelteComponent,  // Svelte 组件
    // 模板类型
    JinjaTemplate,      // Jinja2/Nunjucks
    ThymeleafTemplate,  // Thymeleaf
    FreemarkerTemplate, // Freemarker
    ERBTemplate,        // ERB (Ruby)
    RazorTemplate,      // Razor (C#)
    BladeTemplate,      // Blade (PHP)
    HandlebarsTemplate, // Handlebars/Mustache
    // 其他
    Binary,       // 二进制（应被过滤）
    GitDiffBlock, // Git Diff 块
    Unknown,      // 未知
}

/// 切片标记，用于指示切片中包含的特殊内容
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceFlags {
    pub has_paths: bool,            // 是否包含路径
    pub has_macros: bool,           // 是否包含宏
    pub has_compile_commands: bool, // 是否包含编译命令
    pub has_stack_trace: bool,      // 是否包含堆栈跟踪
    pub has_log_headers: bool,      // 是否包含日志头
}

impl Default for SliceFlags {
    fn default() -> Self {
        Self {
            has_paths: false,
            has_macros: false,
            has_compile_commands: false,
            has_stack_trace: false,
            has_log_headers: false,
        }
    }
}

/// 切片输出结构，text 使用 Cow 以支持借用或拥有数据
#[derive(Debug, Clone)]
pub struct Slice<'a> {
    pub id: SliceId,
    pub text: Cow<'a, str>,
    pub slice_type: SliceType,
    pub offset: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub file_metadata: Option<&'a FileMetadata>,
    pub flags: SliceFlags, // 切片标记
}

/// 切片器配置（简化 MVP 版本）
#[derive(Clone)]
pub struct SlicerConfig {
    pub mode: SliceMode,
    pub skip_empty_lines: bool, // 是否跳过空行（不单独输出）
}

/// 切片模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceMode {
    Line,      // 只按行切片
    Paragraph, // 只按段落切片
    Hybrid,    // 混合模式（智能选择）
}

use std::sync::atomic::AtomicU64;

/// 切片器本身（可持有状态）
pub struct TextSlicer {
    pub(crate) config: SlicerConfig,
    pub(crate) next_id: AtomicU64,
    /// 段落缓冲区
    pub(crate) paragraph_buffer: String,
    pub(crate) paragraph_line_count: usize,
    pub(crate) paragraph_buffer_bytes: usize,
    pub(crate) paragraph_start_line: Option<usize>,
    pub(crate) paragraph_start_offset: Option<usize>,
    /// 当前文件元数据（用于 flush）
    #[allow(dead_code)]
    pub(crate) current_metadata: Option<&'static FileMetadata>,
    /// 字典管理器（可选）
    pub(crate) dict_manager:
        Option<std::sync::Arc<crate::core::dictionary_manager::DictionaryManager>>,
    // 其他策略的内部状态...
}

impl Default for SlicerConfig {
    fn default() -> Self {
        Self {
            mode: SliceMode::Paragraph, // 默认段落模式
            skip_empty_lines: true,     // 默认跳过空行
        }
    }
}

// ========== 框架检测配置 ==========

/// 特征模式类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeaturePatternType {
    /// 简单字符串包含
    Contains,
    /// 前缀匹配
    StartsWith,
    /// 后缀匹配
    EndsWith,
    /// 正则表达式匹配
    Regex,
    /// 特殊字符前缀检测（如 @, :, #）
    PrefixChar,
    /// 成对符号检测（如 {{ }}, {% %}）
    PairedDelimiters,
}

/// 特征模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturePattern {
    /// 模式类型
    pub pattern_type: FeaturePatternType,
    /// 模式字符串
    pub pattern: String,
    /// 配对的结束符（用于 PairedDelimiters）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_delimiter: Option<String>,
}

impl FeaturePattern {
    /// 创建简单包含模式
    pub fn contains(pattern: &str) -> Self {
        FeaturePattern {
            pattern_type: FeaturePatternType::Contains,
            pattern: pattern.to_string(),
            end_delimiter: None,
        }
    }

    /// 创建前缀匹配模式
    pub fn starts_with(pattern: &str) -> Self {
        FeaturePattern {
            pattern_type: FeaturePatternType::StartsWith,
            pattern: pattern.to_string(),
            end_delimiter: None,
        }
    }

    /// 创建成对分隔符模式
    pub fn paired(start: &str, end: &str) -> Self {
        FeaturePattern {
            pattern_type: FeaturePatternType::PairedDelimiters,
            pattern: start.to_string(),
            end_delimiter: Some(end.to_string()),
        }
    }

    /// 创建特殊字符前缀模式
    pub fn prefix_char(pattern: &str) -> Self {
        FeaturePattern {
            pattern_type: FeaturePatternType::PrefixChar,
            pattern: pattern.to_string(),
            end_delimiter: None,
        }
    }
}

/// 检测规则类型
#[derive(Debug, Clone)]
pub enum DetectionRule {
    /// 任意模式匹配（OR 逻辑）
    Any(Vec<FeaturePattern>),
    /// 所有模式必须匹配（AND 逻辑）
    All(Vec<FeaturePattern>),
    /// 组合规则
    Combo {
        /// 必须满足的规则
        required: Vec<FeaturePattern>,
        /// 可选满足的规则（满足其一即可）
        optional: Vec<FeaturePattern>,
    },
}

/// 框架检测配置
#[derive(Debug, Clone)]
pub struct FrameworkDetectionConfig {
    /// 框架名称
    pub name: &'static str,
    /// 对应的切片类型
    pub slice_type: SliceType,
    /// 检测规则
    pub rules: Vec<DetectionRule>,
    /// 优先级（数字越大优先级越高）
    pub priority: u8,
}

impl FrameworkDetectionConfig {
    /// 创建框架配置
    pub const fn new(
        name: &'static str,
        slice_type: SliceType,
        rules: Vec<DetectionRule>,
        priority: u8,
    ) -> Self {
        FrameworkDetectionConfig {
            name,
            slice_type,
            rules,
            priority,
        }
    }
}
