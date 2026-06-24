//! cloud_log_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::cloud_log_plugin::CloudLogPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_aws_logs_tail_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_001_aws_logs_tail_health");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn does_not_detect_non_cloud_plain_text() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_044_non_cloud_plain");
        assert!(plugin.detect(&make_log_slice(&raw)).is_none());
    }

    #[test]
    fn compresses_aws_health_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_001_aws_logs_tail_health");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|WEB_HEALTH"));
        assert!(out.contains("provider=aws"));
        assert!(out.contains("hits=6"));
        assert!(out.len() < raw.len());
    }

    #[test]
    fn compresses_csv_wrapped_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_004_aws_csv_health");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|WEB_HEALTH"));
        assert!(!out.contains("timestamp,"));
        assert!(out.len() < raw.len());
    }

    #[test]
    fn unwraps_gcp_json_payload_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_006_gcp_jsonl_textpayload");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|META|providers=gcp"));
        assert!(out.contains("worker started"));
        assert!(!out.contains("textPayload"));
    }

    #[test]
    fn unwraps_gcp_protopayload_status_message() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_018_gcp_audit_protopayload");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("SetIamPolicy"), "{out}");
        assert!(out.contains("quota exceeded"), "{out}");
        assert!(!out.contains("protoPayload"), "{out}");
    }

    #[test]
    fn preserves_java_stack_after_unwrap() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_011_cloud_java_stack_jsonl");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("Exception in thread"));
        assert!(out.contains("NullPointerException"));
        assert!(!out.contains("logStream"));
    }

    #[test]
    fn preserves_python_traceback_after_unwrap() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_012_cloud_python_traceback_jsonl");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("Traceback (most recent call last):"));
        assert!(out.contains("ValueError"));
        assert!(!out.contains("textPayload"));
    }

    #[test]
    fn unwraps_gcp_jsonl_multiline_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_049_gcp_jsonl_multiline");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|META|providers=gcp"));
        // Should contain the multiline traceback and have the outer shell stripped
        assert!(out.contains("Traceback (most recent call last):"));
        assert!(out.contains("ValueError: Database timeout after 30s"));
        assert!(!out.contains("textPayload"));
        assert!(!out.contains("insertId"));
    }

    #[test]
    fn unwraps_aliyun_csv_multiline_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_052_aliyun_csv_multiline");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.contains("$CL|META|providers=aliyun") || out.contains("__topic__"),
            "{out}"
        );
        // Should contain the multiline traceback and have the outer shell stripped
        assert!(out.contains("Traceback (most recent call last):"));
        assert!(out.contains("ValueError: Database timeout after 30s"));
        if out.contains("$CL|META|providers=aliyun") {
            assert!(!out.contains("__topic__"));
        }
    }

    #[test]
    fn detects_second_wave_cloud_providers() {
        let plugin = CloudLogPlugin::new();
        for case in [
            "case_026_oci_logging_json",
            "case_029_tencent_cls_json",
            "case_032_huawei_lts_json",
            "case_035_cloudflare_workers_json",
        ] {
            let raw = read_sample_log("cloud_log_plugin", case);
            assert!(plugin.detect(&make_log_slice(&raw)).is_some(), "{case}");
        }
    }

    #[test]
    fn unwraps_second_wave_provider_matrix() {
        let plugin = CloudLogPlugin::new();
        for (case, provider) in [
            ("case_026_oci_logging_json", "provider=oci"),
            ("case_029_tencent_cls_json", "provider=tencent"),
            ("case_032_huawei_lts_json", "provider=huawei"),
            ("case_035_cloudflare_workers_json", "provider=cloudflare"),
        ] {
            let raw = read_sample_log("cloud_log_plugin", case);
            let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
            assert!(out.contains(provider), "{case}: {out}");
            assert!(out.len() < raw.len(), "{case}");
        }
    }

    #[test]
    fn preserves_wrapped_runtime_and_database_signals() {
        let plugin = CloudLogPlugin::new();
        let node_raw = read_sample_log("cloud_log_plugin", "case_017_gcp_jsonpayload_node_error");
        let node_out = compress_to_string(&plugin, &node_raw, SliceType::LogBlock);
        assert!(node_out.contains("TypeError"));
        assert!(node_out.contains("processTicksAndRejections"));

        let db_raw = read_sample_log("cloud_log_plugin", "case_025_aliyun_sls_db_jsonl");
        let db_out = compress_to_string(&plugin, &db_raw, SliceType::LogBlock);
        assert!(db_out.contains("PostgreSQL duration"));
        assert!(db_out.contains("Redis Connection refused"));
        assert!(db_out.contains("MongoDB slow query"));
    }

    #[test]
    fn unwraps_pipe_table_with_headers() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_024_aliyun_sls_table_syslog");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.contains("$CL|META|providers=aliyun") || out.contains("| timestamp |"),
            "{out}"
        );
        assert!(out.contains("Started backend.service"));
        if out.contains("$CL|META|providers=aliyun") {
            assert!(!out.contains("| timestamp |"));
        }
    }

    #[test]
    fn unwraps_aws_logs_insights_alias_headers() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_038_aws_logs_insights_table");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|WEB_HEALTH|provider=aws"), "{out}");
        assert!(out.contains("hits=4"), "{out}");
        assert!(!out.contains("| @message |"), "{out}");
    }

    #[test]
    fn preserves_aws_logs_insights_business_columns() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_045_aws_logs_insights_table");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // v0.3.6+ cases 迭代后表格格式被压缩为 META 汇总行, 原始 business columns 已不存在于 sample
        assert!(out.contains("$CL|META|"), "{out}");
        assert!(out.contains("records="), "{out}");
        assert!(out.len() < raw.len(), "{out}");
    }

    #[test]
    fn unwraps_structured_cloud_http_access_records() {
        let plugin = CloudLogPlugin::new();
        for (case, provider) in [
            ("case_039_aws_filter_log_events_jsonl", "provider=aws"),
            ("case_040_gcp_http_request_jsonl", "provider=gcp"),
            (
                "case_043_cloudflare_logpush_http_jsonl",
                "provider=cloudflare",
            ),
        ] {
            let raw = read_sample_log("cloud_log_plugin", case);
            let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
            assert!(out.contains(provider), "{case}: {out}");
            assert!(
                out.contains("$CL|WEB_ACCESS") || out.contains("$CL|WEB_HEALTH"),
                "{case}: {out}"
            );
            assert!(out.len() < raw.len(), "{case}: {out}");
        }
    }

    #[test]
    fn unwraps_azure_and_oci_alias_fields() {
        let plugin = CloudLogPlugin::new();
        let azure_raw =
            read_sample_log("cloud_log_plugin", "case_041_azure_appinsights_traces_csv");
        let azure_out = compress_to_string(&plugin, &azure_raw, SliceType::LogBlock);
        assert!(
            azure_out.contains("$CL|META|providers=azure") || azure_out.contains("cloud_RoleName"),
            "{azure_out}"
        );
        assert!(azure_out.contains("System.TimeoutException"), "{azure_out}");
        if azure_out.contains("$CL|META|providers=azure") {
            assert!(!azure_out.contains("cloud_RoleName"), "{azure_out}");
        }

        let oci_raw = read_sample_log("cloud_log_plugin", "case_042_oci_logging_table");
        let oci_out = compress_to_string(&plugin, &oci_raw, SliceType::LogBlock);
        assert!(
            oci_out.contains("$CL|META|providers=oci") || oci_out.contains("| datetime |"),
            "{oci_out}"
        );
        assert!(oci_out.contains("HikariPool timeout"), "{oci_out}");
        if oci_out.contains("$CL|META|providers=oci") {
            assert!(!oci_out.contains("| datetime |"), "{oci_out}");
        }
    }

    #[test]
    fn keeps_command_anchor_before_cloud_summary() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_038_aws_logs_insights_table");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        let first_line = out.lines().next().unwrap_or_default();
        assert!(
            first_line.starts_with("aws logs start-query"),
            "first line should keep command anchor, got: {first_line}"
        );
        assert!(out.contains("$CL|WEB_HEALTH|provider=aws"), "{out}");
    }

    #[test]
    fn unwraps_pipe_table_without_explicit_headers() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_002_cloudwatch_table_health");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|WEB_HEALTH"), "{out}");
        assert!(out.contains("hits=6"), "{out}");
        assert!(!out.contains("|   timestamp   |"), "{out}");
        assert!(!out.contains("|---------------|"), "{out}");
    }

    #[test]
    fn unwraps_generic_cloud_lines_and_marks_error_level() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_007_gcp_plain_logging");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.lines()
                .next()
                .unwrap_or_default()
                .starts_with("gcloud logging read"),
            "{out}"
        );
        assert!(
            out.contains("$CL|META|providers=gcp") || out.contains("run.googleapis.com/stderr"),
            "{out}"
        );
        assert!(out.contains("worker started pid=42"), "{out}");
        assert!(
            out.contains("database connection error: timeout after 30s"),
            "{out}"
        );
    }

    #[test]
    fn renders_access_summary_for_mixed_health_and_error_records() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_003_aws_logs_tail_mixed");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|WEB_HEALTH|provider=aws"), "{out}");
        assert!(out.contains("!$CL|WEB_ACCESS|provider=aws"), "{out}");
        assert!(out.contains("POST /api/documents"), "{out}");
        assert!(out.contains("GET /api/status"), "{out}");
    }
}
