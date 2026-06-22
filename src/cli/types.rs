//! cli 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 cli 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

use crate::core::compression_pipeline::PipelineError;
use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookShell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

impl HookShell {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            "powershell" | "pwsh" | "ps" => Some(Self::PowerShell),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
        }
    }
}

/// 原始命令行参数 (用于 clap 解析)
#[derive(Parser, Debug)]
#[clap(name = "tokenslim", version = "0.1.0", author = "TokenSlim Team")]
pub struct CliRawArgs {
    /// 运行模式: compress 或 decompress
    #[clap(short, long)]
    pub mode: Option<String>,

    /// 输入文件路径 (默认 stdin)
    #[clap(short, long)]
    pub input: Option<PathBuf>,

    /// 输出文件路径 (默认 stdout)
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// 是否显示详细日志
    #[clap(short, long)]
    pub verbose: bool,

    /// 是否计算 Token 数量
    #[clap(long)]
    pub calc_tokens: bool,

    /// 是否开启日志重排序
    #[clap(long)]
    pub reorder: bool,

    /// 是否开启语义增强
    #[clap(long)]
    pub semantic: bool,

    /// 是否开启归一化
    #[clap(long)]
    pub normalize: bool,

    /// 输出格式 (json, markdown, text)
    #[clap(short, long, default_value = "json")]
    pub format: String,

    /// 配置文件路径
    #[clap(short, long)]
    pub config: Option<PathBuf>,

    /// 为 AI 优化的导出模式 (仅解压)
    #[clap(long)]
    pub ai_export: bool,

    /// AI 信号模式：有损但保留故障现场与上下文（仅解压）
    #[clap(long)]
    pub ai_signal: bool,

    /// 验证静态规则插件的 TOML 文件路径
    #[clap(long)]
    pub verify_rule: Option<PathBuf>,

    /// 验证输入样例（fixture）路径
    #[clap(long)]
    pub verify_fixture: Option<PathBuf>,

    /// 验证期望输出（expected）路径
    #[clap(long)]
    pub verify_expected: Option<PathBuf>,

    /// 安装 shell hook（零侵入集成）
    #[clap(long)]
    pub init_hooks: bool,

    /// 卸载 shell hook
    #[clap(long)]
    pub uninstall_hooks: bool,

    /// 指定 shell 类型（bash/zsh/fish），默认自动探测
    #[clap(long)]
    pub hook_shell: Option<String>,
    /// 仅打印计划变更，不写文件
    #[clap(long)]
    pub dry_run: bool,

    /// 初始化项目配置（生成 .tokenslim.toml）
    #[clap(long)]
    pub init: bool,

    /// 初始化时不安装 shell hooks
    #[clap(long)]
    pub no_hooks: bool,

    /// 强制覆盖已存在的配置文件
    #[clap(long)]
    pub force: bool,

    /// 显示全局累计节省的 Token 统计 (类似 rtk gain)
    #[clap(long)]
    pub gain: bool,
    /// 按日显示 gain 明细（需配合 --gain）
    #[clap(long)]
    pub gain_daily: bool,
    /// 按过滤器显示 gain 明细（需配合 --gain）
    #[clap(long)]
    pub gain_by_filter: bool,
    /// gain 输出 JSON（需配合 --gain）
    #[clap(long)]
    pub gain_json: bool,
    /// gain 按日统计范围（天）
    #[clap(long, default_value_t = 7)]
    pub gain_days: i64,

    /// 执行诊断 (doctor) 功能，例如: encoding
    #[clap(long)]
    pub doctor: Option<String>,

    /// 诊断输出格式: text|json|llm|json-min
    #[clap(long, default_value = "text")]
    pub doctor_format: String,

    /// 严格模式：信息缺失时提高风险等级
    #[clap(long)]
    pub strict: bool,

    /// 注入工作区上下文到 .tokenslim-context.md 文件
    #[clap(long)]
    pub inject: bool,

    /// 生成可执行的修复命令（不自动执行，仅输出建议）
    #[clap(long)]
    pub fix: bool,
    /// 在 verify 阶段启用安全检查
    #[clap(long)]
    pub safety: bool,

    /// 重写命令（测试命令重写引擎）
    #[clap(long)]
    pub rewrite: Option<String>,

    /// 发现缺失的过滤器（扫描 session 文件）
    #[clap(long)]
    pub discover: Vec<PathBuf>,

    /// 包装执行外部命令并压缩其输出 (配合 -- 用法，如 tokenslim --run -- cargo test)
    #[clap(last = true)]
    pub run_command: Vec<String>,

