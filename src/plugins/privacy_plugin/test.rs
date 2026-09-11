use super::PrivacyPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::{DedupConfig, DedupEngine};
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::{Slice, SliceType};
use std::borrow::Cow;
use std::fs;

/// 测试样板辅助：将文本包装为 LogBlock Slice（固定 id=1/offset=0/line_start=1，line_end 取行数），供 detect/compress 测试复用
fn make_slice<'a>(text: &'a str) -> Slice<'a> {
    Slice {
        id: 1,
        text: Cow::Borrowed(text),
        slice_type: SliceType::LogBlock,
        offset: 0,
        line_start: 1,
        line_end: text.lines().count().max(1),
        file_metadata: None,
        flags: Default::default(),
    }
}

/// 测试样板辅助：对文本执行 LogBlock 压缩并拼接返回全部 Text token（挂默认 Dictionary/Dedup/Bump 引擎）
fn compress_to_string(plugin: &PrivacyPlugin, text: &str) -> String {
    let slice = make_slice(text);
    let mut dict = DictionaryEngine::new();
    let mut dedup = DedupEngine::new(DedupConfig::default());
    let arena = bumpalo::Bump::new();
    let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);

    result
        .tokens
        .iter()
        .filter_map(|token| match token {
            Token::Text(value) => Some(value.as_ref()),
            _ => None,
        })
        .collect()
}

/// 检测 + 脱敏契约：多类凭据（AWS AccessKey/Secret、Bearer、GitHub、Postgres URL）应被 detect 命中(1.0)并替换为 P3-209 短占位符，原始秘密不得泄漏
#[test]
fn detects_and_redacts_multiple_credential_types() {
    let plugin = PrivacyPlugin::new();
    let raw = concat!(
        "AWS_ACCESS_KEY_ID=AKIAZZZZZZZZZZZZZZZZ\n",
        "AWS_SECRET_ACCESS_KEY=abcdefghijklmnopqrstuvwxyz0123456789+/\n",
        "Authorization: Bearer token.with.a.synthetic.payload\n",
        "github=ghp_abcdefghijklmnopqrstuvwxyz123456\n",
        "db=postgres://db_user:very-secret@db.internal/app\n",
    );

    assert_eq!(plugin.detect(&make_slice(raw)), Some(1.0));
    let redacted = compress_to_string(&plugin, raw);

    assert!(redacted.contains("[AWSID]"));
    assert!(redacted.contains("[AWSKEY]"));
    assert!(redacted.contains("[BEARER]"));
    assert!(redacted.contains("[GHTOK]"));
    assert!(redacted.contains("postgres://[DBCRED]@db.internal/app"));
    assert!(!redacted.contains("AKIAZZZZZZZZZZZZZZZZ"));
    assert!(!redacted.contains("very-secret"));
    // P3-209 方案 B：5 处占位符 ≥ 阈值 3，必须追加类型图例（不含原文）。
    assert!(
        redacted.contains("[legend] [AWSKEY]"),
        "占位符 ≥3 处应追加图例行，实际：{redacted}"
    );
    assert!(redacted.contains("[AWSID]=aws-access-key-id-redaction"));
    assert!(redacted.contains("(irreversible)"));
}

