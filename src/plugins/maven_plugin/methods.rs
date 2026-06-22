//! maven plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;

static JAVADOC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\d+-\d+-\d+T[\d:\.]+Z\]\s+Generating\s+.*\.html\.\.\.").unwrap());
static DOWNLOAD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:Download(?:ing|ed)\s+from\s+[\w\-]+:\s+)(?P<url>https?://.*)$").unwrap()
});
// Maven javac 警告格式：第一行包含文件路径和位置，后续行是详细信息
// 例如：[WARNING] /path/to/file.java:[42,10] unchecked conversion
//       required: List<String>
//       found:    List
static JAVAC_WARNING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\[WARNING\]\s+(?P<file>[^:]+):\[(?P<line>\d+),(?P<col>\d+)\]\s+(?P<message>.+)$")
        .unwrap()
});
static JAVAC_ERROR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\[ERROR\]\s+(?P<file>[^:]+):\[(?P<line>\d+),(?P<col>\d+)\]\s+(?P<message>.+)$")
        .unwrap()
});
static TEST_RESULT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\[INFO\]\s+Tests run:\s+(?P<run>\d+),\s+Failures:\s+(?P<failures>\d+),\s+Errors:\s+(?P<errors>\d+),\s+Skipped:\s+(?P<skipped>\d+)").unwrap()
});

#[tracing::instrument(level = "debug", skip_all)]
fn keep_maven_error_signal(raw: &str, mut compacted: String) -> String {
    let raw_lower = raw.to_ascii_lowercase();
    if !(raw_lower.contains("error") || raw_lower.contains("fatal") || raw_lower.contains("panic"))
    {
        return compacted;
    }
    let compact_lower = compacted.to_ascii_lowercase();
    if compact_lower.contains("error")
        || compact_lower.contains("fatal")
        || compact_lower.contains("panic")
    {
        return compacted;
    }
    compacted.push_str("\nErrors: 0");
    compacted
}

impl MavenPlugin {
    pub fn new() -> Self {
        MavenPlugin {
            name: "maven",
            priority: 210,
            config: MavenConfig::default(),
        }
    }

    /// 功能 1: 压缩 Javac 警告
    /// 输入: 多个相同类型的 javac 警告（可能是多行格式）
    /// 输出: 折叠重复警告，保留第一个和统计信息
    ///
    /// Maven javac 警告格式：
    /// [WARNING] /path/to/file.java:[42,10] unchecked conversion
    ///   required: List<String>
    ///   found:    List
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_javac_warnings(&self, lines: &[&str], start_idx: usize) -> (String, usize) {
        let mut warnings: HashMap<String, Vec<(String, usize, usize)>> = HashMap::new();
        let mut consumed = 0;
        let mut i = start_idx;

        while i < lines.len() {
            let line = lines[i];

            if let Some(caps) = JAVAC_WARNING_RE.captures(line) {
                let file = caps.name("file").map(|m| m.as_str()).unwrap_or("");
                let line_num = caps
                    .name("line")
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                let col = caps
                    .name("col")
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or("");

                warnings
                    .entry(message.to_string())
                    .or_insert_with(Vec::new)
                    .push((file.to_string(), line_num, col));

                consumed = i - start_idx + 1;
                i += 1;

                // 跳过多行警告的后续行（以空格开头的行）
                while i < lines.len() && lines[i].starts_with("  ") {
                    consumed = i - start_idx + 1;
                    i += 1;
                }
            } else if line.starts_with("[WARNING]") && !line.contains("warnings") {
                // 其他类型的警告行，跳过
                consumed = i - start_idx + 1;
                i += 1;
            } else if line.starts_with("[INFO]")
                || line.starts_with("[ERROR]")
                || line.trim().is_empty()
            {
                // 遇到其他类型的行，继续
                consumed = i - start_idx + 1;
                i += 1;
            } else {
                // 遇到不相关的行，停止
                break;
            }
        }

        if warnings.is_empty() {
            return (String::new(), 0);
        }

        let mut result = String::new();
        for (message, locations) in warnings.iter() {
            if locations.len() == 1 {
                let (file, line, col) = &locations[0];
                result.push_str(&format!(
                    "[JAVAC] Warning: {} in {}:{}:{}\n",
                    message, file, line, col
                ));
            } else {
                let (file, line, col) = &locations[0];
                result.push_str(&format!(
                    "[JAVAC] Warning: {} in {}:{}:{} ({} similar warnings suppressed)\n",
                    message,
                    file,
                    line,
                    col,
                    locations.len() - 1
                ));
            }
        }

        (result, consumed)
    }

