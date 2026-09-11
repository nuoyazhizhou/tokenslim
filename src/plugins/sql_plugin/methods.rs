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
// Q58 处置：头部动作动词。SELECT/INSERT 等强动词必任一条真实 SQL 语句起点，
// 而 IN/LIKE/ALL/EXISTS 为英文散文高频词，单独满足 matches>=2 时（如 `in`+`like`）
// 会把普通文本误判为 SQL。以「至少一个强动词」作为判别前置门槛。
static SQL_HEAD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER|TRUNCATE|MERGE|REPLACE)\b")
        .unwrap()
});
static INSERT_VALUES_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(VALUES\s*)\((?P<vals>.*)\)").unwrap());
static STR_LITERAL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"'(?:''|[^'])*'").unwrap());
static NUM_LITERAL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d+(\.\d+)?\b").unwrap());
// P3-163（P3-127 家族扩展）：`normalize` 每次调用重建数字/未引用字符串抹除正则，
// 提升为进程级预编译（对照本文件既有 Lazy 范式）。
static NUM_NORM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d+\b").unwrap());
static STR_NORM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"'.*?'").unwrap());

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

        // Q58 处置：前置门槛——必须含至少一个强动作动词才判 SQL。
        // 避免 IN/LIKE/ALL/EXISTS 等散文高频词在 matches>=2 时误判普通文本为 SQL。
        if !SQL_HEAD_RE.is_match(text) {
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

        // 0. P2-79 真实脱敏（obfuscate_sensitive=true 时启用）：复用 privacy 插件内置
        //    凭证正则组（单点维护，见 privacy_plugin::redact_with_builtin_patterns），
        //    将 password/secret/api_key/token 等赋值与 Bearer/JWT/连接串凭证替换为
        //    `[TS_*]` 不可逆占位符。置于骨架提取与 VALUES 截断之前，确保敏感值不会
        //    经由任何后续路径残留在产物中。默认 false（R50 行为变更门控），开启后
        //    产物为脱敏文本且与 decompress 恒等语义一致（SQL 骨架化本就声明不可逆）。
        if self.config.obfuscate_sensitive {
            processed =
                crate::plugins::privacy_plugin::redact_with_builtin_patterns(&processed);
        }

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
        let num_re = &*NUM_NORM_RE;
        result = num_re.replace_all(&result, "?").to_string();

        // 抹除字符串常量
        let str_re = &*STR_NORM_RE;
        result = str_re.replace_all(&result, "'?'").to_string();

        result
    }

    /// 执行反向的还原逻辑。利用字典引擎中存储的上下文，将压缩后的 Token 流重新展开为完整、人类可读的原始文本。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        // 骨架化是不可逆的（损失了具体数值），所以解压只能返回处理后的文本
        compressed.to_string()
    }
}
