use serde::{Deserialize, Serialize};

/// 工作区风险分级：Ok（无风险）/Warn（需关注）/Fail（高失败风险）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceRiskLevel {
    Ok,
    Warn,
    Fail,
}

/// 项目信息：主要/次要语言、框架、包管理器、构建/测试命令与版本方言等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub primary: String,
    pub secondary: Vec<String>,
    pub framework: Option<String>,
    pub package_manager: Option<String>,
    pub build: String,
    pub test: String,
    /// 版本方言信息（如 "spring-boot-3"、"c++17"、"python-2.7"）。
    pub dialect: Option<String>,
    /// 由 ORM/迁移文件推断出的数据库类型。
    pub database: Option<String>,
    /// 模块系统（如 "esm"、"cjs"）。
    pub module_system: Option<String>,
}

/// 各语言/工具链探测到的版本号集合（未探测到为 None）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolVersions {
    pub rust: Option<String>,
    pub node: Option<String>,
    pub python: Option<String>,
    pub java: Option<String>,
    pub gcc: Option<String>,
    pub clang: Option<String>,
    pub deno: Option<String>,
    pub msvc: Option<String>,
    pub ninja: Option<String>,
    pub bazel: Option<String>,
    pub make: Option<String>,
    pub cmake: Option<String>,
    pub meson: Option<String>,
    pub julia: Option<String>,
    pub dotnet: Option<String>,
    pub go: Option<String>,
    pub ruby: Option<String>,
    pub php: Option<String>,
    pub swift: Option<String>,
    pub erlang: Option<String>,
    pub fortran: Option<String>,
    pub r_lang: Option<String>,
    pub perl: Option<String>,
    pub lua: Option<String>,
    pub elixir: Option<String>,
    pub haskell: Option<String>,
    pub dart: Option<String>,
    pub scala: Option<String>,
    pub zig: Option<String>,
    pub groovy: Option<String>,
    pub cobol: Option<String>,
}

/// IDE 探测结果：标记当前目录检测到的各类编辑器/IDE。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeInfo {
    pub vscode: bool,
    pub idea: bool,
    pub visual_studio: bool,
    pub xcode: bool,
    pub cursor: bool,
    pub neovim: bool,
    pub eclipse: bool,
    pub sublime: bool,
    pub android_studio: bool,
    pub pycharm: bool,
    pub webstorm: bool,
    pub clion: bool,
    pub goland: bool,
    pub rider: bool,
    pub jupyter: bool,
    pub rstudio: bool,
    pub emacs: bool,
    pub vim: bool,
}

/// 仓库探测结果：git/svn/hg/p4/cvs/bzr/fossil/darcs 等 VCS 是否存在。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub git: bool,
    pub git_branch: Option<String>,
    pub git_dirty: Option<bool>,
    pub svn: bool,
    pub hg: bool,
    pub p4: bool,
    pub cvs: bool,
    pub bzr: bool,
    pub fossil: bool,
    pub darcs: bool,
}

/// 工作区诊断报告全集：风险、环境信号与各探测结构体集合、可执行动作与插件能力。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDoctorReport {
    pub risk: WorkspaceRiskLevel,
    pub encoding_risk: WorkspaceRiskLevel,
    pub os: String,
    pub shell: String,
    pub encoding: String,
    pub project: ProjectInfo,
    pub tools: ToolVersions,
    pub ide: IdeInfo,
    pub repo: RepoInfo,
    pub actions: Vec<String>,
    #[serde(default)]
    pub plugins: Vec<crate::core::plugin_config_loader::PluginSummary>,
}

/// 工作区报告输出格式：Text（纯文本）/Json/LLM 紧凑/JsonMin。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceReportFormat {
    Text,
    Json,
    Llm,
    JsonMin,
}

/// 工作区 LLM 紧凑报告的仓库字段（借用报告生命周期）。
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceLlmRepo<'a> {
    pub v: &'a str,
    pub b: &'a str,
    pub d: Option<bool>,
    pub svn: bool,
    pub hg: bool,
    pub p4: bool,
    pub cvs: bool,
    pub bzr: bool,
    pub fossil: bool,
    pub darcs: bool,
}

/// 工作区 LLM 紧凑报告结构体：以最短字段名承载风险/环境/项目/IDE/仓库/插件摘要。
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceLlmCompact<'a> {
    pub r: &'a str,
    pub enc_risk: &'a str,
    pub os: &'a str,
    pub sh: &'a str,
    pub enc: &'a str,
    pub enc_mixed: bool,
    pub proj: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fwk: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pkg: Option<&'a str>,
    pub ide: Vec<&'a str>,
    pub repo: WorkspaceLlmRepo<'a>,
    pub act: &'a [String],
    pub plugins: Vec<&'a str>,
}
