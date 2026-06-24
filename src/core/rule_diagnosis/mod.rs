//! Static rule diagnosis module
//!
//! Analyzes TOML rule files for:
//! - **Hit rate**: How many rules have valid, matchable patterns
//! - **Conflicts**: Overlapping enter/keep/drop patterns across sections
//! - **Empty rules**: Sections with no usable patterns or invalid regex

use crate::plugins::static_rule_plugin::StaticRuleConfig;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

/// Diagnosis result for a static rule file
#[derive(Debug, Clone, Serialize)]
pub struct RuleDiagnosis {
    pub file: String,
    pub total_sections: usize,
    pub valid_sections: usize,
    pub empty_sections: Vec<String>,
    pub invalid_regex: Vec<RegexError>,
    pub conflicts: Vec<Conflict>,
    pub hit_rate: HitRate,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegexError {
    pub section: String,
    pub field: String,
    pub pattern: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Conflict {
    pub section_a: String,
    pub section_b: String,
    pub kind: ConflictKind,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum ConflictKind {
    /// Two sections have identical enter patterns
    DuplicateEnter,
    /// One section's keep pattern matches another's drop pattern
    KeepDropOverlap,
    /// Two sections have overlapping keep patterns
    KeepOverlap,
}

#[derive(Debug, Clone, Serialize)]
pub struct HitRate {
    pub total_patterns: usize,
    pub valid_patterns: usize,
    pub empty_patterns: usize,
    pub sections_with_aggregates: usize,
    pub sections_without_enter: usize,
}

/// Diagnose a static rule configuration
pub fn diagnose(config: &StaticRuleConfig, file: &str) -> RuleDiagnosis {
    let mut empty_sections = Vec::new();
    let mut invalid_regex = Vec::new();
    let mut conflicts = Vec::new();
    let mut valid_sections = 0usize;
    let mut total_patterns = 0usize;
    let mut valid_patterns = 0usize;
    let mut empty_patterns = 0usize;
    let mut sections_with_aggregates = 0usize;
    let mut sections_without_enter = 0usize;

    // Collect compiled patterns for conflict detection
    let mut enter_patterns: HashMap<String, String> = HashMap::new();
    let mut keep_patterns: HashMap<String, Vec<String>> = HashMap::new();
    let mut drop_patterns: HashMap<String, Vec<String>> = HashMap::new();

    for section in &config.sections {
        let mut section_valid = true;
        let mut section_pattern_count = 0usize;

        // Check enter pattern
        total_patterns += 1;
        if section.enter.is_empty() {
            // Empty enter pattern is technically valid regex but useless for diagnosis
            empty_patterns += 1;
            sections_without_enter += 1;
        } else {
            match Regex::new(&section.enter) {
                Ok(_) => {
                    valid_patterns += 1;
                    section_pattern_count += 1;
                    // Check for duplicate enter patterns
                    if let Some(existing) = enter_patterns.get(&section.enter) {
                        conflicts.push(Conflict {
                            section_a: existing.clone(),
                            section_b: section.name.clone(),
                            kind: ConflictKind::DuplicateEnter,
                            pattern: section.enter.clone(),
                        });
                    } else {
                        enter_patterns.insert(section.enter.clone(), section.name.clone());
                    }
                }
                Err(e) => {
                    invalid_regex.push(RegexError {
                        section: section.name.clone(),
                        field: "enter".to_string(),
                        pattern: section.enter.clone(),
                        error: e.to_string(),
                    });
                    section_valid = false;
                    empty_patterns += 1;
                }
            }
        }

        // Check keep patterns
        for pattern in &section.keep {
            total_patterns += 1;
            section_pattern_count += 1;
            match Regex::new(pattern) {
                Ok(_) => {
                    valid_patterns += 1;
                    keep_patterns
                        .entry(section.name.clone())
                        .or_default()
                        .push(pattern.clone());
                }
                Err(e) => {
                    invalid_regex.push(RegexError {
                        section: section.name.clone(),
                        field: "keep".to_string(),
                        pattern: pattern.clone(),
                        error: e.to_string(),
                    });
                    section_valid = false;
                    empty_patterns += 1;
                }
            }
        }

        // Check drop patterns
        for pattern in &section.drop {
            total_patterns += 1;
            section_pattern_count += 1;
            match Regex::new(pattern) {
                Ok(_) => {
                    valid_patterns += 1;
                    drop_patterns
                        .entry(section.name.clone())
                        .or_default()
                        .push(pattern.clone());
                }
                Err(e) => {
                    invalid_regex.push(RegexError {
                        section: section.name.clone(),
                        field: "drop".to_string(),
                        pattern: pattern.clone(),
                        error: e.to_string(),
                    });
                    section_valid = false;
                    empty_patterns += 1;
                }
            }
        }

        // Check aggregates
        if !section.aggregates.is_empty() {
            sections_with_aggregates += 1;
            for agg in &section.aggregates {
                total_patterns += 1;
                if let Some(pattern) = &agg.pattern {
                    match Regex::new(pattern) {
                        Ok(_) => valid_patterns += 1,
                        Err(e) => {
                            invalid_regex.push(RegexError {
                                section: section.name.clone(),
                                field: format!("aggregate.{}", agg.name),
                                pattern: pattern.clone(),
                                error: e.to_string(),
                            });
                            section_valid = false;
                            empty_patterns += 1;
                        }
                    }
                } else {
                    valid_patterns += 1; // No pattern means match all
                }
            }
        }

        // Check for empty section (no usable patterns)
        if section_pattern_count == 0 && section.aggregates.is_empty() {
            empty_sections.push(section.name.clone());
        }

        if section_valid {
            valid_sections += 1;
        }
    }

    // Detect keep/drop overlaps across sections
    for (section_a, keeps) in &keep_patterns {
        for (section_b, drops) in &drop_patterns {
            if section_a == section_b {
                continue;
            }
            for keep in keeps {
                for drop in drops {
                    if keep == drop {
                        conflicts.push(Conflict {
                            section_a: section_a.clone(),
                            section_b: section_b.clone(),
                            kind: ConflictKind::KeepDropOverlap,
                            pattern: keep.clone(),
                        });
                    }
                }
            }
        }
    }

    // Detect keep overlaps across sections
    let all_sections: Vec<&String> = keep_patterns.keys().collect();
    for i in 0..all_sections.len() {
        for j in (i + 1)..all_sections.len() {
            let section_a = all_sections[i];
            let section_b = all_sections[j];
            for keep_a in &keep_patterns[section_a] {
                for keep_b in &keep_patterns[section_b] {
                    if keep_a == keep_b {
                        conflicts.push(Conflict {
                            section_a: section_a.clone(),
                            section_b: section_b.clone(),
                            kind: ConflictKind::KeepOverlap,
                            pattern: keep_a.clone(),
                        });
                    }
                }
            }
        }
    }

    RuleDiagnosis {
        file: file.to_string(),
        total_sections: config.sections.len(),
        valid_sections,
        empty_sections,
        invalid_regex,
        conflicts,
        hit_rate: HitRate {
            total_patterns,
            valid_patterns,
            empty_patterns,
            sections_with_aggregates,
            sections_without_enter,
        },
    }
}

/// Render diagnosis as human-readable text
pub fn render_diagnosis_text(diagnosis: &RuleDiagnosis) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}\n",
        crate::utils::i18n::t1("doctor_rule_title", &diagnosis.file)
    ));
    out.push_str(&"=".repeat(50));
    out.push_str("\n\n");

    out.push_str(&format!(
        "{}\n",
        crate::utils::i18n::t2(
            "doctor_rule_sections_valid",
            diagnosis.valid_sections,
            diagnosis.total_sections
        )
    ));

    if !diagnosis.empty_sections.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            crate::utils::i18n::t1("doctor_rule_empty", diagnosis.empty_sections.join(", "))
        ));
    }

    if !diagnosis.invalid_regex.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            crate::utils::i18n::t("doctor_rule_invalid_regex")
        ));
        for err in &diagnosis.invalid_regex {
            out.push_str(&format!(
                "  - [{}] {} = `{}`: {}\n",
                err.section, err.field, err.pattern, err.error
            ));
        }
    }

    if !diagnosis.conflicts.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            crate::utils::i18n::t("doctor_rule_conflicts")
        ));
        for conflict in &diagnosis.conflicts {
            let kind_str = match conflict.kind {
                ConflictKind::DuplicateEnter => "duplicate enter",
                ConflictKind::KeepDropOverlap => "keep/drop overlap",
                ConflictKind::KeepOverlap => "keep overlap",
            };
            out.push_str(&format!(
                "  - {} vs {}: {} (`{}`)\n",
                conflict.section_a, conflict.section_b, kind_str, conflict.pattern
            ));
        }
    }

    let hr = &diagnosis.hit_rate;
    let percentage = if hr.total_patterns > 0 {
        hr.valid_patterns as f64 / hr.total_patterns as f64 * 100.0
    } else {
        0.0
    };
    out.push_str(&format!(
        "\n{}\n",
        crate::utils::i18n::t3(
            "doctor_rule_hit_rate",
            hr.valid_patterns,
            hr.total_patterns,
            format!("{:.0}", percentage)
        )
    ));
    out.push_str(&format!(
        "{}\n",
        crate::utils::i18n::t1("doctor_rule_agg_sections", hr.sections_with_aggregates)
    ));
    out.push_str(&format!(
        "{}\n",
        crate::utils::i18n::t1("doctor_rule_no_enter", hr.sections_without_enter)
    ));

    if diagnosis.empty_sections.is_empty()
        && diagnosis.invalid_regex.is_empty()
        && diagnosis.conflicts.is_empty()
    {
        out.push_str(&format!(
            "\n{}\n",
            crate::utils::i18n::t("doctor_rule_no_issues")
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::static_rule_plugin::RuleSection;

    fn make_config(sections: Vec<RuleSection>) -> StaticRuleConfig {
        StaticRuleConfig {
            sections,
            output_template: None,
        }
    }

    fn make_section(name: &str, enter: &str, keep: Vec<&str>, drop: Vec<&str>) -> RuleSection {
        RuleSection {
            name: name.to_string(),
            enter: enter.to_string(),
            exit: None,
            match_pattern: None,
            split_on: None,
            keep: keep.into_iter().map(String::from).collect(),
            drop: drop.into_iter().map(String::from).collect(),
            aggregates: vec![],
        }
    }

    #[test]
    fn diagnosis_detects_valid_rules() {
        let config = make_config(vec![make_section(
            "errors",
            "^ERROR",
            vec!["^ERROR"],
            vec![],
        )]);
        let diagnosis = diagnose(&config, "test.toml");
        assert_eq!(diagnosis.total_sections, 1);
        assert_eq!(diagnosis.valid_sections, 1);
        assert!(diagnosis.empty_sections.is_empty());
        assert!(diagnosis.invalid_regex.is_empty());
        assert!(diagnosis.conflicts.is_empty());
        assert_eq!(diagnosis.hit_rate.valid_patterns, 2); // enter + keep
        assert_eq!(diagnosis.hit_rate.total_patterns, 2);
    }

    #[test]
    fn diagnosis_detects_empty_sections() {
        let config = make_config(vec![RuleSection {
            name: "empty".to_string(),
            enter: "".to_string(),
            exit: None,
            match_pattern: None,
            split_on: None,
            keep: vec![],
            drop: vec![],
            aggregates: vec![],
        }]);
        let diagnosis = diagnose(&config, "test.toml");
        // Empty enter regex "" is valid (matches everything), but section has no keep/drop/aggregates
        // so it's still considered empty (no usable filtering patterns)
        assert_eq!(diagnosis.empty_sections, vec!["empty"]);
        assert_eq!(diagnosis.hit_rate.sections_without_enter, 1);
    }

    #[test]
    fn diagnosis_detects_invalid_regex() {
        let config = make_config(vec![RuleSection {
            name: "bad".to_string(),
            enter: "[invalid".to_string(),
            exit: None,
            match_pattern: None,
            split_on: None,
            keep: vec![],
            drop: vec![],
            aggregates: vec![],
        }]);
        let diagnosis = diagnose(&config, "test.toml");
        assert_eq!(diagnosis.invalid_regex.len(), 1);
        assert_eq!(diagnosis.invalid_regex[0].section, "bad");
        assert_eq!(diagnosis.invalid_regex[0].field, "enter");
    }

    #[test]
    fn diagnosis_detects_duplicate_enter() {
        let config = make_config(vec![
            make_section("errors", "^ERROR", vec![], vec![]),
            make_section("warnings", "^ERROR", vec![], vec![]),
        ]);
        let diagnosis = diagnose(&config, "test.toml");
        assert_eq!(diagnosis.conflicts.len(), 1);
        assert!(matches!(
            diagnosis.conflicts[0].kind,
            ConflictKind::DuplicateEnter
        ));
    }

    #[test]
    fn diagnosis_detects_keep_drop_overlap() {
        let config = make_config(vec![
            make_section("keep_section", "^START", vec!["^ERROR"], vec![]),
            make_section("drop_section", "^BEGIN", vec![], vec!["^ERROR"]),
        ]);
        let diagnosis = diagnose(&config, "test.toml");
        assert_eq!(diagnosis.conflicts.len(), 1);
        assert!(matches!(
            diagnosis.conflicts[0].kind,
            ConflictKind::KeepDropOverlap
        ));
    }

    #[test]
    fn diagnosis_hit_rate_calculation() {
        let config = make_config(vec![
            make_section("a", "^A", vec!["^A1", "^A2"], vec![]),
            make_section("b", "[bad", vec![], vec![]),
        ]);
        let diagnosis = diagnose(&config, "test.toml");
        let hr = &diagnosis.hit_rate;
        // Section a: 1 enter + 2 keep = 3 valid
        // Section b: 1 enter (invalid) = 1 invalid
        assert_eq!(hr.total_patterns, 4);
        assert_eq!(hr.valid_patterns, 3);
        assert_eq!(hr.empty_patterns, 1);
    }

    #[test]
    fn diagnosis_text_rendering() {
        let config = make_config(vec![make_section("ok", "^OK", vec!["^OK"], vec![])]);
        let diagnosis = diagnose(&config, "test.toml");
        let text = render_diagnosis_text(&diagnosis);
        assert!(text.contains(&crate::utils::i18n::t1("doctor_rule_title", "test.toml")));
        assert!(text.contains(&crate::utils::i18n::t2("doctor_rule_sections_valid", 1, 1)));
        assert!(text.contains(crate::utils::i18n::t("doctor_rule_no_issues")));
    }
}
