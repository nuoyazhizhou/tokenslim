#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::cloud_log_plugin::CloudLogPlugin;
    use std::borrow::Cow;

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("cloud_log_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &CloudLogPlugin, text: &str) -> String {
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
    fn generate_cloud_log_showcase_report() {
        let plugin = CloudLogPlugin::new();
        let cases = [
            ("case_001_aws_logs_tail_health", "AWS logs tail 健康检查"),
            (
                "case_002_cloudwatch_table_health",
                "CloudWatch 表格健康检查",
            ),
            ("case_003_aws_logs_tail_mixed", "AWS logs tail 混合状态"),
            ("case_004_aws_csv_health", "AWS CSV 导出健康检查"),
            ("case_005_aws_jsonl_message", "AWS JSONL message"),
            ("case_006_gcp_jsonl_textpayload", "GCP JSONL textPayload"),
            ("case_007_gcp_plain_logging", "GCP 纯文本 logging"),
            ("case_008_azure_csv_appservice", "Azure CSV AppService"),
            ("case_009_azure_table_appservice", "Azure 表格 AppService"),
            ("case_010_aliyun_sls_jsonl", "阿里云 SLS JSONL"),
            ("case_011_cloud_java_stack_jsonl", "云包装 Java 堆栈"),
            (
                "case_012_cloud_python_traceback_jsonl",
                "云包装 Python traceback",
            ),
            (
                "case_013_aws_lambda_report_jsonl",
                "AWS Lambda REPORT JSONL",
            ),
            ("case_014_aws_cloudtrail_jsonl", "AWS CloudTrail JSONL"),
            ("case_015_aws_alb_access_jsonl", "AWS ALB access JSONL"),
            ("case_016_aws_vpc_flow_jsonl", "AWS VPC Flow JSONL"),
            (
                "case_017_gcp_jsonpayload_node_error",
                "GCP jsonPayload Node error",
            ),
            ("case_018_gcp_audit_protopayload", "GCP Audit protoPayload"),
            ("case_019_gcp_gke_plain", "GCP GKE plain logging"),
            ("case_020_azure_monitor_json", "Azure Monitor JSON"),
            (
                "case_021_azure_appinsights_exception_jsonl",
                "Azure AppInsights exception",
            ),
            (
                "case_022_azure_containerapps_plain",
                "Azure Container Apps plain",
            ),
            ("case_023_aliyun_sls_csv_nginx", "阿里云 SLS CSV Nginx"),
            ("case_024_aliyun_sls_table_syslog", "阿里云 SLS 表格 Syslog"),
            ("case_025_aliyun_sls_db_jsonl", "阿里云 SLS DB JSONL"),
            ("case_026_oci_logging_json", "OCI Logging JSON"),
            ("case_027_oci_logging_cli_plain", "OCI CLI plain logging"),
            ("case_028_oci_audit_jsonl", "OCI Audit JSONL"),
            ("case_029_tencent_cls_json", "腾讯云 CLS JSON"),
            ("case_030_tencent_cls_csv", "腾讯云 CLS CSV"),
            ("case_031_tencent_cls_table", "腾讯云 CLS 表格"),
            ("case_032_huawei_lts_json", "华为云 LTS JSON"),
            ("case_033_huawei_lts_csv", "华为云 LTS CSV"),
            ("case_034_huawei_lts_plain", "华为云 LTS plain"),
            (
                "case_035_cloudflare_workers_json",
                "Cloudflare Workers JSON",
            ),
            ("case_036_cloudflare_http_json", "Cloudflare HTTP JSON"),
            (
                "case_037_cloudflare_wrangler_tail_plain",
                "Cloudflare wrangler tail",
            ),
            (
                "case_038_aws_logs_insights_table",
                "AWS Logs Insights @message table",
            ),
            (
                "case_039_aws_filter_log_events_jsonl",
                "AWS filter-log-events JSONL",
            ),
            ("case_040_gcp_http_request_jsonl", "GCP httpRequest JSONL"),
            (
                "case_041_azure_appinsights_traces_csv",
                "Azure AppInsights traces CSV",
            ),
            ("case_042_oci_logging_table", "OCI Logging table"),
            (
                "case_043_cloudflare_logpush_http_jsonl",
                "Cloudflare Logpush HTTP JSONL",
            ),
            ("case_044_non_cloud_plain", "Non-cloud passthrough log"),
            (
                "case_045_aws_logs_insights_table",
                "AWS Logs Insights multiline table",
            ),
            (
                "case_046_aws_logs_insights_table",
                "AWS Logs Insights wrapped message table",
            ),
            (
                "case_047_aws_logs_insights_table",
                "AWS Logs Insights wide table",
            ),
            (
                "case_048_aws_logs_insights_table",
                "AWS Logs Insights request table",
            ),
            (
                "case_049_gcp_jsonl_multiline",
                "GCP JSONL multiline traceback",
            ),
            (
                "case_050_aliyun_csv_multiline",
                "Aliyun SLS CSV multiline traceback",
            ),
            (
                "case_050_aliyun_sls_multiline",
                "Aliyun SLS JSON multiline stacktrace",
            ),
            (
                "case_051_tencent_cls_multiline",
                "Tencent CLS multiline session error",
            ),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Cloud Log AI Compact Showcase\n");
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
            all_output.push_str("\n");
            all_output.push_str(&raw);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }

            all_output.push_str("-- Compact Output (full) --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&compacted);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("cloud_log_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
