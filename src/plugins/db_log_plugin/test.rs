//! db_log_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::db_log_plugin::DbLogPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_pg_case() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_001_pg");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_mysql_case() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_002_mysql");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 法则 A ROI 门控：小样本场景下 $DB|MY| 元字段开销 > 收益，整段回退原文。
        assert!(
            out.len() <= raw.len() + 4,
            "db_log 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 合成大样本：把 case_002 首行重复 60 次，验证插件在多行 mysql 输入下仍稳定。
    #[test]
    fn compresses_bulk_mysql_without_panic() {
        let plugin = DbLogPlugin::new();
        let seed = read_sample_log("db_log_plugin", "case_002_mysql");
        let first_line = seed.lines().next().expect("case_002 应至少一行");
        let mut bulk = String::new();
        for _ in 0..60 {
            bulk.push_str(first_line);
            bulk.push('\n');
        }
        let out = compress_to_string(&plugin, &bulk, SliceType::LogBlock);
        assert!(
            out.len() <= bulk.len(),
            "db_log bulk 压缩不得扩张: raw={} out={}",
            bulk.len(),
            out.len()
        );
    }

    #[test]
    fn detects_mongodb_case() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_013_mongodb_slow");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("MONGO|"));
        assert!(out.contains("Slow query"));
        assert!(out.contains("ns=shop.orders"));
        assert!(out.contains("dur=1284ms"));
    }

    #[test]
    fn detects_redis_case() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_014_redis_events");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("REDIS|"));
        assert!(out.contains("ERR"));
        assert!(out.contains("role=M"));
    }

    #[test]
    fn compresses_postgres_duration_case() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_015_pg_duration");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("SLOW|421.75ms"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn highlights_postgres_lock_and_deadlock_signals() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_016_pg_lock_waits");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("PG|"));
        assert!(out.contains("SLOW|1842.55ms"), "{out}");
        assert!(out.contains("!PG|"), "{out}");
        assert!(out.contains("deadlock detected"), "{out}");
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn extracts_mongodb_command_timeout_details() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_017_mongodb_command_timeout");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("MONGO|"), "{out}");
        assert!(out.contains("aggregate=orders"), "{out}");
        assert!(out.contains("!MONGO|"), "{out}");
        assert!(out.contains("code=50"), "{out}");
        assert!(out.contains("dur=30000ms"), "{out}");
    }

    #[test]
    fn extracts_redis_memory_and_replication_events() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_018_redis_memory_replication");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("!REDIS|"), "{out}");
        assert!(out.contains("OOM command not allowed"), "{out}");
        assert!(out.contains("role=S"), "{out}");
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn preserves_mysql_deadlock_timeout_errors() {
        let plugin = DbLogPlugin::new();
        let raw = read_sample_log("db_log_plugin", "case_020_mysql_deadlock_slow");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("MY|"), "{out}");
        assert!(out.contains("!MY|"), "{out}");
        assert!(out.contains("deadlock detected"), "{out}");
        assert!(out.contains("network partition"), "{out}");
        assert!(out.len() <= raw.len());
    }
}
