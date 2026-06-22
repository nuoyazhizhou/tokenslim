//! NDJSON 插件方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::json_extractor::extract_json_object;
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

impl NdjsonPlugin {
    pub fn new() -> Self {
        Self {
            name: "ndjson",
            priority: 145, // 高于 json_plugin (140)，低于 git_diff_plugin (160)
            ndjson_detect_pattern: Arc::new(
                Regex::new(r#"(?m)^\{"[^"]+":"#).unwrap(), // 每行都以 {"key": 开头
            ),
            go_test_pattern: Arc::new(
                Regex::new(r#""Action":"(run|pass|fail|skip|output)""#).unwrap(),
            ),
            config: NdjsonConfig::default(),
        }
    }

    /// 检测是否为 NDJSON 格式
    ///
    /// 规则：
    /// 1. 至少有 2 行
    /// 2. 每行都是 JSON 对象（以 { 开头，以 } 结尾）
    /// 3. 至少 80% 的行匹配 NDJSON 模式
    #[tracing::instrument(level = "debug", skip_all)]
    fn is_ndjson(&self, text: &str) -> bool {
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();

        if lines.len() < 2 {
            return false;
        }

        let mut json_lines = 0;
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                json_lines += 1;
            }
        }

        // 至少 80% 的行是 JSON 对象
        json_lines as f32 / lines.len() as f32 >= 0.8
    }

    /// 检测是否为 Go test -json 输出
    #[tracing::instrument(level = "debug", skip_all)]
    fn is_go_test_json(&self, text: &str) -> bool {
        self.go_test_pattern.is_match(text)
    }

    /// 解析 Go test -json 输出
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_go_test_events(&self, text: &str) -> Vec<GoTestEvent> {
        let mut events = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 尝试提取 JSON 对象
            if let Some(json_str) = extract_json_object(trimmed) {
                if let Ok(event) = serde_json::from_str::<GoTestEvent>(&json_str) {
                    events.push(event);
                }
            }
        }

