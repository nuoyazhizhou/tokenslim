use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

/// P3-155（P3-127 家族扩展）：`extract_size_kb` 每资产行与 `compress` 主循环内每行
/// 重建 hash 正则、`normalize` 重建 hash/time。提升为进程级 `OnceLock` 预编译
/// （对照 `infra_tools_common.rs` 范式）。
static SIZE_RE: OnceLock<Regex> = OnceLock::new();
static HASH_RE: OnceLock<Regex> = OnceLock::new();
static TIME_RE: OnceLock<Regex> = OnceLock::new();

impl WebpackVitePlugin {
    /// 创建插件实例：初始化名称(webpack_vite)与优先级(82)。
    pub fn new() -> Self {
        Self {
            name: "webpack_vite",
            priority: 82,
        }
    }

    /// 判断是否为噪音行（需要过滤）
    #[tracing::instrument(level = "debug", skip_all)]
    fn is_noise_line(line: &str) -> bool {
        let trimmed = line.trim();

        // 过滤 webpack 版本信息
        if trimmed.starts_with("Version: webpack") {
            return true;
        }

        // 过滤构建时间（但保留 "compiled successfully in" 这种有意义的）
        if trimmed.starts_with("Time:") && !trimmed.contains("compiled") {
            return true;
        }

        // 过滤 "Built at:" 时间戳
        if trimmed.starts_with("Built at:") {
            return true;
        }

        // 过滤 Hash 行（但保留在错误上下文中的）
        if trimmed.starts_with("Hash:") && trimmed.len() < 50 {
            return true;
        }

        false
    }

    /// 从行中提取文件大小（转换为 KiB）
    #[tracing::instrument(level = "debug", skip_all)]
    fn extract_size_kb(line: &str) -> Option<f64> {
        // 匹配 "125.4 KiB" 或 "1.2 MiB"
        let size_re = SIZE_RE.get_or_init(|| Regex::new(r"(\d+\.?\d*)\s*(KiB|MiB|kB|MB)").unwrap());
        if let Some(caps) = size_re.captures(line) {
            let value: f64 = caps.get(1)?.as_str().parse().ok()?;
            let unit = caps.get(2)?.as_str();

            match unit {
                "MiB" | "MB" => Some(value * 1024.0),
                "KiB" | "kB" => Some(value),
                _ => None,
            }
        } else {
            None
        }
    }
}

/// 错误分类器 - 用于统计和折叠重复错误/警告
#[derive(Debug)]
struct ErrorClassifier {
    errors: Vec<String>,
    warnings: Vec<String>,
    error_types: HashMap<String, usize>,
    warning_types: HashMap<String, usize>,
}

impl ErrorClassifier {
    /// 创建空的错误分类器：初始化错误/警告集合与类型计数表。
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            error_types: HashMap::new(),
            warning_types: HashMap::new(),
        }
    }

    /// 分类错误行
    #[tracing::instrument(level = "debug", skip_all)]
    fn classify_error(&mut self, line: &str) {
        if line.contains("ERROR in") || line.contains("Module parse failed") {
            self.errors.push(line.to_string());
            // 提取错误类型
            if let Some(error_type) = Self::extract_error_type(line) {
                *self.error_types.entry(error_type).or_insert(0) += 1;
            }
        }
    }

    /// 分类警告行
    #[tracing::instrument(level = "debug", skip_all)]
    fn classify_warning(&mut self, line: &str) {
        if line.contains("warning:") || line.contains("⚠️") {
            self.warnings.push(line.to_string());
            // 提取警告类型
            if let Some(warning_type) = Self::extract_warning_type(line) {
                *self.warning_types.entry(warning_type).or_insert(0) += 1;
            }
        }
    }

    /// 提取错误类型
    fn extract_error_type(line: &str) -> Option<String> {
        if line.contains("Module parse failed") {
            Some("Module parse failed".to_string())
        } else if line.contains("Cannot find module") {
            Some("Cannot find module".to_string())
        } else if line.contains("Module not found") {
            Some("Module not found".to_string())
        } else {
            None
        }
    }

    /// 提取警告类型
    fn extract_warning_type(line: &str) -> Option<String> {
        if line.contains("console.log") {
            Some("console.log statements".to_string())
        } else if line.contains("not defined") {
            Some("undefined variable".to_string())
        } else if line.contains("deprecated") {
            Some("deprecated API".to_string())
        } else {
            None
        }
    }

    /// 生成构建摘要
    fn generate_summary(&self) -> Option<String> {
        if self.errors.is_empty() && self.warnings.is_empty() {
            return None;
        }

        let mut summary = String::from("[BUILD_SUMMARY] ");
        if !self.errors.is_empty() {
            summary.push_str(&format!("{} errors", self.errors.len()));
        }
        if !self.warnings.is_empty() {
            if !self.errors.is_empty() {
                summary.push_str(", ");
            }
            summary.push_str(&format!("{} warnings", self.warnings.len()));
        }

        Some(summary)
    }
}

