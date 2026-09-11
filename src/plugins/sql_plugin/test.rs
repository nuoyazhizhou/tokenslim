//! sql_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::sql_plugin::types::SqlPlugin;
    use crate::plugins::test_utils::*;

    /// 测试：SELECT 样例被识别。
    #[test]
    fn detects_select_sample() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_001_select");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Line));
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    /// 测试：事务样例被识别。
    #[test]
    fn detects_transaction_sample() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_011_transaction");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Line))
            .is_some());
    }

    /// 测试：复杂样例压缩后不扩张。
    #[test]
    fn compresses_complex_sample_without_expansion() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_003_complex");
        let out = compress_to_string(&plugin, &raw, SliceType::Line);
        assert!(
            out.len() <= raw.len() + 16,
            "sql 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：复杂 JOIN 样例压缩后不扩张。
    #[test]
    fn compresses_complex_join_sample_without_expansion() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_012_complex_join");
        let out = compress_to_string(&plugin, &raw, SliceType::Line);
        assert!(
            out.len() <= raw.len() + 16,
            "sql 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// P2-79 契约：默认配置（obfuscate_sensitive=false）下不得静默脱敏——
    /// 关闭骨架提取以隔离验证：敏感值原样保留、无脱敏占位符产出
    /// （默认骨架路径下字面量本就会被 '?' 抹除，见既有 case_003 契约）。
    #[test]
    fn p2_79_default_config_keeps_sensitive_values_verbatim() {
        let plugin = SqlPlugin {
            config: crate::plugins::sql_plugin::types::SqlConfig {
                extract_skeleton: false,
                ..crate::plugins::sql_plugin::types::SqlConfig::default()
            },
            ..SqlPlugin::new()
        };
        let raw = read_sample_log("sql_plugin", "obfuscate_sensitive_case01");
        let out = compress_to_string(&plugin, &raw, SliceType::Line);
        assert!(
            out.contains("S3cr3tPassw0rd!"),
            "默认配置必须保留原文（不得静默脱敏）"
        );
        assert!(
            !out.contains("[TS_") && !out.contains("[SEC]") && !out.contains("[DBCRED]"),
            "默认配置不得产出脱敏占位符（P3-209 短化后新旧两族都须缺席）"
        );
    }

    /// P2-79 契约：obfuscate_sensitive=true 时真实脱敏生效（纯脱敏路径，骨架关闭）——
    /// password 赋值、postgres 连接串凭证与 Bearer JWT 的值必须被替换为
    /// 脱敏不可逆占位符（P3-209 短化：[SEC]/[DBCRED]/[BEARER]）。
    #[test]
    fn p2_79_obfuscate_sensitive_redacts_credentials() {
        let plugin = SqlPlugin {
            config: crate::plugins::sql_plugin::types::SqlConfig {
                obfuscate_sensitive: true,
                extract_skeleton: false,
                ..crate::plugins::sql_plugin::types::SqlConfig::default()
            },
            ..SqlPlugin::new()
        };
        let raw = read_sample_log("sql_plugin", "obfuscate_sensitive_case01");
        let out = compress_to_string(&plugin, &raw, SliceType::Line);

        assert!(
            !out.contains("S3cr3tPassw0rd!"),
            "password 赋值明文必须被脱敏"
        );
        assert!(
            !out.contains("P@ssw0rd-2026"),
            "连接串凭证明文必须被脱敏"
        );
        assert!(
            !out.contains("eyJhbGciOiJIUzI1NiJ9"),
            "JWT 明文必须被脱敏"
        );
        assert!(out.contains("[SEC]"), "password 赋值应替换为 [SEC]");
        assert!(
            out.contains("[DBCRED]@"),
            "连接串凭证应替换为 [DBCRED]@"
        );
        assert!(out.contains("[BEARER]"), "Bearer 令牌应替换为 [BEARER]");
    }

    /// P2-79 组合语义：obfuscate_sensitive=true 与默认骨架提取并存时——
    /// 引号字面量中的敏感值经「先脱敏后骨架化」不会残留明文（占位符被骨架
    /// 抹为 '?' 属预期），非字面量凭证（连接串/Bearer）的占位符存活。
    #[test]
    fn p2_79_redaction_composes_with_skeleton() {
        let plugin = SqlPlugin {
            config: crate::plugins::sql_plugin::types::SqlConfig {
                obfuscate_sensitive: true,
                ..crate::plugins::sql_plugin::types::SqlConfig::default()
            },
            ..SqlPlugin::new()
        };
        let raw = read_sample_log("sql_plugin", "obfuscate_sensitive_case01");
        let out = compress_to_string(&plugin, &raw, SliceType::Line);

        assert!(
            !out.contains("S3cr3tPassw0rd!")
                && !out.contains("P@ssw0rd-2026")
                && !out.contains("eyJhbGciOiJIUzI1NiJ9"),
            "骨架化路径不得让任何敏感明文残留"
        );
        assert!(
            out.contains("[DBCRED]@") && out.contains("[BEARER]"),
            "非字面量凭证的占位符应在骨架化后存活"
        );
    }
}
