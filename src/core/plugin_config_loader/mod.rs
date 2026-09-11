//! 插件配置加载器
//!
//! 从 config/plugins 目录加载 JSON 配置文件

use crate::utils::i18n::{t1, t2};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;

/// P2-38：`parse_vcs_command_words_from_line` 每次调用从磁盘重载全部 route 配置
/// （`load_run_route_capabilities`），被 15+ VCS 插件 compress 热路径反复调用。route
/// 配置在进程内基本不变，用 `OnceLock` 缓存首次加载结果，避免重复磁盘扫描与解析。
static ROUTE_CAPABILITIES_CACHE: OnceLock<Vec<RunRouteCapability>> = OnceLock::new();

const E_PLUGIN_CONFIG_READ: &str = "E_PLUGIN_CONFIG_READ";
const E_PLUGIN_CONFIG_PARSE: &str = "E_PLUGIN_CONFIG_PARSE";

/// 插件配置文件的原始映射结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfigFile {
    /// 插件显示名称
    pub name: String,
    /// 插件功能描述
    #[serde(default)]
    pub description: String,
    /// 优先级（值越小优先级越高）
    pub priority: u8,
    /// 是否启用该插件
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 自动识别配置
    pub detect: DetectConfig,
    /// 压缩处理配置
    pub compress: CompressConfig,
    /// 解压还原配置
    #[serde(default)]
    pub decompress: DecompressConfig,
}

/// 返回插件 enabled 字段的序列化默认值。
/// 当配置 JSON 未显式声明 enabled 时，插件默认处于启用状态。
fn default_enabled() -> bool {
    true
}

/// 内容检测的相关配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectConfig {
    /// 定义该插件可以处理哪些内容的规则列表
    pub rules: Vec<DetectionRule>,
    /// 最小匹配行比例（0.0 ~ 1.0）
    #[serde(default = "default_min_ratio")]
    pub min_match_ratio: f32,
}

/// 返回检测配置 min_match_ratio 字段的默认比例（0.15）。
/// 即匹配行数占样本行数的最低比例，低于该值则视为内容不匹配。
fn default_min_ratio() -> f32 {
    0.15
}

/// 定义如何识别内容是否属于当前插件的可处理范围
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetectionRule {
    /// 包含任意给定的子字符串
    Any { patterns: Vec<String> },
    /// 符合正则表达式
    Regex { pattern: String },
    /// P2-36：未知/非法 `type` 变体兜底。serde `#[serde(other)]` 捕获 tag 中
    /// 无法映射到已知变体的值，避免整文件反序列化失败而拒载——让单个坏规则
    /// 降级为「该规则被忽略」，而非报废整个插件配置。
    #[serde(other)]
    Unknown,
}

/// 核心压缩转换配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressConfig {
    /// 生成 Token 的前缀字符
    #[serde(default)]
    pub token_prefix: String,
    /// 识别路径并转换为 Token 的正则表达式列表
    #[serde(default)]
    pub path_patterns: Vec<String>,
    /// 识别宏并转换为 Token 的正则表达式列表
    #[serde(default)]
    pub macro_patterns: Vec<String>,
    /// 识别类名并转换为 Token 的正则表达式列表
    #[serde(default)]
    pub class_patterns: Vec<String>,
    /// 识别 Gradle 任务并转换为 Token 的正则表达式列表
    #[serde(default)]
    pub gradle_task_patterns: Vec<String>,
    /// 识别资源标识符并转换为 Token 的正则表达式列表
    #[serde(default)]
    pub resource_patterns: Vec<String>,
    /// 去重引擎配置
    #[serde(default)]
    pub dedup: DedupConfig,
}

/// 插件级别的内容去重配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DedupConfig {
    /// 是否针对此插件的内容开启去重逻辑
    #[serde(default = "default_dedup_enabled")]
    pub enabled: bool,
    /// 最小触发重复的阈值
    #[serde(default = "default_threshold")]
    pub threshold: usize,
}

/// 返回去重配置 enabled 字段的默认值。
/// 当插件配置未声明去重开关时，默认开启内容去重逻辑。
fn default_dedup_enabled() -> bool {
    true
}

/// 返回去重触发阈值 threshold 字段的默认值（1）。
/// 即重复内容出现次数达到该阈值时才进行去重替换。
fn default_threshold() -> usize {
    1
}

/// 解压/还原阶段的识别配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecompressConfig {
    /// 标识此插件生成的 Token 前缀
    #[serde(default)]
    pub token_prefixes: Vec<String>,
}

/// 编译后的检测规则（运行时使用）
#[derive(Debug, Clone)]
pub enum CompiledDetectionRule {
    Any { patterns: Vec<String> },
    Regex { pattern: Arc<Regex> },
}

/// 编译后的检测配置
#[derive(Debug, Clone)]
pub struct CompiledDetectConfig {
    pub rules: Vec<CompiledDetectionRule>,
    pub min_match_ratio: f32,
}

/// 编译后的压缩配置
#[derive(Debug, Clone)]
pub struct CompiledCompressConfig {
    pub token_prefix: String,
    pub path_patterns: Vec<Arc<Regex>>,
    pub macro_patterns: Vec<Arc<Regex>>,
    pub class_patterns: Vec<Arc<Regex>>,
    pub gradle_task_patterns: Vec<Arc<Regex>>,
    pub resource_patterns: Vec<Arc<Regex>>,
    pub dedup_enabled: bool,
    pub dedup_threshold: usize,
}

/// 已经过预编译和优化的完整插件配置（供运行时引擎使用）
#[derive(Debug, Clone)]
pub struct CompiledPluginConfig {
    pub name: String,
    pub description: String,
    pub priority: u8,
    pub enabled: bool,
    pub detect: CompiledDetectConfig,
    pub compress: CompiledCompressConfig,
    pub decompress_token_prefixes: Vec<String>,
}

impl CompiledDetectConfig {
    /// 从原始序列化配置转换，并预编译正则表达式。
    pub fn from_config(config: &DetectConfig) -> Self {
        let rules = config
            .rules
            .iter()
            .map(|r| match r {
                DetectionRule::Any { patterns } => CompiledDetectionRule::Any {
                    patterns: patterns.clone(),
                },
                DetectionRule::Regex { pattern } => match Regex::new(pattern) {
                    Ok(re) => CompiledDetectionRule::Regex {
                        pattern: Arc::new(re),
                    },
                    Err(e) => {
                        log::warn!("{}", t2("core_regex_invalid_pattern", pattern, e));
                        CompiledDetectionRule::Any {
                            patterns: vec![pattern.clone()],
                        }
                    }
                },
                // P2-36：未知 type 变体在反序列化阶段已被兜底为 Unknown，编译时直接忽略，
                // 不作为任何检测规则生效。
                DetectionRule::Unknown => CompiledDetectionRule::Any {
                    patterns: Vec::new(),
                },
            })
            .collect();

        CompiledDetectConfig {
            rules,
            min_match_ratio: config.min_match_ratio,
        }
    }

