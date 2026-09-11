use serde::{Deserialize, Serialize};

/// 编码风险分级：Ok（无风险）/Warn（需关注）/Fail（高失败风险）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncodingRiskLevel {
    Ok,
    Warn,
    Fail,
}

/// 操作系统信号：名称、版本号与区域设置（locale）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsSignal {
    pub name: String,
    pub version: String,
    pub locale: Option<String>,
}

/// Shell 信号：识别出的 shell 名称、原始证据字符串与宿主环境（host）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSignal {
    pub name: String,
    pub raw: String,
    pub host: Option<String>,
}

/// 代码页信号：代码页数值与是否为 UTF-8。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodepageSignal {
    pub value: Option<String>,
    pub is_utf8: Option<bool>,
}

/// 运行时信号：是否探测到、版本信息与附加说明（如 file.encoding）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSignal {
    pub detected: bool,
    pub version: Option<String>,
    pub note: Option<String>,
}

/// 编码诊断报告全集：风险级别、各信号体与解码能力/修复策略/建议清单。
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

/// 诊断报告输出格式：纯文本（Text）或 JSON。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorReportFormat {
    Text,
    Json,
}