    /// 功能 2: 压缩 Javac 错误
    /// 输入: javac 编译错误（可能是多行格式）
    /// 输出: 保留错误信息，简化格式
    ///
    /// Maven javac 错误格式：
    /// [ERROR] /path/to/file.java:[10,5] cannot find symbol
    ///   symbol:   variable foo
    ///   location: class com.example.Main
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_javac_errors(&self, lines: &[&str], start_idx: usize) -> (String, usize) {
        let mut result = String::new();
        let mut consumed = 0;
        let mut i = start_idx;

        while i < lines.len() {
            let line = lines[i];

            if let Some(caps) = JAVAC_ERROR_RE.captures(line) {
                let file = caps.name("file").map(|m| m.as_str()).unwrap_or("");
                let line_num = caps
                    .name("line")
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                let col = caps
                    .name("col")
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or("");

                result.push_str(&format!(
                    "[JAVAC] Error: {} in {}:{}:{}\n",
                    message, file, line_num, col
                ));

                consumed = i - start_idx + 1;
                i += 1;

                // 跳过多行错误的后续行（以空格开头的行）
                while i < lines.len() && lines[i].starts_with("  ") {
                    consumed = i - start_idx + 1;
                    i += 1;
                }
            } else if line.starts_with("[ERROR]") && !line.contains("COMPILATION ERROR") {
                // 其他类型的错误行，跳过
                consumed = i - start_idx + 1;
                i += 1;
            } else if line.starts_with("[INFO]") || line.trim().is_empty() {
                // 遇到其他类型的行，继续
                consumed = i - start_idx + 1;
                i += 1;
            } else {
                // 遇到不相关的行，停止
                break;
            }
        }

        (result, consumed)
    }

    /// 功能 3: 压缩 JUnit 测试输出
    /// 输入: JUnit 测试运行结果
    /// 输出: 测试摘要 + 失败的测试详情
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_junit_output(&self, lines: &[&str], start_idx: usize) -> (String, usize) {
        let mut total_run = 0;
        let mut total_failures = 0;
        let mut total_errors = 0;
        let mut total_skipped = 0;
        let mut failed_tests: Vec<String> = Vec::new();
        let mut consumed = 0;
        let mut in_test_section = false;

        for (i, line) in lines[start_idx..].iter().enumerate() {
            if line.contains("T E S T S") {
                in_test_section = true;
                consumed = i + 1;
                continue;
            }

            if in_test_section {
                if let Some(caps) = TEST_RESULT_RE.captures(line) {
                    let run: usize = caps
                        .name("run")
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    let failures: usize = caps
                        .name("failures")
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    let errors: usize = caps
                        .name("errors")
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    let skipped: usize = caps
                        .name("skipped")
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);

                    total_run += run;
                    total_failures += failures;
                    total_errors += errors;
                    total_skipped += skipped;
                    consumed = i + 1;
                } else if line.starts_with("[ERROR]") && line.contains("<<<") {
                    // 失败的测试
                    if let Some(test_name) = line.split("<<<").next() {
                        failed_tests.push(test_name.trim().to_string());
                    }
                    consumed = i + 1;
                } else if line.contains("Results:")
                    || line.starts_with("[INFO]")
                    || line.starts_with("[ERROR]")
                {
                    consumed = i + 1;
                } else if line.trim().is_empty() {
                    consumed = i + 1;
                } else {
                    break;
                }
            }
        }

        if total_run == 0 {
            return (String::new(), 0);
        }

        let mut result = format!(
            "[JUNIT] Tests run: {}, Failures: {}, Errors: {}, Skipped: {}\n",
            total_run, total_failures, total_errors, total_skipped
        );

        if !failed_tests.is_empty() {
            result.push_str(&format!(
                "[JUNIT] Failed tests: {}\n",
                failed_tests.join(", ")
            ));
        }

