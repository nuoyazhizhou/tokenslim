use super::types::XcodeLogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::sync::Arc;
use std::sync::OnceLock;

/// P3-163（P3-127 家族扩展）：`normalize` 每次调用重建 DerivedData 项目哈希正则，
/// 提升为进程级 `OnceLock` 预编译（对照 `infra_tools_common.rs` 范式）。
static DERIVEDDATA_HASH_RE: OnceLock<Regex> = OnceLock::new();

impl XcodeLogPlugin {
    /// 创建插件实例：初始化名称(xcode_log)、优先级(195)与 Xcode 编译/Clang 正则匹配器。
    pub fn new() -> Self {
        Self {
            name: "xcode_log",
            priority: 195,
            compile_c_pattern: Arc::new(
                Regex::new(
                    r"^(?P<cmd>[A-Z][a-zA-Z0-9_]+)\s+(?P<dest>[^\s]+)(?:\s+(?P<src>[^\s]+))?\s+(?P<rest>\(in target.*)$",
                )
                .unwrap(),
            ),
            clang_pattern: Arc::new(
                Regex::new(
                    r"^(?P<indent>\s*)(?P<bin>/Applications/Xcode\.app/[^\s]+|/usr/bin/[^\s]+)\s+(?P<args>.*)$",
                )
                .unwrap(),
            ),
        }
    }
}

impl Default for XcodeLogPlugin {
    /// 返回默认插件实例：委托 new() 创建（名称/优先级在 Plugin 实现中硬编码）。
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for XcodeLogPlugin {
    /// 返回插件名称标识 "xcode_log"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级(195)，用于压缩调度排序。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测切片前 20 行是否属于 Xcode 构建日志，按匹配行比例返回置信度 0.0~1.0。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lines: Vec<&str> = slice.text.lines().take(20).collect();
        if lines.is_empty() {
            return None;
        }
        let mut matched = 0;
        for line in &lines {
            if self.compile_c_pattern.is_match(line)
                || self.clang_pattern.is_match(line)
                || line.contains("xcodebuild")
                || line.contains("CompileC ")
                || line.contains("Linking ")
            {
                matched += 1;
            }
        }
        let ratio = matched as f32 / lines.len() as f32;
        if ratio >= 0.2 {
            Some(ratio)
        } else {
            None
        }
    }

    /// 压缩 Xcode 构建日志：折叠 /dev/null 探针、将 CompileC/Clang/cd/response 行编码为 $XC IR 标签，并经 ROI 门控输出。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let lines: Vec<&str> = text.lines().collect();
        let mut tokens: Vec<Token<'a>> = Vec::new();
        let mut i = 0usize;
        while i < lines.len() {
            let line = lines[i];

            // 中文注释：将 /dev/null 的编译探针批量折叠，减少无效噪音。
            if line.contains("/dev/null") && (line.contains("clang") || line.contains("libtool")) {
                let mut count = 1usize;
                while i + count < lines.len()
                    && lines[i + count].contains("/dev/null")
                    && (lines[i + count].contains("clang") || lines[i + count].contains("libtool"))
                {
                    count += 1;
                }
                tokens.push(Token::Text(format!("$XC|PROBE|x{}\n", count).into()));
                i += count;
                continue;
            }

            if let Some(caps) = self.compile_c_pattern.captures(line) {
                let cmd = caps.name("cmd").unwrap().as_str();
                let dest = dict_engine.add_path_layered(caps.name("dest").unwrap().as_str());
                let src = caps
                    .name("src")
                    .map(|m| dict_engine.add_path_layered(m.as_str()))
                    .unwrap_or_default();
                let rest = dict_engine.add_macro(caps.name("rest").unwrap().as_str());
                tokens.push(Token::Text(
                    format!("$XC|C|{}|{}|{}|{}\n", cmd, dest, src, rest).into(),
                ));
            } else if let Some(caps) = self.clang_pattern.captures(line) {
                let indent_len = caps.name("indent").unwrap().as_str().len();
                let bin = dict_engine.add_path_layered(caps.name("bin").unwrap().as_str());
                let args = dict_engine.add_macro(caps.name("args").unwrap().as_str());
                tokens.push(Token::Text(
                    format!("$XC|B|{}|{}|{}\n", indent_len, bin, args).into(),
                ));
            } else if line.trim_start().starts_with("cd ") {
                let indent_len = line.chars().take_while(|c| c.is_whitespace()).count();
                let path = dict_engine
                    .add_path_layered(line.trim_start().trim_start_matches("cd ").trim());
                tokens.push(Token::Text(
                    format!("$XC|CD|{}|{}\n", indent_len, path).into(),
                ));
            } else if line.trim_start().starts_with("Using response file: ") {
                let indent_len = line.chars().take_while(|c| c.is_whitespace()).count();
                let path = dict_engine.add_path_layered(
                    line.trim_start()
                        .trim_start_matches("Using response file: ")
                        .trim(),
                );
                tokens.push(Token::Text(
                    format!("$XC|RF|{}|{}\n", indent_len, path).into(),
                ));
            } else {
                tokens.push(Token::Text(format!("{}\n", line).into()));
            }
            i += 1;
        }

