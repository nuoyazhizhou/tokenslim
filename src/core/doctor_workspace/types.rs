use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceRiskLevel {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub primary: String,
    pub secondary: Vec<String>,
    pub framework: Option<String>,
    pub package_manager: Option<String>,
    pub build: String,
    pub test: String,
    /// Version dialect info (e.g. "spring-boot-3", "c++17", "python-2.7")
    pub dialect: Option<String>,
    /// Database type inferred from ORM/migration files
    pub database: Option<String>,
    /// Module system (e.g. "esm", "cjs")
    pub module_system: Option<String>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceReportFormat {
    Text,
    Json,
    Llm,
    JsonMin,
}

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
