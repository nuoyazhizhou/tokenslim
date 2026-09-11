//! SARIF and JUnit artifact summary plugin types.

use std::collections::BTreeMap;

/// SARIF / JUnit 产物汇总插件主体：注册到插件分发器，对 SARIF、JUnit 测试报告做语义摘要压缩。
pub struct ArtifactSummaryPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}

#[derive(Debug, Default)]
/// JUnit 测试报告的结构化摘要：套件数、用例数、失败/错误/跳过计数与耗时等聚合指标。
pub(crate) struct JunitSummary {
    pub suites: usize,
    pub tests: usize,
    pub failures: usize,
    pub errors: usize,
    pub skipped: usize,
    pub time: f64,
    pub cases: Vec<JunitCase>,
    pub properties: Vec<(String, String)>,
}

#[derive(Debug)]
/// 单个 JUnit 测试用例的摘要条目：所属套件、名称、类名、状态与耗时。
pub(crate) struct JunitCase {
    pub suite: String,
    pub name: String,
    pub class_name: String,
    pub status: JunitStatus,
    pub message: String,
    pub time: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// JUnit 用例执行状态枚举：通过 / 失败 / 错误 / 跳过。
pub(crate) enum JunitStatus {
    Pass,
    Failure,
    Error,
    Skipped,
}

#[derive(Debug, Default)]
/// SARIF 静态分析结果的聚合摘要：运行数、结果数、按级别（error/warning/note/none）计数、
/// 工具清单与执行状态（invocation 成功/失败及失败命令），用于判定空结果的来源可信度（SAP-0058）。
pub(crate) struct SarifSummary {
    pub runs: usize,
    pub results: usize,
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
    pub none: usize,
    pub tools: Vec<String>,
    pub findings: Vec<SarifFinding>,
    /// Q504 处置:全量结果的规则 ID 计数（不受 findings 前 12 条采样截断影响），供按频次排序。
    pub rule_counts: BTreeMap<String, usize>,
    /// 执行成功的 invocation 数。
    pub exec_ok: usize,
    /// 执行失败的 invocation 数。
    pub exec_failed: usize,
    /// 执行失败 invocation 的命令行（最多保留 3 条）。
    pub failed_cmds: Vec<String>,
}

#[derive(Debug)]
/// SARIF 单条发现（finding）：严重级别、规则 ID、文件、行号与消息。
pub(crate) struct SarifFinding {
    pub level: String,
    pub rule_id: String,
    pub file: String,
    pub line: Option<u64>,
    pub message: String,
}
