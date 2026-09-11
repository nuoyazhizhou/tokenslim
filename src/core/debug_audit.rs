//! 显式调试模式下的本地 JSONL 压缩审计记录。
//!
//! 审计默认关闭。启用后，每条记录都会在写盘前再次脱敏；模型/路由归因字段
//! 由调用方显式提供，插件执行效果由调度器在本地聚合。

use crate::core::compression::{CompressionMetadata, CompressionOutput};
use crate::core::plugin_dispatcher::{PluginAuditDescriptor, PluginAuditEffect};
use crate::plugins::privacy_plugin::PrivacyPlugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// 审计事件的 JSONL schema 版本号。
///
/// 每当 [`DebugAuditEvent`] 增删字段或改变字段语义时必须递增，
/// 下游解析器据此判断能否安全读取历史审计文件。
pub const DEBUG_AUDIT_SCHEMA_VERSION: u8 = 4;

/// 本次压缩输入的来源，决定审计记录中的 `source_kind` 与是否写入路径字段。
///
/// - [`AuditSource::Text`]：来自内存字符串（如管道/stdin），无路径可记录。
/// - [`AuditSource::File`]：来自磁盘文件，路径会脱敏后写入 `source_path_redacted`。
#[derive(Debug, Clone, Copy)]
pub enum AuditSource<'a> {
    Text,
    File(&'a Path),
}

/// 与具体接入层关联的、可选的非敏感分类信息。
///
/// Provider 身份仅允许使用不可逆标识或粗粒度枚举；调用方不得传入 API key、
/// 原始 endpoint 或账户信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugAuditAttribution {
    pub client_app: Option<String>,
    pub gateway_kind: Option<String>,
    pub endpoint_class: Option<String>,
    pub model_family: Option<String>,
    pub requested_model: Option<String>,
    pub resolved_model: Option<String>,
    pub provider_id_hash: Option<String>,
    pub config_fingerprint: Option<String>,
    pub ruleset_revision: Option<String>,
    pub reviewer_model_id: Option<String>,
}

impl DebugAuditAttribution {
    /// 返回逐字段脱敏后的归因副本，用于写盘前的最后一道防线。
    ///
    /// 绝大多数字段走 [`PrivacyPlugin::redact_text`]（调用方误传凭证时可兜底拦下）；
    /// `provider_id_hash` 例外——它被无条件重新哈希为稳定标识，
    /// 即使调用方违约传入了明文 provider 身份，落盘的也只是哈希值。
    fn redacted(&self, redactor: &PrivacyPlugin) -> Self {
        let redact = |value: &Option<String>| value.as_ref().map(|v| redactor.redact_text(v));
        Self {
            client_app: redact(&self.client_app),
            gateway_kind: redact(&self.gateway_kind),
            endpoint_class: redact(&self.endpoint_class),
            model_family: redact(&self.model_family),
            requested_model: redact(&self.requested_model),
            resolved_model: redact(&self.resolved_model),
            provider_id_hash: self
                .provider_id_hash
                .as_ref()
                .map(|value| stable_identifier_hash(value)),
            config_fingerprint: redact(&self.config_fingerprint),
            ruleset_revision: redact(&self.ruleset_revision),
            reviewer_model_id: redact(&self.reviewer_model_id),
        }
    }
}

/// 把任意标识串折叠为带算法前缀的稳定哈希（形如 `md5:<hex>`）。
///
/// 保留 `md5:` 前缀是为了让审计消费方能识别算法并在将来平滑迁移；
/// 同一输入跨进程、跨版本恒定，因此可用于聚合统计而无需保留原值。
fn stable_identifier_hash(value: &str) -> String {
    format!("md5:{:x}", md5::compute(value.as_bytes()))
}

/// 一条落盘的审计事件，对应 JSONL 文件中的一行。
///
/// 仅实现 `Serialize`（只写不读）：反向解析由外部审计脚本按 `schema_version` 自行处理。
/// 所有可能承载用户数据的字段（`input_redacted`、`source_path_redacted`、
/// `compressed_tokens`、`attribution`）在构造时均已脱敏。
#[derive(Serialize)]
struct DebugAuditEvent {
    schema_version: u8,
    event: &'static str,
    source_kind: &'static str,
    source_path_redacted: Option<String>,
    input_bytes: usize,
    input_redacted: String,
    compressed_tokens: Value,
    compression_metadata: CompressionMetadata,
    attribution: DebugAuditAttribution,
    plugin_chain: Vec<PluginAuditDescriptor>,
    plugin_chain_fingerprint: String,
    plugin_effects: Vec<PluginAuditEffect>,
    /// 插件名 → 回退次数（`parse_tier != full` 的切片数）。用于透视各插件在真实
    /// 流量中"多少走补偿链路、多少走 full"，是判定压缩效果是否被 ROI 门控回退的关键。
    plugin_fallback: HashMap<String, usize>,
    /// 本次压缩窗口内 ANSI 剥离累计删除的字节数。输入含裸码而该值为 0 即为确定 bug。
    ansi_strip_bytes_removed: usize,
    /// 非 generic 插件「真实改变」处理的字节占比（0.0~1.0）。全链 generic 且未改动字节时
    /// 为 0.0——这正是「分类器接线失效」的红灯信号，可跨批量样本统计而不必等个案撞见。
    coverage: f32,
}

