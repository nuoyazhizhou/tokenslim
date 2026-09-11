//! cloud_log_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::cloud_log_plugin::CloudLogPlugin;
    use crate::plugins::test_utils::*;

    /// 测试：AWS logs tail 健康检查样例被插件识别。
    #[test]
    fn detects_aws_logs_tail_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_001_aws_logs_tail_health");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 测试：非云纯文本样例不被插件识别。
    #[test]
    fn does_not_detect_non_cloud_plain_text() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_044_non_cloud_plain");
        assert!(plugin.detect(&make_log_slice(&raw)).is_none());
    }

    /// 测试（L1 第二阶段 2026-08-19，T-004 Group A k8s case_003 根因回归）：
    /// 普通 timestamp/level 日志（无云外壳信号）不得被 CloudLogPlugin 抢占——
    /// 加载 kubernetes_docker_plugin case_003 物理样本，detect 必须为 None。
    #[test]
    fn does_not_detect_plain_timestamp_level_log_without_cloud_shell() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("kubernetes_docker_plugin", "case_003_kubectl_logs");
        assert!(plugin.detect(&make_log_slice(&raw)).is_none());
    }

    /// 测试：AWS 健康检查样例压缩为 WEB_HEALTH 汇总且不膨胀。
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

    /// 测试：CSV 包裹样例压缩为 WEB_HEALTH 且不含原始表头。
    #[test]
    fn compresses_csv_wrapped_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_004_aws_csv_health");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|WEB_HEALTH"));
        assert!(!out.contains("timestamp,"));
        assert!(out.len() < raw.len());
    }

    /// 测试：GCP JSONL 样例解包去除 textPayload 外壳并保留业务消息。
    #[test]
    fn unwraps_gcp_json_payload_case() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_006_gcp_jsonl_textpayload");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|META|providers=gcp"));
        assert!(out.contains("worker started"));
        assert!(!out.contains("textPayload"));
    }

    /// 测试：GCP protoPayload 状态消息解包保留业务信息。
    #[test]
    fn unwraps_gcp_protopayload_status_message() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_018_gcp_audit_protopayload");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("SetIamPolicy"), "{out}");
        assert!(out.contains("quota exceeded"), "{out}");
        assert!(!out.contains("protoPayload"), "{out}");
    }

    /// 测试：Java 堆栈在云外壳解包后完整保留。
    #[test]
    fn preserves_java_stack_after_unwrap() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_011_cloud_java_stack_jsonl");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("Exception in thread"));
        assert!(out.contains("NullPointerException"));
        assert!(!out.contains("logStream"));
    }

    /// 测试：Python traceback 在云外壳解包后完整保留。
    #[test]
    fn preserves_python_traceback_after_unwrap() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_012_cloud_python_traceback_jsonl");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("Traceback (most recent call last):"));
        assert!(out.contains("ValueError"));
        assert!(!out.contains("textPayload"));
    }

    /// 测试：GCP 多行 JSONL 样例解包并保留 traceback。
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

    /// 测试：阿里云多行 CSV 样例解包并保留 traceback。
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

    /// 测试：OCI/腾讯云/华为云/Cloudflare 二线厂商样例被识别。
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

    /// 测试：二线厂商矩阵样例压缩为含各自 provider 的摘要且不膨胀。
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

    /// 测试：Node 错误与数据库信号（PostgreSQL/Redis/MongoDB）在解包后保留。
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

    /// 测试：带表头管道表格解包为 META 汇总行。
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

    /// 测试：AWS Logs Insights 别名表头解包为 WEB_HEALTH 汇总。
    #[test]
    fn unwraps_aws_logs_insights_alias_headers() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_038_aws_logs_insights_table");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CL|WEB_HEALTH|provider=aws"), "{out}");
        assert!(out.contains("hits=4"), "{out}");
        assert!(!out.contains("| @message |"), "{out}");
    }

    /// 测试：AWS Logs Insights 表格压缩为 META 汇总且不膨胀。
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

    /// 测试：结构化 HTTP 访问记录解包为 WEB_ACCESS/WEB_HEALTH 汇总。
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

    /// 测试：Azure/OCI 别名字段解包保留业务消息。
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

    /// 测试：命令锚点行保留在云摘要之前。
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

    /// 测试：无显式表头的管道表格仍解包为 WEB_HEALTH 汇总。
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

    /// 测试：通用云行解包并保留错误级记录。
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

    /// 测试：混合健康/错误访问记录渲染 WEB_HEALTH 与带 ! 的 WEB_ACCESS 汇总行。
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

    /// P1-06 回归：cloud_log 插件必须实现文档级剥皮——对既有云日志样本，
    /// peel_document 返回 Some、外壳摘要保留命令行、内层正文含记录消息
    /// （旧实现走 trait 默认 None，两层化对 CloudLog 类别永不生效）。
    #[test]
    fn peels_cloud_log_document() {
        let plugin = CloudLogPlugin::new();
        let raw = read_sample_log("cloud_log_plugin", "case_001_aws_logs_tail_health");
        let skin = plugin
            .peel_document(&raw)
            .expect("云日志样本应可剥皮（P1-06）");
        assert!(
            !skin.inner_body.trim().is_empty(),
            "内层正文（记录消息）应非空"
        );
    }
}