        // 法则 A ROI 门控：`$XC|` IR 标签在纯文本/特殊字符样本上会反而扩张。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § A.2.4。
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

    /// 解压 Xcode 日志：将 $XC| 系列 IR 标签还原为原始命令行文本。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut out = String::new();
        for line in compressed.lines() {
            if line.starts_with("$XC|C|") {
                let parts: Vec<&str> = line.splitn(6, '|').collect();
                if parts.len() == 6 {
                    let dest = dict.resolve_or_self(parts[3]);
                    let rest = dict.resolve_or_self(parts[5]);
                    if parts[4].is_empty() {
                        out.push_str(&format!("{} {} {}\n", parts[2], dest, rest));
                    } else {
                        let src = dict.resolve_or_self(parts[4]);
                        out.push_str(&format!("{} {} {} {}\n", parts[2], dest, src, rest));
                    }
                    continue;
                }
            } else if line.starts_with("$XC|B|") {
                let parts: Vec<&str> = line.splitn(5, '|').collect();
                if parts.len() == 5 {
                    let indent = " ".repeat(parts[2].parse::<usize>().unwrap_or(0));
                    let bin = dict.resolve_or_self(parts[3]);
                    let args = dict.resolve_or_self(parts[4]);
                    out.push_str(&format!("{}{} {}\n", indent, bin, args));
                    continue;
                }
            } else if line.starts_with("$XC|CD|") {
                let parts: Vec<&str> = line.splitn(4, '|').collect();
                if parts.len() == 4 {
                    let indent = " ".repeat(parts[2].parse::<usize>().unwrap_or(0));
                    let path = dict.resolve_or_self(parts[3]);
                    out.push_str(&format!("{}cd {}\n", indent, path));
                    continue;
                }
            } else if line.starts_with("$XC|RF|") {
                let parts: Vec<&str> = line.splitn(4, '|').collect();
                if parts.len() == 4 {
                    let indent = " ".repeat(parts[2].parse::<usize>().unwrap_or(0));
                    let path = dict.resolve_or_self(parts[3]);
                    out.push_str(&format!("{}Using response file: {}\n", indent, path));
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// 声明下游插件：压缩后交由 smart_path 继续处理路径。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }

    /// 归一化文本：将 DerivedData 项目哈希目录替换为 [PROJECT] 占位符以便去重。
    fn normalize(&self, text: &str) -> String {
        let hash_re = DERIVEDDATA_HASH_RE
            .get_or_init(|| Regex::new(r"/DerivedData/[^/]+-[a-z0-9]+/").unwrap());
        hash_re
            .replace_all(text, "/DerivedData/[PROJECT]/")
            .to_string()
    }
}
