//! Token 追踪系统的数据类型定义

use serde::Serialize;

/// 单次命令执行的追踪记录
#[derive(Debug, Clone)]
pub struct TrackingEvent {
    /// 原始命令行 (如 "git status")
    pub command: String,
    /// 匹配的过滤器名称 (如 "vcs_git")
    pub filter_name: Option<String>,
    /// 原始输入字节数
    pub input_bytes: i64,
    /// 压缩后输出字节数
    pub output_bytes: i64,
    /// 原始输入 Token 估算值 (bytes / 4)
    pub input_tokens: i64,
    /// 压缩后输出 Token 估算值 (bytes / 4)
    pub output_tokens: i64,
    /// 过滤器处理耗时 (毫秒)
    pub filter_time_ms: i64,
    /// 子进程退出码
    pub exit_code: i32,
    /// 项目标识 (当前工作目录名)
    pub project: String,
}

impl TrackingEvent {
    /// 从原始数据构造追踪事件，自动估算 Token 数量
    pub fn new(
        command: &str,
        filter_name: Option<&str>,
        input_bytes: usize,
        output_bytes: usize,
        exit_code: i32,
    ) -> Self {
        let ib = input_bytes as i64;
        let ob = output_bytes as i64;
        Self {
            command: command.to_string(),
            filter_name: filter_name.map(String::from),
            input_bytes: ib,
            output_bytes: ob,
            input_tokens: ib / 4,
            output_tokens: ob / 4,
            filter_time_ms: 0,
            exit_code,
            project: current_project_name(),
        }
    }

    /// 设置过滤器处理耗时
    pub fn with_filter_time(mut self, ms: i64) -> Self {
        self.filter_time_ms = ms;
        self
    }

    /// Token 节省数
    pub fn tokens_saved(&self) -> i64 {
        (self.input_tokens - self.output_tokens).max(0)
    }

    /// 节省百分比
    pub fn savings_pct(&self) -> f64 {
        if self.input_tokens == 0 {
            return 0.0;
        }
        (self.tokens_saved() as f64 / self.input_tokens as f64) * 100.0
    }
}

/// 获取当前项目名称（当前目录名）
fn current_project_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default()
}

/// 聚合统计总览
#[derive(Debug, Clone, Serialize)]
pub struct GainSummary {
    /// 总命令执行次数
    pub total_commands: i64,
    /// 总输入 Token
    pub total_input_tokens: i64,
    /// 总输出 Token
    pub total_output_tokens: i64,
    /// 节省 Token 总数
    pub tokens_saved: i64,
    /// 总体节省百分比
    pub savings_pct: f64,
    /// 总过滤器耗时 (毫秒)
    pub total_filter_time_ms: i64,
    /// 平均过滤器耗时 (毫秒)
    pub avg_filter_time_ms: f64,
}

/// 按日聚合统计
#[derive(Debug, Clone, Serialize)]
pub struct DailyGain {
    /// 日期 (YYYY-MM-DD)
    pub date: String,
    /// 命令执行次数
    pub commands: i64,
    /// 输入 Token
    pub input_tokens: i64,
    /// 输出 Token
    pub output_tokens: i64,
    /// 节省 Token
    pub tokens_saved: i64,
    /// 节省百分比
    pub savings_pct: f64,
    /// 过滤器耗时 (毫秒)
    pub total_filter_time_ms: i64,
}

/// 按过滤器聚合统计
#[derive(Debug, Clone, Serialize)]
pub struct FilterGain {
    /// 过滤器名称
    pub filter_name: String,
    /// 使用次数
    pub commands: i64,
    /// 输入 Token
    pub input_tokens: i64,
    /// 输出 Token
    pub output_tokens: i64,
    /// 节省 Token
    pub tokens_saved: i64,
    /// 节省百分比
    pub savings_pct: f64,
    /// 过滤器耗时 (毫秒)
    pub total_filter_time_ms: i64,
    /// 平均耗时 (毫秒)
    pub avg_filter_time_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracking_event_new() {
        let event = TrackingEvent::new("git status", Some("vcs_git"), 1024, 256, 0);
        assert_eq!(event.command, "git status");
        assert_eq!(event.filter_name.as_deref(), Some("vcs_git"));
        assert_eq!(event.input_bytes, 1024);
        assert_eq!(event.output_bytes, 256);
        assert_eq!(event.input_tokens, 256); // 1024 / 4
        assert_eq!(event.output_tokens, 64); // 256 / 4
        assert_eq!(event.tokens_saved(), 192); // 256 - 64
        assert!((event.savings_pct() - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_tracking_event_zero_input() {
        let event = TrackingEvent::new("empty", None, 0, 0, 0);
        assert_eq!(event.tokens_saved(), 0);
        assert!((event.savings_pct() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_tracking_event_no_savings() {
        let event = TrackingEvent::new("passthrough", None, 100, 100, 0);
        assert_eq!(event.tokens_saved(), 0);
    }

    #[test]
    fn test_tracking_event_with_filter_time() {
        let event =
            TrackingEvent::new("cargo build", Some("gcc_log"), 4096, 512, 0).with_filter_time(15);
        assert_eq!(event.filter_time_ms, 15);
    }
}
