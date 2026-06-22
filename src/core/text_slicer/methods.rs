use super::config_loader::ConfigLoader;
use super::types::*;
use crate::core::observability::{log_progress, ScopeProbe};
use crate::core::stream_reader::SliceInput;
use crate::utils::i18n::t1;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder};

const MAX_PARAGRAPH_LINES: usize = 2_000;
const MAX_PARAGRAPH_BYTES: usize = 128 * 1024;
const E_TEXT_SLICER_AC_TRIGGER_BUILD: &str = "E_TEXT_SLICER_AC_TRIGGER_BUILD";
const E_TEXT_SLICER_AC_HTML_ENTITY_BUILD: &str = "E_TEXT_SLICER_AC_HTML_ENTITY_BUILD";
const E_TEXT_SLICER_AC_XML_PATTERN_BUILD: &str = "E_TEXT_SLICER_AC_XML_PATTERN_BUILD";
const E_TEXT_SLICER_REGEX_MACRO_COMPILE: &str = "E_TEXT_SLICER_REGEX_MACRO_COMPILE";
const E_TEXT_SLICER_REGEX_COMPILE_CMD_COMPILE: &str = "E_TEXT_SLICER_REGEX_COMPILE_CMD_COMPILE";
const E_TEXT_SLICER_REGEX_STACK_TRACE_COMPILE: &str = "E_TEXT_SLICER_REGEX_STACK_TRACE_COMPILE";
const E_TEXT_SLICER_REGEX_LOG_HEADER_COMPILE: &str = "E_TEXT_SLICER_REGEX_LOG_HEADER_COMPILE";

// ========== Aho-Corasick 快速探测器 ==========

/// 基础特殊内容探测器（用于快速判断是否需要启用正则分析）
static SPECIAL_TRIGGER_AC: Lazy<AhoCorasick> = Lazy::new(|| {
    let patterns = vec!["/", "-", "[", ":", "at ", "in ", "T", "Z"];
    AhoCorasickBuilder::new()
        .build(patterns)
        .expect(E_TEXT_SLICER_AC_TRIGGER_BUILD)
});

/// HTML 实体探测器
static HTML_ENTITY_AC: Lazy<AhoCorasick> = Lazy::new(|| {
    let patterns = vec![
        "&nbsp;", "&lt;", "&gt;", "&amp;", "&quot;", "&apos;", "&copy;", "&reg;", "&trade;",
        "&mdash;", "&ndash;", "&lsquo;", "&rsquo;", "&ldquo;", "&rdquo;", "&hellip;", "&bull;",
        "&middot;", "&#", "&#x", "&#X",
    ];
    AhoCorasickBuilder::new()
        .build(patterns)
        .expect(E_TEXT_SLICER_AC_HTML_ENTITY_BUILD)
});

/// XML/MyBatis 模式探测器
static XML_PATTERN_AC: Lazy<AhoCorasick> = Lazy::new(|| {
    let patterns = vec!["#{", "${", "/>", "=\"${", "='${", "=\"#{", "='#{"];
    AhoCorasickBuilder::new()
        .build(patterns)
        .expect(E_TEXT_SLICER_AC_XML_PATTERN_BUILD)
});

// ========== 特殊内容检测正则 ==========

/// 宏检测正则（编译参数）
static MACRO_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(-[DIULfWom][\w\.\-\+=]+|-std=\w+|-O[0-9z]+|-Wall|-Wextra|-fPIC|-fomit-frame-pointer|-pipe)")
        .expect(E_TEXT_SLICER_REGEX_MACRO_COMPILE)
});

/// 编译命令检测正则
static COMPILE_CMD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:\[[\d\-:TZ]+\]\s*)?([\w\-\./]+(?:gcc|g\+\+|clang|ld)(?:\s*[\w\-\./]+)*)\s+.*?\s+-c\s+.*?\s+-o\s+(\S+)")
        .expect(E_TEXT_SLICER_REGEX_COMPILE_CMD_COMPILE)
});

/// 堆栈跟踪检测正则
static STACK_TRACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:at|in)\s+[\w:\(\)\[\]<>]+(?:\s+\(.*\))?")
        .expect(E_TEXT_SLICER_REGEX_STACK_TRACE_COMPILE)
});

