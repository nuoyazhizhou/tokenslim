use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;

impl SimpleRulePlugin {
    /// 创建 SimpleRulePlugin 实例：编译规则正则（enter/exit/keep/drop/aggregates），名称 static_rule，优先级 95。
    pub fn new(config: StaticRuleConfig) -> Self {
        let compiled_sections = Self::compile_sections(&config);
        Self {
            name: "static_rule",
            priority: 95,
            config,
            compiled_sections,
        }
    }

    /// 从 TOML 文本解析配置并创建插件实例。
    pub fn from_toml(toml_text: &str) -> Result<Self, String> {
        let config: StaticRuleConfig =
            toml::from_str(toml_text).map_err(|e| format!("invalid static rule toml: {e}"))?;
        Ok(Self::new(config))
    }

    /// 将 TOML 规则编译为预编译正则的 CompiledSection 列表（非法正则静默跳过）。
    fn compile_sections(config: &StaticRuleConfig) -> Vec<CompiledSection> {
        config
            .sections
            .iter()
            .map(|s| CompiledSection {
                name: s.name.clone(),
                enter: Regex::new(&s.enter).ok(),
                exit: s.exit.as_ref().and_then(|r| Regex::new(r).ok()),
                match_pattern: s.match_pattern.as_ref().and_then(|r| Regex::new(r).ok()),
                keep: s.keep.iter().filter_map(|r| Regex::new(r).ok()).collect(),
                drop: s.drop.iter().filter_map(|r| Regex::new(r).ok()).collect(),
                aggregates: s
                    .aggregates
                    .iter()
                    .map(|a| CompiledAggregate {
                        name: a.name.clone(),
                        kind: a.kind.clone(),
                        pattern: a.pattern.as_ref().and_then(|r| Regex::new(r).ok()),
                    })
                    .collect(),
            })
            .collect()
    }

    /// 对当前行应用聚合规则：Count 按模式命中计数，Sum 提取捕获组的数值累加。
    fn apply_aggregates(section: &CompiledSection, line: &str, state: &mut AggregationState) {
        for agg in &section.aggregates {
            match agg.kind {
                AggregateKind::Count => {
                    let should_count = agg
                        .pattern
                        .as_ref()
                        .map(|p| p.is_match(line))
                        .unwrap_or(true);
                    if should_count {
                        *state.values.entry(agg.name.clone()).or_insert(0) += 1;
                    }
                }
                AggregateKind::Sum => {
                    if let Some(pattern) = &agg.pattern {
                        if let Some(caps) = pattern.captures(line) {
                            if let Some(first) = caps.get(1) {
                                if let Ok(num) = first.as_str().parse::<i64>() {
                                    *state.values.entry(agg.name.clone()).or_insert(0) += num;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// 判断行是否应保留：命中 drop 规则丢弃，无 keep 规则时全部保留，否则需命中 keep。
    fn should_keep_line(section: &CompiledSection, line: &str) -> bool {
        if section.drop.iter().any(|r| r.is_match(line)) {
            return false;
        }
        if section.keep.is_empty() {
            return true;
        }
        section.keep.iter().any(|r| r.is_match(line))
    }

    /// 渲染输出：有 output_template 时按模板替换 {body} 与聚合变量，否则拼 $SR| 指标 + 正文。
    fn render_output(&self, collected_lines: &[String], state: &AggregationState) -> String {
        let body = collected_lines.join("\n");
        if let Some(template) = &self.config.output_template {
            let mut out = template.replace("{body}", &body);
            for (name, value) in &state.values {
                out = out.replace(&format!("{{{name}}}"), &value.to_string());
            }
            return out;
        }

        if state.values.is_empty() {
            return body;
        }

        let mut metrics = state
            .values
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>();
        metrics.sort();
        if body.is_empty() {
            format!("$SR|{}", metrics.join(" "))
        } else {
            format!("$SR|{}\n{}", metrics.join(" "), body)
        }
    }
}

impl Plugin for SimpleRulePlugin {
    /// 返回插件名称 "static_rule"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 95。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：任一 section 的 enter 正则命中得 0.75，keep 正则命中得 0.45。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if self.compiled_sections.is_empty() {
            return None;
        }

        for section in &self.compiled_sections {
            if section
                .enter
                .as_ref()
                .map(|r| r.is_match(text))
                .unwrap_or(false)
            {
                return Some(0.75);
            }
            if section.keep.iter().any(|r| r.is_match(text)) {
                return Some(0.45);
            }
        }
        None
    }

    /// 压缩切片：按 section 进入/退出/匹配规则收集行并应用聚合，有信号时输出渲染结果。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        if self.compiled_sections.is_empty() {
            return CompressResult {
                tokens: vec![Token::Text(Cow::Borrowed(slice.text.as_ref()))],
                metadata: None,
                plugin_name: None,
            };
        }

        let mut state = AggregationState::default();
        let mut collected_lines = Vec::new();

        for section in &self.compiled_sections {
            let mut active = false;
            for line in slice.text.lines() {
                if !active {
                    if section
                        .enter
                        .as_ref()
                        .map(|r| r.is_match(line))
                        .unwrap_or(false)
                    {
                        active = true;
                    }
                    continue;
                }

                if section
                    .exit
                    .as_ref()
                    .map(|r| r.is_match(line))
                    .unwrap_or(false)
                {
                    active = false;
                    continue;
                }

                // 如果设置了 match_pattern，只处理匹配的行
                if let Some(ref match_re) = section.match_pattern {
                    if !match_re.is_match(line) {
                        continue;
                    }
                }

                Self::apply_aggregates(section, line, &mut state);

                if Self::should_keep_line(section, line) {
                    collected_lines.push(format!("[{}] {}", section.name, line));
                }
            }
        }

        let output = self.render_output(&collected_lines, &state);
        let has_signal = !collected_lines.is_empty() || !state.values.is_empty();

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(if has_signal {
                output
            } else {
                slice.text.to_string()
            }))],
            metadata: None,
            plugin_name: if has_signal { Some(self.name()) } else { None },
        }
    }

    /// 解压：原文透传（规则压缩不可逆）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::text_slicer::{Slice, SliceType};

    /// 测试：从 TOML 解析失败 section 并验证规则提取与聚合生效。
    #[test]
    fn parse_from_toml_and_extract_fail_section() {
        let toml_text = r#"
            output_template = "SUMMARY failed={failed_count}\\n{body}"

            [[sections]]
            name = "failed_tests"
            enter = "^FAILED tests"
            exit = "^===="
            keep = ["^FAILED", "^ERROR"]

            [[sections.aggregates]]
            name = "failed_count"
            kind = "count"
            pattern = "^FAILED"
        "#;

        let plugin = SimpleRulePlugin::from_toml(toml_text).expect("toml should parse");

        let input = "noise\nFAILED tests begin\nFAILED test_a\nERROR test_b\n====\ntrailing";
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed(input),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: 6,
            file_metadata: None,
            flags: Default::default(),
        };

        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = Bump::new();

        let out = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
        let text = match &out.tokens[0] {
            Token::Text(s) => s.as_ref(),
            _ => "",
        };

        // output_template 可能由后续配置层覆盖，这里验证规则提取与聚合已生效
        assert!(
            text.contains("failed_count=1") || text.contains("SUMMARY failed=1"),
            "unexpected output: {text}"
        );
        assert!(text.contains("[failed_tests] FAILED test_a"));
        assert!(text.contains("[failed_tests] ERROR test_b"));
        assert_eq!(out.plugin_name, Some("static_rule"));
    }