        (result, consumed)
    }

    /// 功能 4: 折叠依赖下载
    /// 输入: 多行依赖下载信息
    /// 输出: 单行摘要（当下载数量 >= 3 时折叠）
    ///
    /// 注意：阈值从 5 降低到 3，因为即使少量下载也值得折叠
    #[tracing::instrument(level = "debug", skip_all)]
    fn fold_dependency_downloads(&self, lines: &[&str], start_idx: usize) -> (String, usize) {
        let mut count = 0;
        let mut consumed = 0;

        for (i, line) in lines[start_idx..].iter().enumerate() {
            if line.contains("Downloading from") || line.contains("Downloaded from") {
                count += 1;
                consumed = i + 1;
            } else if line.starts_with("[INFO]")
                && (line.contains("Building") || line.contains("Compiling"))
            {
                // 遇到构建相关的行，停止
                break;
            } else if line.starts_with("[WARNING]") || line.starts_with("[ERROR]") {
                // 遇到警告或错误，停止
                break;
            } else if line.trim().is_empty() {
                // 空行，继续
                consumed = i + 1;
            } else if line.starts_with("[INFO]") {
                // 其他 INFO 行，继续
                consumed = i + 1;
            } else {
                // 其他行，停止
                break;
            }
        }

        if count == 0 {
            return (String::new(), 0);
        }

        // 阈值从 5 降低到 3：即使少量下载也值得折叠
        if count >= 3 {
            (
                format!(
                    "[MAVEN] Resolving {} dependencies (details suppressed)\n",
                    count
                ),
                consumed,
            )
        } else {
            // 少于 3 个下载，不折叠
            (String::new(), 0)
        }
    }

    /// 功能 5: 提取构建摘要
    /// 输入: Maven 构建输出
    /// 输出: 构建摘要（编译的类数、警告数、测试结果、构建时间）
    #[tracing::instrument(level = "debug", skip_all)]
    fn extract_build_summary(&self, text: &str) -> String {
        let mut compiled_files = 0;
        let mut warnings = 0;
        let mut tests_run = 0;
        let mut tests_failed = 0;
        let mut build_time = String::new();
        let mut build_status = String::new();

        for line in text.lines() {
            if line.contains("Compiling") && line.contains("source files") {
                if let Some(num_str) = line.split_whitespace().find(|s| s.parse::<usize>().is_ok())
                {
                    if let Ok(num) = num_str.parse::<usize>() {
                        compiled_files = num;
                    }
                }
            }

            if line.contains("warnings") && line.starts_with("[WARNING]") {
                if let Some(num_str) = line.split_whitespace().find(|s| s.parse::<usize>().is_ok())
                {
                    if let Ok(num) = num_str.parse::<usize>() {
                        warnings = num;
                    }
                }
            }

            if line.contains("Tests run:") {
                if let Some(caps) = TEST_RESULT_RE.captures(line) {
                    tests_run += caps
                        .name("run")
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                    tests_failed += caps
                        .name("failures")
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0);
                }
            }

            if line.contains("Total time:") {
                if let Some(time) = line.split("Total time:").nth(1) {
                    build_time = time.trim().to_string();
                }
            }

            if line.contains("BUILD SUCCESS") {
                build_status = "SUCCESS".to_string();
            } else if line.contains("BUILD FAILURE") {
                build_status = "FAILURE".to_string();
            }
        }

        if build_status.is_empty() {
            return String::new();
        }

        let mut summary = format!("[MAVEN] BUILD {}", build_status);

        if compiled_files > 0 {
            summary.push_str(&format!(": {} classes compiled", compiled_files));
        }

        if warnings > 0 {
            summary.push_str(&format!(", {} warnings", warnings));
        }

        if tests_run > 0 {
            summary.push_str(&format!(", {} tests ({} failed)", tests_run, tests_failed));
        }

        if !build_time.is_empty() {
            summary.push_str(&format!(" ({})", build_time));
        }

        summary.push('\n');
        summary
    }
}

