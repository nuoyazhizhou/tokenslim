//! SQLite 追踪器 — 核心 CRUD 与聚合查询
//!
//! 提供 `Tracker` 结构体，封装所有 SQLite 操作。
//! 参考: RTK `other/rtk/src/core/tracking.rs`, TOKF `other/tokf/crates/tokf-common/src/tracking/`

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Deserialize;

use super::types::{DailyGain, FilterGain, GainSummary, TrackingEvent};

/// 默认数据保留天数
const DEFAULT_RETENTION_DAYS: i64 = 90;
const E_TRACKING_DB_PATH: &str = "E_TRACKING_DB_PATH";
const E_TRACKING_CREATE_DIR: &str = "E_TRACKING_CREATE_DIR";
const E_TRACKING_OPEN_DB: &str = "E_TRACKING_OPEN_DB";
const E_TRACKING_SET_WAL: &str = "E_TRACKING_SET_WAL";
#[cfg(test)]
const E_TRACKING_OPEN_MEMORY_DB: &str = "E_TRACKING_OPEN_MEMORY_DB";
const E_TRACKING_LOCK: &str = "E_TRACKING_LOCK";
const E_TRACKING_SCHEMA_CREATE: &str = "E_TRACKING_SCHEMA_CREATE";
const E_TRACKING_RECORD_EVENT: &str = "E_TRACKING_RECORD_EVENT";
const E_TRACKING_SUMMARY_PREPARE: &str = "E_TRACKING_SUMMARY_PREPARE";
const E_TRACKING_SUMMARY_QUERY: &str = "E_TRACKING_SUMMARY_QUERY";
const E_TRACKING_DAILY_PREPARE: &str = "E_TRACKING_DAILY_PREPARE";
const E_TRACKING_DAILY_QUERY: &str = "E_TRACKING_DAILY_QUERY";
const E_TRACKING_DAILY_COLLECT: &str = "E_TRACKING_DAILY_COLLECT";
const E_TRACKING_FILTER_PREPARE: &str = "E_TRACKING_FILTER_PREPARE";
const E_TRACKING_FILTER_QUERY: &str = "E_TRACKING_FILTER_QUERY";
const E_TRACKING_FILTER_COLLECT: &str = "E_TRACKING_FILTER_COLLECT";
const E_TRACKING_CLEANUP_OLD: &str = "E_TRACKING_CLEANUP_OLD";
const E_TRACKING_LEGACY_STATS_READ: &str = "E_TRACKING_LEGACY_STATS_READ";
const E_TRACKING_LEGACY_STATS_PARSE: &str = "E_TRACKING_LEGACY_STATS_PARSE";
const E_TRACKING_LEGACY_MIGRATE: &str = "E_TRACKING_LEGACY_MIGRATE";
const LEGACY_MIGRATION_KEY: &str = "legacy_stats_migrated_v1";

#[derive(Debug, Default, Deserialize)]
struct LegacyStats {
    #[serde(default)]
    total_runs: u64,
    #[serde(default)]
    total_original_bytes: u64,
    #[serde(default)]
    total_compressed_bytes: u64,
    #[serde(default)]
    total_original_tokens: u64,
    #[serde(default)]
    total_compressed_tokens: u64,
}

/// 获取默认数据库路径
///
/// - Windows: `%APPDATA%/tokenslim/tracking.db`
/// - Unix: `~/.local/share/tokenslim/tracking.db`
pub fn default_db_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()
    } else {
        std::env::var("HOME").ok().map(|h| {
            PathBuf::from(h)
                .join(".local")
                .join("share")
                .to_string_lossy()
                .to_string()
        })
    }?;
    let dir = PathBuf::from(base).join("tokenslim");
    Some(dir.join("tracking.db"))
}

/// SQLite 追踪器
///
/// 管理数据库连接，提供记录和查询接口。
///
/// ## 线程安全
///
/// 内部使用 `Mutex<Connection>` 保证多线程安全。
/// 所有公开方法都获取锁后执行。
pub struct Tracker {
    conn: Mutex<Connection>,
}

impl Tracker {
    /// 打开或创建默认路径的数据库
    pub fn open_default() -> Result<Self, String> {
        let path = default_db_path().ok_or_else(|| E_TRACKING_DB_PATH.to_string())?;
        let tracker = Self::open(&path)?;
        let _ = tracker.migrate_legacy_stats_if_needed();
        Ok(tracker)
    }

