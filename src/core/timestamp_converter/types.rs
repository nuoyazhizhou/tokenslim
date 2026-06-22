//! 时间戳转换器类型定义

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;

#[allow(dead_code)]
static TIMESTAMP_REGEXES: Lazy<Vec<(Regex, TimestampFormat)>> = Lazy::new(|| {
    vec![
        // ISO 8601: [2026-03-05T02:52:31.597Z]
        (
            Regex::new(r"\[\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z\]").unwrap(),
            TimestampFormat::Iso8601,
        ),
        // HH:MM:SS
        (
            Regex::new(r"\b\d{2}:\d{2}:\d{2}\b").unwrap(),
            TimestampFormat::TimeOnly,
        ),
        // Unix 毫秒时间戳
        (
            Regex::new(r"\b\d{13}\b").unwrap(),
            TimestampFormat::UnixMillis,
        ),
    ]
});

/// 时间戳转换器，用于识别并归一化日志中的绝对时间。
pub struct TimestampConverter {
    /// 基准时间戳（流中遇到的第一个合法时间点）
    base_timestamp: Option<DateTime<Utc>>,
    /// 自动探测到的时间戳格式
    format: TimestampFormat,
}

/// 支持自动识别的时间戳字符串格式。
#[derive(Debug, Clone, PartialEq)]
pub enum TimestampFormat {
    /// 完整 ISO 8601 格式，如 `[2026-03-05T02:52:31.597Z]`
    Iso8601,
    /// 仅包含时间的分秒格式，如 `17:37:26`
    TimeOnly,
    /// 13 位 Unix 毫秒时间戳数字
    UnixMillis,
    /// 尚未识别或不支持的格式
    Unknown,
}

impl TimestampConverter {
    /// 创建一个新的时间戳转换器，状态为空。
    pub fn new() -> Self {
        Self {
            base_timestamp: None,
            format: TimestampFormat::Unknown,
        }
    }

    pub fn base_timestamp(&self) -> Option<DateTime<Utc>> {
        self.base_timestamp
    }

    pub fn set_base_timestamp(&mut self, base: Option<DateTime<Utc>>) {
        self.base_timestamp = base;
        self.format = TimestampFormat::Iso8601;
    }

    /// 快速探测给定字符串片段符合哪种时间戳格式。
    pub fn detect_format(&self, text: &str) -> TimestampFormat {
        // ISO 8601: [2026-03-05T02:52:31.597Z]
        if text.contains('T') && text.contains('Z') && text.len() == 24 {
            return TimestampFormat::Iso8601;
        }

        // HH:MM:SS: 17:37:26
        if text.len() == 8 && text.chars().filter(|c| *c == ':').count() == 2 {
            return TimestampFormat::TimeOnly;
        }

        // Unix 毫秒时间戳（13 位数字）
        if text.chars().filter(|c| c.is_ascii_digit()).count() == 13 {
            return TimestampFormat::UnixMillis;
        }

        TimestampFormat::Unknown
    }

    /// 尝试将各种格式的字符串解析为 UTC 时间对象。
    pub fn extract_timestamp(&self, text: &str) -> Option<DateTime<Utc>> {
        // 尝试解析标准的 RFC 3339 / ISO 8601 格式
        if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
            return Some(dt.with_timezone(&Utc));
        }

        // 处理 HH:MM:SS 格式（假定日期为系统今天的 UTC 日期）
        if text.len() == 8 && text.chars().filter(|c| *c == ':').count() == 2 {
            if let Ok(dt) = chrono::NaiveTime::parse_from_str(text, "%H:%M:%S") {
                let today = Utc::now().date_naive();
                let datetime = today.and_time(dt).and_local_timezone(Utc).unwrap();
                return Some(datetime);
            }
        }

        // 处理纯数字的 Unix 毫秒级时间戳
        if let Ok(millis) = text.parse::<i64>() {
            if millis > 1000000000000 {
                // 判断为合理的毫秒量级（非秒级）
                return DateTime::from_timestamp_millis(millis);
            }
        }

        None
    }

    /// 预处理单行文本。剥离时间戳、[Pipeline] 等通用前缀。
    pub fn convert_line<'a>(
        &mut self,
        line: std::borrow::Cow<'a, str>,
    ) -> std::borrow::Cow<'a, str> {
        let (prefix_opt, rest) = self.extract_prefixes_and_rest(line.clone());
        if let Some(prefix) = prefix_opt {
            let mut result = String::with_capacity(line.len());
            result.push_str(&prefix);
            if !rest.is_empty() {
                result.push(' ');
                result.push_str(&rest);
            }
            std::borrow::Cow::Owned(result)
        } else {
            line
        }
    }

    pub fn extract_prefixes_and_rest<'a>(
        &mut self,
        line: std::borrow::Cow<'a, str>,
    ) -> (Option<String>, std::borrow::Cow<'a, str>) {
        if line.is_empty() {
            return (None, line);
        }

        let mut rest = line.as_ref();
        let mut prefix_tokens = Vec::new();

        if rest.starts_with('[') && rest.len() >= 20 {
            if let Some(bracket_end) = rest[1..30.min(rest.len())].find(']') {
                let ts_raw = &rest[1..bracket_end + 1];
                if ts_raw.contains('T') && (ts_raw.contains(':') || ts_raw.contains('-')) {
                    let mut parse_str = ts_raw.to_string();
                    if !parse_str.ends_with('Z') {
                        parse_str.push('Z');
                    }

                    if let Ok(dt) = DateTime::parse_from_rfc3339(&parse_str) {
                        let dt_utc = dt.with_timezone(&Utc);
                        if self.base_timestamp.is_none() {
                            self.base_timestamp = Some(dt_utc);
                            self.format = TimestampFormat::Iso8601;
                        }

                        if let Some(base) = self.base_timestamp {
                            let ms = (dt_utc - base).num_milliseconds();
                            prefix_tokens.push(format!("[T+{}ms]", ms));
                            rest = rest[bracket_end + 2..].trim_start();
                        }
                    }
                }
            }
        }

        if rest.starts_with("[Pipeline]") {
            prefix_tokens.push("$PL".to_string());
            rest = rest[10..].trim_start();
        }

        if prefix_tokens.is_empty() {
            return (None, line);
        }

        let prefix_str = prefix_tokens.join(" ");
        (Some(prefix_str), std::borrow::Cow::Owned(rest.to_string()))
    }

    /// 重置基准时间戳（用于切换处理新文件或新流）。
    pub fn reset(&mut self) {
        self.base_timestamp = None;
        self.format = TimestampFormat::Unknown;
    }
}

impl Default for TimestampConverter {
    fn default() -> Self {
        Self::new()
    }
}