    /// 在给定文本的前 20 行运行检测逻辑，返回匹配度权重（0.0 ~ 1.0）。
    pub fn match_text(&self, text: &str) -> Option<f32> {
        let lines: Vec<&str> = text.lines().take(20).collect();
        if lines.is_empty() {
            return None;
        }

        let mut matched = 0;
        for line in &lines {
            for rule in &self.rules {
                match rule {
                    CompiledDetectionRule::Any { patterns } => {
                        if patterns.iter().any(|p| line.contains(p)) {
                            matched += 1;
                            break;
                        }
                    }
                    CompiledDetectionRule::Regex { pattern } => {
                        if pattern.is_match(line) {
                            matched += 1;
                            break;
                        }
                    }
                }
            }
        }

        let ratio = matched as f32 / lines.len() as f32;
        if ratio > self.min_match_ratio {
            Some(ratio)
        } else {
            None
        }
    }
}

impl CompiledCompressConfig {
    /// 转换并预编译压缩阶段所需的所有正则表达式模式。
    pub fn from_config(config: &CompressConfig) -> Self {
        let compile_pattern = |p: &String| -> Arc<Regex> {
            match Regex::new(p) {
                Ok(re) => Arc::new(re),
                Err(e) => {
                    log::warn!("{}", t2("core_regex_invalid_pattern", p, e));
                    Arc::new(Regex::new("^$").unwrap())
                }
            }
        };

        CompiledCompressConfig {
            token_prefix: config.token_prefix.clone(),
            path_patterns: config.path_patterns.iter().map(compile_pattern).collect(),
            macro_patterns: config.macro_patterns.iter().map(compile_pattern).collect(),
            class_patterns: config.class_patterns.iter().map(compile_pattern).collect(),
            gradle_task_patterns: config
                .gradle_task_patterns
                .iter()
                .map(compile_pattern)
                .collect(),
            resource_patterns: config
                .resource_patterns
                .iter()
                .map(compile_pattern)
                .collect(),
            dedup_enabled: config.dedup.enabled,
            dedup_threshold: config.dedup.threshold,
        }
    }
}

impl CompiledPluginConfig {
    /// 深度转换并编译完整的插件配置。
    pub fn from_config(config: &PluginConfigFile) -> Self {
        CompiledPluginConfig {
            name: config.name.clone(),
            description: config.description.clone(),
            priority: config.priority,
            enabled: config.enabled,
            detect: CompiledDetectConfig::from_config(&config.detect),
            compress: CompiledCompressConfig::from_config(&config.compress),
            decompress_token_prefixes: config.decompress.token_prefixes.clone(),
        }
    }
}

/// 插件配置加载器
pub struct PluginConfigLoader {
    config_dir: PathBuf,
}

impl PluginConfigLoader {
    /// 创建一个新的配置加载器，并自动定位配置目录。
    pub fn new() -> Self {
        let config_dir = Self::find_config_dir();
        Self { config_dir }
    }

    /// 静态辅助方法：尝试在多个标准路径（当前目录、Exe 目录、父目录）下寻找配置文件夹。
    pub fn find_config_dir() -> PathBuf {
        let mut dirs = vec![
            PathBuf::from("./config/plugins"),
            PathBuf::from("config/plugins"),
        ];

        // 添加 exe 目录下的 config/plugins
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                dirs.push(parent.join("config/plugins"));
                // Also probe one level up — the npm-shipped package
                // puts `config/plugins/` at the *package root*
                // (`node_modules/@tokenslim/cli-binary-<plat>/config/`)
                // and only the binary under `bin/`, not a `config/`
                // sibling of `bin/`.  Without this, an `npm install
                // tokenslim-sdk` user (who has no dev tree on disk)
                // gets an empty plugin list and every dispatch falls
                // through to the no-plugin fallback.  The build script
                // (scripts/build-npm-binary-package.mjs §"Stage the
                // plugin configs") is responsible for producing this
                // layout; this branch is the matching probe.
                if let Some(grandparent) = parent.parent() {
                    dirs.push(grandparent.join("config/plugins"));
                }
            }
        }

        // 添加开发环境的 config/plugins
        dirs.push(PathBuf::from("../../config/plugins"));

        for dir in dirs {
            if dir.exists() {
                return dir;
            }
        }

        PathBuf::from("./config/plugins")
    }

    /// 解析配置目录（`config/` 目录本身），作为全库统一路径基准。
    ///
    /// P2-27/P3-57：收敛 feature_reader 特征库、gain 定价配置、path_optimizer
    /// 内置配置等曾各自写死 `config/...` CWD 相对路径的问题——换目录运行时
    /// 会静默失效。本函数复用 `find_config_dir` 的多路径探测结果取父目录，
    /// 与插件配置目录解析保持同一基准；探测失败时回退 `"config"`，与旧
    /// CWD 相对行为一致。
    pub fn resolve_config_dir() -> PathBuf {
        Self::find_config_dir()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("config"))
    }

    /// 判断路径是否为插件配置文件：仅 `.json` 且非路由配置（`*.route.json`）。
    /// 路由配置与插件配置同处 `config/plugins` 目录，但属另一配置族
    /// （`load_run_route_capabilities` 按 `*_route.json`/`*.route.json` 后缀加载），
    /// 插件目录扫描必须跳过，否则每次压缩都会对路由文件误解析并告警。
    fn is_plugin_config_file(path: &Path) -> bool {
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            return false;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        !(name.ends_with("_route.json") || name.ends_with(".route.json"))
    }

    /// 扫描配置目录，返回全部插件配置（含 disabled），按加载顺序排列。
    /// 供 CLI `config plugin status` 展示完整清单（`load_all_configs` 只返回启用项，
    /// 会漏掉被禁用的插件）。
    pub fn load_all_raw_configs(&self) -> Vec<PluginConfigFile> {
        let mut configs = Vec::new();

        if !self.config_dir.exists() {
            log::warn!(
                "{}",
                t1(
                    "core_plugin_config_dir_not_found",
                    format!("{:?}", self.config_dir)
                )
            );
            return configs;
        }

        if let Ok(entries) = fs::read_dir(&self.config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if Self::is_plugin_config_file(&path) {
                    match self.load_config(&path) {
                        Ok(config) => configs.push(config),
                        // P2-36：单文件加载/解析失败显式告警（同 load_all_configs）。
                        Err(e) => {
                            log::warn!("{E_PLUGIN_CONFIG_PARSE}:{:?}:{e}", path);
                        }
                    }
                }
            }
        }

        configs
    }

    /// 扫描配置目录，加载并返回所有已启用的插件配置映射表。
    pub fn load_all_configs(&self) -> HashMap<String, PluginConfigFile> {
        let mut configs = HashMap::new();

        if !self.config_dir.exists() {
            log::warn!(
                "{}",
                t1(
                    "core_plugin_config_dir_not_found",
                    format!("{:?}", self.config_dir)
                )
            );
            return configs;
        }

        if let Ok(entries) = fs::read_dir(&self.config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if Self::is_plugin_config_file(&path) {
                    match self.load_config(&path) {
                        Ok(config) => {
                            if config.enabled {
                                log::debug!("{}", t1("core_plugin_config_loaded", &config.name));
                                configs.insert(config.name.clone(), config);
                            }
                        }
                        // P2-36：单文件加载/解析失败不再静默吞掉（此前 `if let Ok` 统一丢弃），
                        // 显式告警以便定位坏配置，同时不影响其它配置文件加载。
                        Err(e) => {
                            log::warn!("{E_PLUGIN_CONFIG_PARSE}:{:?}:{e}", path);
                        }
                    }
                }
            }
        }

        configs
    }

    /// 从特定 JSON 文件路径加载单个插件配置。
    pub fn load_config(&self, path: &Path) -> Result<PluginConfigFile, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("{E_PLUGIN_CONFIG_READ}:{e}"))?;

        let mut config: PluginConfigFile =
            serde_json::from_str(&content).map_err(|e| format!("{E_PLUGIN_CONFIG_PARSE}:{e}"))?;

        let override_key = format!("plugins.{}.enabled", config.name);
        if let Some(enabled_override) =
            crate::core::config_manager::ConfigManager::get_bool(&override_key)
        {
            config.enabled = enabled_override;
        }

        Ok(config)
    }
}

