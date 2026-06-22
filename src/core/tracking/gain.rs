//! Gain 报告渲染 — 终端输出与 JSON 序列化
//!
//! 提供 summary / daily / by_filter 三种报告维度的渲染函数。
//! 参考: TOKF `other/tokf/crates/tokf-cli/src/gain.rs` + `gain_render/`

use super::tracker::Tracker;
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

fn load_pricing_config() -> PricingConfig {
    let toml_str = fs::read_to_string("config/plugins.toml").unwrap_or_default();
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
        Err(e) => return format!("无法打开追踪数据库: {e}"),
    };

    let summary = match tracker.get_summary() {
        Ok(s) => s,
        Err(e) => return format!("查询统计失败: {e}"),
    };

    if summary.total_commands == 0 {
        return "尚未记录任何压缩执行。运行 `tokenslim run <command>` 开始节省 Token！".to_string();
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
        "TokenSlim 累计节省报告\n\
         ========================\n\
         \n\
         使用统计:\n\
           总执行次数:        {}\n\
           输入 Token:        {}\n\
           输出 Token:        {}\n\
           节省 Token:        {}\n\
           总体压缩率:        {:.1}%\n\
         \n\
         价值估算:\n\
           节省 Token 总数:   {} tokens\n",
        format_num(summary.total_commands),
        format_tokens(summary.total_input_tokens),
        format_tokens(summary.total_output_tokens),
        format_tokens(tokens_saved),
        ratio,
        format_num(tokens_saved)
    );

    let default_model = pricing_config.default.unwrap_or_else(|| "claude-4.8".to_string());
    
    // Default model first, then the rest
    if let Some(&price) = models.get(&default_model) {
        let estimated_usd = (tokens_saved as f64 / 1_000_000.0) * price;
        out.push_str(&format!(
            "           {:<16} ${:.2} USD (${:.2}/1M)\n",
            format!("{}:", default_model), estimated_usd, price
        ));
    }

    for (model, price) in &models {
        if model == &default_model {
            continue;
        }
        let estimated_usd = (tokens_saved as f64 / 1_000_000.0) * price;
        out.push_str(&format!(
            "           {:<16} ${:.2} USD (${:.2}/1M)\n",
            format!("{}:", model), estimated_usd, price
        ));
    }

    out.push_str("\n         * 价值按 config/plugins.toml 中的配置估算\n");
    out
}

/// 渲染按日报告（纯文本）
pub fn render_gain_report_daily(days: i64) -> String {
    let tracker = match Tracker::open_default() {
        Ok(t) => t,
        Err(e) => return format!("无法打开追踪数据库: {e}"),
    };

    let daily = match tracker.get_daily(days) {
        Ok(d) => d,
        Err(e) => return format!("查询按日统计失败: {e}"),
    };

    if daily.is_empty() {
        return format!("最近 {days} 天无记录。");
    }

    let mut out = format!("TokenSlim 按日统计 (最近 {days} 天)\n");
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
        Err(e) => return format!("无法打开追踪数据库: {e}"),
    };

    let filters = match tracker.get_by_filter() {
        Ok(f) => f,
        Err(e) => return format!("查询按过滤器统计失败: {e}"),
    };

    if filters.is_empty() {
        return "无过滤器记录。".to_string();
    }

    let mut out = String::from("TokenSlim 按过滤器统计\n");
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

    #[test]
    fn test_format_num_zero() {
        assert_eq!(format_num(0), "0");
    }

    #[test]
    fn test_format_num_small() {
        assert_eq!(format_num(999), "999");
    }

    #[test]
    fn test_format_num_thousand() {
        assert_eq!(format_num(1000), "1,000");
    }

    #[test]
    fn test_format_num_large() {
        assert_eq!(format_num(84320), "84,320");
    }

    #[test]
    fn test_format_num_negative() {
        assert_eq!(format_num(-73080), "-73,080");
    }

    #[test]
    fn test_format_num_million() {
        assert_eq!(format_num(1_234_567), "1,234,567");
    }

    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(500), "500");
    }

    #[test]
    fn test_format_tokens_k() {
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(59_234), "59.2K");
    }

    #[test]
    fn test_format_tokens_m() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(1_234_567), "1.2M");
    }

    #[test]
    fn test_format_tokens_negative() {
        assert_eq!(format_tokens(-1000), "-1.0K");
    }

    #[test]
    fn test_format_bytes_b() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(2048), "2.00 KB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
    }

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
    }
}
