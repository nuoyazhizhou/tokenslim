use super::types::SyslogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, DocumentSkin, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::sync::Arc;

impl SyslogPlugin {
    /// 创建 SyslogPlugin 实例（名称 syslog，优先级 160），预编译 syslog 行解析正则。
    pub fn new() -> Self {
        Self {
            name: "syslog",
            priority: 160,
            syslog_pattern: Arc::new(
                Regex::new(
                    r#"^(?P<time>[A-Z][a-z]{2}\s+\d+\s+\d{2}:\d{2}:\d{2})\s+(?P<host>[a-zA-Z0-9_\-\.]+)\s+(?P<proc>[a-zA-Z0-9_\-\.]+)(?:\[(?P<pid>\d+)\])?:\s+(?P<msg>.*)$"#,
                )
                .unwrap(),
            ),
        }
    }
}

impl Default for SyslogPlugin {
    /// Default 实现：等价于 new()。
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SyslogPlugin {
    /// 返回插件名称 "syslog"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 160。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：前 5 行中 syslog 格式匹配占比 ≥50% 时命中。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lines: Vec<&str> = slice.text.lines().take(5).collect();
        if lines.is_empty() {
            return None;
        }
        let mut matched = 0;
        for line in &lines {
            if self.syslog_pattern.is_match(line) {
                matched += 1;
            }
        }
        let ratio = matched as f32 / lines.len() as f32;
        if ratio >= 0.5 {
            Some(ratio)
        } else {
            None
        }
    }

    /// 压缩切片：syslog 行 host/proc 字典化并编码为 $SYS| 行，ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let mut tokens: Vec<Token<'a>> = Vec::new();

        for line in text.lines() {
            if let Some(caps) = self.syslog_pattern.captures(line) {
                // 对 host / process 名称做字典化。字典命中率高时（跨多行同主机同进程）
                // 能显著压缩；小样本无共享时字典引擎会直接返回原文。
                let host_token = dict_engine.add_macro(caps.name("host").unwrap().as_str());
                let proc_token = dict_engine.add_macro(caps.name("proc").unwrap().as_str());
                let pid = caps.name("pid").map(|m| m.as_str()).unwrap_or("");
                tokens.push(Token::Text(
                    format!(
                        "$SYS|{}|{}|{}|{}|{}\n",
                        caps.name("time").unwrap().as_str(),
                        host_token,
                        proc_token,
                        pid,
                        caps.name("msg").unwrap().as_str()
                    )
                    .into(),
                ));
            } else {
                tokens.push(Token::Text(format!("{}\n", line).into()));
            }
        }

        // 法则 A ROI 门控：syslog 的 `$SYS|` 头与 5 个 `|` 分隔符会给每行带来约 9B 固定
        // 开销。当样本行数少（2-10 行）且 host/proc 字典命中率低时，整段会反而扩张。
        // 此处整段 prefer_non_expanding，扩张则回退原文。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § C.2.1。
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

    /// 文档级剥皮（P1-06）：把 syslog 头部外壳与内层非 syslog 正文分离。
    ///
    /// - 外壳摘要：首行锚点（法则 0）+ 其余 syslog 行的 `$SYS|` 紧凑形式。
    ///   与行级 compress 同一编码但**无字典化**——peel 阶段拿不到字典引擎，
    ///   host/proc 保留明文，解压侧 `resolve_or_self` 原样透传，保证可还原。
    /// - 内层正文：非 syslog 行（如嵌入的多行堆栈/裸工具输出），交内层管线
    ///   重新切片定向压缩。
    /// - 无内层正文（纯 syslog 流）返回 `None`，回退行级压缩路径，避免行为漂移。
    fn peel_document(&self, text: &str) -> Option<DocumentSkin> {
        let mut skin_lines = Vec::new();
        let mut inner_lines = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let is_skin = i == 0 // 首行锚点留在皮侧（法则 0）
                || self.syslog_pattern.is_match(line);
            if is_skin {
                skin_lines.push(line);
            } else {
                inner_lines.push(line);
            }
        }

        if inner_lines.is_empty() {
            return None;
        }

        let mut summary = String::with_capacity(text.len() / 2);
        if let Some(anchor) = skin_lines.first() {
            summary.push_str(anchor);
            summary.push('\n');
        }
        for line in skin_lines.iter().skip(1) {
            if let Some(caps) = self.syslog_pattern.captures(line) {
                summary.push_str(&format!(
                    "$SYS|{}|{}|{}|{}|{}\n",
                    caps.name("time").unwrap().as_str(),
                    caps.name("host").unwrap().as_str(),
                    caps.name("proc").unwrap().as_str(),
                    caps.name("pid").map(|m| m.as_str()).unwrap_or(""),
                    caps.name("msg").unwrap().as_str()
                ));
            } else {
                summary.push_str(line);
                summary.push('\n');
            }
        }

        Some(DocumentSkin {
            summary,
            inner_body: inner_lines.join("\n"),
        })
    }

    /// 解压：将 $SYS| 行还原为 syslog 原文（host/proc 用词典还原）。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut out = String::new();
        for line in compressed.lines() {
            if line.starts_with("$SYS|") {
                let parts: Vec<&str> = line.splitn(6, '|').collect();
                if parts.len() == 6 {
                    let host = dict.resolve_or_self(parts[2]);
                    let proc_name = dict.resolve_or_self(parts[3]);
                    if parts[4].is_empty() {
                        out.push_str(&format!(
                            "{} {} {}: {}\n",
                            parts[1], host, proc_name, parts[5]
                        ));
                    } else {
                        out.push_str(&format!(
                            "{} {} {}[{}]: {}\n",
                            parts[1], host, proc_name, parts[4], parts[5]
                        ));
                    }
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// 返回后续插件列表（smart_path）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}
