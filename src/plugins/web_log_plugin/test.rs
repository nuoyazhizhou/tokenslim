//! web_log_plugin 测试模块（文件驱动，严禁 Hardcode）
#[cfg(test)]
mod tests {
    use crate::core::dictionary_engine::Dictionary;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::*;
    use crate::plugins::web_log_plugin::WebLogPlugin;

    /// 测试辅助：压缩样例并断言。
    fn compress_case(case_id: &str) -> (String, String) {
        let plugin = WebLogPlugin::new();
        let raw = read_sample_log("web_log_plugin", case_id);
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        (raw, out)
    }

    /// 测试辅助：断言 ROI 不扩张。
    fn assert_roi(raw: &str, out: &str) {
        assert!(
            out.len() <= raw.len() + 4,
            "web_log 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：访问日志样例被识别。
    #[test]
    fn detects_access_case() {
        let plugin = WebLogPlugin::new();
        let raw = read_sample_log("web_log_plugin", "case_001_access");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 测试：错误日志样例压缩不扩张。
    #[test]
    fn compresses_error_case_without_expansion() {
        let (raw, out) = compress_case("case_002_error");
        assert_roi(&raw, &out);
    }

    /// 测试：AWS logs tail 健康样例被聚合。
    #[test]
    fn aggregates_aws_logs_tail_health_case() {
        let (raw, out) = compress_case("case_013_aws_logs_tail_health");
        assert!(out.contains("$W|SUMMARY"));
        assert!(out.contains("GET /health"));
        assert!(out.contains("records=10"));
        assert!(out.len() < raw.len());
    }

    /// 测试：CloudWatch Insights 表格样例被聚合。
    #[test]
    fn aggregates_cloudwatch_insights_table_case() {
        let (raw, out) = compress_case("case_014_cloudwatch_insights_health_table");
        assert!(out.contains("$W|SUMMARY"));
        assert!(!out.contains("|   timestamp"));
        assert!(out.len() < raw.len());
    }

    /// 测试：错误状态保留为异常。
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

    /// 测试：Nginx 维度被聚合。
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

    /// 门控测试：小样本纯扫描（单一 BadBot 源探测敏感路径）应触发 v3 SCAN 聚合。
    ///
    /// 方案 B：v3 IR 触发条件由「记录数 ≥24」放宽为「记录数 ≥24 或存在真实扫描组」，
    /// 使 case_021（20 条）这类小样本攻击扫描走 SCAN 聚合，将逐路径 ANOMALY 折叠为
    /// 单条 SCAN 行，同时抑制纯扫描源的冗余 ANOMALY。
    #[test]
    fn collapses_small_scan_into_scan_row() {
        let (raw, out) = compress_case("case_021_apache_combined_bot");
        // 扫描被聚合为单条 SCAN 行。
        assert!(out.contains("!$W|SCAN"), "应折叠出 SCAN 行:\n{out}");
        assert!(
            out.contains("hits=8"),
            "SCAN 应统计 BadBot 探测命中数:\n{out}"
        );
        assert!(
            out.contains("targets=7"),
            "SCAN 应列出全部探测目标数:\n{out}"
        );
        assert!(out.contains("ua=$UA"), "SCAN 应标识 BadBot UA:\n{out}");
        // 纯扫描源的逐路径 ANOMALY 已抑制，避免与 SCAN 行重复。
        assert!(
            !out.contains("!$W|ANOMALY"),
            "纯扫描源 ANOMALY 应被 SCAN 行替代:\n{out}"
        );
        // 正常流量例行行（Googlebot/products）与摘要仍需保留。
        assert!(out.contains("$W|ROUTINE"), "应保留正常例行行:\n{out}");
        assert!(out.contains("$W|SUMMARY"), "应保留访问摘要:\n{out}");
        assert!(out.contains("4xx=8"), "SUMMARY 应正确计数 4xx:\n{out}");
        // ROI 不扩张。
        assert_roi(&raw, &out);
    }

    /// 回归测试：跨多 IP 的分布式扫描应归并为单条 SCAN 行。
    ///
    /// case_017 的攻击源分散在多个 IP（curl/masscan/python-requests/Go-http-client），
    /// 单个 IP|UA 组不足以触发单组扫描阈值。方案 B 补丁在窗口级识别敏感探针源：
    /// 当 ≥2 源对敏感路径发起 ≥6 次探测时，将整片归并为单条 SCAN 行并把全部扫描
    /// 源 IP 纳入抑制，避免聚合退化为逐行透传。
    #[test]
    fn coalesces_distributed_scan_into_scan_row() {
        let (raw, out) = compress_case("case_017_nginx_404_scan");
        // 应选中聚合（SCAN）而非逐行 $W|A 透传。
        assert!(
            !out.contains("$W|A|"),
            "分布式扫描应走聚合而非逐行透传:\n{out}"
        );
        // 单条 SCAN 行覆盖整片敏感探针（4 源、17 次命中、6 个目标）。
        assert!(out.contains("!$W|SCAN"), "应归并出 SCAN 行:\n{out}");
        assert!(
            out.contains("source=Mixed(4)"),
            "SCAN 应表达多源扫描:\n{out}"
        );
        assert!(out.contains("hits=17"), "SCAN 应统计敏感探针命中数:\n{out}");
        assert!(out.contains("targets=6"), "SCAN 应统计敏感目标数:\n{out}");
        // 扫描源 token 已建立，摘要保留 4xx 计数。
        assert!(out.contains("$IP_ATK"), "应标识扫描源 token:\n{out}");
        assert!(out.contains("4xx=20"), "SUMMARY 应正确计数 4xx:\n{out}");
        // ROI 收缩。
        assert_roi(&raw, &out);
        assert!(out.len() < raw.len(), "分布式扫描应压缩:\n{out}");
    }

    /// 测试：5xx 尖峰保持可见。
    #[test]
    fn keeps_5xx_spike_visible() {
        let (_raw, out) = compress_case("case_018_nginx_5xx_spike");
        assert!(out.contains("5xx=12"));
        assert!(out.contains("500"));
        assert!(out.contains("502"));
        assert!(out.contains("503"));
        assert!(out.contains("!$W|ANOMALY"));
    }

    /// 测试：慢请求被突出显示。
    #[test]
    fn highlights_slow_requests() {
        let (_raw, out) = compress_case("case_019_nginx_slow_requests");
        assert!(out.contains("!$W|SLOW"));
        assert!(out.contains("/api/search"));
        assert!(out.contains("ms="));
    }

    /// 测试：支持 Apache common 与 combined 格式。
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

    /// 测试：支持云包裹格式。
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

    /// 测试：uvicorn 访问行（`INFO: ip:port - "METHOD path" status`）无 User-Agent 字段，
    /// 日志级别前缀不得被误当作 UA 计入 TOP_UA（回归 case_027）。
    #[test]
    fn does_not_mistake_uvicorn_level_as_ua() {
        let (_raw, out) = compress_case("case_027_aws_csv_uvicorn");
        assert!(out.contains("$W|SUMMARY"));
        assert!(out.contains("$W|TOP_UA"));
        // ua 应为 "-"（无），而非日志级别串 "INFO"；字典行也绝不出现 "$UAx=INFO" 条目。
        // （ANOMALY 的 sample 字段保留的原始行片段含 "INFO:" 属合理追溯信息，不在本断言范围内。）
        let top_ua_line = out.lines().find(|l| l.contains("$W|TOP_UA|")).unwrap_or("");
        assert!(
            top_ua_line.contains("|TOP_UA|-") || top_ua_line.contains("-:"),
            "TOP_UA 应折叠为无 UA（-），而非日志级别 INFO:\n{top_ua_line}"
        );
        assert!(
            !out.contains("=INFO"),
            "字典不应出现 $UAx=INFO 条目:\n{out}"
        );
    }

    /// 测试：无表头的单行云 CSV 包装器（`timestamp,provider,"message"`，CSV 双引号转义）
    /// 也能被解析，包装行内的 5xx/4xx 信号聚合进 records，而不落为透传行。
    #[test]
    fn parses_inline_cloud_csv_wrapper_anomaly() {
        let (_raw, out) = compress_case("case_036_mixed_cloud_wrappers");
        assert!(out.contains("$W|SUMMARY"), "应输出 web 摘要:\n{out}");
        // 包装行中的 POST /api/status 500 应被计入 5xx 统计，并作为 ANOMALY 上报。
        assert!(out.contains("5xx=1"), "500 信号应聚合进 records:\n{out}");
        assert!(
            out.contains("!$W|ANOMALY|500"),
            "500 应上报为 ANOMALY，而非丢失或透传:\n{out}"
        );
        // 500 包装行不应再以原始 CSV 双引号包装形式作为透传行原样输出。
        // （ANOMALY 的 sample 字段会以规范化单引号呈现源行，故此处仅校验原始包装形态已消失。）
        assert!(
            !out.contains("\"\"POST /api/status"),
            "包装行不应作为透传行原样输出:\n{out}"
        );
    }

    /// 测试：路由 ID 被归一化。
    #[test]
    fn normalizes_route_ids() {
        let (_raw, out) = compress_case("case_031_route_id_normalization");
        assert!(out.contains("/api/orders/:id"));
    }

    /// 测试：v3 折叠 health/static 并输出字典。
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

    /// 测试：v3 突出扫描与突发。
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

    /// 测试：v3 支持云包裹与慢路由。
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

    /// 测试：v3 泛化路由 ID 与 ALB 原生格式。
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

    /// 测试：v3 支持 W3C 边缘与 IIS 格式。
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

    /// 测试：v3 支持 Envoy/Istio 访问日志。
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

    /// 测试：纯健康汇总使用紧凑分支。
    #[test]
    fn health_only_summary_uses_compact_branch() {
        let (_raw, out) = compress_case("case_013_aws_logs_tail_health");
        assert!(out.contains("$W|SUMMARY|records=10"));
        assert!(out.contains("|4xx=0|5xx=0|"));
        assert!(!out.contains("window="));
    }

    /// 测试：透传行保留在摘要之前。
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

    /// 测试：聚合摘要的局部 DICT_IP/DICT_UA 在 decompress 时被行内替换，
    /// 且局部字典行（$W|DICT_*）不泄漏到输出（T-011 根治）。
    #[test]
    fn decompress_resolves_local_dict_and_forbids_dict_line_leak() {
        let plugin = WebLogPlugin::new();
        // 空全局字典；聚合 token 全部由压缩侧写入的局部协议行承载。
        let dict = Dictionary::new();
        let compressed = concat!(
            "$W|SUMMARY|records=24|ips=5\n",
            "$W|DICT_IP|$IP1=10.0.0.52(Internal),$IP2=10.0.0.53(Internal),$IP_ATK=203.0.113.45(Documentation/Scanner)\n",
            "$W|DICT_UA|$UA_BROWSER=Mozilla/5.0(Browser),$UA_KUBE=kube-probe/1.29(Health/Kubernetes)\n",
            "$W|ROUTINE|kind=static|200 OK|GET /static/app.css|count=4|ips=$IP1|ua=$UA_BROWSER|avg_ms=11\n",
            "!$W|SCAN|source=$IP_ATK|ua=$UA_KUBE|window=..|targets=5|sample=/.env\n",
            "$W|A|$IP2|2026-01-01|GET|/x|200|10|-|$UA_KUBE\n",
        );
        let out = plugin.decompress(compressed, &dict);
        // 局部协议行禁止泄漏
        assert!(
            !out.contains("$W|DICT_"),
            "DICT 行不得泄漏到解压输出: {out}"
        );
        // 无 $IP / $UA token 残留（含 $IP_ATK 这类非数字后缀 token）
        assert!(!out.contains("$IP"), "不得残留 $IP token: {out}");
        assert!(!out.contains("$UA"), "不得残留 $UA token: {out}");
        // 摘要行行内替换生效
        assert!(out.contains("10.0.0.52"), "IP1 应被还原: {out}");
        assert!(out.contains("10.0.0.53"), "IP2 应被还原: {out}");
        assert!(out.contains("203.0.113.45"), "IP_ATK 应被还原: {out}");
        assert!(out.contains("Mozilla/5.0"), "UA_BROWSER 应被还原: {out}");
        assert!(out.contains("kube-probe/1.29"), "UA_KUBE 应被还原: {out}");
        // $W|A| 行式还原也走局部映射优先路径
        assert!(
            out.contains(
                "10.0.0.53 - - [2026-01-01] \"GET /x HTTP/1.1\" 200 10 \"-\" \"kube-probe/1.29\""
            ),
            "A 行应还原: {out}"
        );
    }

    /// 测试：case_045 式 R1/R3 重叠语义——$W|ROUTINE 的 ips=Mixed(N) 不是 token（保持不变），
    /// 但 SCAN 引用的 $IP_ATK/$UA_REQ 必须被局部字典还原，且 DICT 行不泄漏（R1 全清、R3 低保留率独立）。
    #[test]
    fn decompress_case045_overlap_resolves_tokens_without_dict_leak() {
        let plugin = WebLogPlugin::new();
        let dict = Dictionary::new();
        let compressed = concat!(
            "$W|SUMMARY|records=24|4xx=5|5xx=3\n",
            "$W|DICT_IP|$IP1=198.51.100.10(Documentation/Edge),$IP_ATK=203.0.113.45(Documentation/Scanner)\n",
            "$W|DICT_UA|$UA_ELB=ELB-HealthChecker/2.0(Health/ALB),$UA_REQ=python-requests/2.31(Bot/Script)\n",
            "$W|ROUTINE|kind=health|200 OK|GET /health|count=6|ips=$IP1|ua=$UA_ELB|avg_ms=9\n",
            "$W|ROUTINE|kind=static|200 OK|GET /static/app.css|count=4|ips=Mixed(4)|ua=$UA_ELB|avg_ms=27\n",
            "!$W|SCAN|source=$IP_ATK|ua=$UA_REQ|window=..|targets=5|sample=/.env\n",
        );
        let out = plugin.decompress(compressed, &dict);
        assert!(!out.contains("$W|DICT_"), "DICT 行不得泄漏: {out}");
        assert!(!out.contains("$IP"), "不得残留 $IP token: {out}");
        assert!(!out.contains("$UA"), "不得残留 $UA token: {out}");
        // Mixed(N) 等非 token 片段原样保留（R3 低保留率语义不影响 R1 可逆性）
        assert!(out.contains("Mixed(4)"), "Mixed(N) 片段应保留: {out}");
        // 被引用的 token 全部还原
        assert!(out.contains("198.51.100.10"), "IP1 应还原: {out}");
        assert!(out.contains("203.0.113.45"), "IP_ATK 应还原: {out}");
        assert!(
            out.contains("ELB-HealthChecker/2.0"),
            "UA_ELB 应还原: {out}"
        );
        assert!(out.contains("python-requests/2.31"), "UA_REQ 应还原: {out}");
    }
}
