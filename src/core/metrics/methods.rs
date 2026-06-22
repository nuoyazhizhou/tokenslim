//! metrics 方法实现
//!
//! # 方法概述
//!
//! 本模块实现了 metrics 模块的主要业务逻辑。
//! 包含所有公共 API 的实现，以及内部辅助函数。

use super::types::*;
use chrono::Utc;
use std::collections::HashMap;
use std::time::{Duration, Instant};

impl MetricsCollector {
    // ========== MVP 函数 ==========

    /// 创建新的 MetricsCollector 实例
    ///
    /// # 参数
    /// - `config`: 收集器配置
    ///
    /// # 返回值
    /// - `Self`: MetricsCollector 实例
    ///
    /// MVP 优先级
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            config,
            start_time: Some(Instant::now()),
            total_input_size: 0,
            total_output_size: 0,
            slice_count: 0,
            module_timings: ModuleTiming::default(),
            module_timings_current: HashMap::new(),
            plugin_stats: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// 设置总体输入大小。在该流程开始时由流水线分发器调用。
    pub fn set_input_size(&mut self, size: usize) {
        if !self.config.enabled {
            return;
        }
        self.total_input_size = size;
    }

    /// 设置总体输出大小。在流程结束或阶段性产出时调用。
    pub fn set_output_size(&mut self, size: usize) {
        if !self.config.enabled {
            return;
        }
        self.total_output_size = size;
    }

    /// 增加处理的切片（Slice）计数。
    pub fn add_slice_count(&mut self, count: usize) {
        if !self.config.enabled {
            return;
        }
        self.slice_count += count;
    }

    /// 开始特定模块的计时。
    ///
    /// # 参数
    /// - `module_name`: 模块标识符。建议使用预定义的常量。
    pub fn start_module(&mut self, module_name: &str) {
        if self.config.enabled && self.config.enable_module_timing {
            self.module_timings_current
                .insert(module_name.to_string(), Instant::now());
        }
    }

    /// 结束特定模块的计时并累加到总耗时。
    pub fn end_module(&mut self, module_name: &str) {
        if self.config.enabled && self.config.enable_module_timing {
            if let Some(start_time) = self.module_timings_current.remove(module_name) {
                let duration = start_time.elapsed();
                match module_name {
                    "stream_reader" => self.module_timings.stream_reader += duration,
                    "text_slicer" => self.module_timings.text_slicer += duration,
                    "content_analyzer" => self.module_timings.content_analyzer += duration,
                    "plugin_dispatcher" => self.module_timings.plugin_dispatcher += duration,
                    "dictionary_engine" => self.module_timings.dictionary_engine += duration,
                    "dedup_engine" => self.module_timings.dedup_engine += duration,
                    "compression_pipeline" => self.module_timings.compression_pipeline += duration,
                    "rehydration_pipeline" => self.module_timings.rehydration_pipeline += duration,
                    _ => {}
                }
            }
        }
    }

    /// 记录插件检测（Detect）阶段的耗时。
    pub fn record_plugin_detect(&mut self, plugin_name: &str, duration: Duration) {
        if self.config.enabled && self.config.enable_plugin_stats {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.detect_calls += 1;
            stats.total_detect_time += duration;
        }
    }

    /// 批量记录插件 detect 指标。
    pub fn record_plugin_detect_batch(
        &mut self,
        plugin_name: &str,
        calls: usize,
        total_duration: Duration,
    ) {
        if self.config.enabled && self.config.enable_plugin_stats && calls > 0 {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.detect_calls += calls;
            stats.total_detect_time += total_duration;
        }
    }

    /// 记录插件压缩（Compress）阶段的耗时。
    pub fn record_plugin_compress(&mut self, plugin_name: &str, duration: Duration) {
        if self.config.enabled && self.config.enable_plugin_stats {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.compress_calls += 1;
            stats.total_compress_time += duration;
        }
    }

    /// 批量记录插件 compress 指标。
    pub fn record_plugin_compress_batch(
        &mut self,
        plugin_name: &str,
        calls: usize,
        total_duration: Duration,
    ) {
        if self.config.enabled && self.config.enable_plugin_stats && calls > 0 {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.compress_calls += calls;
            stats.total_compress_time += total_duration;
        }
    }

    /// 记录插件解压（Decompress）或还原阶段的耗时。
    pub fn record_plugin_decompress(&mut self, plugin_name: &str, duration: Duration) {
        if self.config.enabled && self.config.enable_plugin_stats {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.decompress_calls += 1;
            stats.total_decompress_time += duration;
        }
    }

    /// 增加插件触发 Panic 的计数。
    pub fn inc_plugin_panic(&mut self, plugin_name: &str) {
        if self.config.enabled && self.config.enable_plugin_stats {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.panic_count += 1;
        }
    }

    /// 增加插件执行超时的计数。
    pub fn inc_plugin_timeout(&mut self, plugin_name: &str) {
        if self.config.enabled && self.config.enable_plugin_stats {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.timeout_count += 1;
        }
    }

    /// 增加插件回退（Fallback）到通用逻辑的计数。
    pub fn inc_plugin_fallback(&mut self, plugin_name: &str) {
        if self.config.enabled && self.config.enable_plugin_stats {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.fallback_count += 1;
        }
    }

    /// 批量增加插件 fallback 计数。
    pub fn inc_plugin_fallback_by(&mut self, plugin_name: &str, count: usize) {
        if self.config.enabled && self.config.enable_plugin_stats && count > 0 {
            let stats = self
                .plugin_stats
                .entry(plugin_name.to_string())
                .or_default();
            stats.fallback_count += count;
        }
    }

    /// 记录一条错误日志。
    pub fn log_error(&mut self, error: ErrorLog) {
        if self.config.enabled
            && self.config.enable_error_logging
            && self.errors.len() < self.config.max_error_logs
        {
            self.errors.push(error);
        }
    }

    /// 记录一条插件/调度链路错误。
    pub fn log_plugin_error(
        &mut self,
        module: &str,
        plugin: Option<&str>,
        error_type: &str,
        message: &str,
        slice_id: Option<u64>,
    ) {
        self.log_error(ErrorLog {
            timestamp: Utc::now(),
            module: module.to_string(),
            plugin: plugin.map(|s| s.to_string()),
            error_type: error_type.to_string(),
            message: message.to_string(),
            slice_id,
        });
    }

    /// 生成当前指标的静态快照。包含处理性能、压缩比及各插件统计。
    pub fn snapshot(&self) -> MetricsSnapshot {
        let processing_time = self
            .start_time
            .map_or(Duration::from_secs(0), |start| start.elapsed());
        let compression_ratio = if self.total_input_size > 0 {
            self.total_output_size as f32 / self.total_input_size as f32
        } else {
            0.0
        };

        MetricsSnapshot {
            total_input_size: self.total_input_size,
            total_output_size: self.total_output_size,
            compression_ratio,
            slice_count: self.slice_count,
            processing_time,
            module_timings: self.module_timings.clone(),
            plugin_stats: self.plugin_stats.clone(),
            errors: self.errors.clone(),
        }
    }

    /// 重置所有统计指标。
    pub fn reset(&mut self) {
        self.start_time = Some(Instant::now());
        self.total_input_size = 0;
        self.total_output_size = 0;
        self.slice_count = 0;
        self.module_timings = ModuleTiming::default();
        self.module_timings_current.clear();
        self.plugin_stats.clear();
        self.errors.clear();
    }

    // ========== 未来函数（待实现） ==========
}
