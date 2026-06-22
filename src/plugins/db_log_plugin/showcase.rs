#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::db_log_plugin::DbLogPlugin;
    use std::borrow::Cow;

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("db_log_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &DbLogPlugin, text: &str) -> String {
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed(text),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: text.lines().count().max(1),
            file_metadata: None,
            flags: Default::default(),
        };
        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = bumpalo::Bump::new();
        let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
        result
            .tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.as_ref()),
                _ => None,
            })
            .collect::<String>()
    }

    #[test]
    fn generate_db_log_showcase_report() {
        let plugin = DbLogPlugin::new();
        let cases = [
            ("case_001_pg", "PostgreSQL relation error"),
            ("case_002_mysql", "MySQL aborted connection warning"),
            ("case_001_pg_standard", "PostgreSQL standard logs"),
            ("case_002_mysql_standard", "MySQL standard logs"),
            ("case_003_pg_noise", "PostgreSQL noisy logs"),
            ("case_004_mysql_noise", "MySQL noisy logs"),
            ("case_005_pg_long_line", "PostgreSQL long line"),
            ("case_006_mysql_empty", "MySQL empty input"),
            ("case_007_pg_single_line", "PostgreSQL single line"),
            ("case_008_mysql_special_chars", "MySQL special chars"),
            ("case_009_pg_mixed", "PostgreSQL mixed logs"),
            ("case_010_mysql_mixed", "MySQL mixed logs"),
            ("case_011_pg_no_compress", "PostgreSQL no-compress case"),
            ("case_012_mysql_no_compress", "MySQL no-compress case"),
            ("case_013_mongodb_slow", "MongoDB slow query"),
            ("case_014_redis_events", "Redis event logs"),
            ("case_015_pg_duration", "PostgreSQL duration logs"),
            ("case_016_pg_lock_waits", "PostgreSQL lock/deadlock logs"),
            (
                "case_017_mongodb_command_timeout",
                "MongoDB command timeout logs",
            ),
            (
                "case_018_redis_memory_replication",
                "Redis memory and replication logs",
            ),
            (
                "case_019_pg_autovacuum_checkpoint",
                "PostgreSQL autovacuum/checkpoint logs",
            ),
            (
                "case_020_mysql_deadlock_slow",
                "MySQL deadlock/timeout logs",
            ),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  DB Log AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (case_id, title) in cases {
            let file_name = format!("{}.log", case_id);
            let raw = read_sample(&file_name);

            let original_lines = raw.lines().count();
            let original_bytes = raw.len();
            let compacted = compress_text(&plugin, &raw);
            let compact_lines = if compacted.is_empty() {
                0
            } else {
                compacted.lines().count()
            };
            let compact_bytes = compacted.len();
            let compression_ratio = if original_bytes > 0 {
                (1.0 - compact_bytes as f64 / original_bytes as f64) * 100.0
            } else {
                0.0
            };

            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!("\nCase {} - {} ({})\n", case_id, title, file_name));
            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!(
                "\nOriginal: {} lines, {} bytes | Compact: {} lines, {} bytes | Compression: {:.1}%\n",
                original_lines, original_bytes, compact_lines, compact_bytes, compression_ratio
            ));

            all_output.push_str("-- Case text --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push('\n');
            all_output.push_str(&raw);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }

            all_output.push_str("-- Compact Output (full) --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push('\n');
            all_output.push_str(&compacted);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("db_log_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
