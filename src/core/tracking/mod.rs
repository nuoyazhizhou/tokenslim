//! Token 追踪系统 — SQLite 持久化的 Token 节省记录与多维统计
//!
//! 本模块提供每次 `tokenslim run` 执行的详细追踪记录，包括：
//! - 命令行原文、匹配的过滤器名称
//! - 输入/输出字节数和 Token 估算值
//! - 执行耗时与退出码
//! - 按日/过滤器/项目的多维聚合统计
//!
//! ## 存储
//!
//! - 位置: `%APPDATA%/tokenslim/tracking.db` (Windows) 或 `~/.local/share/tokenslim/tracking.db` (Unix)
//! - 引擎: SQLite (rusqlite bundled)
//! - 保留策略: 90 天自动清理
//!
//! ## 用法
//!
//! ```ignore
//! use tokenslim::core::tracking::{Tracker, record_command};
//!
//! // 快速记录一次执行
//! record_command("git status", Some("vcs_git"), 1024, 256);
//!
//! // 使用 Tracker 进行多维查询
//! let tracker = Tracker::open_default().unwrap();
//! let summary = tracker.get_summary().unwrap();
//! println!("累计节省: {} tokens", summary.tokens_saved);
//! ```
//!
//! 参考: RTK `other/rtk/src/core/tracking.rs`, TOKF `other/tokf/crates/tokf-common/src/tracking/`

pub mod gain;
mod tracker;
mod types;

pub use gain::{
    render_gain_report_by_filter, render_gain_report_daily, render_gain_report_summary,
};
pub use tracker::{record_command, Tracker};
pub use types::{DailyGain, FilterGain, GainSummary, TrackingEvent};
