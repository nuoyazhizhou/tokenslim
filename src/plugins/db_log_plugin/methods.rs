use super::types::DbLogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use serde_json::Value;
use std::sync::Arc;

impl DbLogPlugin {
    /// 创建 DbLogPlugin 实例（名称 db_log，优先级 165），预编译 PG/MySQL/Mongo/Redis 解析正则。
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn new() -> Self {
        Self {
            name: "db_log",
            priority: 165,
            pg_pattern: Arc::new(Regex::new(
                r#"^(?P<time>\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2}\.\d{3}\s[A-Z]+)\s+\[(?P<pid>\d+)\]\s+(?P<level>[A-Z]+):\s+(?P<msg>.*)$"#,
            ).unwrap()),
            pg_duration_pattern: Arc::new(Regex::new(
                r#"^(?P<time>\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2}\.\d{3}\s[A-Z]+)\s+\[(?P<pid>\d+)\]\s+LOG:\s+duration:\s+(?P<duration>[0-9.]+)\s+ms\s+(?P<msg>.*)$"#,
            ).unwrap()),
            mysql_pattern: Arc::new(Regex::new(
                r#"^(?P<time>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{6}Z)\s+(?P<tid>\d+)\s+\[(?P<level>[A-Za-z]+)\]\s+(?P<msg>.*)$"#,
            ).unwrap()),
            mongo_json_pattern: Arc::new(Regex::new(
                r#""ctx"\s*:\s*"(?P<ctx>[^"]+)".*"msg"\s*:\s*"(?P<msg>[^"]+)""#,
            ).unwrap()),
            redis_pattern: Arc::new(Regex::new(
                r#"^(?P<pid>\d+):(?P<role>[A-Z])\s+(?P<time>\d{1,2}\s+\w+\s+\d{4}\s+\d{2}:\d{2}:\d{2}\.\d{3})\s+(?P<level>[*#-])\s+(?P<msg>.*)$"#,
            ).unwrap()),
        }
    }