    /// 打开或创建指定路径的数据库
    pub fn open(path: &PathBuf) -> Result<Self, String> {
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("{E_TRACKING_CREATE_DIR}:{parent:?}:{e}"))?;
        }

        let conn =
            Connection::open(path).map_err(|e| format!("{E_TRACKING_OPEN_DB}:{path:?}:{e}"))?;

        // 启用 WAL 模式以支持并发读写
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("{E_TRACKING_SET_WAL}:{e}"))?;

        let tracker = Self {
            conn: Mutex::new(conn),
        };
        tracker.ensure_schema()?;
        Ok(tracker)
    }

    /// 创建内存数据库（用于测试）
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn =
            Connection::open_in_memory().map_err(|e| format!("{E_TRACKING_OPEN_MEMORY_DB}:{e}"))?;
        let tracker = Self {
            conn: Mutex::new(conn),
        };
        tracker.ensure_schema()?;
        Ok(tracker)
    }

    /// 确保数据库表结构存在
    fn ensure_schema(&self) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("{E_TRACKING_LOCK}:{e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS commands (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                command TEXT NOT NULL,
                filter_name TEXT,
                input_bytes INTEGER NOT NULL DEFAULT 0,
                output_bytes INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                filter_time_ms INTEGER NOT NULL DEFAULT 0,
                exit_code INTEGER NOT NULL DEFAULT 0,
                project TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_commands_timestamp ON commands(timestamp);
            CREATE INDEX IF NOT EXISTS idx_commands_project ON commands(project);
            CREATE INDEX IF NOT EXISTS idx_commands_filter ON commands(filter_name);

            CREATE TABLE IF NOT EXISTS legacy_totals (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                total_runs INTEGER NOT NULL DEFAULT 0,
                total_input_tokens INTEGER NOT NULL DEFAULT 0,
                total_output_tokens INTEGER NOT NULL DEFAULT 0,
                total_input_bytes INTEGER NOT NULL DEFAULT 0,
                total_output_bytes INTEGER NOT NULL DEFAULT 0,
                imported_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .map_err(|e| format!("{E_TRACKING_SCHEMA_CREATE}:{e}"))?;
        Ok(())
    }

    fn legacy_stats_path() -> Option<PathBuf> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()?;
        Some(PathBuf::from(home).join(".tokenslim").join("stats.json"))
    }

    fn load_legacy_stats(path: &Path) -> Result<LegacyStats, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("{E_TRACKING_LEGACY_STATS_READ}:{path:?}:{e}"))?;
        serde_json::from_str::<LegacyStats>(&content)
            .map_err(|e| format!("{E_TRACKING_LEGACY_STATS_PARSE}:{path:?}:{e}"))
    }

    pub fn migrate_legacy_stats_if_needed(&self) -> Result<bool, String> {
        let legacy_path = Self::legacy_stats_path();
        let legacy_stats = legacy_path
            .as_ref()
            .filter(|p| p.exists())
            .and_then(|p| Self::load_legacy_stats(p).ok());

        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("{E_TRACKING_LOCK}:{e}"))?;

        let migrated = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![LEGACY_MIGRATION_KEY],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if migrated.is_some() {
            return Ok(false);
        }

        let mut status = "missing";
        let mut imported = false;

        if let Some(stats) = legacy_stats {
            let runs = stats.total_runs as i64;
            let input_tokens = stats.total_original_tokens as i64;
            let output_tokens = stats.total_compressed_tokens as i64;
            let input_bytes = stats.total_original_bytes as i64;
            let output_bytes = stats.total_compressed_bytes as i64;
            if runs > 0 || input_tokens > 0 || output_tokens > 0 {
                conn.execute(
                    "INSERT INTO legacy_totals(
                        id, total_runs, total_input_tokens, total_output_tokens,
                        total_input_bytes, total_output_bytes, imported_at
                    ) VALUES (1, ?1, ?2, ?3, ?4, ?5, datetime('now'))
                    ON CONFLICT(id) DO UPDATE SET
                        total_runs = excluded.total_runs,
                        total_input_tokens = excluded.total_input_tokens,
                        total_output_tokens = excluded.total_output_tokens,
                        total_input_bytes = excluded.total_input_bytes,
                        total_output_bytes = excluded.total_output_bytes,
                        imported_at = excluded.imported_at",
                    params![runs, input_tokens, output_tokens, input_bytes, output_bytes],
                )
                .map_err(|e| format!("{E_TRACKING_LEGACY_MIGRATE}:{e}"))?;
                status = "imported";
                imported = true;
            } else {
                status = "empty";
            }
        } else if legacy_path.is_some() {
            status = "parse_error";
        }

        conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES(?1, ?2)",
            params![LEGACY_MIGRATION_KEY, status],
        )
        .map_err(|e| format!("{E_TRACKING_LEGACY_MIGRATE}:{e}"))?;

        Ok(imported)
    }

    /// 记录一次命令执行
    pub fn record(&self, event: &TrackingEvent) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("{E_TRACKING_LOCK}:{e}"))?;
        conn.execute(
            "INSERT INTO commands (command, filter_name, input_bytes, output_bytes,
             input_tokens, output_tokens, filter_time_ms, exit_code, project)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.command,
                event.filter_name,
                event.input_bytes,
                event.output_bytes,
                event.input_tokens,
                event.output_tokens,
                event.filter_time_ms,
                event.exit_code,
                event.project,
            ],
        )
        .map_err(|e| format!("{E_TRACKING_RECORD_EVENT}:{e}"))?;
        Ok(())
    }

    /// 获取总览统计
    pub fn get_summary(&self) -> Result<GainSummary, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("{E_TRACKING_LOCK}:{e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    COUNT(*) AS total_commands,
                    COALESCE(SUM(input_tokens), 0) AS total_input_tokens,
                    COALESCE(SUM(output_tokens), 0) AS total_output_tokens,
                    COALESCE(SUM(filter_time_ms), 0) AS total_filter_time_ms
                 FROM commands",
            )
            .map_err(|e| format!("{E_TRACKING_SUMMARY_PREPARE}:{e}"))?;

        let (live_commands, live_input_tokens, live_output_tokens, total_filter_time_ms) = stmt
            .query_row([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| format!("{E_TRACKING_SUMMARY_QUERY}:{e}"))?;

        let (legacy_runs, legacy_input_tokens, legacy_output_tokens): (i64, i64, i64) = conn
            .query_row(
                "SELECT total_runs, total_input_tokens, total_output_tokens
                 FROM legacy_totals WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or((0, 0, 0));

        let total_commands = live_commands + legacy_runs;
        let total_input_tokens = live_input_tokens + legacy_input_tokens;
        let total_output_tokens = live_output_tokens + legacy_output_tokens;
        let tokens_saved = (total_input_tokens - total_output_tokens).max(0);
        let savings_pct = if total_input_tokens == 0 {
            0.0
        } else {
            (tokens_saved as f64 / total_input_tokens as f64) * 100.0
        };
        let avg_filter_time_ms = if live_commands == 0 {
            0.0
        } else {
            total_filter_time_ms as f64 / live_commands as f64
        };

        Ok(GainSummary {
            total_commands,
            total_input_tokens,
            total_output_tokens,
            tokens_saved,
            savings_pct,
            total_filter_time_ms,
            avg_filter_time_ms,
        })
    }

    /// 获取按日聚合统计
    ///
    /// `days` 参数限制返回最近 N 天的数据。
    pub fn get_daily(&self, days: i64) -> Result<Vec<DailyGain>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("{E_TRACKING_LOCK}:{e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    date(timestamp) AS day,
                    COUNT(*) AS commands,
                    COALESCE(SUM(input_tokens), 0) AS input_tokens,
                    COALESCE(SUM(output_tokens), 0) AS output_tokens,
                    COALESCE(SUM(filter_time_ms), 0) AS filter_time_ms
                 FROM commands
                 WHERE timestamp >= datetime('now', ?1)
                 GROUP BY day
                 ORDER BY day DESC",
            )
            .map_err(|e| format!("{E_TRACKING_DAILY_PREPARE}:{e}"))?;

        let rows = stmt
            .query_map(params![format!("-{days} days")], |row| {
                let commands: i64 = row.get(1)?;
                let input_tokens: i64 = row.get(2)?;
                let output_tokens: i64 = row.get(3)?;
                let total_filter_time_ms: i64 = row.get(4)?;
                let tokens_saved = (input_tokens - output_tokens).max(0);
                let savings_pct = if input_tokens == 0 {
                    0.0
                } else {
                    (tokens_saved as f64 / input_tokens as f64) * 100.0
                };
                Ok(DailyGain {
                    date: row.get(0)?,
                    commands,
                    input_tokens,
                    output_tokens,
                    tokens_saved,
                    savings_pct,
                    total_filter_time_ms,
                })
            })
            .map_err(|e| format!("{E_TRACKING_DAILY_QUERY}:{e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("{E_TRACKING_DAILY_COLLECT}:{e}"))?;

        Ok(rows)
    }

    /// 获取按过滤器聚合统计
    pub fn get_by_filter(&self) -> Result<Vec<FilterGain>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("{E_TRACKING_LOCK}:{e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    COALESCE(filter_name, 'passthrough') AS filter_name,
                    COUNT(*) AS commands,
                    COALESCE(SUM(input_tokens), 0) AS input_tokens,
                    COALESCE(SUM(output_tokens), 0) AS output_tokens,
                    COALESCE(SUM(filter_time_ms), 0) AS filter_time_ms
                 FROM commands
                 GROUP BY filter_name
                 ORDER BY commands DESC",
            )
            .map_err(|e| format!("{E_TRACKING_FILTER_PREPARE}:{e}"))?;

        let mut rows = stmt
            .query_map([], |row| {
                let commands: i64 = row.get(1)?;
                let input_tokens: i64 = row.get(2)?;
                let output_tokens: i64 = row.get(3)?;
                let total_filter_time_ms: i64 = row.get(4)?;
                let tokens_saved = (input_tokens - output_tokens).max(0);
                let savings_pct = if input_tokens == 0 {
                    0.0
                } else {
                    (tokens_saved as f64 / input_tokens as f64) * 100.0
                };
                let avg_filter_time_ms = if commands == 0 {
                    0.0
                } else {
                    total_filter_time_ms as f64 / commands as f64
                };
                Ok(FilterGain {
                    filter_name: row.get(0)?,
                    commands,
                    input_tokens,
                    output_tokens,
                    tokens_saved,
                    savings_pct,
                    total_filter_time_ms,
                    avg_filter_time_ms,
                })
            })
            .map_err(|e| format!("{E_TRACKING_FILTER_QUERY}:{e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("{E_TRACKING_FILTER_COLLECT}:{e}"))?;

        let legacy = conn
            .query_row(
                "SELECT total_runs, total_input_tokens, total_output_tokens
                 FROM legacy_totals WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .unwrap_or((0, 0, 0));
        if legacy.0 > 0 {
            let tokens_saved = (legacy.1 - legacy.2).max(0);
            let savings_pct = if legacy.1 == 0 {
                0.0
            } else {
                (tokens_saved as f64 / legacy.1 as f64) * 100.0
            };
            rows.push(FilterGain {
                filter_name: "legacy_stats".to_string(),
                commands: legacy.0,
                input_tokens: legacy.1,
                output_tokens: legacy.2,
                tokens_saved,
                savings_pct,
                total_filter_time_ms: 0,
                avg_filter_time_ms: 0.0,
            });
        }

        rows.sort_by(|a, b| {
            b.commands
                .cmp(&a.commands)
                .then_with(|| a.filter_name.cmp(&b.filter_name))
        });

        Ok(rows)
    }

    /// 清理超过保留期的记录
    pub fn cleanup_older_than(&self, days: i64) -> Result<usize, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| format!("{E_TRACKING_LOCK}:{e}"))?;
        let deleted = conn
            .execute(
                "DELETE FROM commands WHERE timestamp < datetime('now', ?1)",
                params![format!("-{days} days")],
            )
            .map_err(|e| format!("{E_TRACKING_CLEANUP_OLD}:{e}"))?;
        Ok(deleted)
    }

    /// 自动清理（默认 90 天）
    pub fn auto_cleanup(&self) -> Result<usize, String> {
        self.cleanup_older_than(DEFAULT_RETENTION_DAYS)
    }
}

/// 快捷函数：同步记录一次命令执行
///
/// 适用于从 `cli/methods.rs` 的 run 模式中调用。
/// 该函数会在当前线程完成一次 SQLite 写入，确保进程退出前数据已落盘。
pub fn record_command(
    command: &str,
    filter_name: Option<&str>,
    input_bytes: usize,
    output_bytes: usize,
    exit_code: i32,
) {
    let event = TrackingEvent::new(command, filter_name, input_bytes, output_bytes, exit_code);
    if let Ok(tracker) = Tracker::open_default() {
        tracker.auto_cleanup().ok();
        tracker.record(&event).ok();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn new_tracker() -> Tracker {
        Tracker::open_in_memory().expect("无法创建测试用内存数据库")
    }

    fn record_test_event(
        tracker: &Tracker,
        command: &str,
        filter: Option<&str>,
        input: i64,
        output: i64,
    ) {
        let event = TrackingEvent {
            command: command.to_string(),
            filter_name: filter.map(String::from),
            input_bytes: input,
            output_bytes: output,
            input_tokens: input / 4,
            output_tokens: output / 4,
            filter_time_ms: 10,
            exit_code: 0,
            project: "test_project".to_string(),
        };
        tracker.record(&event).expect("记录失败");
    }

    #[test]
    fn test_open_in_memory() {
        let tracker = new_tracker();
        let summary = tracker.get_summary().unwrap();
        assert_eq!(summary.total_commands, 0);
    }

    #[test]
    fn test_record_and_summary() {
        let tracker = new_tracker();
        record_test_event(&tracker, "git status", Some("vcs_git"), 1024, 256);
        record_test_event(&tracker, "cargo build", Some("gcc_log"), 4096, 512);

        let summary = tracker.get_summary().unwrap();
        assert_eq!(summary.total_commands, 2);
        assert_eq!(summary.total_input_tokens, 1280); // 256 + 1024
        assert_eq!(summary.total_output_tokens, 192); // 64 + 128
        assert_eq!(summary.tokens_saved, 1088); // 1280 - 192
    }

    #[test]
    fn test_get_by_filter() {
        let tracker = new_tracker();
        record_test_event(&tracker, "git status", Some("vcs_git"), 1000, 200);
        record_test_event(&tracker, "git log", Some("vcs_git"), 2000, 300);
        record_test_event(&tracker, "cargo build", Some("gcc_log"), 4000, 500);
        record_test_event(&tracker, "echo hello", None, 100, 100);

        let filters = tracker.get_by_filter().unwrap();
        // 按 commands DESC 排序，vcs_git 应该排第一（2 次）
        assert_eq!(filters.len(), 3); // vcs_git, gcc_log, passthrough
        assert_eq!(filters[0].filter_name, "vcs_git");
        assert_eq!(filters[0].commands, 2);
    }

    #[test]
    fn test_get_daily() {
        let tracker = new_tracker();
        // 记录今天的命令
        record_test_event(&tracker, "git status", Some("vcs_git"), 1000, 200);
        record_test_event(&tracker, "cargo test", Some("gcc_log"), 2000, 400);

        let daily = tracker.get_daily(7).unwrap();
        assert!(!daily.is_empty());
        let today = &daily[0];
        assert_eq!(today.commands, 2);
    }

    #[test]
    fn test_cleanup() {
        let tracker = new_tracker();
        record_test_event(&tracker, "test", None, 100, 50);

        // 清理 0 天前的数据（应该保留今天的）
        let deleted = tracker.cleanup_older_than(0).unwrap();
        assert_eq!(deleted, 0);

        let summary = tracker.get_summary().unwrap();
        assert_eq!(summary.total_commands, 1);
    }

    #[test]
    fn test_empty_summary() {
        let tracker = new_tracker();
        let summary = tracker.get_summary().unwrap();
        assert_eq!(summary.total_commands, 0);
        assert_eq!(summary.tokens_saved, 0);
        assert!((summary.savings_pct - 0.0).abs() < 0.01);
        assert!((summary.avg_filter_time_ms - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_record_command_quick() {
        // 测试后台记录函数（在内存数据库中无法直接测试，仅验证不 panic）
        // 使用默认路径，可能在 CI 失败，但不应 panic
        // 这里只验证函数签名正确
        let event = TrackingEvent::new("test_cmd", Some("test_filter"), 100, 50, 0);
        assert_eq!(event.command, "test_cmd");
    }

    #[test]
    fn test_multiple_records_tokens_saved() {
        let tracker = new_tracker();
        // 模拟多次压缩执行
        for i in 0..5 {
            record_test_event(
                &tracker,
                &format!("cmd_{i}"),
                Some("test_filter"),
                1000 + (i * 100),
                200 + (i * 20),
            );
        }

        let summary = tracker.get_summary().unwrap();
        assert_eq!(summary.total_commands, 5);
        assert!(summary.tokens_saved > 0);
        assert!(summary.savings_pct > 0.0);
    }
}
