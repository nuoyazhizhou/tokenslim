//! 时间戳转换器类型定义

use chrono::{DateTime, Utc};

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

    /// 返回已探测到的基准时间戳（若存在），后续相对时间偏移均以此为锚点计算。
    pub fn base_timestamp(&self) -> Option<DateTime<Utc>> {
        self.base_timestamp
    }

    /// 设置基准时间戳并将格式固定为 ISO8601，用于把后续绝对时间转换为相对偏移。
    pub fn set_base_timestamp(&mut self, base: Option<DateTime<Utc>>) {
        self.base_timestamp = base;
        self.format = TimestampFormat::Iso8601;
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

    /// 从一行中提取中括号包裹的时间戳前缀并返回剩余文本；首次命中时自动记录基准时间。
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
            // P2-57（D-1）：旧版 `rest[1..30.min(rest.len())]` 的字节 30 可切断多字节字符
            // 生产 panic（CJK 日志热路径）。改为先按 char 边界找 `]` 再校验位置，
            // 全程不触碰非边界字节。
            if let Some(bracket_off) = rest[1..].find(']') {
                if bracket_off < 29 {
                    let bracket_end = bracket_off + 1; // `]` 的字节偏移
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
                                // 从 `]` 后 trim，避免 `[bracket_end + 2..]` 在多字节字符
                                // 中间切片（原版还多跳 1 字节，可能误吃下一个字符）
                                rest = rest[bracket_end + 1..].trim_start();
                            }
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
    /// 返回时间戳转换器默认配置（基准时间为空、格式未知）。
    fn default() -> Self {
        Self::new()
    }
}