impl Default for PluginConfigLoader {
    /// Default trait 实现，直接复用 new 构造器创建配置加载器。
    fn default() -> Self {
        Self::new()
    }
}

/// Run 模式路由配置（JSON）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRouteCapability {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_run_route_priority")]
    pub priority: u32,
    pub route: RunRouteConfig,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub workspace_commands: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub run_parse_hints: HashMap<String, RunParseHints>,
    #[serde(default)]
    pub run_tool_aliases: HashMap<String, String>,
    #[serde(default)]
    pub run_intents: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub is_fallback: bool,
}

/// 返回路由能力 priority 字段的默认值（10）。
/// 路由能力按优先级从高到低排序，值越大越优先匹配。
fn default_run_route_priority() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRouteConfig {
    #[serde(default)]
    pub command_keywords: Vec<String>,
    #[serde(default)]
    pub command_regex: Option<String>,
    #[serde(default)]
    pub command_arg_prefixes: Vec<RunRouteArgPrefix>,
    #[serde(default)]
    pub route_group: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRouteArgPrefix {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunParseHints {
    #[serde(default)]
    pub options_with_value: Vec<String>,
    #[serde(default)]
    pub inline_value_prefixes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RunRouteDecision {
    pub plugin_name: String,
    pub route_group: String,
    pub intent: Option<String>,
    pub is_fallback: bool,
    pub command_keyword: String,
    pub matched_by: String,
    pub matched_pattern: Option<String>,
    pub priority: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceCommandCapability {
    pub plugin_name: String,
    pub route_group: String,
    pub skills: Vec<String>,
    pub workspace_commands: HashMap<String, Vec<String>>,
}

/// 将单个路由能力与命令关键字、参数进行匹配，命中时构造运行路由决策。
/// 优先尝试参数前缀匹配（arg_prefix），随后尝试关键字精确匹配与正则匹配；
/// 能力被禁用或为兜底路由时直接返回 None。
fn match_run_route_capability(
    cap: &RunRouteCapability,
    keyword: &str,
    args: &[String],
) -> Option<RunRouteDecision> {
    if !cap.enabled || cap.is_fallback {
        return None;
    }

    if let Some(pattern) = route_arg_prefix_matches(cap, keyword, args) {
        return Some(RunRouteDecision {
            plugin_name: cap.name.clone(),
            route_group: cap.route.route_group.clone(),
            intent: resolve_vcs_intent_for_route(cap, keyword, args),
            is_fallback: false,
            command_keyword: keyword.to_string(),
            matched_by: "arg_prefix".to_string(),
            matched_pattern: Some(pattern),
            priority: Some(cap.priority),
        });
    }

    let mut matched_by = None;
    let matched = if cap
        .route
        .command_keywords
        .iter()
        .any(|kw| kw.as_str() == keyword)
    {
        matched_by = Some(("keyword".to_string(), Some(keyword.to_string())));
        true
    } else if let Some(regex) = &cap.route.command_regex {
        let matched = Regex::new(regex)
            .map(|re| re.is_match(keyword))
            .unwrap_or(false);
        if matched {
            matched_by = Some(("regex".to_string(), Some(regex.clone())));
        }
        matched
    } else {
        false
    };

    if !matched {
        return None;
    }

    let (matched_by, matched_pattern) = matched_by.unwrap_or_else(|| ("unknown".to_string(), None));
    Some(RunRouteDecision {
        plugin_name: cap.name.clone(),
        route_group: cap.route.route_group.clone(),
        intent: resolve_vcs_intent_for_route(cap, keyword, args),
        is_fallback: false,
        command_keyword: keyword.to_string(),
        matched_by,
        matched_pattern,
        priority: Some(cap.priority),
    })
}

/// 返回内置的最小路由能力集合，仅含一个 generic_text 兜底路由。
/// 业务路由规则必须由 config 下的 route.json 提供，此处兜底仅用于
/// 配置缺失或损坏时避免硬失败。
fn builtin_run_route_capabilities() -> Vec<RunRouteCapability> {
    // Keep built-ins minimal: route business rules must come from config/*.route.json.
    // This fallback only prevents hard failures when route config is missing/corrupted.
    vec![RunRouteCapability {
        name: "generic_text".to_string(),
        enabled: true,
        priority: 0,
        route: RunRouteConfig {
            command_keywords: Vec::new(),
            command_regex: None,
            command_arg_prefixes: Vec::new(),
            route_group: "generic".to_string(),
        },
        skills: Vec::new(),
        workspace_commands: HashMap::new(),
        run_parse_hints: HashMap::new(),
        run_tool_aliases: HashMap::new(),
        run_intents: HashMap::new(),
        is_fallback: true,
    }]
}

/// 从程序路径或命令名中提取路由匹配用的关键字。
/// 去掉外层双引号、取文件名部分并转为小写，再剥离 .exe/.cmd/.bat/.com/.ps1 后缀。
fn command_keyword_for_route(prog: &str) -> String {
    let file = std::path::Path::new(prog.trim_matches('"'))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(prog)
        .to_ascii_lowercase();

    for suffix in [".exe", ".cmd", ".bat", ".com", ".ps1"] {
        if let Some(stripped) = file.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }

    file
}

/// 将命令行文本拆分为参数序列，支持单引号、双引号与反斜杠转义。
/// 引号未闭合时返回 None 表示无法解析；双引号内的反斜杠仅在
/// 后随引号或反斜杠时作为转义符，避免误吞 Windows 路径分隔符。
fn parse_command_line_tokens_for_route(line: &str) -> Option<Vec<String>> {
    #[derive(Clone, Copy)]
    enum QuoteMode {
        None,
        Single,
        Double,
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut mode = QuoteMode::None;
    let mut escaped = false;

    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match mode {
            QuoteMode::None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else if ch == '\'' {
                    mode = QuoteMode::Single;
                } else if ch == '"' {
                    mode = QuoteMode::Double;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Single => {
                if ch == '\'' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Double => {
                if escaped {
                    current.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    // Windows 路径中的反斜杠不能被无条件吞掉，仅在转义引号/反斜杠时生效。
                    match chars.peek() {
                        Some('"') | Some('\\') => escaped = true,
                        _ => current.push(ch),
                    }
                } else if ch == '"' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
        }
    }

    match mode {
        QuoteMode::None => {}
        QuoteMode::Single | QuoteMode::Double => return None,
    }

    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Some(tokens)
}

/// 判断索引 idx 之后是否还存在参数，用于带值选项是否应消费下一个参数。
fn has_arg_after(args: &[String], idx: usize) -> bool {
    idx + 1 < args.len()
}

/// 判断参数 arg 是否为该工具声明为"需要独立取值"的选项。
/// 依据 run_parse_hints 中 options_with_value 列表进行大小写不敏感匹配。
fn is_option_with_value_for_tool(cap: &RunRouteCapability, tool_keyword: &str, arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    cap.run_parse_hints.get(tool_keyword).is_some_and(|hints| {
        hints
            .options_with_value
            .iter()
            .any(|opt| lower == opt.to_ascii_lowercase())
    })
}

/// 判断参数 arg 是否以"内联取值"形式携带值，例如 --flag=value。
/// 除 --xxx=yyy 形态外，还匹配 run_parse_hints 中 inline_value_prefixes 声明的前缀。
fn has_inline_option_value_for_tool(
    cap: &RunRouteCapability,
    tool_keyword: &str,
    arg: &str,
) -> bool {
    let lower = arg.to_ascii_lowercase();
    if lower.starts_with("--") && lower.contains('=') {
        return true;
    }
    cap.run_parse_hints.get(tool_keyword).is_some_and(|hints| {
        hints.inline_value_prefixes.iter().any(|prefix| {
            let prefix_lower = prefix.to_ascii_lowercase();
            lower.starts_with(&prefix_lower) && lower.len() > prefix_lower.len()
        })
    })
}

/// 在参数列表中定位第一个子命令的索引位置。
/// 依次跳过分隔符 --、内联取值选项、带值选项及其取值、以及其余短选项，
/// 未找到子命令时返回 None（如裸命令 `git` 无参数的情况）。
fn find_subcommand_index_for_route(
    cap: &RunRouteCapability,
    tool_keyword: &str,
    args: &[String],
) -> Option<usize> {
    let mut idx = 0usize;
    while idx < args.len() {
        let arg = &args[idx];
        if arg == "--" {
            idx += 1;
            break;
        }

        if has_inline_option_value_for_tool(cap, tool_keyword, arg) {
            idx += 1;
            continue;
        }

        if is_option_with_value_for_tool(cap, tool_keyword, arg) {
            idx += if has_arg_after(args, idx) { 2 } else { 1 };
            continue;
        }

        if arg.starts_with('-') {
            idx += 1;
            continue;
        }

        return Some(idx);
    }

    if idx < args.len() {
        Some(idx)
    } else {
        None
    }
}

/// 根据命令关键字与首个子命令解析 VCS 类命令的意图（如 status/log）。
/// 意图映射来自 run_intents 配置，未显式映射的子命令统一归为 other。
fn resolve_vcs_intent_for_route(
    cap: &RunRouteCapability,
    keyword: &str,
    args: &[String],
) -> Option<String> {
    let vcs_entries = cap.run_intents.get(keyword)?;
    let sub_idx = find_subcommand_index_for_route(cap, keyword, args)?;
    let cmd = args[sub_idx].to_ascii_lowercase();
    Some(
        vcs_entries
            .get(&cmd)
            .cloned()
            .unwrap_or_else(|| "other".to_string()),
    )
}

/// 检查路由能力声明的参数前缀是否与当前命令匹配。
/// 当工具关键字一致且子命令之后的前缀参数按序全部匹配时，
/// 返回格式化的匹配描述字符串，否则返回 None。
fn route_arg_prefix_matches(
    cap: &RunRouteCapability,
    keyword: &str,
    args: &[String],
) -> Option<String> {
    for prefix in &cap.route.command_arg_prefixes {
        if !prefix.command.eq_ignore_ascii_case(keyword) || prefix.args.is_empty() {
            continue;
        }

        let Some(sub_idx) = find_subcommand_index_for_route(cap, keyword, args) else {
            continue;
        };
        let remaining = &args[sub_idx..];
        if remaining.len() < prefix.args.len() {
            continue;
        }

        let matches = prefix
            .args
            .iter()
            .zip(remaining.iter())
            .all(|(expected, actual)| expected.eq_ignore_ascii_case(actual));
        if matches {
            return Some(format!("{} {}", prefix.command, prefix.args.join(" ")));
        }
    }
    None
}

/// 从程序名与参数列表解析 VCS 命令词序列。
/// 先筛选出启用且属于 vcs 分组、关键字匹配的路由能力，
/// 再定位首个子命令索引并返回其后的全部参数（转为小写）。
fn parse_vcs_command_words_from_argv_with_caps(
    capabilities: &[RunRouteCapability],
    prog: &str,
    args: &[String],
) -> Option<(String, Vec<String>)> {
    let keyword = command_keyword_for_route(prog);
    let vcs_cap = capabilities.iter().find(|cap| {
        cap.enabled
            && !cap.is_fallback
            && (cap.name == "vcs" || cap.route.route_group == "vcs")
            && cap
                .route
                .command_keywords
                .iter()
                .any(|kw| kw.eq_ignore_ascii_case(&keyword))
    })?;
    let sub_idx = find_subcommand_index_for_route(vcs_cap, &keyword, args)?;
    let words = args[sub_idx..]
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Some((keyword, words))
}

/// 从命令首行解析 VCS 命令词序列（工具名 + 跳过全局参数后的子命令词）。
/// 返回 `(tool_keyword, words)`，其中 `words[0]` 为子命令。
pub fn parse_vcs_command_words_from_line(line: &str) -> Option<(String, Vec<String>)> {
    let tokens = parse_command_line_tokens_for_route(line.trim())?;
    if tokens.is_empty() {
        return None;
    }
    let args = tokens.iter().skip(1).cloned().collect::<Vec<_>>();
    let caps = ROUTE_CAPABILITIES_CACHE.get_or_init(|| {
        let config_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("config")
            .join("plugins");
        load_run_route_capabilities(Some(&config_dir))
    });
    parse_vcs_command_words_from_argv_with_caps(caps, &tokens[0], &args)
}

/// 扫描配置目录下所有 *_route.json / *.route.json 文件并解析路由能力。
/// 按优先级从高到低排序；目录为空或全部解析失败时回退到内置兜底路由，
/// 并在结果中补充缺失的 fallback 能力。
pub fn load_run_route_capabilities(config_dir: Option<&Path>) -> Vec<RunRouteCapability> {
    let mut capabilities = Vec::new();

    let resolved_dir = config_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(PluginConfigLoader::find_config_dir);

    if let Ok(entries) = fs::read_dir(resolved_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_route_json = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with("_route.json") || n.ends_with(".route.json"))
                .unwrap_or(false);
            if !is_route_json {
                continue;
            }
            // P2-37：route 是活链路（15+ VCS 插件依赖）。此前双层 `if let Ok` 吞掉
            // 读取失败与解析失败——坏 route 静默落 builtin 兜底、命令落错分组且无告警。
            // 现对读取/解析失败显式 `log::warn!`，保留降级但让问题可被发现。
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<RunRouteCapability>(&content) {
                    Ok(cap) => {
                        if cap.enabled {
                            capabilities.push(cap);
                        }
                    }
                    Err(e) => {
                        log::warn!("{E_PLUGIN_CONFIG_PARSE}:{:?}:{e}", path);
                    }
                },
                Err(e) => {
                    log::warn!("{E_PLUGIN_CONFIG_READ}:{:?}:{e}", path);
                }
            }
        }
    }

    if capabilities.is_empty() {
        return builtin_run_route_capabilities();
    }

    if !capabilities.iter().any(|c| c.is_fallback) {
        let builtins = builtin_run_route_capabilities();
        if let Some(fallback) = builtins.iter().find(|c| c.is_fallback) {
            capabilities.push(fallback.clone());
        }
    }

    capabilities.sort_by(|a, b| b.priority.cmp(&a.priority));
    capabilities
}

/// 加载各路由能力暴露的 skills 与 workspace_commands，转换为工作区命令能力列表。
/// 仅保留启用、非兜底且至少声明了技能或工作区命令的路由。
pub fn load_workspace_command_capabilities(
    config_dir: Option<&Path>,
) -> Vec<WorkspaceCommandCapability> {
    load_run_route_capabilities(config_dir)
        .into_iter()
        .filter(|cap| cap.enabled && !cap.is_fallback)
        .filter(|cap| !cap.workspace_commands.is_empty() || !cap.skills.is_empty())
        .map(|cap| WorkspaceCommandCapability {
            plugin_name: cap.name,
            route_group: cap.route.route_group,
            skills: cap.skills,
            workspace_commands: cap.workspace_commands,
        })
        .collect()
}

/// 根据命令关键字与参数从路由能力列表中解析最终运行路由决策。
/// 匹配顺序：先全局做参数前缀匹配，再做关键字/正则匹配，
/// 最后回退到 fallback 路由；无任何兜底时使用内置 generic_text。
pub fn resolve_run_route(
    capabilities: &[RunRouteCapability],
    prog: &str,
    args: &[String],
) -> RunRouteDecision {
    let keyword = command_keyword_for_route(prog);

    for cap in capabilities {
        if !cap.enabled || cap.is_fallback {
            continue;
        }

        if let Some(pattern) = route_arg_prefix_matches(cap, &keyword, args) {
            return RunRouteDecision {
                plugin_name: cap.name.clone(),
                route_group: cap.route.route_group.clone(),
                intent: resolve_vcs_intent_for_route(cap, &keyword, args),
                is_fallback: false,
                command_keyword: keyword,
                matched_by: "arg_prefix".to_string(),
                matched_pattern: Some(pattern),
                priority: Some(cap.priority),
            };
        }
    }

    for cap in capabilities {
        if !cap.enabled
            || cap.is_fallback
            || route_arg_prefix_matches(cap, &keyword, args).is_some()
        {
            continue;
        }

        if let Some(decision) = match_run_route_capability(cap, &keyword, args) {
            return decision;
        }
    }

    if let Some(fallback) = capabilities.iter().find(|c| c.is_fallback) {
        return RunRouteDecision {
            plugin_name: fallback.name.clone(),
            route_group: fallback.route.route_group.clone(),
            intent: None,
            is_fallback: true,
            command_keyword: keyword,
            matched_by: "fallback".to_string(),
            matched_pattern: Some(fallback.name.clone()),
            priority: Some(fallback.priority),
        };
    }

    RunRouteDecision {
        plugin_name: "generic_text".to_string(),
        route_group: "generic".to_string(),
        intent: None,
        is_fallback: true,
        command_keyword: keyword,
        matched_by: "builtin_fallback".to_string(),
        matched_pattern: Some("generic_text".to_string()),
        priority: None,
    }
}

/// 枚举给定命令可能命中的所有路由候选（供调试解释用途），并做去重。
/// 先收集参数前缀匹配的候选，再收集关键字/正则匹配的候选，
/// 两者皆空时补充兜底路由。
pub fn explain_run_route_candidates(
    capabilities: &[RunRouteCapability],
    prog: &str,
    args: &[String],
) -> Vec<RunRouteDecision> {
    let keyword = command_keyword_for_route(prog);
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for cap in capabilities {
        if !cap.enabled || cap.is_fallback {
            continue;
        }
        if let Some(pattern) = route_arg_prefix_matches(cap, &keyword, args) {
            let key = format!("{}|arg_prefix|{}", cap.name, pattern);
            if seen.insert(key) {
                out.push(RunRouteDecision {
                    plugin_name: cap.name.clone(),
                    route_group: cap.route.route_group.clone(),
                    intent: resolve_vcs_intent_for_route(cap, &keyword, args),
                    is_fallback: false,
                    command_keyword: keyword.clone(),
                    matched_by: "arg_prefix".to_string(),
                    matched_pattern: Some(pattern),
                    priority: Some(cap.priority),
                });
            }
        }
    }

    for cap in capabilities {
        if !cap.enabled || cap.is_fallback {
            continue;
        }
        if let Some(decision) = match_run_route_capability(cap, &keyword, args) {
            if decision.matched_by == "arg_prefix" {
                continue;
            }
            let key = format!(
                "{}|{}|{}",
                decision.plugin_name,
                decision.matched_by,
                decision.matched_pattern.as_deref().unwrap_or("")
            );
            if seen.insert(key) {
                out.push(decision);
            }
        }
    }

    if out.is_empty() {
        if let Some(fallback) = capabilities.iter().find(|c| c.is_fallback) {
            out.push(RunRouteDecision {
                plugin_name: fallback.name.clone(),
                route_group: fallback.route.route_group.clone(),
                intent: None,
                is_fallback: true,
                command_keyword: keyword,
                matched_by: "fallback".to_string(),
                matched_pattern: Some(fallback.name.clone()),
                priority: Some(fallback.priority),
            });
        }
    }

    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSummary {
    pub name: String,
    pub description: String,
    pub category: String,
    pub skills: Vec<String>,
}

/// 汇总当前可用的插件能力摘要，分为 run_route 路由类与 filter 过滤类。
/// 路由类取命令关键字与参数前缀作为技能列表；过滤类按名称排序，
/// 均以 PluginSummary 形式返回供展示或调试。
pub fn get_all_plugin_capabilities(config_dir: Option<&Path>) -> Vec<PluginSummary> {
    let mut summaries = Vec::new();

    let run_routes = load_run_route_capabilities(config_dir);
    for route in run_routes {
        if route.enabled && !route.is_fallback {
            let mut skills = route.route.command_keywords.clone();
            for prefix in &route.route.command_arg_prefixes {
                if !skills.contains(&prefix.command) {
                    skills.push(prefix.command.clone());
                }
            }
            if skills.is_empty() {
                skills = route.skills.clone();
            }
            summaries.push(PluginSummary {
                name: route.name,
                description: "Run Route Proxy / Interceptor".to_string(),
                category: "run_route".to_string(),
                skills,
            });
        }
    }

    let loader = if let Some(dir) = config_dir {
        PluginConfigLoader {
            config_dir: dir.to_path_buf(),
        }
    } else {
        PluginConfigLoader::new()
    };
    let filter_configs = loader.load_all_configs();
    let mut filters: Vec<_> = filter_configs.into_values().filter(|c| c.enabled).collect();
    filters.sort_by(|a, b| a.name.cmp(&b.name));
    for config in filters {
        summaries.push(PluginSummary {
            name: config.name,
            description: config.description,
            category: "filter".to_string(),
            skills: vec![],
        });
    }

    summaries
}

#[cfg(test)]
mod run_route_tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;

    /// 测试：load_run_route_capabilities 能从临时目录的 route.json 读取路由，
    /// 且 resolve_run_route 能解析出正确的路由分组与意图。
    #[test]
    fn load_run_route_capabilities_reads_route_json() {
        let unique = format!(
            "tokenslim_route_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let route_file = dir.join("vcs_plugin.route.json");
        std::fs::write(
            &route_file,
            r#"{
  "name": "vcs",
  "enabled": true,
  "priority": 100,
  "route": {"command_keywords":["git"],"route_group":"vcs"},
  "run_intents": {"git":{"status":"status"}}
}"#,
        )
        .expect("write route file");

        let caps = load_run_route_capabilities(Some(&dir));
        let route = resolve_run_route(&caps, "git", &["status".to_string()]);
        assert_eq!(route.route_group, "vcs");
        assert_eq!(route.intent.as_deref(), Some("status"));

        let _ = std::fs::remove_file(route_file);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 测试：load_workspace_command_capabilities 能暴露路由配置中的
    /// skills 与 workspace_commands 到工作区命令能力列表。
    #[test]
    fn load_workspace_command_capabilities_exposes_skills_and_commands() {
        let unique = format!(
            "tokenslim_workspace_caps_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        std::fs::write(
            dir.join("build_plugin.route.json"),
            r#"{
  "name": "build",
  "enabled": true,
  "priority": 40,
  "skills": ["build", "test"],
  "workspace_commands": {
    "rust": ["cargo check", "cargo test"]
  },
  "route": {"command_keywords":["cargo"],"route_group":"build"}
}"#,
        )
        .expect("write build route file");

        let caps = load_workspace_command_capabilities(Some(&dir));
        assert_eq!(caps.len(), 1);
        let cap = &caps[0];
        assert_eq!(cap.plugin_name, "build");
        assert!(cap.skills.iter().any(|skill| skill == "build"));
        assert_eq!(
            cap.workspace_commands
                .get("rust")
                .expect("rust workspace commands"),
            &vec!["cargo check".to_string(), "cargo test".to_string()]
        );

        let _ = std::fs::remove_file(dir.join("build_plugin.route.json"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 测试：参数前缀路由优先于关键字路由。同一工具 az 下，
    /// 以 pipelines 开头的命令命中 build 分组，以 repos 开头的命令命中 vcs 分组。
    #[test]
    fn resolve_run_route_prefers_arg_prefix_over_keyword_route() {
        let unique = format!(
            "tokenslim_route_arg_prefix_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        std::fs::write(
            dir.join("vcs_plugin.route.json"),
            r#"{
  "name": "vcs",
  "enabled": true,
  "priority": 100,
  "route": {"command_keywords":["az"],"route_group":"vcs"},
  "run_parse_hints": {"az":{"options_with_value":["--subscription"],"inline_value_prefixes":["--subscription="]}},
  "run_intents": {"az":{"repos":"other"}}
}"#,
        )
        .expect("write vcs route file");
        std::fs::write(
            dir.join("ci_log.route.json"),
            r#"{
  "name": "ci_log",
  "enabled": true,
  "priority": 160,
  "route": {
    "command_arg_prefixes":[{"command":"az","args":["pipelines"]}],
    "route_group":"build"
  },
  "run_parse_hints": {"az":{"options_with_value":["--subscription"],"inline_value_prefixes":["--subscription="]}}
}"#,
        )
        .expect("write ci route file");

        let caps = load_run_route_capabilities(Some(&dir));
        let pipeline_route = resolve_run_route(
            &caps,
            "az",
            &[
                "--subscription".to_string(),
                "sub-001".to_string(),
                "pipelines".to_string(),
                "runs".to_string(),
                "show".to_string(),
            ],
        );
        assert_eq!(pipeline_route.route_group, "build");
        assert_eq!(pipeline_route.plugin_name, "ci_log");
        assert_eq!(pipeline_route.matched_by, "arg_prefix");

        let repos_route = resolve_run_route(
            &caps,
            "az",
            &[
                "--subscription".to_string(),
                "sub-001".to_string(),
                "repos".to_string(),
                "show".to_string(),
            ],
        );
        assert_eq!(repos_route.route_group, "vcs");
        assert_eq!(repos_route.plugin_name, "vcs");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// 测试：解析 git 命令时跳过 -C、--git-dir 等全局选项，
    /// 正确定位子命令 log 并解析出意图。
    #[test]
    fn resolve_run_route_skips_git_global_options_before_subcommand() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_dir = repo_root.join("config").join("plugins");
        let caps = load_run_route_capabilities(Some(&config_dir));

        let route = resolve_run_route(
            &caps,
            "C:\\Program Files\\Git\\cmd\\git.exe",
            &[
                "-C".to_string(),
                "C:\\repo".to_string(),
                "--git-dir=.git".to_string(),
                "log".to_string(),
                "-n".to_string(),
                "2".to_string(),
            ],
        );
        assert_eq!(route.route_group, "vcs");
        assert_eq!(route.intent.as_deref(), Some("log"));
    }

    /// 测试：svn/hg/p4/fossil/cvs/bzr/darcs/repo/gh/glab/az/bitbucket/gerrit
    /// 等 VCS 工具的全局选项均被正确跳过，子命令与意图解析符合预期。
    #[test]
    fn resolve_run_route_skips_other_vcs_global_options_before_subcommand() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_dir = repo_root.join("config").join("plugins");
        let caps = load_run_route_capabilities(Some(&config_dir));

        let svn_route = resolve_run_route(
            &caps,
            "svn",
            &[
                "--config-dir".to_string(),
                "C:\\Users\\alice\\AppData\\Roaming\\Subversion".to_string(),
                "update".to_string(),
            ],
        );
        assert_eq!(svn_route.route_group, "vcs");
        assert_eq!(svn_route.intent.as_deref(), Some("status"));

        let hg_route = resolve_run_route(
            &caps,
            "hg",
            &[
                "--repository".to_string(),
                "C:\\repo".to_string(),
                "log".to_string(),
                "-l".to_string(),
                "5".to_string(),
            ],
        );
        assert_eq!(hg_route.route_group, "vcs");
        assert_eq!(hg_route.intent.as_deref(), Some("log"));

        let p4_route = resolve_run_route(
            &caps,
            "p4",
            &[
                "-p".to_string(),
                "ssl:perforce:1666".to_string(),
                "-u".to_string(),
                "alice".to_string(),
                "describe".to_string(),
                "-s".to_string(),
                "12345".to_string(),
            ],
        );
        assert_eq!(p4_route.route_group, "vcs");
        assert_eq!(p4_route.intent.as_deref(), Some("log"));

        let fossil_route = resolve_run_route(
            &caps,
            "fossil",
            &[
                "--repository".to_string(),
                "C:\\repo\\project.fossil".to_string(),
                "timeline".to_string(),
            ],
        );
        assert_eq!(fossil_route.route_group, "vcs");
        assert_eq!(fossil_route.intent.as_deref(), Some("log"));

        let cvs_route = resolve_run_route(
            &caps,
            "cvs",
            &[
                "-d".to_string(),
                ":pserver:alice@example.com:/cvsroot".to_string(),
                "status".to_string(),
            ],
        );
        assert_eq!(cvs_route.route_group, "vcs");
        assert_eq!(cvs_route.intent.as_deref(), Some("status"));

        let bzr_route = resolve_run_route(
            &caps,
            "bzr",
            &[
                "--directory".to_string(),
                "C:\\repo".to_string(),
                "log".to_string(),
            ],
        );
        assert_eq!(bzr_route.route_group, "vcs");
        assert_eq!(bzr_route.intent.as_deref(), Some("log"));

        let darcs_route = resolve_run_route(
            &caps,
            "darcs",
            &[
                "--repo".to_string(),
                "C:\\repo".to_string(),
                "log".to_string(),
            ],
        );
        assert_eq!(darcs_route.route_group, "vcs");
        assert_eq!(darcs_route.intent.as_deref(), Some("log"));

        let repo_route = resolve_run_route(
            &caps,
            "repo",
            &[
                "--repo-url".to_string(),
                "https://gerrit.googlesource.com/git-repo".to_string(),
                "status".to_string(),
            ],
        );
        assert_eq!(repo_route.route_group, "vcs");
        assert_eq!(repo_route.intent.as_deref(), Some("status"));

        let gh_route = resolve_run_route(
            &caps,
            "gh",
            &[
                "--repo".to_string(),
                "owner/project".to_string(),
                "pr".to_string(),
                "list".to_string(),
            ],
        );
        assert_eq!(gh_route.route_group, "vcs");
        assert_eq!(gh_route.intent.as_deref(), Some("other"));

        let glab_route = resolve_run_route(
            &caps,
            "glab",
            &[
                "--repo".to_string(),
                "group/project".to_string(),
                "mr".to_string(),
                "list".to_string(),
            ],
        );
        assert_eq!(glab_route.route_group, "vcs");
        assert_eq!(glab_route.intent.as_deref(), Some("other"));

        let az_route = resolve_run_route(
            &caps,
            "az",
            &[
                "--subscription".to_string(),
                "sub-001".to_string(),
                "repos".to_string(),
                "list".to_string(),
            ],
        );
        assert_eq!(az_route.route_group, "vcs");
        assert_eq!(az_route.intent.as_deref(), Some("other"));

        let bitbucket_route = resolve_run_route(
            &caps,
            "bitbucket",
            &[
                "--workspace".to_string(),
                "team-a".to_string(),
                "pr".to_string(),
                "list".to_string(),
            ],
        );
        assert_eq!(bitbucket_route.route_group, "vcs");
        assert_eq!(bitbucket_route.intent.as_deref(), Some("other"));

        let gerrit_route = resolve_run_route(
            &caps,
            "gerrit",
            &[
                "--host".to_string(),
                "gerrit.example.com".to_string(),
                "query".to_string(),
                "status:open".to_string(),
            ],
        );
        assert_eq!(gerrit_route.route_group, "vcs");
        assert_eq!(gerrit_route.intent.as_deref(), Some("log"));
    }

    /// 测试辅助函数：返回文本中第一个非空行（去空白后判断），无则返回 None。
    fn first_non_empty_line(text: &str) -> Option<&str> {
        text.lines().find(|line| !line.trim().is_empty())
    }

    /// 测试：遍历 samples/vcs_*_plugin 下的日志样例，断言 vcs 路由配置覆盖了
    /// 所有样例的工具关键字、工具别名族与子命令意图映射。
    #[test]
    fn vcs_route_config_covers_all_sample_command_heads() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_dir = repo_root.join("config").join("plugins");
        let caps = load_run_route_capabilities(Some(&config_dir));
        let vcs_cap = caps
            .iter()
            .find(|cap| cap.name == "vcs")
            .expect("vcs route config should exist");

        let route_keywords: BTreeSet<String> = vcs_cap
            .route
            .command_keywords
            .iter()
            .map(|kw| kw.to_ascii_lowercase())
            .collect();
        let supported_tool_families: BTreeSet<&str> =
            BTreeSet::from(["git", "svn", "hg", "p4", "cvs", "bzr", "fossil", "darcs"]);

        let samples_dir = repo_root.join("samples");
        let mut checked_cases = 0usize;

        let sample_dirs = fs::read_dir(&samples_dir)
            .expect("read samples dir")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("vcs_") && n.ends_with("_plugin"))
            })
            .collect::<Vec<_>>();

        for dir in sample_dirs {
            for file in fs::read_dir(&dir).expect("read vcs sample dir").flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("log") {
                    continue;
                }

                let content = fs::read_to_string(&path)
                    .unwrap_or_else(|_| panic!("read sample failed: {}", path.display()));
                let first = first_non_empty_line(&content)
                    .unwrap_or_else(|| panic!("missing first command line: {}", path.display()));
                // P2-39：契约覆盖断言必须走生产分词器（parse_command_line_tokens_for_route），
                // 不得用行为不同的测试辅助版——否则「测试通过」不代表「生产解析正确」。
                let tokens = parse_command_line_tokens_for_route(first.trim())
                    .unwrap_or_else(|| {
                        panic!(
                            "production tokenizer rejected first command line: {}",
                            path.display()
                        )
                    });
                assert!(
                    !tokens.is_empty(),
                    "empty command tokens in {}",
                    path.display()
                );

                let keyword = command_keyword_for_route(&tokens[0]);
                assert!(
                    route_keywords.contains(&keyword),
                    "tool keyword '{}' from {} is missing in vcs route keywords",
                    keyword,
                    path.display()
                );
                let tool_family = vcs_cap.run_tool_aliases.get(&keyword).unwrap_or_else(|| {
                    panic!(
                        "tool keyword '{}' from {} is missing in run_tool_aliases",
                        keyword,
                        path.display()
                    )
                });
                assert!(
                    supported_tool_families.contains(tool_family.to_ascii_lowercase().as_str()),
                    "tool alias '{} -> {}' from {} is not a supported VCS family",
                    keyword,
                    tool_family,
                    path.display()
                );

                let args = tokens.iter().skip(1).cloned().collect::<Vec<_>>();
                let route = resolve_run_route(&caps, &tokens[0], &args);
                assert_eq!(
                    route.route_group,
                    "vcs",
                    "sample {} should route to vcs",
                    path.display()
                );

                let Some(sub_idx) = find_subcommand_index_for_route(vcs_cap, &keyword, &args)
                else {
                    // 允许“裸命令”样本（如 `git`）只校验路由，不强制校验子命令意图映射。
                    if args.is_empty() {
                        checked_cases += 1;
                        continue;
                    }
                    panic!("subcommand not found in {}", path.display());
                };
                let subcommand = args[sub_idx].to_ascii_lowercase();

                let intents = vcs_cap.run_intents.get(&keyword).unwrap_or_else(|| {
                    panic!(
                        "tool '{}' from {} is missing run_intents mapping",
                        keyword,
                        path.display()
                    )
                });
                assert!(
                    intents.contains_key(&subcommand),
                    "subcommand '{} {}' from {} is missing in run_intents",
                    keyword,
                    subcommand,
                    path.display()
                );

                let expected_intent = intents.get(&subcommand).expect("intent exists");
                assert_eq!(
                    route.intent.as_deref(),
                    Some(expected_intent.as_str()),
                    "sample {} intent mismatch",
                    path.display()
                );

                checked_cases += 1;
            }
        }

        assert!(checked_cases > 0, "no vcs sample cases were checked");
    }

    /// 测试：路由配置目录不存在时，load_run_route_capabilities 仅返回
    /// 内置 generic_text 兜底路由，resolve_run_route 同样回退到该路由。
    #[test]
    fn load_run_route_capabilities_uses_minimal_generic_fallback_when_missing() {
        let unique = format!(
            "tokenslim_route_missing_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let missing_dir = std::env::temp_dir().join(unique);
        // Do not create this directory to emulate missing route config.

        let caps = load_run_route_capabilities(Some(&missing_dir));
        assert_eq!(caps.len(), 1, "fallback should contain only generic route");
        assert_eq!(caps[0].name, "generic_text");
        assert!(caps[0].is_fallback);
        assert_eq!(caps[0].route.route_group, "generic");

        let route = resolve_run_route(&caps, "git", &["status".to_string()]);
        assert_eq!(route.route_group, "generic");
        assert!(route.is_fallback);
    }

    /// 测试：遍历样例日志首行，断言 vcs 路由 JSON 完整覆盖所有工具关键字
    /// 与子命令的意图映射，且所有首行均能被分词并定位子命令。
    #[test]
    fn vcs_route_json_covers_all_sample_first_line_subcommands() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_dir = repo_root.join("config").join("plugins");
        let samples_dir = repo_root.join("samples");

        let capabilities = load_run_route_capabilities(Some(&config_dir));
        let vcs_cap = capabilities
            .iter()
            .find(|cap| cap.name == "vcs" || cap.route.route_group == "vcs")
            .expect("vcs route capability should exist");

        let mut missing_tools = BTreeSet::new();
        let mut missing_subcommands = BTreeSet::new();
        let mut unparsed_cases = BTreeSet::new();

        let entries = std::fs::read_dir(&samples_dir).expect("samples dir should exist");
        for entry in entries.flatten() {
            let dir_path = entry.path();
            let Some(dir_name) = dir_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !dir_name.starts_with("vcs_") || !dir_name.ends_with("_plugin") {
                continue;
            }
            if !dir_path.is_dir() {
                continue;
            }

            let case_files = std::fs::read_dir(&dir_path).expect("sample plugin dir should exist");
            for case_entry in case_files.flatten() {
                let path = case_entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("log") {
                    continue;
                }

                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Some(first_line) = content.lines().find(|l| !l.trim().is_empty()) else {
                    continue;
                };

                let Some(tokens) = parse_command_line_tokens_for_route(first_line.trim()) else {
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("<unknown>");
                    unparsed_cases.insert(format!("tokenize_failed ({file_name})"));
                    continue;
                };
                if tokens.is_empty() {
                    continue;
                }
                let tool_lc = command_keyword_for_route(&tokens[0]);
                let args = tokens.into_iter().skip(1).collect::<Vec<_>>();
                let Some(sub_idx) = find_subcommand_index_for_route(vcs_cap, &tool_lc, &args)
                else {
                    if args.is_empty() {
                        continue;
                    }
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("<unknown>");
                    unparsed_cases.insert(format!("{tool_lc}:subcommand_not_found ({file_name})"));
                    continue;
                };
                let sub_lc = args[sub_idx].to_ascii_lowercase();

                if !vcs_cap
                    .route
                    .command_keywords
                    .iter()
                    .any(|kw| kw.eq_ignore_ascii_case(&tool_lc))
                {
                    missing_tools.insert(tool_lc.clone());
                    continue;
                }

                let Some(sub_map) = vcs_cap.run_intents.get(&tool_lc) else {
                    missing_tools.insert(tool_lc);
                    continue;
                };

                if !sub_map.contains_key(&sub_lc) {
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("<unknown>");
                    missing_subcommands.insert(format!("{tool_lc}:{sub_lc} ({file_name})"));
                }
            }
        }

        assert!(
            missing_tools.is_empty(),
            "vcs route missing tool mappings: {:?}",
            missing_tools
        );
        assert!(
            missing_subcommands.is_empty(),
            "vcs route missing subcommand mappings: {:?}",
            missing_subcommands
        );
        assert!(
            unparsed_cases.is_empty(),
            "sample first-line parsing failed for: {:?}",
            unparsed_cases
        );
    }

    /// 测试：parse_vcs_command_words_from_line 能跳过 git/az 的全局选项，
    /// 正确返回工具关键字与子命令词序列。
    #[test]
    fn parse_vcs_command_words_from_line_skips_global_options() {
        let parsed = parse_vcs_command_words_from_line(
            r#""C:\Program Files\Git\cmd\git.exe" -C "C:\repo with space" --git-dir=.git log -n 2"#,
        )
        .expect("git command should be parsed");
        assert_eq!(parsed.0, "git");
        assert_eq!(parsed.1.first().map(String::as_str), Some("log"));

        let parsed_az = parse_vcs_command_words_from_line(
            r#"az --subscription sub-001 repos list --output table"#,
        )
        .expect("az command should be parsed");
        assert_eq!(parsed_az.0, "az");
        assert_eq!(parsed_az.1.first().map(String::as_str), Some("repos"));
        assert_eq!(parsed_az.1.get(1).map(String::as_str), Some("list"));
    }
}

#[cfg(test)]
mod plugin_config_file_tests {
    use super::*;

    /// 测试：is_plugin_config_file 只识别真正的插件配置 `.json`，
    /// 路由配置（`*.route.json`）与其它扩展名一律跳过——防止目录扫描误解析告警。
    #[test]
    fn plugin_config_file_predicate_filters_route_files() {
        // 合法插件配置：普通 .json 通过
        assert!(PluginConfigLoader::is_plugin_config_file(Path::new(
            "gcc_log.json"
        )));
        // 路由配置两族后缀均拒绝
        assert!(!PluginConfigLoader::is_plugin_config_file(Path::new(
            "vcs_plugin.route.json"
        )));
        assert!(!PluginConfigLoader::is_plugin_config_file(Path::new(
            "ci_log.route.json"
        )));
        // 非 json 扩展名拒绝
        assert!(!PluginConfigLoader::is_plugin_config_file(Path::new(
            "gcc_log.toml"
        )));
        assert!(!PluginConfigLoader::is_plugin_config_file(Path::new(
            "gcc_log.json.bak"
        )));
    }
}
