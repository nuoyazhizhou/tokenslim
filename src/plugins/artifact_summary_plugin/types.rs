//! SARIF and JUnit artifact summary plugin types.

pub struct ArtifactSummaryPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}

#[derive(Debug, Default)]
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
pub(crate) struct JunitCase {
    pub suite: String,
    pub name: String,
    pub class_name: String,
    pub status: JunitStatus,
    pub message: String,
    pub time: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JunitStatus {
    Pass,
    Failure,
    Error,
    Skipped,
}

#[derive(Debug, Default)]
pub(crate) struct SarifSummary {
    pub runs: usize,
    pub results: usize,
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
    pub none: usize,
    pub tools: Vec<String>,
    pub findings: Vec<SarifFinding>,
}

#[derive(Debug)]
pub(crate) struct SarifFinding {
    pub level: String,
    pub rule_id: String,
    pub file: String,
    pub line: Option<u64>,
    pub message: String,
}
