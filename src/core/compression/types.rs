//! compression 类型定义

use crate::core::dictionary_engine::Dictionary;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Token 类型，表示压缩后的文本片段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "'a: 'static"))]
pub enum Token<'a> {
    Text(Cow<'a, str>),
    DictRef(Cow<'a, str>),
    Marker {
        kind: MarkerKind,
        value: Cow<'a, str>,
    },
}

impl<'a> Token<'a> {
    /// 将借用的 [`Token`] 转换为拥有所有权的 [`Token<'static>`]，把内部所有 `Cow` 字符串深拷贝为自有数据。
    pub fn into_owned(self) -> Token<'static> {
        match self {
            Token::Text(s) => Token::Text(Cow::Owned(s.into_owned())),
            Token::DictRef(s) => Token::DictRef(Cow::Owned(s.into_owned())),
            Token::Marker { kind, value } => Token::Marker {
                kind,
                value: Cow::Owned(value.into_owned()),
            },
        }
    }

    /// 估算该 Token 序列化后的大致字节数（近似值，用于容量规划与配额判断）。
    pub fn estimated_size(&self) -> usize {
        match self {
            Token::Text(s) => s.len(),
            Token::DictRef(s) => s.len(),
            Token::Marker { value, .. } => value.len() + 4,
        }
    }

    /// 估算该 Token 对应的大致 token 数量（按 4 字节/token 粗略折算，仅供压缩比评估）。
    pub fn estimated_tokens(&self) -> usize {
        match self {
            Token::Text(s) => s.len() / 4,
            Token::DictRef(_) => 1,
            Token::Marker { .. } => 2,
        }
    }
}

/// 标记类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkerKind {
    StackFrame,
    LogLine,
    HtmlBlock,
    CodeBlock,
    JsonBlock,
}

/// 压缩输出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionOutput {
    pub tokens: Vec<Token<'static>>,
    pub dictionary: Dictionary,
    pub metadata: CompressionMetadata,
}

/// 压缩元数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompressionMetadata {
    pub original_size: usize,
    pub compressed_size: usize,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub token_savings: usize,
    pub compression_ratio: f32,
    pub token_ratio: f32,
    pub slice_count: usize,
    pub processing_time_ms: u128,
    pub order_info: Option<OrderInfo>,
    pub base_timestamp: Option<String>,
    /// 源编码名：压缩入口 `encoding_fallback::decode_with_fallback` 实测得到的编码名，
    /// 供解压侧按原编码回写字节（P1-08 字节级可逆 round-trip）。
    ///
    /// - `None`：未经 CLI 解码入口（库调用直接传 `&str`），或输入为纯 UTF-8；
    /// - `Some(name)`：可逆性由 [`crate::core::encoding_fallback::is_roundtrip_safe`] 判定，
    ///   `utf-8-lossy` / `mixed-auto` / UTF-32 三类**不可逆**，解压侧会显式拒绝回写。
    ///
    /// 兼容性：`#[serde(default)]` 使既有产物（无此字段）反序列化为 `None`；
    /// `skip_serializing_if` 使 `None` 不参与序列化——**UTF-8 输入的产物字节零变化**，
    /// 保证既有冻结基线零漂移。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_encoding: Option<String>,
}

/// 重排序信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    pub context_groups: usize,
}
