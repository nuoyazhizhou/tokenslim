use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncodingRiskLevel {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsSignal {
    pub name: String,
    pub version: String,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSignal {
    pub name: String,
    pub raw: String,
    pub host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodepageSignal {
    pub value: Option<String>,
    pub is_utf8: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSignal {
    pub detected: bool,
    pub version: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingDoctorReport {
    pub risk: EncodingRiskLevel,
    pub os: OsSignal,
    pub shell: Option<ShellSignal>,
    pub codepage: Option<CodepageSignal>,
    pub powershell: RuntimeSignal,
    pub python: RuntimeSignal,
    pub node: RuntimeSignal,
    pub jdk: RuntimeSignal,
    pub supported_decoders: Vec<String>,
    pub recommended_expansions: Vec<String>,
    pub repair_strategy_profile: Vec<String>,
    pub repair_confidence_profile: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorReportFormat {
    Text,
    Json,
}