    /// 按数据库类型压缩单行：优先匹配 PG duration/PG/MySQL/Mongo JSON/Mongo/Redis 模式，
    /// 输出带前缀的紧凑行；无法识别返回 None。
    #[tracing::instrument(level = "debug", skip_all)]
    fn compact_db_line(&self, line: &str) -> Option<String> {
        if let Some(caps) = self.pg_duration_pattern.captures(line) {
            let msg = caps.name("msg").unwrap().as_str();
            let query = compact_query_label(msg);
            let duration = caps.name("duration").unwrap().as_str();
            let tag = if duration.parse::<f64>().unwrap_or(0.0) >= 100.0 {
                "SLOW"
            } else {
                "DUR"
            };
            return Some(format!(
                "PG|pid={}|{}|{}ms|{}",
                caps.name("pid").unwrap().as_str(),
                tag,
                duration,
                query
            ));
        }
        if let Some(caps) = self.pg_pattern.captures(line) {
            let level = caps.name("level").unwrap().as_str();
            let prefix = if is_db_error_level(level) {
                "!PG"
            } else {
                "PG"
            };
            return Some(format!(
                "{}|pid={}|{}|{}",
                prefix,
                caps.name("pid").unwrap().as_str(),
                level,
                caps.name("msg").unwrap().as_str()
            ));
        }
        if let Some(caps) = self.mysql_pattern.captures(line) {
            let level = caps.name("level").unwrap().as_str();
            let prefix = if is_db_error_level(level) {
                "!MY"
            } else {
                "MY"
            };
            return Some(format!(
                "{}|tid={}|{}|{}",
                prefix,
                caps.name("tid").unwrap().as_str(),
                level,
                caps.name("msg").unwrap().as_str()
            ));
        }
        if let Some(compacted) = compact_mongo_json_line(line) {
            return Some(compacted);
        }
        if let Some(caps) = self.mongo_json_pattern.captures(line) {
            let msg = caps.name("msg").unwrap().as_str();
            let ctx = caps.name("ctx").unwrap().as_str();
            let level = line
                .split(r#""s":"#)
                .nth(1)
                .and_then(|rest| rest.split('"').nth(1))
                .unwrap_or("?");
            let prefix = if is_db_error_level(level) {
                "!MONGO"
            } else {
                "MONGO"
            };
            return Some(format!("{prefix}|ctx={ctx}|{level}|{msg}"));
        }
        if let Some(caps) = self.redis_pattern.captures(line) {
            let level = match caps.name("level").unwrap().as_str() {
                "#" => "ERR",
                "*" => "INF",
                "-" => "DBG",
                other => other,
            };
            let prefix = if level == "ERR" { "!REDIS" } else { "REDIS" };
            let msg = compact_redis_message(caps.name("msg").unwrap().as_str());
            return Some(format!(
                "{}|role={}|{}|{}",
                prefix,
                caps.name("role").unwrap().as_str(),
                level,
                msg
            ));
        }
        None
    }
}

/// 用 serde_json 解析 MongoDB JSON 日志行，提取 msg/s/ctx/c/attr 字段并压缩为 MONGO| 行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_mongo_json_line(line: &str) -> Option<String> {
    let json = serde_json::from_str::<Value>(line).ok()?;
    let msg = json.get("msg")?.as_str()?;
    let level = json.get("s").and_then(Value::as_str).unwrap_or("?");
    let ctx = json.get("ctx").and_then(Value::as_str).unwrap_or("-");
    let component = json.get("c").and_then(Value::as_str).unwrap_or("-");
    let attr = json.get("attr").unwrap_or(&Value::Null);
    let duration = attr
        .get("durationMillis")
        .and_then(Value::as_i64)
        .map(|v| format!("dur={}ms", v));
    let ns = attr
        .get("ns")
        .and_then(Value::as_str)
        .map(|v| format!("ns={v}"));
    let command = attr.get("command").and_then(compact_mongo_command);
    let error = attr
        .get("error")
        .and_then(Value::as_str)
        .map(|v| format!("err={v}"));
    let code = attr
        .get("code")
        .map(|v| format!("code={}", compact_json_value(v)));

    let mut fields = vec![
        format!("ctx={ctx}"),
        component.to_string(),
        level.to_string(),
        msg.to_string(),
    ];
    for field in [ns, command, duration, error, code].into_iter().flatten() {
        fields.push(field);
    }

    let prefix = if is_db_error_level(level) || msg.to_ascii_lowercase().contains("error") {
        "!MONGO"
    } else {
        "MONGO"
    };
    Some(format!("{prefix}|{}", fields.join("|")))
}

/// 从 MongoDB command 对象中提取首个已知命令名（find/aggregate/insert 等）及其紧凑值。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_mongo_command(command: &Value) -> Option<String> {
    let object = command.as_object()?;
    for key in [
        "find",
        "aggregate",
        "insert",
        "update",
        "delete",
        "count",
        "distinct",
    ] {
        if let Some(value) = object.get(key) {
            return Some(format!("{key}={}", compact_json_value(value)));
        }
    }
    object.keys().next().map(|key| format!("cmd={key}"))
}

/// 按字节上限截断字符串并保证落在 UTF-8 字符边界上，避免 `&s[..n]` 在居中切断多字节字符时触发 panic（Q518 处置）。
#[tracing::instrument(level = "debug", skip_all)]
fn truncate_utf8_safe(s: &str, max_bytes: usize) -> &str {
    let mut end = max_bytes.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 紧凑化 JSON 值：字符串原样，其他序列化后超 80 字符截断。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_json_value(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    let raw = value.to_string();
    if raw.len() > 80 {
        format!("{}...", truncate_utf8_safe(&raw, 80))
    } else {
        raw
    }
}

/// 从 PG duration 消息中提取 SQL 标签：剥离 statement:/execute <unnamed>: 前缀后压缩。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_query_label(msg: &str) -> String {
    let msg = msg.trim();
    let lower = msg.to_ascii_lowercase();
    for prefix in ["statement:", "execute <unnamed>:"] {
        if let Some(rest) = lower
            .find(prefix)
            .and_then(|idx| msg.get(idx + prefix.len()..))
        {
            return compact_sql_statement(rest.trim());
        }
    }
    compact_sql_statement(msg)
}