/// 日志头检测正则
static LOG_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\[(?P<ts>\d{4}-\d{2}-\d{2}T[\w:\.]+Z)\]\s*")
        .expect(E_TEXT_SLICER_REGEX_LOG_HEADER_COMPILE)
});

/// 检测文本中的特殊内容并添加到字典管理器
fn detect_and_add_special_content(
    text: &str,
    dict_manager: &Option<std::sync::Arc<crate::core::dictionary_manager::DictionaryManager>>,
) -> SliceFlags {
    let mut flags = SliceFlags::default();

    // 使用 Aho-Corasick 进行单次扫描快速判断
    if !SPECIAL_TRIGGER_AC.is_match(text) {
        // 如果连最基本的特征字符都没有，且不符合 Windows 路径特征，直接跳过
        if !(text.len() > 3 && text.as_bytes().get(1).copied() == Some(b':')) {
            return flags;
        }
    }

    if let Some(manager) = dict_manager {
        // Path extraction heuristic check
        if text.contains('/') || (text.len() > 3 && text.as_bytes().get(1).copied() == Some(b':')) {
            flags.has_paths = true;
        }

        if text.contains('-') {
            if MACRO_RE.is_match(text) {
                flags.has_macros = true;
                let macros: Vec<String> = MACRO_RE
                    .find_iter(text)
                    .map(|m| m.as_str().to_string())
                    .collect();
                if !macros.is_empty() {
                    manager.add_macros(macros);
                }
            }
            if COMPILE_CMD_RE.is_match(text) {
                flags.has_compile_commands = true;
                let commands: Vec<String> = COMPILE_CMD_RE
                    .find_iter(text)
                    .map(|m| m.as_str().to_string())
                    .collect();
                if !commands.is_empty() {
                    manager.add_compile_commands(commands);
                }
            }
        }

        if text.contains("at ") || text.contains("in ") {
            if STACK_TRACE_RE.is_match(text) {
                flags.has_stack_trace = true;
            }
        }

        if text.contains('[') && text.contains('T') && text.contains('Z') {
            if LOG_HEADER_RE.is_match(text) {
                flags.has_log_headers = true;
            }
        }
    } else {
        // Fast path: heuristic check for path-like content (used by SmartPathPlugin.detect)
        flags.has_paths =
            text.contains('/') || (text.len() > 3 && text.as_bytes().get(1).copied() == Some(b':'));
    }

    flags
}

// ========== 框架检测配置数据 ==========

/// 全局配置缓存（懒加载）
static FRAMEWORK_CONFIGS: Lazy<Vec<FrameworkDetectionConfig>> = Lazy::new(|| {
    let loader = ConfigLoader::new();
    match loader.load_all_configs() {
        Ok(configs) => {
            log::info!(
                "{}",
                t1("core_framework_config_summary_loaded", configs.len())
            );
            configs
        }
        Err(e) => {
            log::warn!("{}", t1("core_framework_config_summary_failed", e));
            Vec::new()
        }
    }
});

/// 获取所有框架的检测配置
pub fn get_framework_configs() -> &'static [FrameworkDetectionConfig] {
    &FRAMEWORK_CONFIGS
}

