//! web_log_plugin 测试模块（文件驱动，严禁 Hardcode）
#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::*;
    use crate::plugins::web_log_plugin::WebLogPlugin;

    fn compress_case(case_id: &str) -> (String, String) {
        let plugin = WebLogPlugin::new();
        let raw = read_sample_log("web_log_plugin", case_id);
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        (raw, out)
    }

    fn assert_roi(raw: &str, out: &str) {
        assert!(
            out.len() <= raw.len() + 4,
            "web_log 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn detects_access_case() {
        let plugin = WebLogPlugin::new();
        let raw = read_sample_log("web_log_plugin", "case_001_access");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_error_case_without_expansion() {
        let (raw, out) = compress_case("case_002_error");
        assert_roi(&raw, &out);
    }

    #[test]
    fn aggregates_aws_logs_tail_health_case() {
        let (raw, out) = compress_case("case_013_aws_logs_tail_health");
        assert!(out.contains("$W|SUMMARY"));
        assert!(out.contains("GET /health"));
        assert!(out.contains("records=10"));
        assert!(out.len() < raw.len());
    }

    #[test]
    fn aggregates_cloudwatch_insights_table_case() {
        let (raw, out) = compress_case("case_014_cloudwatch_insights_health_table");
        assert!(out.contains("$W|SUMMARY"));
        assert!(!out.contains("|   timestamp"));
        assert!(out.len() < raw.len());
    }

    #[test]
    fn preserves_error_status_as_anomaly() {
        let (_raw, out) = compress_case("case_015_aws_logs_tail_mixed_status");
        assert!(out.contains("$W|SUMMARY"));
        assert!(out.contains("5xx=2"));
        assert!(out.contains("!$W|ANOMALY"));
        assert!(out.contains("/api/documents"));
        assert!(out.contains("500"));
        assert!(out.contains("/api/status"));
        assert!(out.contains("503"));
    }

    #[test]
    fn aggregates_nginx_dimensions() {
        let (_raw, out) = compress_case("case_016_nginx_health_aggregate");
        assert!(out.contains("$W|SUMMARY"));
        assert!(out.contains("2xx=6"));
        assert!(out.contains("$W|TOP_URL"));
        assert!(out.contains("$W|TOP_IP"));
        assert!(out.contains("$W|TOP_UA"));
        assert!(out.contains("GET /health"));
    }

    #[test]
    fn keeps_404_scan_visible() {
        let (_raw, out) = compress_case("case_017_nginx_404_scan");
        assert!(out.contains("4xx=20"));
        assert!(out.contains("!$W|ANOMALY"));
        assert!(out.contains("/wp-login.php"));
        assert!(out.contains("203.0.113.77"));
    }

    #[test]
    fn keeps_5xx_spike_visible() {
        let (_raw, out) = compress_case("case_018_nginx_5xx_spike");
        assert!(out.contains("5xx=12"));
        assert!(out.contains("500"));
        assert!(out.contains("502"));
        assert!(out.contains("503"));
        assert!(out.contains("!$W|ANOMALY"));
    }

    #[test]
    fn highlights_slow_requests() {
        let (_raw, out) = compress_case("case_019_nginx_slow_requests");
        assert!(out.contains("!$W|SLOW"));
        assert!(out.contains("/api/search"));
        assert!(out.contains("ms="));
    }

    #[test]
    fn supports_apache_common_and_combined() {
        let (_raw_common, out_common) = compress_case("case_020_apache_common");
        assert!(out_common.contains("$W|SUMMARY"));
        assert!(out_common.contains("4xx=2"));

        let (_raw_combined, out_combined) = compress_case("case_021_apache_combined_bot");
        assert!(out_combined.contains("$W|TOP_UA"));
        assert!(out_combined.contains("Googlebot"));
        assert!(out_combined.contains("BadBot"));
    }

    #[test]
    fn supports_cloud_wrapped_formats() {
        for case_id in [
            "case_023_cloudflare_json",
            "case_024_gcp_http_request_json",
            "case_025_azure_csv_message",
            "case_026_oci_json_message",
            "case_027_aws_csv_uvicorn",
            "case_028_gcp_json_message_nginx",
            "case_029_cloudflare_csv_direct",
            "case_030_nginx_json_access",
        ] {
            let (_raw, out) = compress_case(case_id);
            assert!(
                out.contains("$W|SUMMARY"),
                "{case_id} should produce web summary, got:\n{out}"
            );
        }
    }

    #[test]
    fn normalizes_route_ids() {
        let (_raw, out) = compress_case("case_031_route_id_normalization");
        assert!(out.contains("/api/orders/:id"));
    }

    #[test]
    fn access_v3_folds_health_static_and_dictionaries() {
        let (raw, out) = compress_case("case_037_access_v3_health_static");
        assert!(out.contains("$W|SUMMARY"));
        assert!(out.contains("$W|DICT_IP"));
        assert!(out.contains("$W|DICT_UA"));
        assert!(out.contains("$W|ROUTINE|kind=health"));
        assert!(out.contains("$W|ROUTINE|kind=static"));
        assert!(out.contains("$UA_KUBE"));
        assert!(out.contains("avg_ms="));
        assert!(out.len() < raw.len());
    }

    #[test]
    fn access_v3_spotlights_scan_and_burst() {
        let (_raw_scan, out_scan) = compress_case("case_038_access_v3_404_sensitive_scan");
        assert!(out_scan.contains("!$W|SCAN"));
        assert!(out_scan.contains("$IP_ATK"));
        assert!(out_scan.contains("$UA_REQ"));
        assert!(out_scan.contains(".env"));

        let (_raw_burst, out_burst) = compress_case("case_039_access_v3_503_checkout_burst");
        assert!(out_burst.contains("!$W|BURST"));
        assert!(out_burst.contains("503 Service Unavailable"));
        assert!(out_burst.contains("/api/v1/checkout"));
        assert!(out_burst.contains("err_rate=50.0%"));
    }

    #[test]
    fn access_v3_supports_cloud_wrappers_and_slow_routes() {
        let (_raw_cloud, out_cloud) = compress_case("case_040_access_v3_mixed_cloud_wrappers");
        assert!(out_cloud.contains("$W|DICT_IP"));
        assert!(out_cloud.contains("oci"));
        assert!(out_cloud.contains("!$W|SCAN"));
        assert!(out_cloud.contains("!$W|BURST"));

        let (_raw_slow, out_slow) = compress_case("case_042_access_v3_slow_export");
        assert!(out_slow.contains("$W|ROUTINE"));
        assert!(out_slow.contains("/api/v1/export_report"));
        assert!(out_slow.contains("!$W|SLOW"));
    }

    #[test]
    fn access_v3_generalizes_route_ids_and_alb_native() {
        let (_raw_route, out_route) = compress_case("case_043_access_v3_route_id_routine");
        assert!(out_route.contains("/api/orders/:id"));
        assert!(out_route.contains("$W|ROUTINE|kind=routine|200 OK|GET /api/orders/:id"));

        let (_raw_alb, out_alb) = compress_case("case_044_access_v3_alb_native");
        assert!(out_alb.contains("ALB:http"));
        assert!(out_alb.contains("!$W|SCAN"));
        assert!(out_alb.contains("!$W|BURST"));
        assert!(out_alb.contains("/api/v1/checkout"));
    }

    #[test]
    fn access_v3_supports_w3c_edge_and_iis_formats() {
        let (_raw_cf, out_cf) = compress_case("case_045_cloudfront_w3c_access");
        assert!(out_cf.contains("$W|SUMMARY"));
        assert!(out_cf.contains("CloudFront"));
        assert!(out_cf.contains("$W|DIAG|err_rate="));
        assert!(out_cf.contains("noise=health:"));
        assert!(out_cf.contains("!$W|SCAN"));
        assert!(out_cf.contains("!$W|BURST"));

        let (raw_iis, out_iis) = compress_case("case_047_iis_w3c_access");
        let plugin = WebLogPlugin::new();
        assert!(plugin.detect(&make_log_slice(&raw_iis)).is_some());
        assert_roi(&raw_iis, &out_iis);
        assert!(out_iis.contains("$W|SUMMARY"));
        assert!(out_iis.contains("IIS_W3C"));
        assert!(out_iis.contains("noise=health:"));
        assert!(out_iis.contains("!$W|SLOW"));
        assert!(out_iis.contains("/api/export/report"));
    }

    #[test]
    fn access_v3_supports_envoy_istio_access_logs() {
        let (_raw, out) = compress_case("case_046_envoy_istio_access");
        assert!(out.contains("$W|SUMMARY"));
        assert!(out.contains("Envoy"));
        assert!(out.contains("$W|ROUTINE|kind=health"));
        assert!(out.contains("$W|ROUTINE|kind=static"));
        assert!(out.contains("/api/items/:id"));
        assert!(out.contains("!$W|SCAN"));
        assert!(out.contains("!$W|BURST"));
        assert!(out.contains("!$W|SLOW"));
    }

    #[test]
    fn health_only_summary_uses_compact_branch() {
        let (_raw, out) = compress_case("case_013_aws_logs_tail_health");
        assert!(out.contains("$W|SUMMARY|records=10"));
        assert!(out.contains("|4xx=0|5xx=0|"));
        assert!(!out.contains("window="));
    }

    #[test]
    fn keeps_passthrough_lines_before_summary() {
        let (_raw, out) = compress_case("case_048_access_v3_with_passthrough");
        let pass_idx = out.find("UNMATCHED_DIAGNOSTIC:").unwrap_or(usize::MAX);
        let summary_idx = out.find("$W|SUMMARY").unwrap_or(usize::MAX);
        assert!(
            pass_idx != usize::MAX,
            "passthrough line should be preserved"
        );
        assert!(summary_idx != usize::MAX, "summary should exist");
        assert!(
            pass_idx < summary_idx,
            "passthrough should appear before summary: pass_idx={pass_idx}, summary_idx={summary_idx}"
        );
    }
}
