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
    pub fn new() -> Self {
        Self {
            timestamp_converter: TimestampConverter::new(),
        }
    }

    pub fn convert_line<'a>(&mut self, line: Cow<'a, str>) -> Cow<'a, str> {
        self.timestamp_converter.convert_line(line)
    }

    pub fn base_timestamp(&self) -> Option<DateTime<Utc>> {
        self.timestamp_converter.base_timestamp()
    }

    pub fn set_base_timestamp(&mut self, base: Option<DateTime<Utc>>) {
        self.timestamp_converter.set_base_timestamp(base);
    }

    pub fn reset_timestamp(&mut self) {
        self.timestamp_converter.reset();
    }

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
    fn default() -> Self {
        Self::new()
    }
}
