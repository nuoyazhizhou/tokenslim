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
    Repeat {
        token: Box<Token<'a>>,
        count: usize,
    },
    Marker {
        kind: MarkerKind,
        value: Cow<'a, str>,
    },
    /// v2.0: 增量差异 Token。
    Diff {
        base: Cow<'a, str>,
        patch: Cow<'a, str>,
    },
}

impl<'a> Token<'a> {
    pub fn into_owned(self) -> Token<'static> {
        match self {
            Token::Text(s) => Token::Text(Cow::Owned(s.into_owned())),
            Token::DictRef(s) => Token::DictRef(Cow::Owned(s.into_owned())),
            Token::Repeat { token, count } => Token::Repeat {
                token: Box::new(token.into_owned()),
                count,
            },
            Token::Marker { kind, value } => Token::Marker {
                kind,
                value: Cow::Owned(value.into_owned()),
            },
            Token::Diff { base, patch } => Token::Diff {
                base: Cow::Owned(base.into_owned()),
                patch: Cow::Owned(patch.into_owned()),
            },
        }
    }

    pub fn estimated_size(&self) -> usize {
        match self {
            Token::Text(s) => s.len(),
            Token::DictRef(s) => s.len(),
            Token::Repeat { token, .. } => token.estimated_size() + 4,
            Token::Marker { value, .. } => value.len() + 4,
            Token::Diff { base, patch } => base.len() + patch.len() + 10,
        }
    }

    pub fn estimated_tokens(&self) -> usize {
        match self {
            Token::Text(s) => s.len() / 4,
            Token::DictRef(_) => 1,
            Token::Repeat { token, .. } => token.estimated_tokens() + 1,
            Token::Marker { .. } => 2,
            Token::Diff { .. } => 5,
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
}

/// 重排序信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    pub context_groups: usize,
}