impl Plugin for WebpackVitePlugin {
    /// 返回插件名称标识 "webpack_vite"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级(82)，用于压缩调度排序。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测切片文本是否属于 Webpack/Vite 构建日志，按 Vite 特征、Webpack 特征或通用资产列表特征返回置信度 0.0~0.9。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();

        // 1. Vite 特征
        if text.contains("VITE v")
            || text.contains("ready in")
            || text.contains("computing gzip size")
        {
            return Some(0.9);
        }

        // 2. Webpack 特征
        if text.contains("Hash: ")
            || text.contains("Version: webpack")
            || text.contains("Child mini-css-extract-plugin")
        {
            return Some(0.9);
        }

        // 3. 通用资产列表特征
        if text.contains("dist/")
            && (text.contains(".js") || text.contains(".css") || text.contains(".map"))
        {
            return Some(0.7);
        }

        None
    }

    /// 压缩 Webpack/Vite 构建日志：过滤噪音行、折叠 HMR/警告/资产/模块族群、提取哈希入字典，并经 ROI 门控输出。
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 短样本快速路径（< 50 字节直接返回，避免扩张）
        if text.len() < 50 {
            return CompressResult {
                tokens: vec![Token::Text(Cow::Borrowed(text))],
                metadata: None,
                plugin_name: Some(self.name()),
            };
        }

        let mut result = String::with_capacity(text.len());
        let lines: Vec<&str> = text.lines().collect();
        let mut classifier = ErrorClassifier::new();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // 过滤噪音行（法则 E：零容忍废话）
            if Self::is_noise_line(line) {
                i += 1;
                continue;
            }

            // 分类错误和警告
            classifier.classify_error(line);
            classifier.classify_warning(line);

            // 折叠 HMR 更新（Vite 热模块替换）
            if line.contains("[vite] hmr update")
                || (line.contains("hmr update") && line.contains("/"))
            {
                let mut count = 1;

                // 跳过紧跟的 "✓ N module transformed" 行
                if i + 1 < lines.len()
                    && lines[i + 1].contains("module")
                    && lines[i + 1].contains("transformed")
                {
                    i += 1;
                }

                while i + count < lines.len() {
                    let next = lines[i + count];
                    if next.contains("[vite] hmr update")
                        || (next.contains("hmr update") && next.contains("/"))
                    {
                        count += 1;
                        // 跳过紧跟的 "✓ N module transformed" 行
                        if i + count < lines.len()
                            && lines[i + count].contains("module")
                            && lines[i + count].contains("transformed")
                        {
                            count += 1;
                        }
                        continue;
                    }
                    break;
                }

                if count >= 2 {
                    result.push_str(&format!("[HMR] {} modules updated\n", (count + 1) / 2));
                    i += count;
                    continue;
                }
            }

            // 折叠重复警告（⚠️ warning:）
            if line.contains("⚠️") && line.contains("warning:") {
                let mut count = 1;
                let mut warnings = vec![line.to_string()];

                while i + count < lines.len() {
                    let next = lines[i + count];
                    if next.contains("⚠️") && next.contains("warning:") {
                        classifier.classify_warning(next);
                        warnings.push(next.to_string());
                        count += 1;
                        continue;
                    }
                    break;
                }

                if count >= 3 {
                    // 每条 warning 的消息体都是定位构建问题的信号，不能只保留计数。
                    for warning in warnings {
                        result.push_str(&warning);
                        result.push('\n');
                    }
                    i += count;
                    continue;
                }
            }

            // 折叠 "modules by path" 汇总行
            if line.trim().starts_with("modules by path") {
                let mut count = 1;
                let indent_level = line.len() - line.trim_start().len();

                while i + count < lines.len() {
                    let next = lines[i + count];
                    let next_indent = next.len() - next.trim_start().len();

                    // 如果是更深层次的缩进（子模块），继续折叠
                    if next_indent > indent_level && !next.trim().is_empty() {
                        count += 1;
                        continue;
                    }
                    break;
                }

                if count >= 3 {
                    result.push_str(&format!("[MODULES] {} module groups\n", count));
                    i += count;
                    continue;
                }
            }

            // 识别并聚合资产列表 (e.g., dist/assets/index-abc.js  45.2 kB)
            if (line.contains("dist/") || line.contains("assets/") || line.contains("Asset"))
                && (line.contains("kB") || line.contains("MB") || line.contains("Size"))
            {
                let mut count = 1;
                let mut total_size_kb = 0.0;

                while i + count < lines.len() {
                    let next = lines[i + count];
                    if (next.contains("dist/")
                        || next.contains("assets/")
                        || next.contains(".js")
                        || next.contains(".css"))
                        && (next.contains("kB") || next.contains("MB") || next.contains("emitted"))
                    {
                        // 尝试提取大小
                        if let Some(size) = Self::extract_size_kb(next) {
                            total_size_kb += size;
                        }
                        count += 1;
                        continue;
                    }
                    break;
                }

                // 只有超过 3 个资产才折叠（族群 F 约束）
                if count >= 3 {
                    result.push_str(&format!(
                        "[FRONTEND_ASSETS: {} items, {:.1} KiB total]\n",
                        count, total_size_kb
                    ));
                    i += count;
                    continue;
                }
            }

            // 折叠重复的模块转换消息
            if line.contains("module") && line.contains("transformed") {
                let mut count = 1;
                while i + count < lines.len() {
                    let next = lines[i + count];
                    if next.contains("module") && next.contains("transformed") {
                        count += 1;
                        continue;
                    }
                    break;
                }

                if count >= 3 {
                    result.push_str(&format!("[MODULES] {} modules transformed\n", count));
                    i += count;
                    continue;
                }
            }

            // 压缩模块列表（[数字] ./path/to/file.js）
            if line.trim().starts_with('[') && line.contains("./") {
                let mut count = 1;
                while i + count < lines.len() {
                    let next = lines[i + count];
                    if next.trim().starts_with('[') && next.contains("./") {
                        count += 1;
                        continue;
                    }
                    break;
                }

                if count >= 5 {
                    result.push_str(&format!("[MODULES] {} modules built]\n", count));
                    i += count;
                    continue;
                }
            }

            // 提取哈希并存入字典
            let mut processed_line = line.to_string();
            let hash_re = HASH_RE.get_or_init(|| Regex::new(r"\b[0-9a-f]{20,}\b").unwrap());
            processed_line = hash_re
                .replace_all(&processed_line, |caps: &regex::Captures| {
                    dict_engine.add_macro(caps.get(0).unwrap().as_str())
                })
                .to_string();

            result.push_str(&processed_line);
            result.push('\n');
            i += 1;
        }

        // 添加构建摘要（如果有错误或警告）
        if let Some(summary) = classifier.generate_summary() {
            result.push_str(&summary);
            result.push('\n');
        }

        // 法则 A ROI 门控：确保压缩不会扩张
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, result);

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压 Webpack/Vite 日志：本插件为无损透传，直接返回原字符串。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    /// 归一化文本：将构建哈希替换为 [HASH]、构建耗时替换为 [TIME]，以便去重。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除构建哈希
        let hash_re = HASH_RE.get_or_init(|| Regex::new(r"\b[0-9a-f]{20,}\b").unwrap());
        result = hash_re.replace_all(&result, "[HASH]").to_string();

        // 抹除构建耗时
        let time_re = TIME_RE.get_or_init(|| Regex::new(r"\d+ms|\d+\.\d+s").unwrap());
        result = time_re.replace_all(&result, "[TIME]").to_string();

        result
    }
}

impl Clone for WebpackVitePlugin {
    /// 返回克隆的插件实例：复制名称与优先级。
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
        }
    }
}
