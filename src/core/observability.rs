use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sysinfo::System;

pub static GLOBAL_PROFILER: Lazy<Mutex<HashMap<String, (usize, u128)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// 记录一次命名事件的耗时到全局性能分析器。
/// 累加调用次数与累计总耗时，供 dump_profile 汇总。
pub fn record_profile(name: &str, duration_ms: u128) {
    if let Ok(mut map) = GLOBAL_PROFILER.lock() {
        let entry = map.entry(name.to_string()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += duration_ms;
    }
}

/// 导出全局性能分析器数据。
/// 按总耗时降序排序后写入 docs/profile.txt。
pub fn dump_profile() {
    if let Ok(map) = GLOBAL_PROFILER.lock() {
        let mut entries: Vec<_> = map.iter().collect();
        entries.sort_by_key(|&(_, &(_, duration))| std::cmp::Reverse(duration));
        let mut s = String::new();
        s.push_str("==== Global Profiler Dump ====\n");
        for (name, (count, duration)) in entries {
            s.push_str(&format!(
                "{}, count: {}, total_ms: {}, avg_ms: {:.2}\n",
                name,
                count,
                duration,
                *duration as f64 / *count as f64
            ));
        }
        s.push_str("==============================\n");
        std::fs::write("docs/profile.txt", s).unwrap_or(());
    }
}

const MON_TAG: &str = "[TS_MON]";

/// 读取 TS_MON_VERBOSE 环境变量。
/// 判断是否开启监控详细输出（值为 1 或 true 时开启）。
fn monitor_verbose() -> bool {
    std::env::var("TS_MON_VERBOSE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 读取 TS_MON_MIN_MS 环境变量作为最小发射阈值（毫秒）。
/// 未设置或解析失败时回退为 200ms。
fn monitor_min_emit_ms() -> u128 {
    std::env::var("TS_MON_MIN_MS")
        .ok()
        .and_then(|v| v.parse::<u128>().ok())
        .unwrap_or(200)
}

/// 以 info 级别输出监控消息。
/// verbose 模式下同时打印到标准错误，便于实时观察。
fn emit_info(message: &str) {
    log::info!("{}", message);
    if monitor_verbose() {
        eprintln!("{}", message);
    }
}

/// 以 warn 级别输出监控消息。
/// 始终额外打印到标准错误，确保告警可见。
fn emit_warn(message: &str) {
    log::warn!("{}", message);
    eprintln!("{}", message);
}

/// 以 debug 级别输出监控消息。
/// verbose 模式下同时打印到标准错误。
fn emit_debug(message: &str) {
    log::debug!("{}", message);
    if monitor_verbose() {
        eprintln!("{}", message);
    }
}

pub struct ScopeProbe {
    scope: &'static str,
    action: &'static str,
    start: Instant,
    start_available_mem: u64,
    fields: Vec<(String, String)>,
    warn_threshold_ms: Option<u128>,
}

impl ScopeProbe {
    /// 构造作用域探针 ScopeProbe。
    /// 记录起始时刻与起始可用内存，并输出 event=start 监控事件。
    pub fn new(scope: &'static str, action: &'static str) -> Self {
        let start_available_mem = available_memory_bytes();
        emit_debug(&format!(
            "{} event=start scope={} action={}",
            MON_TAG, scope, action
        ));
        Self {
            scope,
            action,
            start: Instant::now(),
            start_available_mem,
            fields: Vec::new(),
            warn_threshold_ms: None,
        }
    }

    /// 为作用域探针设置告警阈值（毫秒）。
    /// 析构时若耗时超过该阈值则以 warn 级别输出 end 事件。
    pub fn with_warn_threshold_ms(mut self, threshold_ms: u128) -> Self {
        self.warn_threshold_ms = Some(threshold_ms);
        self
    }

    /// 为作用域探针追加一个结构化字段（键值对）。
    /// 返回自身以支持链式调用。
    pub fn add_field<S: Into<String>, T: ToString>(&mut self, key: S, value: T) -> &mut Self {
        self.fields.push((key.into(), value.to_string()));
        self
    }
}

impl Drop for ScopeProbe {
    /// ScopeProbe 的析构逻辑。
    /// 计算耗时与内存变化，按阈值决定以 warn 或 info 输出 event=end。
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        let elapsed_ms = elapsed.as_millis();
        let end_available_mem = available_memory_bytes();
        let mem_delta = end_available_mem as i128 - self.start_available_mem as i128;
        let field_text = self
            .fields
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<String>>()
            .join(" ");

        let message = format!(
            "{} event=end scope={} action={} elapsed_ms={} mem_avail_delta_bytes={} {}",
            MON_TAG, self.scope, self.action, elapsed_ms, mem_delta, field_text
        );

        if let Some(threshold) = self.warn_threshold_ms {
            if elapsed_ms >= threshold {
                emit_warn(&message);
                return;
            }
        }

        if monitor_verbose() || elapsed_ms >= monitor_min_emit_ms() {
            log::info!("{}", message);
            eprintln!("{}", message);
        }
    }
}

/// 记录进度类监控事件。
/// 携带 items 数量与 detail 描述，便于观察批量任务推进。
pub fn log_progress(scope: &str, action: &str, items: usize, detail: &str) {
    emit_info(&format!(
        "{} event=progress scope={} action={} items={} {}",
        MON_TAG, scope, action, items, detail
    ));
}

/// 记录对象大小类监控事件。
/// 携带 object 名称与 bytes 大小，用于追踪内存占用。
pub fn log_object_size(scope: &str, action: &str, object: &str, bytes: usize) {
    emit_info(&format!(
        "{} event=object_size scope={} action={} object={} bytes={}",
        MON_TAG, scope, action, object, bytes
    ));
}

/// 记录锁等待/持有类监控事件。
/// 等待超过 20ms 或持有超过 50ms 时升级为 warn 级别。
pub fn log_lock(scope: &str, lock_name: &str, wait: Duration, hold: Duration) {
    let level_warn = wait.as_millis() > 20 || hold.as_millis() > 50;
    if level_warn {
        emit_warn(&format!(
            "{} event=lock scope={} lock={} wait_ms={} hold_ms={}",
            MON_TAG,
            scope,
            lock_name,
            wait.as_millis(),
            hold.as_millis()
        ));
    } else {
        emit_debug(&format!(
            "{} event=lock scope={} lock={} wait_ms={} hold_ms={}",
            MON_TAG,
            scope,
            lock_name,
            wait.as_millis(),
            hold.as_millis()
        ));
    }
}

/// 记录疑似空转或长耗时循环的监控事件。
/// 携带 idle_ms 与 detail，用于发现热点循环。
pub fn log_loop_suspect(scope: &str, action: &str, detail: &str, elapsed_since_last_ms: u128) {
    emit_warn(&format!(
        "{} event=loop_suspect scope={} action={} idle_ms={} {}",
        MON_TAG, scope, action, elapsed_since_last_ms, detail
    ));
}

/// 通过 sysinfo 获取系统当前可用内存字节数。
/// 供 ScopeProbe 记录内存变化以辅助性能分析。
fn available_memory_bytes() -> u64 {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.available_memory() * 1024
}