/// 通用框架检测函数
fn detect_framework_syntax(text: &str, config: &FrameworkDetectionConfig) -> bool {
    fn match_pattern(text: &str, pattern: &FeaturePattern) -> bool {
        match pattern.pattern_type {
            FeaturePatternType::Contains => text.contains(&pattern.pattern),
            FeaturePatternType::StartsWith => text.trim_start().starts_with(&pattern.pattern),
            FeaturePatternType::EndsWith => text.trim_end().ends_with(&pattern.pattern),
            FeaturePatternType::PrefixChar => {
                text.contains(&pattern.pattern) && {
                    let parts: Vec<&str> = text.split(&pattern.pattern).collect();
                    parts.len() > 1 && parts[1].chars().next().map_or(false, |c| c.is_alphabetic())
                }
            }
            FeaturePatternType::PairedDelimiters => {
                text.contains(&pattern.pattern)
                    && pattern
                        .end_delimiter
                        .as_ref()
                        .map_or(true, |end| text.contains(end))
            }
            FeaturePatternType::Regex => false,
        }
    }

    fn match_rule(text: &str, rule: &DetectionRule) -> bool {
        match rule {
            DetectionRule::Any(patterns) => patterns.iter().any(|p| match_pattern(text, p)),
            DetectionRule::All(patterns) => patterns.iter().all(|p| match_pattern(text, p)),
            DetectionRule::Combo { required, optional } => {
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

impl TextSlicer {
    // ========== MVP 函数 ==========

    /// 创建一个新的 TextSlicer 实例。
    ///
    /// # 参数
    /// - `config`: 切片器配置。
    pub fn new(config: SlicerConfig) -> Self {
        let _probe = ScopeProbe::new("text_slicer", "new");
        TextSlicer {
            config,
            next_id: AtomicU64::new(1),
            paragraph_buffer: String::with_capacity(4096),
            paragraph_line_count: 0,
            paragraph_buffer_bytes: 0,
            paragraph_start_line: None,
            paragraph_start_offset: None,
            current_metadata: None,
            dict_manager: None,
        }
    }

    /// 创建一个带有字典管理器的 TextSlicer 实例。
    /// 字典管理器用于在切片过程中自动提取并注册特殊内容（路径、宏、命令等）。
    pub fn with_dict_manager(
        config: SlicerConfig,
        dict_manager: std::sync::Arc<crate::core::dictionary_manager::DictionaryManager>,
    ) -> Self {
        let _probe = ScopeProbe::new("text_slicer", "with_dict_manager");
        TextSlicer {
            config,
            next_id: AtomicU64::new(1),
            paragraph_buffer: String::with_capacity(4096),
            paragraph_line_count: 0,
            paragraph_buffer_bytes: 0,
            paragraph_start_line: None,
            paragraph_start_offset: None,
            current_metadata: None,
            dict_manager: Some(dict_manager),
        }
    }

    /// 并行处理文本切片。
    /// 这里的“并行”主要发生在多个 SliceInput 之间。
    ///
    /// # 参数
    /// - `inputs`: 输入切片列表。
    ///
    /// # 返回值
    /// - `Vec<Slice>`: 处理后的切片列表。
    pub fn process_parallel<'a>(&self, inputs: Vec<SliceInput<'a>>) -> Vec<Slice<'a>> {
        use rayon::prelude::*;

        let input_count = inputs.len();
        let mut probe =
            ScopeProbe::new("text_slicer", "process_parallel").with_warn_threshold_ms(500);
        probe.add_field("inputs", input_count);
        probe.add_field("mode", format!("{:?}", self.config.mode));
        log_progress(
            "text_slicer",
            "process_parallel.start",
            input_count,
            &format!("mode={:?}", self.config.mode),
        );

        // 为每个线程创建一个新的TextSlicer实例，避免状态竞争
        let outputs = inputs
            .into_par_iter()
            .map(|input| {
                // 创建一个新的TextSlicer实例，复制当前配置和字典管理器
                let mut slicer = TextSlicer::new(self.config.clone());
                if let Some(dict_manager) = &self.dict_manager {
                    slicer.dict_manager = Some(dict_manager.clone());
                }

                match self.config.mode {
                    SliceMode::Line => slicer.slice_line(&input),
                    SliceMode::Paragraph => slicer
                        .slice_paragraph(&input)
                        .unwrap_or_else(|| slicer.slice_line(&input)),
                    SliceMode::Hybrid => {
                        // 尝试按标签切片
                        if let Some(slice) = slicer.slice_by_tags(&input) {
                            slice
                        } else if let Some(slice) = slicer.slice_by_indent(&input) {
                            slice
                        } else {
                            slicer
                                .slice_paragraph(&input)
                                .unwrap_or_else(|| slicer.slice_line(&input))
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        log_progress(
            "text_slicer",
            "process_parallel.done",
            outputs.len(),
            "completed",
        );
        outputs
    }

    /// 按行切片。这是最基础的切片方法，每个输入对应一个 Slice。
    pub fn slice_line<'a>(&self, input: &SliceInput<'a>) -> Slice<'a> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let flags = detect_and_add_special_content(input.raw.as_ref(), &self.dict_manager);

        Slice {
            id,
            text: input.raw.clone(),
            slice_type: SliceType::Line,
            offset: input.offset,
            line_start: input.line_number,
            line_end: input.line_number,
            file_metadata: input.file_metadata,
            flags,
        }
    }

    /// 按段落切片。将连续的非空行聚合为一个段落 Slice。
    ///
    /// # 功能说明
    /// - 空行会触发缓冲区冲刷（flush），形成一个段落。
    /// - 达到最大行数或最大字节限制时也会触发冲刷。
    /// - 支持流式处理状态维护。
    pub fn slice_paragraph<'a>(&mut self, input: &SliceInput<'a>) -> Option<Slice<'a>> {
        let start_slice = std::time::Instant::now();
        let raw_text = input.raw.as_ref();

        if raw_text.trim().is_empty() {
            // 空行处理
            if !self.paragraph_buffer.is_empty() {
                // 缓冲区非空，输出段落
                let res = self.flush_paragraph_buffer(
                    input.line_number.saturating_sub(1),
                    input.file_metadata,
                );
                crate::core::observability::record_profile(
                    "slice_paragraph_flush_empty_time",
                    start_slice.elapsed().as_millis(),
                );
                res
            } else {
                // 缓冲区为空，直接输出空行
                let id = self.next_id.fetch_add(1, Ordering::SeqCst);
                let flags = detect_and_add_special_content(input.raw.as_ref(), &self.dict_manager);
                let res = Some(Slice {
                    id,
                    text: input.raw.clone(),
                    slice_type: SliceType::Line,
                    offset: input.offset,
                    line_start: input.line_number,
                    line_end: input.line_number,
                    file_metadata: input.file_metadata,
                    flags,
                });
                crate::core::observability::record_profile(
                    "slice_paragraph_empty_time",
                    start_slice.elapsed().as_millis(),
                );
                res
            }
        } else {
            // 非空行，添加到缓冲区
            if self.paragraph_buffer.is_empty() {
                // 开始新段落
                self.paragraph_start_line = Some(input.line_number);
                self.paragraph_start_offset = Some(input.offset);
            } else {
                self.paragraph_buffer.push('\n');
                self.paragraph_buffer_bytes = self.paragraph_buffer_bytes.saturating_add(1);
            }

            self.paragraph_buffer.push_str(raw_text);
            self.paragraph_line_count += 1;
            self.paragraph_buffer_bytes =
                self.paragraph_buffer_bytes.saturating_add(raw_text.len());

            if self.paragraph_line_count >= MAX_PARAGRAPH_LINES
                || self.paragraph_buffer_bytes >= MAX_PARAGRAPH_BYTES
            {
                let res = self.flush_paragraph_buffer(input.line_number, input.file_metadata);
                crate::core::observability::record_profile(
                    "slice_paragraph_flush_time",
                    start_slice.elapsed().as_millis(),
                );
                return res;
            }

            crate::core::observability::record_profile(
                "slice_paragraph_push_time",
                start_slice.elapsed().as_millis(),
            );
            None
        }
    }

    /// 强制冲刷所有缓冲区。在处理完所有输入行后调用，确保最后一段内容被输出。
    pub fn flush(&mut self) -> Vec<Slice<'static>> {
        let mut slices = Vec::new();

        // 处理未完成的段落
        if !self.paragraph_buffer.is_empty() {
            let line_start = self.paragraph_start_line.unwrap_or(0);
            let line_end = line_start + self.paragraph_line_count.saturating_sub(1);
            if let Some(slice) = self.flush_paragraph_buffer(line_end, None) {
                slices.push(slice);
            }
        }

        slices
    }

    /// 内部逻辑：冲刷段落缓冲区并重置统计信息。
    fn flush_paragraph_buffer<'a>(
        &mut self,
        line_end: usize,
        file_metadata: Option<&'a crate::core::stream_reader::FileMetadata>,
    ) -> Option<Slice<'a>> {
        if self.paragraph_buffer.is_empty() {
            return None;
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let paragraph_text =
            std::mem::replace(&mut self.paragraph_buffer, String::with_capacity(4096));

        let line_start = self.paragraph_start_line.unwrap_or(line_end);
        let offset = self.paragraph_start_offset.unwrap_or(0);
        let flags = detect_and_add_special_content(&paragraph_text, &self.dict_manager);

        self.paragraph_line_count = 0;
        self.paragraph_buffer_bytes = 0;
        self.paragraph_start_line = None;
        self.paragraph_start_offset = None;

        Some(Slice {
            id,
            text: Cow::Owned(paragraph_text),
            slice_type: SliceType::Paragraph,
            offset,
            line_start,
            line_end,
            file_metadata,
            flags,
        })
    }

    // ========== 未来函数（待实现） ==========

    /// 按缩进切片（用于代码块识别）
    ///
    /// # 功能说明
    /// 根据行的缩进级别判断是否属于代码块。
    /// 连续缩进的行会被识别为 CodeBlock。
    ///
    /// # 使用场景
    /// - Python 代码块识别
    /// - YAML/JSON 等缩进敏感格式
    /// - 日志中的堆栈跟踪
    ///
    /// # 返回值
    /// - Some(Slice): 识别为代码块
    /// - None: 不是代码块，继续其他处理
    pub fn slice_by_indent<'a>(&self, input: &SliceInput<'a>) -> Option<Slice<'a>> {
        let text = input.raw.as_ref();

        // 计算缩进级别（空格数）
        let indent_level = text.chars().take_while(|c| c.is_whitespace()).count();

        // 空行或无缩进，不处理
        if indent_level == 0 || text.trim().is_empty() {
            return None;
        }

        // 有缩进，识别为代码块
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let flags = detect_and_add_special_content(input.raw.as_ref(), &self.dict_manager);
        Some(Slice {
            id,
            text: input.raw.clone(),
            slice_type: SliceType::CodeBlock,
            offset: input.offset,
            line_start: input.line_number,
            line_end: input.line_number,
            file_metadata: input.file_metadata,
            flags,
        })
    }

    /// 按标签切片（支持 18 种类型）
    ///
    /// # 支持的类型
    /// ## 完整文档（3 种）
    /// - 完整 HTML 文档 (DOCTYPE, html 标签)
    /// - 完整 XML 文档 (xml 声明)
    /// - SVG 文件 (svg 标签)
    ///
    /// ## 模板和片段（15 种）
    /// - MyBatis Mapper XML (#{}, ${}, <select>, <insert> 等)
    /// - Spring Beans XML (<bean>, <context:*> 等)
    /// - Vue 单文件组件 (v-*, @, :, {{ }}, <script setup>)
    /// - Thymeleaf 模板 (th:*, [[${}]])
    /// - Freemarker 模板 (${ }, <#if>, <@macro>)
    /// - Handlebars 模板 ({{ }}, {{#if}}, {{each}})
    /// - Jinja2 模板 ({{ }}, {% %}, {# #})
    /// - ERB 模板 (<% %>, <%= %>)
    /// - JSX/TSX 组件 (className, onClick, <Component />, {expr})
    /// - HTML 片段 (不完整标签，HTML 实体)
    /// - XML 配置文件片段 (命名空间标签)
    /// - 模板引擎混合语法
    /// - 命名空间 XML (mybatis:, spring:, c: 等)
    /// - SQL 标签内容 (MyBatis, jOOQ)
    /// - HTML 实体内容 (&nbsp;, &lt;, &#160; 等)
    pub fn slice_by_tags<'a>(&self, input: &SliceInput<'a>) -> Option<Slice<'a>> {
        let text = input.raw.as_ref();
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return None;
        }

        // ========== 策略 1: 检测完整文档 ==========
        if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html") {
            return Some(self.create_tag_slice(input, SliceType::HtmlBlock));
        }

        if trimmed.starts_with("<?xml") {
            return Some(self.create_tag_slice(input, SliceType::XmlBlock));
        }

        if trimmed.starts_with("<svg") {
            return Some(self.create_tag_slice(input, SliceType::HtmlBlock));
        }

        // ========== 策略 2: 检测 XML 标签片段 ==========
        if Self::contains_xml_tag_pattern(trimmed) {
            return Some(self.create_tag_slice(input, SliceType::HtmlBlock));
        }

        // ========== 策略 3: 使用配置驱动的框架检测引擎 ==========
        let configs = get_framework_configs();
        for config in configs {
            if detect_framework_syntax(trimmed, config) {
                return Some(self.create_tag_slice(input, config.slice_type));
            }
        }

        // ========== 策略 4: 检测 HTML 实体 ==========
        if Self::contains_html_entities(trimmed) {
            return Some(self.create_tag_slice(input, SliceType::HtmlBlock));
        }

        if trimmed.starts_with("diff --git") {
            return Some(self.create_tag_slice(input, SliceType::GitDiffBlock));
        }

        None
    }

    /// 创建标签切片（辅助函数）
    fn create_tag_slice<'a>(&self, input: &SliceInput<'a>, slice_type: SliceType) -> Slice<'a> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let flags = detect_and_add_special_content(input.raw.as_ref(), &self.dict_manager);

        Slice {
            id,
            text: input.raw.clone(),
            slice_type,
            offset: input.offset,
            line_start: input.line_number,
            line_end: input.line_number,
            file_metadata: input.file_metadata,
            flags,
        }
    }

    /// 检测 XML 标签模式（用于不完整片段）
    fn contains_xml_tag_pattern(text: &str) -> bool {
        // 使用 Aho-Corasick 快速检测
        if XML_PATTERN_AC.is_match(text) {
            return true;
        }

        // 命名空间标签：<namespace:tag (需要稍微复杂的检查)
        if text.contains('<') && text.contains(':') {
            let parts: Vec<&str> = text.split(':').collect();
            if parts.len() >= 2 {
                let before_colon = parts[0];
                if before_colon.contains('<') {
                    return true;
                }
            }
        }

        false
    }

    /// 检测 HTML 实体
    fn contains_html_entities(text: &str) -> bool {
        // 使用 Aho-Corasick 快速检测已知模式
        HTML_ENTITY_AC.is_match(text)
    }

    /// 根据配置的切片模式，将输入行分发到对应的分片逻辑中。
    pub fn push_slices_by_mode<'a>(&mut self, input: &SliceInput<'a>, out: &mut Vec<Slice<'a>>) {
        let is_blank_line = input.raw.as_ref().trim().is_empty();
        let had_paragraph_buffer = !self.paragraph_buffer.is_empty();

        match self.config.mode {
            SliceMode::Line => out.push(self.slice_line(input)),
            SliceMode::Paragraph => {
                if let Some(slice) = self.slice_paragraph(input) {
                    if is_blank_line {
                        if had_paragraph_buffer {
                            // 空行触发段落冲刷：补回 2 个换行（段落尾 + 空行本身）。
                            out.push(slice);
                            if !self.config.skip_empty_lines {
                                out.push(self.slice_explicit_newlines(input, 2));
                            }
                        } else if !self.config.skip_empty_lines {
                            // 连续空行或前导空行：每个空行补回 1 个换行。
                            out.push(self.slice_explicit_newlines(input, 1));
                        }
                    } else {
                        out.push(slice);
                    }
                }
            }
            SliceMode::Hybrid => {
                if let Some(slice) = self.slice_by_tags(input) {
                    out.push(slice);
                } else if let Some(slice) = self.slice_by_indent(input) {
                    out.push(slice);
                } else if let Some(slice) = self.slice_paragraph(input) {
                    if is_blank_line {
                        if had_paragraph_buffer {
                            out.push(slice);
                            if !self.config.skip_empty_lines {
                                out.push(self.slice_explicit_newlines(input, 2));
                            }
                        } else if !self.config.skip_empty_lines {
                            out.push(self.slice_explicit_newlines(input, 1));
                        }
                    } else {
                        out.push(slice);
                    }
                }
            }
        }
    }

    fn slice_explicit_newlines<'a>(&self, input: &SliceInput<'a>, count: usize) -> Slice<'a> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let newline_count = count.max(1);

        Slice {
            id,
            text: Cow::Owned("\n".repeat(newline_count)),
            slice_type: SliceType::Line,
            offset: input.offset,
            line_start: input.line_number,
            line_end: input.line_number,
            file_metadata: input.file_metadata,
            flags: SliceFlags::default(),
        }
    }
}
