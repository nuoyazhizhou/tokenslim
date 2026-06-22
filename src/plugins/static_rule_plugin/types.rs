use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StaticRuleConfig {
    #[serde(default)]
    pub sections: Vec<RuleSection>,
    pub output_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleSection {
    pub name: String,
    pub enter: String,
    pub exit: Option<String>,
    /// 在 ACTIVE 状态下，只收集匹配此模式的行（可选）
    #[serde(rename = "match")]
    pub match_pattern: Option<String>,
    /// 将收集的行按此分隔符切割为 blocks（可选）
    pub split_on: Option<String>,
    #[serde(default)]
    pub keep: Vec<String>,
    #[serde(default)]
    pub drop: Vec<String>,
    #[serde(default)]
    pub aggregates: Vec<AggregateRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateRule {
    pub name: String,
    pub kind: AggregateKind,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregateKind {
    Count,
    Sum,
}

#[derive(Default)]
pub struct CompiledSection {
    pub name: String,
    pub enter: Option<Regex>,
    pub exit: Option<Regex>,
    /// 在 ACTIVE 状态下，只收集匹配此模式的行
    pub match_pattern: Option<Regex>,
    /// 将收集的行按此分隔符切割为 blocks
    pub split_on: Option<Regex>,
    pub keep: Vec<Regex>,
    pub drop: Vec<Regex>,
    pub aggregates: Vec<CompiledAggregate>,
}

pub struct CompiledAggregate {
    pub name: String,
    pub kind: AggregateKind,
    pub pattern: Option<Regex>,
}

pub struct SimpleRulePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: StaticRuleConfig,
    pub(crate) compiled_sections: Vec<CompiledSection>,
}

#[derive(Default)]
pub struct AggregationState {
    pub values: HashMap<String, i64>,
}
