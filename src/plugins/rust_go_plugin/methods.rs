use super::types::RustGoPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::keep_error_signal;
use bumpalo::Bump;
use regex::Regex;
use std::sync::Arc;

impl RustGoPlugin {
    pub fn new() -> Self {
        Self {
            name: "rust_go",
            priority: 185,
            rust_compile_pattern: Arc::new(
                Regex::new(r"^(?P<prefix>\s*-->\s*)(?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+)$")
                    .unwrap(),
            ),
            go_panic_pattern: Arc::new(
                Regex::new(r"^goroutine\s+(?P<id>\d+)\s+\[(?P<state>[^\]]+)\]:$").unwrap(),
            ),
            go_frame_pattern: Arc::new(
                Regex::new(
                    r"^\t(?P<file>[^:]+):(?P<line>\d+)(?:\s+\+(?P<offset>0x[0-9a-fA-F]+))?$",
                )
                .unwrap(),
            ),
        }
    }
}

impl Default for RustGoPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl RustGoPlugin {
    /// 应用高级压缩功能（Cargo 输出、测试输出）
    /// 遵循压缩协议 V1 法则 E（零容忍废话），参见 docs/development/PLUGIN_DEVELOPMENT.md §8
    #[tracing::instrument(level = "debug", skip_all)]
    fn apply_advanced_compression(&self, text: &str) -> String {
        let mut result = String::new();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // 功能 1: 折叠 Cargo 编译输出
            if line.trim().starts_with("Compiling ") {
                let (folded, consumed) = self.fold_cargo_compiling(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&folded);
                    i += consumed;
                    continue;
                }
            }