    /// 解释 run 路由选择，不执行外部命令
    #[clap(long)]
    pub explain_route: bool,
    #[clap(long)]
    pub explain_command: Option<String>,
    #[clap(long, default_value_t = 0.15)]
    pub explain_fallback_gap: f32,
    #[clap(long)]
    pub explain_replay_out: Option<PathBuf>,

    /// 预设配置: fast (速度优先), balanced (均衡), ai (AI 信号优先)
    #[clap(long)]
    pub preset: Option<String>,

    /// repair-file: 原地覆盖输入文件
    #[clap(long)]
    pub inplace: bool,

    /// repair-file: 原地覆盖前生成 .bak 备份
    #[clap(long)]
    pub backup: bool,

    /// repair-file: 仅处理匹配模式的文件（可重复）
    #[clap(long)]
    pub include: Vec<String>,

    /// repair-file: 排除匹配模式的文件（可重复）
    #[clap(long)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorKind {
    Encoding,
    Workspace,
    Rule,
    Env,
}

impl std::str::FromStr for DoctorKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "encoding" => Ok(Self::Encoding),
            "workspace" => Ok(Self::Workspace),
            "rule" => Ok(Self::Rule),
            "env" => Ok(Self::Env),
            _ => Err(format!("unsupported doctor kind: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorOutputFormat {
    Text,
    Json,
    Llm,
    JsonMin,
}

impl std::str::FromStr for DoctorOutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "text" | "txt" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "llm" => Ok(Self::Llm),
            "json-min" | "jsonmin" => Ok(Self::JsonMin),
            _ => Err(format!("unsupported doctor format: {s}")),
        }
    }
}

/// 输出格式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFormat {
    Json,     // 标准 JSON 格式，包含完整元数据
    Markdown, // AI 友好格式，包含 YAML 字典摘要
    Text,     // 纯文本格式（仅 Payload）
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "md" | "markdown" => Ok(OutputFormat::Markdown),
            "text" | "txt" => Ok(OutputFormat::Text),
            _ => Err(format!("Unknown format: {}", s)),
        }
    }
}

/// 命令行参数结构体
#[derive(Debug)]
pub struct CliArgs {
    pub mode: CliMode,
    pub input: InputSource,
    pub output: OutputTarget,
    pub verbose: bool,
    pub calc_tokens: bool,
    pub reorder: bool,
    pub semantic: bool,
    pub normalize: bool,
    pub ai_export: bool,
    pub ai_signal: bool,
    pub output_format: OutputFormat,
    pub verify_rule: Option<PathBuf>,
    pub verify_fixture: Option<PathBuf>,
    pub verify_expected: Option<PathBuf>,
    pub init_hooks: bool,
    pub uninstall_hooks: bool,
    pub hook_shell: Option<HookShell>,
    pub dry_run: bool,
    pub init: bool,
    pub no_hooks: bool,
    pub force: bool,
    pub gain: bool,
    pub gain_daily: bool,
    pub gain_by_filter: bool,
    pub gain_json: bool,
    pub gain_days: i64,
    pub doctor: Option<DoctorKind>,
    pub doctor_format: DoctorOutputFormat,
    pub doctor_strict: bool,
    pub inject: bool,
    pub config: Option<PathBuf>,
    pub run_command: Vec<String>,
    pub explain_route: bool,
    pub explain_command: Option<String>,
    pub explain_fallback_gap: f32,
    pub explain_replay_out: Option<PathBuf>,
    pub preset: Option<Preset>,
    pub fix: bool,
    pub safety: bool,
    pub rewrite: Option<String>,
    pub discover: Vec<PathBuf>,
    pub inplace: bool,
    pub backup: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// 预设配置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Fast,
    Balanced,
    Ai,
}

impl std::str::FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fast" => Ok(Self::Fast),
            "balanced" => Ok(Self::Balanced),
            "ai" => Ok(Self::Ai),
            _ => Err(format!(
                "unsupported preset: {s} (expected fast|balanced|ai)"
            )),
        }
    }
}

/// 操作模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliMode {
    Compress,
    Decompress,
    Init,
    Run,
    HooksStatus,
    ExplainPlugin,
    Plugins,
    RepairFile,
}

/// 输入源
#[derive(Debug)]
pub enum InputSource {
    File(PathBuf),
    Stdin,
}

/// 输出目标
#[derive(Debug)]
pub enum OutputTarget {
    File(PathBuf),
    Stdout,
}

/// 命令行错误类型
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    InvalidArgs(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Compression(String),
    #[error("{0}")]
    Decompression(String),
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Pipeline(#[from] PipelineError),
    #[error("{0}")]
    Serialization(#[from] serde_json::Error),
}
