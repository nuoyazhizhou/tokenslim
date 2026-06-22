use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sysinfo::System;

pub static GLOBAL_PROFILER: Lazy<Mutex<HashMap<String, (usize, u128)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn record_profile(name: &str, duration_ms: u128) {
    if let Ok(mut map) = GLOBAL_PROFILER.lock() {
        let entry = map.entry(name.to_string()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += duration_ms;
    }
}

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

fn monitor_verbose() -> bool {
    std::env::var("TS_MON_VERBOSE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn monitor_min_emit_ms() -> u128 {
    std::env::var("TS_MON_MIN_MS")
        .ok()
        .and_then(|v| v.parse::<u128>().ok())
        .unwrap_or(200)
}

fn emit_info(message: &str) {
    log::info!("{}", message);
    if monitor_verbose() {
        eprintln!("{}", message);
    }
}

fn emit_warn(message: &str) {
    log::warn!("{}", message);
    eprintln!("{}", message);
}

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

    pub fn with_warn_threshold_ms(mut self, threshold_ms: u128) -> Self {
        self.warn_threshold_ms = Some(threshold_ms);
        self
    }

    pub fn add_field<S: Into<String>, T: ToString>(&mut self, key: S, value: T) -> &mut Self {
        self.fields.push((key.into(), value.to_string()));
        self
    }
}

impl Drop for ScopeProbe {
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

pub fn log_progress(scope: &str, action: &str, items: usize, detail: &str) {
    emit_info(&format!(
        "{} event=progress scope={} action={} items={} {}",
        MON_TAG, scope, action, items, detail
    ));
}

pub fn log_object_size(scope: &str, action: &str, object: &str, bytes: usize) {
    emit_info(&format!(
        "{} event=object_size scope={} action={} object={} bytes={}",
        MON_TAG, scope, action, object, bytes
    ));
}

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

pub fn log_loop_suspect(scope: &str, action: &str, detail: &str, elapsed_since_last_ms: u128) {
    emit_warn(&format!(
        "{} event=loop_suspect scope={} action={} idle_ms={} {}",
        MON_TAG, scope, action, elapsed_since_last_ms, detail
    ));
}

fn available_memory_bytes() -> u64 {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.available_memory() * 1024
}
