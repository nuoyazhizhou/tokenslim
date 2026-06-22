//! SQL 插件方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static SQL_KEYWORDS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|TRUNCATE|MERGE|REPLACE|FROM|WHERE|JOIN|GROUP\s+BY|ORDER\s+BY|HAVING|LIMIT|OFFSET|UNION|ALL|EXISTS|IN|BETWEEN|LIKE|IS\s+NULL|IS\s+NOT\s+NULL)\b").unwrap()
});
static INSERT_VALUES_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(VALUES\s*)\((?P<vals>.*)\)").unwrap());
static STR_LITERAL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"'(?:''|[^'])*'").unwrap());
static NUM_LITERAL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d+(\.\d+)?\b").unwrap());
#[allow(dead_code)]
static SENSITIVE_COLS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(password|passwd|pwd|secret|token|key|credential|auth)\b").unwrap()
});

impl SqlPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        SqlPlugin {
            name: "sql",
            priority: 110,
            config: SqlConfig::default(),
        }
    }

    /// 内部辅助函数：执行与 extract skeleton 相关的具体逻辑。
    fn extract_skeleton(&self, sql: &str) -> String {
        // 1. 替换字符串字面量
        let step1 = STR_LITERAL_RE.replace_all(sql, "'?'");
        // 2. 替换数字字面量
        let step2 = NUM_LITERAL_RE.replace_all(&step1, "?");
        step2.into_owned()
    }

    /// 内部辅助函数：执行与 truncate insert values 相关的具体逻辑。
    fn truncate_insert_values(&self, sql: &str) -> String {
        INSERT_VALUES_RE
            .replace_all(sql, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let vals = &caps["vals"];
                if vals.len() > self.config.max_insert_values_len {
                    format!("{}(... {} bytes truncated ...)", prefix, vals.len())
                } else {
                    caps.get(0).unwrap().as_str().to_string()
                }
            })
            .into_owned()
    }
}

impl Plugin for SqlPlugin {
    /// 返回插件的唯一标识名称，用于日志记录和监控。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件的执行优先级。数值越小，执行调度越靠前。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 分析输入的文本切片，检测是否符合当前插件的处理特征，并返回一个 0.0 到 1.0 的置信度（Confidence）。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if text.len() < self.config.min_sql_length {
            return None;
        }

        let matches = SQL_KEYWORDS_RE.find_iter(text).count();
        if matches > 0 {
            // 根据关键词数量和密度计算置信度
            // 如果包含多个不同的关键词，置信度更高
            let score = (matches as f32 * 15.0 / text.len() as f32).min(0.95);
            if score > 0.25 || matches >= 2 {
                return Some(score.max(0.4));
            }
        }
        None
    }

    /// 执行核心的压缩与特征提取逻辑。将输入文本中的重复长字符串、路径、包名等转换为紧凑的 Token，并存入字典引擎。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let mut processed = text.to_string();

        // 1. 如果是 INSERT 语句，检查是否需要截断巨大的 VALUES
        if processed.to_uppercase().contains("INSERT") {
            processed = self.truncate_insert_values(&processed);
        }

        // 2. 提取语法骨架（如果配置开启）
        if self.config.extract_skeleton {
            // 只有在 SQL 比较长时才提取骨架，保留短查询的完整性以便 AI 理解上下文
            if processed.len() > 50 {
                processed = self.extract_skeleton(&processed);
            }
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(processed))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 对文本进行归一化处理（用于日志比对）。消除时间戳、随机 Hash、乱序参数等 Diff 噪音。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除 SQL 中的数值常量
        let num_re = regex::Regex::new(r"\b\d+\b").unwrap();
        result = num_re.replace_all(&result, "?").to_string();

        // 抹除字符串常量
        let str_re = regex::Regex::new(r"'.*?'").unwrap();
        result = str_re.replace_all(&result, "'?'").to_string();

        result
    }

    /// 执行反向的还原逻辑。利用字典引擎中存储的上下文，将压缩后的 Token 流重新展开为完整、人类可读的原始文本。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        // 骨架化是不可逆的（损失了具体数值），所以解压只能返回处理后的文本
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<SqlConfig>() {
            self.config = new_config.clone();
            Ok(())
        } else {
            Err("Invalid config type".to_string())
        }
    }
}