        events
    }

    /// 聚合 Go test 事件
    #[tracing::instrument(level = "debug", skip_all)]
    fn aggregate_go_test_events(&self, events: Vec<GoTestEvent>) -> HashMap<String, PackageInfo> {
        let mut packages: HashMap<String, PackageInfo> = HashMap::new();

        for event in events {
            let package_name = event.package.unwrap_or_else(|| "unknown".to_string());
            let package = packages
                .entry(package_name.clone())
                .or_insert_with(|| PackageInfo::new(package_name));

            match event.action.as_str() {
                "run" => {
                    // 测试开始 - 不做任何操作，等待最终结果
                    // 这样 update_test 会在第一次调用时创建测试并正确计数
                }
                "pass" | "fail" | "skip" => {
                    // 测试最终结果 - 这里会创建测试并正确计数
                    if let Some(test_name) = event.test {
                        let result = match event.action.as_str() {
                            "pass" => TestResult::Pass,
                            "fail" => TestResult::Fail,
                            "skip" => TestResult::Skip,
                            _ => unreachable!(),
                        };
                        package.update_test(test_name, result, event.elapsed);
                    }
                }
                "output" => {
                    // 测试输出
                    if self.config.show_test_output {
                        if let (Some(test_name), Some(output)) = (event.test, event.output) {
                            package.add_test_output(&test_name, output);
                        }
                    }
                }
                _ => {
                    // 忽略其他事件（pause, cont, bench）
                }
            }
        }

        packages
    }

    /// 渲染 Go test 结果摘要
    #[tracing::instrument(level = "debug", skip_all)]
    fn render_go_test_summary(
        &self,
        packages: HashMap<String, PackageInfo>,
        dict_engine: &mut DictionaryEngine,
    ) -> String {
        let mut lines = Vec::new();

        // 命令锚点（法则 0）
        lines.push("go test -json".to_string());

        // 按包名排序
        let mut package_names: Vec<String> = packages.keys().cloned().collect();
        package_names.sort();

        for package_name in package_names {
            let package = &packages[&package_name];

            // 压缩包名路径
            let compressed_package = dict_engine.add_path_layered(&package.name);

            // 包摘要行
            let summary = format!(
                "PKG: {} | {} tests, {} passed, {} failed, {} skipped",
                compressed_package,
                package.tests.len(),
                package.passed,
                package.failed,
                package.skipped
            );
            lines.push(summary);

            // 测试详情（只显示失败的测试）
            let mut test_names: Vec<String> = package.tests.keys().cloned().collect();
            test_names.sort();

            for test_name in test_names {
                let test = &package.tests[&test_name];

                match test.result {
                    TestResult::Fail => {
                        let elapsed_str = test
                            .elapsed
                            .map(|e| format!(" ({:.2}s)", e))
                            .unwrap_or_default();
                        lines.push(format!("  ✗ {}{}", test.name, elapsed_str));

                        // 显示失败输出（截断）
                        if self.config.show_test_output && !test.output.is_empty() {
                            let max_lines = self.config.max_output_lines.min(test.output.len());
                            for output_line in test.output.iter().take(max_lines) {
                                lines.push(format!("    {}", output_line.trim()));
                            }
                            if test.output.len() > max_lines {
                                lines.push(format!(
                                    "    ... {} more lines truncated ...",
                                    test.output.len() - max_lines
                                ));
                            }
                        }
                    }
                    TestResult::Pass => {
                        // 通过的测试只显示名称和耗时
                        if let Some(elapsed) = test.elapsed {
                            if elapsed > 1.0 {
                                // 只显示耗时超过 1 秒的测试
                                lines.push(format!("  ✓ {} ({:.2}s)", test.name, elapsed));
                            }
                        }
                    }
                    TestResult::Skip => {
                        // 跳过的测试不显示
                    }
                }
            }
        }

        // 总体摘要
        let total_packages = packages.len();
        let total_tests: usize = packages.values().map(|p| p.tests.len()).sum();
        let total_passed: usize = packages.values().map(|p| p.passed).sum();
        let total_failed: usize = packages.values().map(|p| p.failed).sum();
        let total_skipped: usize = packages.values().map(|p| p.skipped).sum();

        lines.push(format!(
            "SUMMARY: {} packages, {} tests, {} passed, {} failed, {} skipped",
            total_packages, total_tests, total_passed, total_failed, total_skipped
        ));

        lines.join("\n")
    }

    /// 通用 NDJSON 压缩（非 Go test 场景）
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_generic_ndjson(&self, text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let total_lines = lines.len();

        if total_lines <= 10 {
            // 少于 10 行，直接返回
            return text.to_string();
        }

        // 显示前 5 行和后 5 行
        let mut result = Vec::new();
        for line in lines.iter().take(5) {
            result.push(line.to_string());
        }
        result.push(format!("... {} lines truncated ...", total_lines - 10));
        for line in lines.iter().skip(total_lines - 5) {
            result.push(line.to_string());
        }

        result.join("\n")
    }
}

impl Plugin for NdjsonPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.trim();

        // 检测 NDJSON 格式
        if !self.is_ndjson(text) {
            return None;
        }

        // 如果是 Go test -json，返回高置信度
        if self.is_go_test_json(text) {
            return Some(0.95);
        }

        // 通用 NDJSON
        Some(0.85)
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        let compacted = if self.config.go_test_mode && self.is_go_test_json(text) {
            // Go test -json 特定压缩
            let events = self.parse_go_test_events(text);
            let packages = self.aggregate_go_test_events(events);
            self.render_go_test_summary(packages, dict_engine)
        } else {
            // 通用 NDJSON 压缩
            self.compress_generic_ndjson(text)
        };

        // 法则 A ROI 门控
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        // NDJSON 压缩是有损的（聚合），无法完全还原
        // 只能还原路径字典
        let pattern = Regex::new(r"(\$[MP]\d+)").unwrap();
        pattern
            .replace_all(compressed, |caps: &regex::Captures| {
                let token = caps.get(1).unwrap().as_str();
                if let Some(original) = dict.resolve(token) {
                    original.to_string()
                } else {
                    token.to_string()
                }
            })
            .into_owned()
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<NdjsonConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config".to_string())
    }
}

impl Clone for NdjsonPlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            ndjson_detect_pattern: self.ndjson_detect_pattern.clone(),
            go_test_pattern: self.go_test_pattern.clone(),
            config: self.config.clone(),
        }
    }
}