/// 脱敏契约：LLM API Key（openai 大小写/分隔符变体、deepseek、anthropic、CLI `--api-key`）应统一替换为 `[LLMKEY]`/`[SEC]`，原值全部消除
#[test]
fn redacts_llm_api_keys_across_case_and_separator_variants() {
    let plugin = PrivacyPlugin::new();
    let raw = concat!(
        "OPENAI_API_KEY=synthetic_openai_key_123\n",
        "openai_api_key=synthetic_openai_key_456\n",
        "OpenAi_Api_Key: synthetic_openai_key_789\n",
        "OpenAiApi_Key=synthetic_openai_key_abc\n",
        "OpenAi_ApiKey=synthetic_openai_key_def\n",
        "OpenAiApiKey=synthetic_openai_key_ghi\n",
        "oPeNaIaPiKeY=synthetic_openai_key_jkl\n",
        "DEEPSEEK_API_KEY=synthetic_deepseek_key_123\n",
        "anthropic-api-key=synthetic_anthropic_key_123\n",
        "--api-key synthetic_cli_key_123\n",
    );

    let redacted = compress_to_string(&plugin, raw);

    // 正文占位符计数须排除图例行（图例自身含占位符字面量，属预期）。
    let body = redacted
        .lines()
        .filter(|l| !l.starts_with("[legend]"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(body.matches("[LLMKEY]").count(), 9);
    assert!(body.contains("--api-key [SEC]"));
    assert!(redacted.contains("[legend]"), "10 处占位符应追加图例：{redacted}");
    assert!(!redacted.contains("synthetic_openai_key"));
    assert!(!redacted.contains("synthetic_deepseek_key"));
    assert!(!redacted.contains("synthetic_anthropic_key"));
    assert!(!redacted.contains("synthetic_cli_key"));
}

/// 用户规则契约：从本地 TOML 规则文件（`(?i)` 大小写不敏感）加载自定义规则并替换为 `[UR]`
#[test]
fn loads_case_insensitive_user_rules_from_local_file() {
    let path = std::env::temp_dir().join(format!(
        "tokenslim-privacy-rule-{}.toml",
        std::process::id()
    ));
    fs::write(
        &path,
        "# local rule\n(?i)\\bINTERNAL_AGENT_[A-Z0-9]{8}\\b\n",
    )
    .expect("write temporary privacy rule");

    let plugin = PrivacyPlugin::from_rule_file(&path);
    // P2-79 修正后内置通用赋值正则会命中裸 `token=` 赋值并先替换为 [SEC]，
    // 故本测试改用无赋值形态的裸令牌文本，专注验证「用户规则加载 + (?i) 大小写
    // 不敏感」本身（内置规则拦截赋值形态由 sql_plugin p2_79 契约测试覆盖）。
    let redacted = compress_to_string(&plugin, "value internal_agent_abcd1234 here");
    let _ = fs::remove_file(&path);

    assert_eq!(redacted, "value [UR] here");
}

/// 脱敏 + 不可逆契约：PRIVATE KEY 块整体替换为 `[PK]`；decompress 不得还原原始秘密（解压结果等于脱敏结果）
#[test]
fn redacts_private_key_and_never_restores_secret() {
    let plugin = PrivacyPlugin::new();
    let raw = "before\n-----BEGIN PRIVATE KEY-----\nsynthetic-private-key-material\n-----END PRIVATE KEY-----\nafter";
    let redacted = compress_to_string(&plugin, raw);

    assert_eq!(redacted, "before\n[PK]\nafter");
    assert_eq!(
        plugin.decompress(&redacted, &Dictionary::default()),
        redacted
    );
}

/// P3-209 方案 B 阈值契约：占位符恰好 2 处（< 阈值 3）时不得追加图例（图例开销大于理解收益）
#[test]
fn omits_legend_below_occurrence_threshold() {
    let plugin = PrivacyPlugin::new();
    let raw = concat!(
        "OPENAI_API_KEY=synthetic_openai_key_123\n",
        "DEEPSEEK_API_KEY=synthetic_deepseek_key_456\n",
    );
    let redacted = compress_to_string(&plugin, raw);

    assert_eq!(redacted.matches("[LLMKEY]").count(), 2);
    assert!(!redacted.contains("[legend]"), "2 处占位符不追加图例：{redacted}");
}

/// P3-209 安全边界契约：图例只描述类型语义——输出中不得出现「占位符 → 原文」映射（原始秘密只在旧文本中，图例行不得携带）
#[test]
fn legend_describes_types_not_values() {
    let plugin = PrivacyPlugin::new();
    let raw = concat!(
        "AWS_ACCESS_KEY_ID=AKIAZZZZZZZZZZZZZZZZ\n",
        "AWS_SECRET_ACCESS_KEY=abcdefghijklmnopqrstuvwxyz0123456789+/\n",
        "Authorization: Bearer token.with.a.synthetic.payload\n",
    );
    let redacted = compress_to_string(&plugin, raw);

    // 图例行只允许包含「短占位符=类型含义」对与 (irreversible) 标记。
    let legend_line = redacted
        .lines()
        .find(|l| l.starts_with("[legend]"))
        .expect("3 处占位符应产出图例行");
    assert!(legend_line.starts_with("[legend] ["));
    assert!(legend_line.ends_with("(irreversible)"));
    // 图例行内不得出现任何原始秘密片段。
    assert!(!legend_line.contains("AKIA"));
    assert!(!legend_line.contains("abcdefghijklmnopqrstuvwxyz0123456789"));
    assert!(!legend_line.contains("token.with"));
}

/// 反向契约：非敏感文本应 detect 为 None、`redact_text` 原样透传，不阻塞下游插件
#[test]
fn leaves_non_sensitive_text_for_downstream_plugins() {
    let plugin = PrivacyPlugin::new();
    let raw = "error: failed to compile src/main.rs";

    assert_eq!(plugin.detect(&make_slice(raw)), None);
    assert_eq!(plugin.redact_text(raw), raw);
}

/// P3-210 词表豁免契约：词表内的词**整体命中**自定义规则时原样保留（消除「替换物比
/// 被掩码短词更贵」的膨胀）；全部命中被豁免时 detect 同步为 None（探测与改写口径一致）
#[test]
fn allowlist_exempts_exact_user_rule_matches() {
    let plugin = PrivacyPlugin::with_custom_patterns(["wiimu", "synthetic_worker"])
        .with_allowlist(["wiimu".to_string()]);
    let raw = "wiimu wiimu synthetic_worker synthetic_worker";

    // 词表词 wiimu 原样保留；非词表词 synthetic_worker 照常替换。
    assert_eq!(plugin.redact_text(raw), "wiimu wiimu [UR] [UR]");
    // 全部剩余改写被豁免后…此处仍有 synthetic_worker 命中，detect 应为 Some；
    // 纯词表词文本则探测为 None（脱敏探测与实际改写严格一致）。
    assert_eq!(plugin.detect(&make_slice(raw)), Some(1.0));
    assert_eq!(plugin.detect(&make_slice("wiimu wiimu")), None);
}

/// P3-210 保守侧契约：词表词只作为更大匹配片段的**子串**出现时不豁免（照常脱敏）；
/// 词表对**内置凭证规则永不生效**——即便词表里写着整枚 AWS Key 也必须替换
#[test]
fn allowlist_never_exempts_partial_or_builtin_matches() {
    // 子串场景：规则 `wiimu[0-9]+` 命中 "wiimu123"，匹配片段 ≠ 词表词 "wiimu" → 仍替换。
    let partial = PrivacyPlugin::with_custom_patterns(["wiimu[0-9]+"])
        .with_allowlist(["wiimu".to_string()]);
    assert_eq!(partial.redact_text("user wiimu123 ok"), "user [UR] ok");

    // 内置规则场景：词表含整枚 AWS Key，内置规则照样脱敏（安全 > 节省）。
    let builtin =
        PrivacyPlugin::with_custom_patterns(Vec::<String>::new())
            .with_allowlist(["AKIAZZZZZZZZZZZZZZZZ".to_string()]);
    let redacted = builtin.redact_text("key=AKIAZZZZZZZZZZZZZZZZ");
    assert!(redacted.contains("[AWSID]"), "内置规则不受词表影响：{redacted}");
    assert!(!redacted.contains("AKIA"));
}

/// P3-210 词表文件加载契约：`#` 注释行与空行忽略、词按 trim 后精确匹配；
/// 文件缺失视为空词表（不豁免任何词）
#[test]
fn loads_allowlist_file_ignoring_comments_and_blanks() {
    let pid = std::process::id();
    let rule_path = std::env::temp_dir().join(format!("tokenslim-p3210-rule-{pid}.toml"));
    let allow_path = std::env::temp_dir().join(format!("tokenslim-p3210-allow-{pid}.txt"));
    fs::write(&rule_path, "\\b(?:wiimu|synthetic_worker)\\b\n").expect("write rule file");
    fs::write(
        &allow_path,
        "# allowlist\n\n  wiimu  \n# not-a-word\n",
    )
    .expect("write allowlist file");

    let plugin =
        PrivacyPlugin::from_rule_file_with_allowlist(&rule_path, &allow_path);
    let redacted = plugin.redact_text("wiimu synthetic_worker wiimu");
    let _ = fs::remove_file(&rule_path);
    let _ = fs::remove_file(&allow_path);

    assert_eq!(redacted, "wiimu [UR] wiimu");
}