/// 追加一条压缩审计记录。
///
/// `audit_path` 必须由用户通过调试开关显式提供。为避免审计文件本身泄露凭证，
/// 本函数会在序列化前对输入、来源路径、归因字符串和压缩 token JSON 再次脱敏。
pub(crate) fn write_compression_event(
    audit_path: &Path,
    source: AuditSource<'_>,
    input: &str,
    output: &CompressionOutput,
    attribution: &DebugAuditAttribution,
    plugin_chain: Vec<PluginAuditDescriptor>,
    plugin_effects: Vec<PluginAuditEffect>,
    plugin_fallback: HashMap<String, usize>,
    ansi_strip_bytes_removed: usize,
) -> io::Result<()> {
    let redactor = PrivacyPlugin::new();
    let input_redacted = redactor.redact_text(input);
    let source_path_redacted = match source {
        AuditSource::Text => None,
        AuditSource::File(path) => Some(redactor.redact_text(&path.to_string_lossy())),
    };
    let token_json = serde_json::to_string(&output.tokens).map_err(json_error)?;
    let token_json_redacted = redactor.redact_text(&token_json);
    let compressed_tokens = serde_json::from_str(&token_json_redacted).map_err(json_error)?;
    let plugin_chain_fingerprint = plugin_chain_fingerprint(&plugin_chain);
    let coverage = coverage_from_effects(&plugin_effects, input.len());
    let event = DebugAuditEvent {
        schema_version: DEBUG_AUDIT_SCHEMA_VERSION,
        event: "compression",
        source_kind: match source {
            AuditSource::Text => "text",
            AuditSource::File(_) => "file",
        },
        source_path_redacted,
        input_bytes: input.len(),
        input_redacted,
        compressed_tokens,
        compression_metadata: audit_metadata_from_tokens(input, &output.tokens, &output.metadata),
        attribution: attribution.redacted(&redactor),
        plugin_chain,
        plugin_chain_fingerprint,
        plugin_effects,
        plugin_fallback,
        ansi_strip_bytes_removed,
        coverage,
    };

    if let Some(parent) = audit_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(audit_path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &event).map_err(json_error)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// 基于原始输入与 token 列表重算审计用的压缩指标。
///
/// 不直接复用流水线 metadata 的体积/Token 字段，而是就地重算——审计需要的是「本次落盘的
/// token 集合」对应的指标，与流水线内部可能带有的中间统计口径解耦。
/// `slice_count`、`processing_time_ms`、`order_info`、`base_timestamp` 无法从 token 反推，
/// 因此原样沿用 `pipeline_metadata`。
/// 空输入时两个比值取 `1.0`，避免除零并表达「未发生压缩」。
fn audit_metadata_from_tokens(
    input: &str,
    tokens: &[crate::core::compression::Token<'static>],
    pipeline_metadata: &CompressionMetadata,
) -> CompressionMetadata {
    let original_size = input.len();
    let compressed_size: usize = tokens.iter().map(|token| token.estimated_size()).sum();
    let original_tokens = original_size / 4;
    let compressed_tokens: usize = tokens.iter().map(|token| token.estimated_tokens()).sum();
    CompressionMetadata {
        original_size,
        compressed_size,
        original_tokens,
        compressed_tokens,
        token_savings: original_tokens.saturating_sub(compressed_tokens),
        compression_ratio: if original_size == 0 {
            1.0
        } else {
            compressed_size as f32 / original_size as f32
        },
        token_ratio: if original_tokens == 0 {
            1.0
        } else {
            compressed_tokens as f32 / original_tokens as f32
        },
        slice_count: pipeline_metadata.slice_count,
        processing_time_ms: pipeline_metadata.processing_time_ms,
        order_info: pipeline_metadata.order_info.clone(),
        base_timestamp: pipeline_metadata.base_timestamp.clone(),
        source_encoding: None,
    }
}
/// 计算插件压缩「覆盖率」：非 generic 插件真实改变了字节的部分占输入的比重（0.0~1.0）。
///
/// 统计口径：累加所有 `changed == true` 且 `plugin_id != "generic_text"` 的插件贡献字节，
/// 除以输入总字节，上限封顶 1.0（避免多插件命中同一片时被重复累加撑爆）。
///
/// 语义：coverage 接近 0.0 意味着这次压缩没有真正落到任何专用插件——正是
/// 「分类器接线失效/ANSI 前缀剥坏」这类 bug 的红灯，跨大量样本统计远胜个案撞见。
/// 空输入或无变更返回 0.0。
fn coverage_from_effects(effects: &[PluginAuditEffect], input_bytes: usize) -> f32 {
    if input_bytes == 0 {
        return 0.0;
    }
    let real_bytes: usize = effects
        .iter()
        .filter(|e| e.changed && e.plugin_id != "generic_text")
        .map(|e| e.input_bytes)
        .sum();
    (real_bytes.min(input_bytes) as f32) / (input_bytes as f32)
}

/// 计算插件链指纹：把 `plugin_id@priority` 按链上顺序用 `|` 拼接后取哈希。
///
/// 指纹对顺序敏感，便于在海量审计记录中按「相同插件链」快速分组，
/// 或在回归时比对某次压缩是否走了预期的插件路径，而无需逐条展开 `plugin_chain`。
fn plugin_chain_fingerprint(chain: &[PluginAuditDescriptor]) -> String {
    let canonical = chain
        .iter()
        .map(|plugin| format!("{}@{}", plugin.plugin_id, plugin.priority))
        .collect::<Vec<_>>()
        .join("|");
    format!("md5:{:x}", md5::compute(canonical.as_bytes()))
}

/// 把 `serde_json` 错误包装为 [`io::Error`]，统一 [`write_compression_event`] 的返回类型。
///
/// 归类为 [`io::ErrorKind::InvalidData`]：序列化失败意味着 token 数据本身不可表达为 JSON，
/// 属于数据问题而非 IO 故障。
fn json_error(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::{
        write_compression_event, AuditSource, DebugAuditAttribution, DEBUG_AUDIT_SCHEMA_VERSION,
    };
    use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
    use crate::core::plugin_dispatcher::{PluginAuditDescriptor, PluginAuditEffect};
    use std::borrow::Cow;
    use std::fs;

    fn output_with_text(text: &str) -> CompressionOutput {
        CompressionOutput {
            tokens: vec![Token::Text(Cow::Owned(text.to_string()))],
            dictionary: Default::default(),
            metadata: CompressionMetadata::default(),
        }
    }

    #[test]
    fn writes_redacted_jsonl_event_with_attribution_and_plugin_effects() {
        let path = std::env::temp_dir().join(format!(
            "tokenslim-debug-audit-{}.jsonl",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let raw_input = "openai_api_key=synthetic_openai_key_123";
        let output = output_with_text("OpenAiApiKey=synthetic_openai_key_456");
        let attribution = DebugAuditAttribution {
            client_app: Some("codex".to_string()),
            gateway_kind: Some("cliproxyapi".to_string()),
            model_family: Some("openai".to_string()),
            requested_model: Some("gpt-test".to_string()),
            provider_id_hash: Some("provider:synthetic_openai_key_789".to_string()),
            ..Default::default()
        };
        write_compression_event(
            &path,
            AuditSource::Text,
            raw_input,
            &output,
            &attribution,
            vec![PluginAuditDescriptor {
                plugin_id: "privacy".to_string(),
                priority: 0,
            }],
            vec![PluginAuditEffect {
                plugin_id: "privacy".to_string(),
                priority: 0,
                invocation_count: 1,
                input_bytes: 42,
                output_estimated_bytes: 20,
                changed: true,
            }],
            std::collections::HashMap::from([("privacy".to_string(), 1)]),
            12,
        )
        .expect("write audit event");

        let content = fs::read_to_string(&path).expect("read audit event");
        let _ = fs::remove_file(&path);
        let event: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSONL");
        assert_eq!(event["schema_version"], DEBUG_AUDIT_SCHEMA_VERSION);
        assert_eq!(event["source_kind"], "text");
        assert!(event["input_redacted"]
            .as_str()
            .expect("redacted input")
            .contains("[LLMKEY]"));
        assert!(event["compressed_tokens"]
            .to_string()
            .contains("[LLMKEY]"));
        assert_eq!(
            event["compression_metadata"]["original_size"],
            raw_input.len()
        );
        assert!(
            event["compression_metadata"]["compressed_size"]
                .as_u64()
                .expect("compressed size")
                > 0
        );
        assert!(
            event["compression_metadata"]["original_tokens"]
                .as_u64()
                .expect("original token estimate")
                > 0
        );
        assert!(
            event["compression_metadata"]["compressed_tokens"]
                .as_u64()
                .expect("compressed token estimate")
                > 0
        );
        assert_eq!(event["attribution"]["client_app"], "codex");
        assert!(event["attribution"]["provider_id_hash"]
            .as_str()
            .expect("provider hash")
            .starts_with("md5:"));
        assert_eq!(event["plugin_chain"][0]["plugin_id"], "privacy");
        assert_eq!(event["plugin_effects"][0]["invocation_count"], 1);
        assert!(event["plugin_chain_fingerprint"]
            .as_str()
            .expect("fingerprint")
            .starts_with("md5:"));
        assert!(!content.contains("synthetic_openai_key"));
    }
}
