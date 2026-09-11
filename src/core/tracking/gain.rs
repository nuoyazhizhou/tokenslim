//! Gain 报告渲染 — 终端输出与 JSON 序列化
//!
//! 提供 summary / daily / by_filter 三种报告维度的渲染函数。
//! 参考: TOKF `other/tokf/crates/tokf-cli/src/gain.rs` + `gain_render/`

use super::tracker::Tracker;
use crate::utils::i18n::{t, t1};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize, Default)]
struct PricingConfig {
    default: Option<String>,
    models: Option<HashMap<String, f64>>,
}

#[derive(Debug, Deserialize, Default)]
struct ConfigRoot {
    pricing: Option<PricingConfig>,
}

/// 从配置目录的 plugins.toml 读取定价配置；文件缺失或解析失败时返回默认空配置。
/// P3-52/P3-57：路径锚定统一配置基准（`PluginConfigLoader::resolve_config_dir`），
/// 不再依赖 CWD 相对路径——换目录运行时定价配置静默失效。
fn load_pricing_config() -> PricingConfig {
    let pricing_path = crate::core::plugin_config_loader::PluginConfigLoader::resolve_config_dir()
        .join("plugins.toml");
    let toml_str = fs::read_to_string(pricing_path).unwrap_or_default();
    let root: ConfigRoot = toml::from_str(&toml_str).unwrap_or_default();
    root.pricing.unwrap_or_default()
}

const E_GAIN_JSON_SERIALIZE: &str = "E_GAIN_JSON_SERIALIZE";

/// 格式化数字 — 千分位分隔
pub fn format_num(n: i64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let neg = n < 0;
    let s = n.abs().to_string();
    let mut result = String::new();
    let mut count = 0;
    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }
    if neg {
        result.push('-');
    }
    result.chars().rev().collect()
}

