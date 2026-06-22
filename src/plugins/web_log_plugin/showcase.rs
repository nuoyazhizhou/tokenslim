#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::web_log_plugin::WebLogPlugin;
    use std::borrow::Cow;

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("web_log_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &WebLogPlugin, text: &str) -> String {
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
    fn generate_web_log_showcase_report() {
        let plugin = WebLogPlugin::new();
        let cases = [
            ("case_001_access", "Nginx Access Log"),
            ("case_002_error", "Nginx Error Log"),
            ("case_003_access_post", "POST access"),
            ("case_004_access_static", "Static asset access"),
            ("case_005_noise", "Noise mixed"),
            ("case_006_long_line", "Long line"),
            ("case_007_single_line", "Single line"),
            ("case_008_empty", "Empty input"),
            ("case_009_special_chars", "Special chars"),
            ("case_010_mixed", "Mixed web log"),
            ("case_011_no_compress", "No-compress fallback"),
            ("case_012_various_status", "Various status"),
            ("case_013_aws_logs_tail_health", "AWS logs tail health"),
            (
                "case_014_cloudwatch_insights_health_table",
                "CloudWatch Insights health table",
            ),
            ("case_015_aws_logs_tail_mixed_status", "AWS mixed status"),
            ("case_016_nginx_health_aggregate", "Nginx health aggregate"),
            ("case_017_nginx_404_scan", "Nginx 404 scan"),
            ("case_018_nginx_5xx_spike", "Nginx 5xx spike"),
            ("case_019_nginx_slow_requests", "Nginx slow requests"),
            ("case_020_apache_common", "Apache common log"),
            ("case_021_apache_combined_bot", "Apache bot traffic"),
            ("case_022_ingress_nginx", "Ingress Nginx"),
            ("case_023_cloudflare_json", "Cloudflare JSON"),
            ("case_024_gcp_http_request_json", "GCP httpRequest JSON"),
            ("case_025_azure_csv_message", "Azure CSV message"),
            ("case_026_oci_json_message", "OCI JSON message"),
            ("case_027_aws_csv_uvicorn", "AWS CSV uvicorn"),
            ("case_028_gcp_json_message_nginx", "GCP JSON message Nginx"),
            ("case_029_cloudflare_csv_direct", "Cloudflare CSV direct"),
            ("case_030_nginx_json_access", "Nginx JSON access"),
            ("case_031_route_id_normalization", "Route id normalization"),
            (
                "case_032_user_agent_distribution",
                "User-Agent distribution",
            ),
            ("case_033_static_asset_mix", "Static asset mix"),
            ("case_034_referer_mix", "Referer mix"),
            ("case_035_websocket_and_redirect", "WebSocket and redirect"),
            ("case_036_mixed_cloud_wrappers", "Mixed cloud wrappers"),
            (
                "case_037_access_v3_health_static",
                "Access v3 health/static folding",
            ),
            (
                "case_038_access_v3_404_sensitive_scan",
                "Access v3 sensitive 404 scan",
            ),
            (
                "case_039_access_v3_503_checkout_burst",
                "Access v3 checkout 503 burst",
            ),
            (
                "case_040_access_v3_mixed_cloud_wrappers",
                "Access v3 mixed cloud wrappers",
            ),
            (
                "case_041_access_v3_bot_ua_dictionary",
                "Access v3 bot/user-agent dictionary",
            ),
            ("case_042_access_v3_slow_export", "Access v3 slow export"),
            (
                "case_043_access_v3_route_id_routine",
                "Access v3 route id routine",
            ),
            ("case_044_access_v3_alb_native", "Access v3 ALB native"),
            ("case_045_cloudfront_w3c_access", "CloudFront W3C access"),
            ("case_046_envoy_istio_access", "Envoy/Istio access"),
            ("case_047_iis_w3c_access", "IIS W3C access"),
            (
                "case_048_access_v3_with_passthrough",
                "Access v3 with passthrough diagnostics",
            ),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Web Log AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (case_id, title) in cases {
            let file_name = format!("{}.log", case_id);
            let raw = read_sample(&file_name);

            if raw.is_empty() && case_id != "case_008_empty" {
                all_output.push_str(&format!(
                    "[SKIP] {} - file not found: {}\n\n",
                    case_id, file_name
                ));
                continue;
            }

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
                .join("web_log_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
