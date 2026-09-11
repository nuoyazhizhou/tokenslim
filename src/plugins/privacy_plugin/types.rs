use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

const USER_RULE_FILE: &str = ".tokenslim-redact.toml";

/// 用户词表文件（P3-210）：一行一词，`#` 开头为注释。
///
/// 词表内的词在**整体命中**用户自定义规则时豁免替换（原样保留）；对内置凭证规则
/// **永不生效**。豁免是用户的显式声明「该词不敏感」，责任归属用户——压缩器绝不
/// 按 token 成本自动跳过脱敏（安全 > 节省）。
const ALLOWLIST_FILE: &str = ".tokenslim-redact-allowlist";

/// 内置凭证正则组的进程级单例（不含用户自定义规则文件）。
///
/// P2-79：供其他插件在 `obfuscate_sensitive` 类开关开启时复用同一套凭证正则，
/// 保证脱敏口径单点维护，避免正则组多处复制后各自漂移。
static BUILTIN_REDACTOR: OnceLock<PrivacyPlugin> = OnceLock::new();

/// 以内置凭证正则组脱敏文本（不含用户自定义规则文件）。
///
/// 与 [`PrivacyPlugin::redact_text`] 的内置规则部分完全同口径：凭证替换为
/// 短类型化占位符（`[PK]`/`[AWSID]` 等，P3-209），单向不可逆。每次调用零正则编译开销
/// （OnceLock 复用）。P3-210 词表对内置规则永不生效，故内置单例不受词表影响。
pub(crate) fn redact_with_builtin_patterns(text: &str) -> String {
    BUILTIN_REDACTOR.get_or_init(PrivacyPlugin::base).redact_text(text)
}

/// 在压缩流水线最前端识别并移除高置信度凭证。
///
/// 此插件不持久化原文或占位符映射。其输出可继续交由后续插件压缩，
/// 但绝不能用于恢复真实 secret。
pub struct PrivacyPlugin {
    name: &'static str,
    priority: u8,
    aws_access_key_pattern: Arc<Regex>,
    aws_secret_assignment_pattern: Arc<Regex>,
    github_token_pattern: Arc<Regex>,
    bearer_pattern: Arc<Regex>,
    jwt_pattern: Arc<Regex>,
    private_key_pattern: Arc<Regex>,
    connection_uri_pattern: Arc<Regex>,
    llm_key_assignment_pattern: Arc<Regex>,
    generic_secret_assignment_pattern: Arc<Regex>,
    secret_flag_pattern: Arc<Regex>,
    custom_patterns: Vec<Regex>,
    /// P3-210 词表：整体命中自定义规则时豁免替换的词集合。
    allowlist: HashSet<String>,
}

impl PrivacyPlugin {
    /// 创建使用内置规则和当前目录本地规则文件的隐私插件。
    ///
    /// 同时加载同目录下的词表文件 [`ALLOWLIST_FILE`]（存在时生效；缺失视为空词表）。
    pub fn new() -> Self {
        let mut plugin = Self::from_rule_file(USER_RULE_FILE);
        plugin.allowlist = Self::load_allowlist(Path::new(ALLOWLIST_FILE));
        plugin
    }

    /// 使用指定的本地规则文件创建插件。文件中每个非空、非注释行是一条正则表达式。
    pub fn from_rule_file(path: impl AsRef<Path>) -> Self {
        let mut plugin = Self::base();
        plugin.custom_patterns = Self::load_custom_patterns(path.as_ref());
        plugin
    }

    /// 使用指定的规则文件与词表文件创建插件（P3-210，主要用于嵌入式调用与测试）。
    ///
    /// 词表文件缺失视为空词表（不豁免任何词）；格式见 [`ALLOWLIST_FILE`]。
    pub fn from_rule_file_with_allowlist(
        rule_path: impl AsRef<Path>,
        allowlist_path: impl AsRef<Path>,
    ) -> Self {
        let mut plugin = Self::from_rule_file(rule_path);
        plugin.allowlist = Self::load_allowlist(allowlist_path.as_ref());
        plugin
    }

    /// 设置词表（P3-210，主要用于测试与嵌入式调用）：词表内的词整体命中自定义规则时
    /// 原样保留，不替换为占位符。
    pub fn with_allowlist(mut self, words: impl IntoIterator<Item = String>) -> Self {
        self.allowlist = words.into_iter().collect();
        self
    }