/// 格式化 Token 数量 — K/M 后缀
pub fn format_tokens(n: i64) -> String {
    let abs_n = n.unsigned_abs();
    let sign = if n < 0 { "-" } else { "" };
    if abs_n >= 1_000_000 {
        format!("{}{:.1}M", sign, abs_n as f64 / 1_000_000.0)
    } else if abs_n >= 1_000 {
        format!("{}{:.1}K", sign, abs_n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// 格式化字节数 — B/KB/MB/GB
pub fn format_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.2} KB", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.2} MB", n as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", n as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}

/// 渲染总览报告（纯文本）
pub fn render_gain_report_summary() -> String {
    let tracker = match Tracker::open_default() {
        Ok(t) => t,
        Err(e) => return t1("tracking_open_failed", e),
    };

    let summary = match tracker.get_summary() {
        Ok(s) => s,
        Err(e) => return t1("gain_report_query_stats_failed", e),
    };

    if summary.total_commands == 0 {
        return t("gain_report_no_records").to_string();
    }

    let tokens_saved = summary.tokens_saved;
    let ratio = summary.savings_pct;

    let pricing_config = load_pricing_config();
    let models = pricing_config.models.unwrap_or_else(|| {
        let mut default_models = HashMap::new();
        default_models.insert("claude-4.8".to_string(), 5.00);
        default_models.insert("gpt-5.5".to_string(), 5.00);
        default_models.insert("gemini-3.1-pro".to_string(), 2.00);
        default_models
    });

    let mut out = format!(
        "{}\n\
         ========================\n\
         \n\
         {}\n\
         {}\n\
         {}\n\
         {}\n\
         {}\n\
         {}\n\
         \n\
         {}\n\
         {}\n",
        t("gain_report_title_summary"),
        t("gain_report_usage_stats"),
        t1(
            "gain_report_usage_total_commands",
            format_num(summary.total_commands)
        ),
        t1(
            "gain_report_usage_input_tokens",
            format_tokens(summary.total_input_tokens)
        ),
        t1(
            "gain_report_usage_output_tokens",
            format_tokens(summary.total_output_tokens)
        ),
        t1("gain_report_usage_saved_tokens", format_tokens(tokens_saved)),
        t1("gain_report_usage_ratio", format!("{:.1}", ratio)),
        t("gain_report_value_estimate"),
        t1(
            "gain_report_value_total_saved",
            format_num(tokens_saved)
        ),
    );

    let default_model = pricing_config
        .default
        .unwrap_or_else(|| "claude-4.8".to_string());

    // Default model first, then the rest
    if let Some(&price) = models.get(&default_model) {
        let estimated_usd = (tokens_saved as f64 / 1_000_000.0) * price;
        out.push_str(&format!(
            "           {:<16} ${:.2} USD (${:.2}/1M)\n",
            format!("{}:", default_model),
            estimated_usd,
            price
        ));
    }

    for (model, price) in &models {
        if model == &default_model {
            continue;
        }
        let estimated_usd = (tokens_saved as f64 / 1_000_000.0) * price;
        out.push_str(&format!(
            "           {:<16} ${:.2} USD (${:.2}/1M)\n",
            format!("{}:", model),
            estimated_usd,
            price
        ));
    }

    out.push_str(&format!("\n         {}\n", t("gain_report_pricing_note")));
    out
}

/// 渲染按日报告（纯文本）
pub fn render_gain_report_daily(days: i64) -> String {
    let tracker = match Tracker::open_default() {
        Ok(t) => t,
        Err(e) => return t1("tracking_open_failed", e),
    };

    let daily = match tracker.get_daily(days) {
        Ok(d) => d,
        Err(e) => return t1("gain_report_query_daily_failed", e),
    };

    if daily.is_empty() {
        return t1("gain_report_no_daily_records", days);
    }

    let mut out = format!("{}\n", t1("gain_report_title_daily", days));
    out.push_str("========================================\n\n");

    for d in &daily {
        out.push_str(&format!(
            "  {}  runs: {:4}  saved: {} est. ({:.1}%)\n",
            d.date,
            d.commands,
            format_tokens(d.tokens_saved),
            d.savings_pct,
        ));
    }

    out
}

/// 渲染按过滤器报告（纯文本）
pub fn render_gain_report_by_filter() -> String {
    let tracker = match Tracker::open_default() {
        Ok(t) => t,
        Err(e) => return t1("tracking_open_failed", e),
    };

    let filters = match tracker.get_by_filter() {
        Ok(f) => f,
        Err(e) => return t1("gain_report_query_filter_failed", e),
    };

    if filters.is_empty() {
        return t("gain_report_no_filter_records").to_string();
    }

    let mut out = format!("{}\n", t("gain_report_title_by_filter"));
    out.push_str("========================\n\n");

    for f in &filters {
        out.push_str(&format!(
            "  {:30}  runs: {:4}  saved: {} est. ({:.1}%)\n",
            f.filter_name,
            f.commands,
            format_tokens(f.tokens_saved),
            f.savings_pct,
        ));
    }

    out
}

/// 获取总览统计的 JSON 字符串
pub fn render_gain_json() -> Result<String, String> {
    let tracker = Tracker::open_default()?;
    let summary = tracker.get_summary()?;
    serde_json::to_string_pretty(&summary).map_err(|e| format!("{E_GAIN_JSON_SERIALIZE}:{e}"))
}

/// 获取按日统计的 JSON 字符串
pub fn render_gain_daily_json(days: i64) -> Result<String, String> {
    let tracker = Tracker::open_default()?;
    let daily = tracker.get_daily(days)?;
    serde_json::to_string_pretty(&daily).map_err(|e| format!("{E_GAIN_JSON_SERIALIZE}:{e}"))
}

/// 获取按过滤器统计的 JSON 字符串
pub fn render_gain_by_filter_json() -> Result<String, String> {
    let tracker = Tracker::open_default()?;
    let filters = tracker.get_by_filter()?;
    serde_json::to_string_pretty(&filters).map_err(|e| format!("{E_GAIN_JSON_SERIALIZE}:{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：0 格式化为 "0"。
    #[test]
    fn test_format_num_zero() {
        assert_eq!(format_num(0), "0");
    }

    /// 测试：三位数以内不插入分隔符。
    #[test]
    fn test_format_num_small() {
        assert_eq!(format_num(999), "999");
    }

    /// 测试：1000 格式化为千分位 "1,000"。
    #[test]
    fn test_format_num_thousand() {
        assert_eq!(format_num(1000), "1,000");
    }

    /// 测试：大数 84320 格式化为 "84,320"。
    #[test]
    fn test_format_num_large() {
        assert_eq!(format_num(84320), "84,320");
    }

    /// 测试：负数 -73080 格式化为 "-73,080"。
    #[test]
    fn test_format_num_negative() {
        assert_eq!(format_num(-73080), "-73,080");
    }

    /// 测试：百万级数字 1234567 格式化为 "1,234,567"。
    #[test]
    fn test_format_num_million() {
        assert_eq!(format_num(1_234_567), "1,234,567");
    }

    /// 测试：小于 1000 的 token 数原样输出。
    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(500), "500");
    }

    /// 测试：1000 及以上格式化为 K 后缀（保留 1 位小数）。
    #[test]
    fn test_format_tokens_k() {
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(59_234), "59.2K");
    }

    /// 测试：100 万及以上格式化为 M 后缀。
    #[test]
    fn test_format_tokens_m() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(1_234_567), "1.2M");
    }

    /// 测试：负数 token 数带负号并格式化为 K 后缀。
    #[test]
    fn test_format_tokens_negative() {
        assert_eq!(format_tokens(-1000), "-1.0K");
    }

    /// 测试：小于 1024 字节原样输出 B。
    #[test]
    fn test_format_bytes_b() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    /// 测试：1024 字节格式化为 KB（保留 2 位小数）。
    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(2048), "2.00 KB");
    }

    /// 测试：1MB 字节格式化为 MB。
    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
    }

    /// 测试：1GB 字节格式化为 GB。
    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
    }
}