            // 功能 3: 压缩 Cargo test 输出
            if line.starts_with("running ") && line.contains(" tests") {
                let (compressed, consumed) = self.compress_cargo_test(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 4: 压缩 Go test 输出
            if line.starts_with("=== RUN   ") {
                let (compressed, consumed) = self.compress_go_test(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            result.push_str(line);
            result.push('\n');
            i += 1;
        }

        result
    }

    /// 功能 1: 折叠 Cargo 编译输出
    /// 输入: "   Compiling libc v0.2.139\n   Compiling cfg-if v1.0.0\n..."
    /// 输出: "[CARGO] Compiling 25 crates (details suppressed)\n"
    #[tracing::instrument(level = "debug", skip_all)]
    fn fold_cargo_compiling(&self, lines: &[&str]) -> (String, usize) {
        let mut count = 0;
        let mut consumed = 0;
        let mut finished_line = String::new();

        for line in lines {
            if line.trim().starts_with("Compiling ") {
                count += 1;
                consumed += 1;
            } else if line.trim().starts_with("Finished ") {
                finished_line = line.to_string();
                consumed += 1;
                break;
            } else {
                break;
            }
        }

        if count == 0 {
            return (String::new(), 0);
        }

        let mut result = format!("[CARGO] Compiling {} crates (details suppressed)\n", count);
        if !finished_line.is_empty() {
            result.push_str(&format!("[CARGO] {}\n", finished_line.trim()));
        }

        (result, consumed)
    }

    /// 功能 2: 提取错误码统计
    /// 输入: 包含多个 "error[E0425]" 的文本
    /// 输出: "[ERROR_STATS] E0425 occurred 3 times, E0308 occurred 2 times\n"
    /// 注意: 仅当有重复错误码时才输出统计（单次出现的错误不统计）
    #[tracing::instrument(level = "debug", skip_all)]
    fn extract_error_code_stats(&self, text: &str) -> String {
        use std::collections::HashMap;

        let error_pattern = Regex::new(r"error\[(?P<code>E\d+)\]").unwrap();
        let mut error_counts: HashMap<String, usize> = HashMap::new();

        for line in text.lines() {
            if let Some(caps) = error_pattern.captures(line) {
                let code = caps.name("code").unwrap().as_str();
                *error_counts.entry(code.to_string()).or_insert(0) += 1;
            }
        }

        // 仅保留出现 2 次及以上的错误码（单次出现的不统计）
        let repeated_errors: HashMap<String, usize> = error_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .collect();

        if repeated_errors.is_empty() {
            return String::new();
        }

        // 按出现次数降序排序
        let mut sorted: Vec<_> = repeated_errors.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        let mut stats: Vec<String> = sorted
            .iter()
            .map(|(code, count)| format!("error[{}] occurred {} times", code, count))
            .collect();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("error:") || trimmed.starts_with("For more information") {
                stats.push(trimmed.to_string());
            }
        }

        format!("[ERROR_STATS] {}\n", stats.join("; "))
    }

    /// 功能 3: 压缩 Cargo test 输出
    /// 输入: "running 120 tests\ntest tests::test_foo ... ok\n..."
    /// 输出: "[TEST] Running 120 tests\n[TEST] 117 passed, 3 failed, 1 ignored\n"
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_cargo_test(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut ignored = 0;
        let mut total = 0;
        let mut failed_tests = Vec::new();
        let mut in_failure_details = false;

        // 第一行: "running N tests"
        if let Some(first_line) = lines.first() {
            if let Some(num_str) = first_line
                .strip_prefix("running ")
                .and_then(|s| s.strip_suffix(" tests"))
            {
                if let Ok(num) = num_str.parse::<usize>() {
                    total = num;
                    consumed += 1;
                }
            }
        }

        // 收集测试结果
        for line in &lines[consumed..] {
            if line.starts_with("test ") {
                if line.contains(" ... ok") {
                    passed += 1;
                } else if line.contains(" ... FAILED") {
                    failed += 1;
                    // 提取测试名称
                    if let Some(test_name) = line
                        .strip_prefix("test ")
                        .and_then(|s| s.split(" ... ").next())
                    {
                        failed_tests.push(test_name.to_string());
                    }
                } else if line.contains(" ... ignored") {
                    ignored += 1;
                }
                consumed += 1;
            } else if line.starts_with("failures:") {
                in_failure_details = true;
                consumed += 1;
            } else if in_failure_details {
                consumed += 1;
                if line.starts_with("test result:") {
                    break;
                }
            } else if line.starts_with("test result:") {
                consumed += 1;
                break;
            } else if line.trim().is_empty() {
                consumed += 1;
            } else {
                break;
            }
        }

        let mut result = format!("[TEST] Running {} tests\n", total);
        result.push_str(&format!(
            "[TEST] {} passed, {} failed, {} ignored (details below)\n",
            passed, failed, ignored
        ));

        // 保留失败的测试名称
        for test_name in failed_tests {
            result.push_str(&format!("test {} ... FAILED\n", test_name));
        }

        (result, consumed)
    }

    /// 功能 4: 压缩 Go test 输出
    /// 输入: "=== RUN   TestFoo\n--- PASS: TestFoo (0.00s)\n..."
    /// 输出: "[GO TEST] 45 passed, 1 failed (0.123s)\n"
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_go_test(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut failed_tests = Vec::new();
        let mut total_time = String::new();

        for line in lines {
            if line.starts_with("=== RUN   ") {
                consumed += 1;
            } else if line.starts_with("--- PASS: ") {
                passed += 1;
                consumed += 1;
            } else if line.starts_with("--- FAIL: ") {
                failed += 1;
                // 提取测试名称
                if let Some(test_name) = line
                    .strip_prefix("--- FAIL: ")
                    .and_then(|s| s.split(' ').next())
                {
                    failed_tests.push(test_name.to_string());
                }
                consumed += 1;
            } else if line.starts_with("PASS") || line.starts_with("FAIL") {
                consumed += 1;
                break;
            } else if line.starts_with("ok  \t") {
                // 提取总时间
                if let Some(time_str) = line.split_whitespace().last() {
                    total_time = time_str.to_string();
                }
                consumed += 1;
                break;
            } else if line.trim().is_empty() || line.starts_with("    ") {
                consumed += 1;
            } else {
                break;
            }
        }

        let mut result = format!("[GO TEST] {} passed, {} failed", passed, failed);
        if !total_time.is_empty() {
            result.push_str(&format!(" ({})", total_time));
        }
        result.push('\n');

        // 保留失败的测试
        for test_name in failed_tests {
            result.push_str(&format!(
                "=== RUN   {}\n--- FAIL: {}\n",
                test_name, test_name
            ));
        }

        (result, consumed)
    }
}

impl Plugin for RustGoPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lines: Vec<&str> = slice.text.lines().take(15).collect();
        if lines.is_empty() {
            return None;
        }

        let mut match_count = 0;
        for line in &lines {
            if line.starts_with("error[E")
                || line.starts_with("warning: ")
                || line.starts_with("panic: ")
            {
                match_count += 2;
            }
            if self.rust_compile_pattern.is_match(line)
                || self.go_panic_pattern.is_match(line)
                || self.go_frame_pattern.is_match(line)
            {
                match_count += 1;
            }
        }

        let ratio = match_count as f32 / lines.len() as f32;
        if ratio >= 0.3 {
            Some(ratio.min(1.0))
        } else {
            None
        }
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 先应用高级压缩功能
        let preprocessed = self.apply_advanced_compression(text);

        let mut tokens: Vec<Token<'a>> = Vec::new();

