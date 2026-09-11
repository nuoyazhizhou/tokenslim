//! 静态规则插件类型定义。

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 静态规则插件的根配置：一组 section 与可选的输出模板。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StaticRuleConfig {
    #[serde(default)]
    pub sections: Vec<RuleSection>,
    pub output_template: Option<String>,
}

/// 单个规则 section：定义进入/退出标记、保留与丢弃规则及聚合器。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleSection {
    pub name: String,
    pub enter: String,
    pub exit: Option<String>,
    /// 在 ACTIVE 状态下，只收集匹配此模式的行（可选）
    #[serde(rename = "match")]
    pub match_pattern: Option<String>,
    #[serde(default)]
    pub keep: Vec<String>,
    #[serde(default)]
    pub drop: Vec<String>,
    #[serde(default)]
    pub aggregates: Vec<AggregateRule>,
}

/// 聚合规则：对匹配行做计数（Count）或求和（Sum），并输出到命名指标。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateRule {
    pub name: String,
    pub kind: AggregateKind,
    pub pattern: Option<String>,
}

/// 聚合器类型：计数（count）或求和（sum）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregateKind {
    Count,
    Sum,
}

/// 编译后的 section：将配置中的正则文本预编译为 Regex 实例，供运行时高效匹配。
#[derive(Default)]
pub struct CompiledSection {
    pub name: String,
    pub enter: Option<Regex>,
    pub exit: Option<Regex>,
    /// 在 ACTIVE 状态下，只收集匹配此模式的行
    pub match_pattern: Option<Regex>,
    pub keep: Vec<Regex>,
    pub drop: Vec<Regex>,
    pub aggregates: Vec<CompiledAggregate>,
}

/// 编译后的聚合器：聚合名称、类型与预编译的匹配正则。
pub struct CompiledAggregate {
    pub name: String,
    pub kind: AggregateKind,
    pub pattern: Option<Regex>,
}

/// 静态规则压缩插件主体，持有配置与预编译的 section 列表。
pub struct SimpleRulePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: StaticRuleConfig,
    pub(crate) compiled_sections: Vec<CompiledSection>,
}

/// 聚合状态：累积各聚合器在压缩过程中产生的数值中间结果。
#[derive(Default)]
pub struct AggregationState {
    pub values: HashMap<String, i64>,
}
