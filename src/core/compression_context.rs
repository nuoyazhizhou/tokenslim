//! 压缩上下文能力封装
//!
//! 该模块将路径压缩与时间戳归一化能力集中封装为 `CompressionContext`，
//! 供 Pipeline/Dispatcher/Plugin 按需传递和调用。

use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::timestamp_converter::TimestampConverter;
use bumpalo::Bump;
use chrono::{DateTime, Utc};
use std::borrow::Cow;

pub struct CompressionContext {
    timestamp_converter: TimestampConverter,
}

impl CompressionContext {
    /// 构造压缩上下文。
    /// 初始化内部的时间戳转换器，作为路径与时间戳归一化的统一能力入口。
    pub fn new() -> Self {
        Self {
            timestamp_converter: TimestampConverter::new(),
        }
    }

    /// 转换单行中的时间戳。
    /// 将输入行交给内部时间戳转换器做归一化处理，无时间戳时原样返回。
    pub fn convert_line<'a>(&mut self, line: Cow<'a, str>) -> Cow<'a, str> {
        self.timestamp_converter.convert_line(line)
    }

    /// 获取归一化所用的基准时间戳。
    /// 未设置基准时返回 None。
    pub fn base_timestamp(&self) -> Option<DateTime<Utc>> {
        self.timestamp_converter.base_timestamp()
    }

    /// 设置基准时间戳。
    /// 传入 None 可清空基准，使后续转换重新建立基准。
    pub fn set_base_timestamp(&mut self, base: Option<DateTime<Utc>>) {
        self.timestamp_converter.set_base_timestamp(base);
    }

    /// 重置时间戳转换器。
    /// 清除已累计的基准时间与内部状态。
    pub fn reset_timestamp(&mut self) {
        self.timestamp_converter.reset();
    }

    /// 在作用域内压缩文本中的路径。
    /// 委托路径压缩器做归一化，复用字典引擎与可选内存 Arena 以控制分配开销。
    pub fn compress_path_scoped<'a>(
        &self,
        text: &'a str,
        dict_engine: &mut DictionaryEngine,
        arena: Option<&'a Bump>,
    ) -> Cow<'a, str> {
        crate::core::path_compressor::methods::replace_paths_in_text_scoped(
            text,
            dict_engine,
            arena,
        )
    }
}

impl Default for CompressionContext {
    /// 提供 CompressionContext 的默认构造。
    /// 等价于调用 new()，便于在需要 Default trait 的容器与初始化场景中使用。
    fn default() -> Self {
        Self::new()
    }
}