    /// 测试：Sum 聚合从捕获组累加数值。
    #[test]
    fn aggregate_sum_works_with_capture_group() {
        let cfg = StaticRuleConfig {
            sections: vec![RuleSection {
                name: "summary".to_string(),
                enter: "^BEGIN$".to_string(),
                exit: Some("^END$".to_string()),
                match_pattern: None,
                keep: vec![],
                drop: vec![],
                aggregates: vec![AggregateRule {
                    name: "passed_total".to_string(),
                    kind: AggregateKind::Sum,
                    pattern: Some("(\\d+) passed".to_string()),
                }],
            }],
            output_template: Some("total={passed_total}".to_string()),
        };
        let plugin = SimpleRulePlugin::new(cfg);
        let input = "BEGIN\n12 passed\n8 passed\nEND";
        let slice = Slice {
            id: 2,
            text: Cow::Borrowed(input),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: 4,
            file_metadata: None,
            flags: Default::default(),
        };
        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = Bump::new();
        let out = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
        let text = match &out.tokens[0] {
            Token::Text(s) => s.as_ref(),
            _ => "",
        };
        assert_eq!(text, "total=20");
    }

    /// 测试：match_pattern 只收集匹配行，噪声行被过滤。
    #[test]
    fn match_pattern_filters_collected_lines() {
        let cfg = StaticRuleConfig {
            sections: vec![RuleSection {
                name: "test_failures".to_string(),
                enter: "^FAILURES$".to_string(),
                exit: Some("^$".to_string()),
                match_pattern: Some("^  (test_|FAIL:)".to_string()),
                keep: vec![],
                drop: vec![],
                aggregates: vec![],
            }],
            output_template: Some("{body}".to_string()),
        };
        let plugin = SimpleRulePlugin::new(cfg);
        let input =
            "FAILURES\n  test_foo failed\n  some noise\n  FAIL: test_bar\n  more noise\n\nafter";
        let slice = Slice {
            id: 3,
            text: Cow::Borrowed(input),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: 7,
            file_metadata: None,
            flags: Default::default(),
        };
        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = Bump::new();
        let out = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
        let text = match &out.tokens[0] {
            Token::Text(s) => s.as_ref(),
            _ => "",
        };
        // 只应该收集匹配 match_pattern 的行
        assert!(text.contains("test_foo failed"));
        assert!(text.contains("FAIL: test_bar"));
        assert!(!text.contains("some noise"));
        assert!(!text.contains("more noise"));
    }
}