    /// 使用调用方提供的正则表达式创建插件，主要用于嵌入式调用和测试。
    pub fn with_custom_patterns<I, S>(patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut plugin = Self::base();
        plugin.custom_patterns = patterns
            .into_iter()
            .filter_map(|pattern| Regex::new(pattern.as_ref()).ok())
            .collect();
        plugin
    }

    /// 构造不含用户自定义规则的插件骨架：固定 `name`/`priority`，并一次性编译全部内置凭证正则。
    ///
    /// 供 [`Self::from_rule_file`] 与 [`Self::with_custom_patterns`] 复用，二者随后各自填充
    /// `custom_patterns`。此处所有 `expect` 作用于编译期确定的字面量正则，正常构建下不会 panic。
    fn base() -> Self {
        Self {
            name: "privacy",
            // 调度器按更小的 priority 优先选择插件，因此该插件必须为最先执行的 0。
            priority: 0,
            aws_access_key_pattern: Arc::new(
                Regex::new(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b")
                    .expect("invalid AWS access-key regex"),
            ),
            aws_secret_assignment_pattern: Arc::new(
                Regex::new(
                    r"(?im)\b((?:aws_)?secret(?:_access)?_key\s*[=:]\s*)([A-Za-z0-9/+=]{24,})",
                )
                .expect("invalid AWS secret-assignment regex"),
            ),
            github_token_pattern: Arc::new(
                Regex::new(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b")
                    .expect("invalid GitHub-token regex"),
            ),
            bearer_pattern: Arc::new(
                Regex::new(r"(?i)(authorization\s*:\s*bearer\s+)([A-Za-z0-9._~+/=-]{8,})")
                    .expect("invalid bearer-token regex"),
            ),
            jwt_pattern: Arc::new(
                Regex::new(r"\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\b")
                    .expect("invalid JWT regex"),
            ),
            private_key_pattern: Arc::new(
                Regex::new(
                    r"(?s)-----BEGIN (?:[A-Z0-9 ]+ )?PRIVATE KEY-----.*?-----END (?:[A-Z0-9 ]+ )?PRIVATE KEY-----",
                )
                .expect("invalid private-key regex"),
            ),
            connection_uri_pattern: Arc::new(
                Regex::new(
                    r"(?i)\b((?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|mssql)://)[^@\s/]+@",
                )
                .expect("invalid connection-URI regex"),
            ),
            // 字段名规则使用 (?i)；大小写、下划线/连字符变体均不能绕过。
            llm_key_assignment_pattern: Arc::new(
                Regex::new(
                    r#"(?im)((?:["']?)(?:openai|deepseek|anthropic|gemini|google|azure[_-]?openai|groq|mistral|cohere|huggingface|openrouter|xai|replicate)[_-]?(?:api[_-]?key|token)(?:["']?)\s*[:=]\s*["']?)([A-Za-z0-9_-][^\s,"'}\]]*)"#,
                )
                .expect("invalid LLM API-key assignment regex"),
            ),
            generic_secret_assignment_pattern: Arc::new(
                Regex::new(
                    // P2-79 修正：字段名前缀 `(?:[A-Za-z_][A-Za-z0-9_.-]*)?` 改为可选——
                    // 旧式必选前缀会吃掉首字符，导致裸字段名（如 `password=`、`secret=`）
                    // 永远无法命中（首字符被前缀消费后余下部分配不上关键词）。可选前缀
                    // 是旧匹配集合的纯超集，只新增裸字段名命中，不改变既有脱敏行为。
                    r#"(?im)((?:["']?)(?:[A-Za-z_][A-Za-z0-9_.-]*)?(?:api[_-]?key|token|secret|password|client[_-]?secret|private[_-]?key)[A-Za-z0-9_.-]*(?:["']?)\s*[:=]\s*["']?)([A-Za-z0-9_-][^\s,"'}\]]*)"#,
                )
                .expect("invalid generic secret-assignment regex"),
            ),
            secret_flag_pattern: Arc::new(
                Regex::new(
                    r"(?i)(--(?:api[_-]?key|token|secret|password|client[_-]?secret|private[_-]?key)(?:=|\s+))([A-Za-z0-9_-][^\s]+)",
                )
                .expect("invalid secret-flag regex"),
            ),
            custom_patterns: Vec::new(),
            allowlist: HashSet::new(),
        }
    }

    /// 从本地规则文件按「一行一条正则」读取用户自定义脱敏规则。
    ///
    /// 文件不存在或不可读时返回空集（视为未配置，不视为错误）；忽略空行与以 `#` 开头的注释行；
    /// 无法编译的正则会被静默丢弃，不会中断构造流程。
    fn load_custom_patterns(path: &Path) -> Vec<Regex> {
        let Ok(text) = fs::read_to_string(path) else {
            return Vec::new();
        };

        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| Regex::new(line).ok())
            .collect()
    }

    /// 读取词表文件（P3-210）：每个非空、非注释行是一个豁免词。
    ///
    /// 文件不存在或不可读时返回空集（视为未配置，不视为错误）；词按原文精确匹配
    ///（区分大小写、不做子串匹配）。豁免只作用于用户自定义规则的**整体命中**。
    fn load_allowlist(path: &Path) -> HashSet<String> {
        let Ok(text) = fs::read_to_string(path) else {
            return HashSet::new();
        };

        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect()
    }

    /// 将凭证替换为不携带原文、不可回填的类型化占位符。
    ///
    /// P3-209 占位符短化（AGENTS.md「优化第一性原理」第 3 问：机制自身也要过秤）：
    /// 旧 `[TS_*]` 长字面量（≈5~8 token/处）在高频命中场景（用户规则命中用户名/组名等
    /// 重复词）使脱敏产物 token 净膨胀——实测 386B/96tok → 440B/110tok。短字面量
    /// （≈2~3 token/处）信息量与旧形完全等价（同为零：不区分命中哪条规则、不携带原文）。
    /// 图例含义见 [`PLACEHOLDER_LEGEND`]。
    ///
    /// P3-210 词表豁免：自定义规则整体命中词表词时原样保留（见 [`ALLOWLIST_FILE`]），
    /// 消除「替换物比被掩码短词更贵」的残余膨胀；内置凭证规则不受词表影响。
    #[tracing::instrument(level = "debug", skip(self, text), fields(plugin = "privacy"))]
    pub(crate) fn redact_text(&self, text: &str) -> String {
        let text = self.private_key_pattern.replace_all(text, "[PK]");
        let text = self
            .aws_secret_assignment_pattern
            .replace_all(&text, "${1}[AWSKEY]");
        let text = self
            .aws_access_key_pattern
            .replace_all(&text, "[AWSID]");
        let text = self.github_token_pattern.replace_all(&text, "[GHTOK]");
        let text = self
            .bearer_pattern
            .replace_all(&text, "${1}[BEARER]");
        let text = self.jwt_pattern.replace_all(&text, "[JWT]");
        let text = self
            .connection_uri_pattern
            .replace_all(&text, "${1}[DBCRED]@");
        let text = self
            .llm_key_assignment_pattern
            .replace_all(&text, "${1}[LLMKEY]");
        let text = self
            .generic_secret_assignment_pattern
            .replace_all(&text, "${1}[SEC]");
        let text = self
            .secret_flag_pattern
            .replace_all(&text, "${1}[SEC]");

        self.custom_patterns
            .iter()
            .fold(text.into_owned(), |redacted, pattern| {
                pattern
                    .replace_all(&redacted, |caps: &regex::Captures| {
                        let matched = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
                        // P3-210 词表豁免：仅当匹配片段恰好等于词表词时原样保留；
                        // 词表词只作为更大匹配片段的子串出现时照常脱敏（保守侧优先）。
                        if self.allowlist.contains(matched) {
                            matched.to_string()
                        } else {
                            "[UR]".to_string()
                        }
                    })
                    .into_owned()
            })
    }

    /// 判断文本中是否存在会被脱敏的敏感值。
    ///
    /// 实现方式是完整执行一遍 [`Self::redact_text`] 再与原文比较——只要输出发生变化即认为命中。
    /// 好处是探测口径与实际脱敏结果严格一致，不会出现「探测未命中但压缩时被改写」的偏差；
    /// 代价是探测阶段就跑完了全部正则替换。
    fn contains_sensitive_value(&self, text: &str) -> bool {
        self.redact_text(text) != text
    }
}

/// 短占位符 → 类型含义（P3-209 方案 B 图例数据源）。
///
/// 只描述占位符**类型语义**，不含原文、不建立任何还原映射——与 [`PrivacyPlugin`]
/// 的不可逆安全边界一致。顺序即图例输出顺序（按文本中常见度排列便于阅读）。
const PLACEHOLDER_LEGEND: &[(&str, &str)] = &[
    ("[UR]", "user-rule-redaction"),
    ("[SEC]", "secret-redaction"),
    ("[LLMKEY]", "llm-api-key-redaction"),
    ("[AWSKEY]", "aws-secret-key-redaction"),
    ("[AWSID]", "aws-access-key-id-redaction"),
    ("[GHTOK]", "github-token-redaction"),
    ("[BEARER]", "bearer-token-redaction"),
    ("[JWT]", "jwt-redaction"),
    ("[DBCRED]", "db-credential-redaction"),
    ("[PK]", "private-key-redaction"),
];

/// 图例追加的最小占位符总数（P3-209 方案 B 阈值）：低于该值时图例行开销
/// 大于理解收益，不追加（第一性原理第 3 问）。
const LEGEND_MIN_OCCURRENCES: usize = 3;

/// P3-209 方案 B：当短占位符出现总数 ≥ [`LEGEND_MIN_OCCURRENCES`] 时，在输出
/// 尾部追加一行类型图例（如 `[legend] [UR]=user-rule-redaction ... (irreversible)`），
/// 使下游 LLM 无需猜测短占位符含义（第一性原理第 1 问「理解优先」）。
///
/// 图例只描述类型语义：不含原文、不建立占位符→原文映射，不破坏不可逆安全边界。
/// 仅在 `compress` 输出侧调用——`detect` 的探测比较（`redact_text` vs 原文）与
/// sql_plugin 复用路径不受影响。
fn append_legend_if_warranted(redacted: String) -> String {
    let total: usize = PLACEHOLDER_LEGEND
        .iter()
        .map(|(placeholder, _)| redacted.matches(placeholder).count())
        .sum();
    if total < LEGEND_MIN_OCCURRENCES {
        return redacted;
    }
    let mut legend = String::from("[legend]");
    for (placeholder, meaning) in PLACEHOLDER_LEGEND {
        if redacted.contains(placeholder) {
            legend.push_str(&format!(" {placeholder}={meaning}"));
        }
    }
    legend.push_str(" (irreversible)");
    format!("{redacted}\n{legend}")
}

impl Default for PrivacyPlugin {
    /// 委托 [`PrivacyPlugin::new`] 构造实例：内置规则 + 当前工作目录下的本地规则文件。
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for PrivacyPlugin {
    /// 返回插件在调度器与审计产物中的稳定标识 `"privacy"`。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回调度优先级 `0`。调度器按数值升序择优，故本插件始终位于压缩链最前端，
    /// 保证凭证在进入字典、去重等有状态引擎之前就已被移除。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 探测切片是否含凭证：命中返回固定置信度 `1.0`，未命中返回 `None`。
    ///
    /// 置信度取二值而非按命中数量分级——脱敏属于安全兜底，一旦命中就必须无条件抢占该切片，
    /// 不允许其他插件凭更高分数先行处理原始明文。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        self.contains_sensitive_value(slice.text.as_ref())
            .then_some(1.0)
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, slice, _dict_engine, _dedup_engine, _arena),
        fields(plugin = "privacy")
    )]
    /// 将切片文本整体脱敏后作为单个 [`Token::Text`] 输出，并附带不可逆标记。
    ///
    /// 刻意不使用字典引擎、去重引擎与 arena（三个参数均以 `_` 前缀忽略）：占位符本身无需入字典，
    /// 也绝不建立「占位符 → 原文」映射，从物理上断掉还原路径。
    /// P3-209 方案 B：占位符总数 ≥ [`LEGEND_MIN_OCCURRENCES`] 时在尾部追加类型图例
    /// （见 [`append_legend_if_warranted`]，仅描述类型语义，不含原文）。
    /// metadata 写入 `privacy.redacted=true`、`privacy.mode=irreversible`
    /// 以及 `privacy.user_rule_count`（当前生效的用户自定义规则条数）。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let redacted = append_legend_if_warranted(self.redact_text(slice.text.as_ref()));
        let mut metadata = HashMap::new();
        metadata.insert("privacy.redacted".to_string(), "true".to_string());
        metadata.insert("privacy.mode".to_string(), "irreversible".to_string());
        metadata.insert(
            "privacy.user_rule_count".to_string(),
            self.custom_patterns.len().to_string(),
        );

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(redacted))],
            metadata: Some(metadata),
            plugin_name: Some(self.name()),
        }
    }

    /// 解压侧的空操作：原样返回输入，不查字典、不做任何替换。
    ///
    /// 脱敏是单向变换，`[TS_*]` 占位符不携带可还原信息，因此这里既无法也不应尝试恢复原始凭证。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        // 脱敏是单向操作；占位符必须原样保留，不能尝试恢复真实 secret。
        compressed.to_string()
    }
}