        // 法则 A.2.1 修复：去掉 IR 标签，直接使用路径字典 + 冒号分隔符
        // 原格式：` --> src/main.rs:5:9`（19B）
        // 新格式：` --> $Pn:5:9`（约 13B，取决于路径字典 token 长度）
        // 不再使用 `$RG|R| --> |$Pn|5|9`（27B）的 IR 标签格式
        for line in preprocessed.lines() {
            if let Some(caps) = self.rust_compile_pattern.captures(line) {
                let file_token = dict_engine.add_path_layered(caps.name("file").unwrap().as_str());
                // 直接输出：前缀 + 路径token + :行:列
                tokens.push(Token::Text(
                    format!(
                        "{}{}:{}:{}\n",
                        caps.name("prefix").unwrap().as_str(),
                        file_token,
                        caps.name("line").unwrap().as_str(),
                        caps.name("col").unwrap().as_str()
                    )
                    .into(),
                ));
                continue;
            }
            if let Some(caps) = self.go_panic_pattern.captures(line) {
                // Go panic 行保持简洁格式，去掉 IR 标签
                tokens.push(Token::Text(
                    format!(
                        "goroutine {} [{}]:\n",
                        caps.name("id").unwrap().as_str(),
                        caps.name("state").unwrap().as_str()
                    )
                    .into(),
                ));
                continue;
            }
            if let Some(caps) = self.go_frame_pattern.captures(line) {
                let file_token = dict_engine.add_path_layered(caps.name("file").unwrap().as_str());
                let line_num = caps.name("line").unwrap().as_str();
                // Go 栈帧格式：\t文件:行 +偏移
                if let Some(offset) = caps.name("offset") {
                    tokens.push(Token::Text(
                        format!("\t{}:{} +{}\n", file_token, line_num, offset.as_str()).into(),
                    ));
                } else {
                    tokens.push(Token::Text(
                        format!("\t{}:{}\n", file_token, line_num).into(),
                    ));
                }
                continue;
            }
            tokens.push(Token::Text(format!("{}\n", line).into()));
        }

        // 法则 A ROI 门控：去掉 IR 标签后，压缩格式更紧凑。
        // 但小样本 / 单行 / 无字典命中场景下仍可能扩张，
        // 整段 prefer_non_expanding 回退原文。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § A.2.1。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();

        // 功能 2: 错误码统计作为附加价值信息
        // 在 ROI 门控之后添加，不影响压缩率计算
        let error_stats = self.extract_error_code_stats(text);
        let mut final_text = keep_error_signal(
            text,
            crate::core::utils::roi::prefer_non_expanding(text, compacted),
        );

        // 如果有错误统计且文本中包含错误，优先采用统计摘要；摘要仍需通过 ROI 门控。
        if !error_stats.is_empty() && text.contains("error[E") {
            let candidate = keep_error_signal(text, error_stats);
            final_text = crate::core::utils::roi::prefer_non_expanding(text, candidate);
        }

        CompressResult {
            tokens: vec![Token::Text(final_text.into())],
            metadata: None,
            plugin_name: Some(self.name),
        }
    }

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut result = String::new();
        for line in compressed.lines() {
            // 新格式不再使用 IR 标签，直接是原始格式
            // 只需要解析路径 token 即可
            // 格式：` --> $Pn:5:9` 或 `\t$Pn:42 +0x123`

            // Rust 编译路径格式：` --> $Pn:line:col`
            if let Some(caps) = self.rust_compile_pattern.captures(line) {
                let file_str = caps.name("file").unwrap().as_str();
                let file = dict.resolve_or_self(file_str);
                result.push_str(&format!(
                    "{}{}:{}:{}\n",
                    caps.name("prefix").unwrap().as_str(),
                    file,
                    caps.name("line").unwrap().as_str(),
                    caps.name("col").unwrap().as_str()
                ));
                continue;
            }

            // Go panic 格式：goroutine N [state]:
            if let Some(caps) = self.go_panic_pattern.captures(line) {
                result.push_str(&format!(
                    "goroutine {} [{}]:\n",
                    caps.name("id").unwrap().as_str(),
                    caps.name("state").unwrap().as_str()
                ));
                continue;
            }

            // Go 栈帧格式：\t$Pn:line +offset
            if let Some(caps) = self.go_frame_pattern.captures(line) {
                let file_str = caps.name("file").unwrap().as_str();
                let file = dict.resolve_or_self(file_str);
                if let Some(offset) = caps.name("offset") {
                    result.push_str(&format!(
                        "\t{}:{} +{}\n",
                        file,
                        caps.name("line").unwrap().as_str(),
                        offset.as_str()
                    ));
                } else {
                    result.push_str(&format!(
                        "\t{}:{}\n",
                        file,
                        caps.name("line").unwrap().as_str()
                    ));
                }
                continue;
            }

            result.push_str(line);
            result.push('\n');
        }
        result
    }

    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}