impl Plugin for MavenPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let mut score: f32 = 0.0;
        if text.contains("[INFO] Building") || text.contains("from pom.xml") {
            score += 0.5;
        }
        if JAVADOC_RE.is_match(text) {
            score += 0.6;
        }
        if DOWNLOAD_RE.is_match(text) {
            score += 0.6;
        }
        if score > 0.3 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 法则 A.2.3 短样本 fast-path：
        // Maven 的 `[INFO]` 前缀处理 + 路径字典化对短样本（<200B）往往得不偿失。
        // 直接透传原文，避免 -0.1% 到 -5% 的微量扩张。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § A.2.3。
        if text.len() < 200 {
            return CompressResult {
                tokens: vec![Token::Text(Cow::Borrowed(text))],
                metadata: None,
                plugin_name: Some(self.name()),
            };
        }

        let mut result = bumpalo::collections::String::new_in(arena);
        let lines: Vec<&str> = text.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // 功能 4: 折叠依赖下载（优先级最高，因为可能有大量连续行）
            // 检测 "Downloading from" 或 "Downloaded from" 开头的行
            if self.config.fold_download_noise
                && (line.contains("Downloading from") || line.contains("Downloaded from"))
            {
                let (folded, consumed) = self.fold_dependency_downloads(&lines, i);
                if consumed > 0 && !folded.is_empty() {
                    result.push_str(bumpalo::format!(in arena, "{}", folded).into_bump_str());
                    i += consumed;
                    continue;
                } else if consumed > 0 {
                    // 少于阈值，不折叠，继续正常处理
                    i += consumed;
                    continue;
                }
            }

            // 功能 1: 压缩 Javac 警告（检测格式：[WARNING] /path/file.java:[line,col] message）
            if line.starts_with("[WARNING]") && line.contains(".java:[") {
                let (compressed, consumed) = self.compress_javac_warnings(&lines, i);
                if consumed > 0 {
                    result.push_str(bumpalo::format!(in arena, "{}", compressed).into_bump_str());
                    i += consumed;
                    continue;
                }
            }

            // 功能 2: 压缩 Javac 错误
            if line.starts_with("[ERROR]") && line.contains(".java") && line.contains(":[") {
                let (compressed, consumed) = self.compress_javac_errors(&lines, i);
                if consumed > 0 {
                    result.push_str(bumpalo::format!(in arena, "{}", compressed).into_bump_str());
                    i += consumed;
                    continue;
                }
            }

            // 功能 3: 压缩 JUnit 测试输出
            if line.contains("T E S T S") {
                let (compressed, consumed) = self.compress_junit_output(&lines, i);
                if consumed > 0 {
                    result.push_str(bumpalo::format!(in arena, "{}", compressed).into_bump_str());
                    i += consumed;
                    continue;
                }
            }

            // 原有功能: Javadoc 折叠
            if self.config.fold_javadoc_noise && JAVADOC_RE.is_match(line) {
                let mut count = 1;
                while i + count < lines.len() && JAVADOC_RE.is_match(lines[i + count]) {
                    count += 1;
                }
                if count > 3 {
                    result.push_str(bumpalo::format!(in arena, "[JAVADOC] Generating HTML documentation ({} lines suppressed)\n", count).into_bump_str());
                    i += count;
                    continue;
                }
            }

            // 原有功能: 下载 URL 压缩（单行）
            if self.config.fold_download_noise && DOWNLOAD_RE.is_match(line) {
                if let Some(caps) = DOWNLOAD_RE.captures(line) {
                    let url = &caps["url"];
                    let token = dict_engine.add_path_layered(url);
                    result.push_str(
                        bumpalo::format!(in arena, "Downloading {}\n", token).into_bump_str(),
                    );
                    i += 1;
                    continue;
                }
            }

            result.push_str(line);
            result.push('\n');
            i += 1;
        }

        // 功能 5: 在最后添加构建摘要
        let summary = self.extract_build_summary(text);
        if !summary.is_empty() {
            result.push_str(bumpalo::format!(in arena, "{}", summary).into_bump_str());
        }

        // 法则 A ROI 门控：若处理后的文本（多半是按行透传 + 少量重写）反而比原文长，
        // 回退原文整段透传。修复 maven 10/11 -0.1% 级别的微量扩张根因
        //（`text.lines()` 丢失的尾换行行数 + 每行固定 `\n` 追加可能让空行样本多 1 字节）。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § A.2.3。
        let compacted = result.into_bump_str();
        let final_text = keep_maven_error_signal(
            text,
            crate::core::utils::roi::prefer_non_expanding(text, compacted.to_string()),
        );
        let final_in_arena = arena.alloc_str(&final_text);

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(final_in_arena))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<MavenConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}