/// 压缩 SQL 语句：折叠空白为单行，超过 120 字符截断。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_sql_statement(sql: &str) -> String {
    let one_line = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.len() > 120 {
        format!("{}...", truncate_utf8_safe(&one_line, 120))
    } else {
        one_line
    }
}

/// 压缩 Redis 消息：折叠连续空白为单空格。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_redis_message(msg: &str) -> String {
    msg.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 判断日志级别是否为错误级（ERROR/ERR/FATAL/PANIC/SEVERE/E/W/WARNING）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_db_error_level(level: &str) -> bool {
    matches!(
        level.to_ascii_uppercase().as_str(),
        "ERROR" | "ERR" | "FATAL" | "PANIC" | "SEVERE" | "E" | "W" | "WARNING"
    )
}

impl Default for DbLogPlugin {
    /// Default 实现：等价于 new()。
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for DbLogPlugin {
    /// 返回插件名称 "db_log"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 165。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：前 10 行中可压缩行占比 ≥30% 时命中，返回匹配比例。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lines: Vec<&str> = slice.text.lines().take(10).collect();
        if lines.is_empty() {
            return None;
        }
        let mut matched = 0;
        for line in &lines {
            if self.compact_db_line(line).is_some() {
                matched += 1;
            }
        }
        let ratio = matched as f32 / lines.len() as f32;
        if ratio >= 0.3 {
            Some(ratio)
        } else {
            None
        }
    }

    /// 压缩切片：逐行压缩为紧凑行，整体做 ROI 门控，无收益时回退原文。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let mut tokens: Vec<Token<'a>> = Vec::new();
        for line in text.lines() {
            if let Some(compacted) = self.compact_db_line(line) {
                tokens.push(Token::Text(compacted.into()));
            } else {
                tokens.push(Token::Text(line.to_string().into()));
            }
            tokens.push(Token::Text("\n".into()));
        }

        // 法则 A ROI 门控：$DB|PG|/$DB|MY| 元字段给每行增加 7-8B 固定开销，
        // 小样本累积必然扩张。整段 prefer_non_expanding 回退原文。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § C.2.2。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(final_text.into())],
            metadata: None,
            plugin_name: Some(self.name),
        }
    }

    /// 解压：将紧凑 DB 行（`PG|`/`!PG|`/`MY|`/`!MY|` 前缀）还原为可读近似文本，其余行原样输出。
    ///
    /// 协议以 `compact_db_line` 实际产出的格式为准（如 `PG|pid=123|ERR|msg`、`MY|tid=7|DBG|msg`），
    /// 而不是旧实现臆测的 `$DB|PG|…` 前缀 —— 后者 strip 永命中、解压恒空操作（Q517 处置）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        let mut out = String::new();
        for line in compressed.lines() {
            let mut parts: [&str; 3] = ["", "", ""];
            let mut marker: &str = "";
            for m in ["!PG", "PG", "!MY", "MY"] {
                let Some(rest) = line.strip_prefix(&format!("{m}|")) else {
                    continue;
                };
                marker = m;
                let mut it = rest.splitn(3, '|');
                parts[0] = it.next().unwrap_or(""); // pid=/tid= 元数据
                parts[1] = it.next().unwrap_or(""); // level / DUR / SLOW
                parts[2] = it.next().unwrap_or(""); // msg / query
                break;
            }
            if marker.is_empty() {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            out.push_str(&format!(
                "[{}] {} {}: {}\n",
                marker, parts[0], parts[1], parts[2]
            ));
        }
        out
    }

    /// 返回后续插件列表（smart_path）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}
