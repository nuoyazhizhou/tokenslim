//! rehydration pipeline 方法实现

use super::types::*;
use crate::core::compression::{CompressionOutput, Token};
use crate::core::dictionary_engine::Dictionary;
use crate::core::metrics::{MetricsCollector, MetricsConfig};
use crate::core::plugin_dispatcher::Plugin;
use std::cell::RefCell;
use std::collections::HashMap;

impl RehydrationPipeline {
    /// 创建一个新的 RehydrationPipeline 实例。
    pub fn new(
        dict: Dictionary,
        mut plugins: Vec<Box<dyn Plugin>>,
        config: RehydrationConfig,
    ) -> Self {
        // P1-02：按 priority 升序稳定排序，decompress 链顺序确定化。
        plugins.sort_by_key(|p| p.priority());

        Self {
            dict,
            plugins,
            config,
            metrics: None,
        }
    }

    /// P2-63：注入指标采集器，使解压路径能记录 `decompress_calls`。
    /// 不改动已构造实例的插件顺序等既有状态。
    pub fn with_metrics(&mut self, metrics: MetricsCollector) -> &mut Self {
        self.metrics = Some(RefCell::new(metrics));
        self
    }

    /// 在解压循环中记录单次插件解压耗时（P2-63）。
    fn record_decompress_time(&self, plugin_name: &str, duration: std::time::Duration) {
        if let Some(cell) = &self.metrics {
            if let Ok(mut m) = cell.try_borrow_mut() {
                m.record_plugin_decompress(plugin_name, duration);
            }
        }
    }

    /// 对压缩输出结果执行完整的还原流程。
    pub fn rehydrate(&self, output: &CompressionOutput) -> Result<String, RehydrationError> {
        self.rehydrate_with(output, false)
    }

    /// 为 AI 消费导出特殊格式的文本（保留路径压缩，内联语义宏，丢弃噪音宏）。
    ///
    /// 结合 **上下文感知行过滤 (Context-Aware Line Filtering)**，去除无关噪声（如常规构建信息），
    /// 同时保留包含 Error/Warning/Fail/Fatal/Exception 的上下文窗口（前后1行）。
    /// 在导出的文本首部还会包含 **Base Timestamp Inclusion** 以便计算相对耗时，
    /// 极大优化了 AI 阅读 Token 消耗。
    pub fn rehydrate_for_ai(&self, output: &CompressionOutput) -> Result<String, RehydrationError> {
        self.rehydrate_with(output, true)
    }

    /// 完整还原流程的统一实现（P2-16）。
    ///
    /// `rehydrate`/`rehydrate_for_ai` 曾是 30 行几乎逐字的双份实现（仅解析策略
    /// 与 AI 附加步骤之差），P1-01「AI 模式改了、普通模式没改」的漂移正源于此。
    /// 现将差异收敛为 `ai_mode` 参数：
    /// - 解析策略：普通 `resolve_recursive` / AI `resolve_for_ai`；
    /// - AI 附加步骤：语义行过滤 + Base Timestamp 前缀；
    /// - 严格模式残留检查的 `$D` 豁免（仅 AI）。
    fn rehydrate_with(
        &self,
        output: &CompressionOutput,
        ai_mode: bool,
    ) -> Result<String, RehydrationError> {
        let text = self.rehydrate_tokens_with(&output.tokens, ai_mode)?;

        let mut final_text = text;
        // 1. 插件级还原（有序遍历，P1-02；AI 模式依然需要还原非路径的特殊编码）
        for plugin in &self.plugins {
            let start = std::time::Instant::now();
            final_text = plugin.decompress(&final_text, &self.dict);
            self.record_decompress_time(plugin.name(), start.elapsed());
        }

        // 2. 通用元数据还原。
        // P1-01：`resolve_recursive` 从 `$PL/$FL` 门控中提出、无条件执行——
        // 压缩侧路径 token 是内联进 Text 的 `$Pn`（非 DictRef），绝大多数
        // 输出不含 `$PL`/`$FL`，旧门控使全局兜底解析被跳过，`$Pn` 残留
        // 在用户可见输出。AI 模式本就无条件解析，两模式行为不一致佐证
        // 此处门控是遗漏而非设计。
        if final_text.contains("$PL") {
            final_text = final_text.replace("$PL ", "[Pipeline] ");
            final_text = final_text.replace("$PL", "[Pipeline]");
        }
        final_text = if ai_mode {
            self.dict.resolve_for_ai(&final_text)
        } else {
            self.dict.resolve_recursive(&final_text)
        };

        // 3. v2.0: 处理模糊去重还原 (FUZZY_DUP)
        if final_text.contains("// [FUZZY_DUP]") {
            final_text = self.restore_fuzzy_dups_with(&final_text, ai_mode);
        }

        // 4. AI 附加步骤：终极降维——基于语义的行级降噪 (Context-Aware Line
        // Filtering) 与 Base Timestamp 说明前缀。
        if ai_mode {
            final_text = Self::filter_semantic_lines_for_ai(&final_text);
            if let Some(ts) = &output.metadata.base_timestamp {
                let prefix = format!("Note: [T+Xms] are relative to Base Timestamp {}\n\n", ts);
                final_text.insert_str(0, &prefix);
            }
        }

        // P2-65：严格模式下检查字典键形态 token 是否仍有残留（宽松模式原样保留）。
        // AI 模式：$D 目录 token 为 resolve_for_ai 的刻意保留，检查时豁免。
        self.strict_residue_check(&final_text, ai_mode)?;

        Ok(final_text)
    }

    /// P2-65：严格模式残留检查。`fallback_on_error=false` 时，解压终态文本若仍含
    /// 字典键形态（`$P\d+`/`$PK\d+`/`$D\d+`/`$M\d+`/`$C\d+`）的未解析
    /// token，构造 `UnknownToken` 上传；宽松模式（默认）原样放行，与历史行为一致。
    /// AI 模式下 `$D\d+` 为刻意保留（见 `find_unresolved_dict_token` 的 ai_mode
    /// 参数说明），不算残留。
    fn strict_residue_check(&self, text: &str, ai_mode: bool) -> Result<(), RehydrationError> {
        if self.config.fallback_on_error {
            return Ok(());
        }
        if let Some(tok) = find_unresolved_dict_token(text, ai_mode) {
            return Err(RehydrationError::UnknownToken(tok));
        }
        Ok(())
    }

    /// 基于语义对文本做行级降噪：标记包含 Error/Warning/Fail/Fatal/Exception 的行及其前后各一行
    /// 为上下文保留，同时保留分隔线、目录头、元数据（branch/commit/exit code/耗时）等自说明行，
    /// 其余普通构建行折叠为一行 "Skipped N normal build lines" 提示。
    fn filter_semantic_lines_for_ai(text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let mut keep = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            let lower = line.to_lowercase();

            let is_context_trigger = lower.contains("error")
                || lower.contains("warning")
                || lower.contains("fail")
                || lower.contains("fatal")
                || lower.contains("exception");

            let is_self_only = line.starts_with("==========")
                || line.starts_with("[Directories]")
                || line.starts_with("[Semantic Logs]")
                || line.starts_with("Note: [T+")
                || line.contains("[TokenSlim AI Mode:")
                || (line.starts_with("$D") && line.contains(": "));

            let is_metadata_line = lower.contains("branch")
                || lower.contains("commit")
                || lower.contains("exit code")
                || lower.contains("return code")
                || lower.contains("duration")
                || lower.contains("elapsed")
                || lower.contains("cost time")
                || lower.contains("耗时");

            if is_context_trigger {
                if i > 0 {
                    keep[i - 1] = true;
                }
                keep[i] = true;
                if i + 1 < lines.len() {
                    keep[i + 1] = true;
                }
            } else if is_self_only || is_metadata_line {
                keep[i] = true;
            }
        }

        let mut result = String::with_capacity(text.len() / 4);
        let mut skip_count = 0;

        for (i, line) in lines.iter().enumerate() {
            if keep[i] {
                if skip_count > 0 {
                    result.push_str(&format!(
                        "... [TokenSlim AI Mode: Skipped {} normal build lines] ...\n",
                        skip_count
                    ));
                    skip_count = 0;
                }
                result.push_str(line);
                result.push('\n');
            } else {
                skip_count += 1;
            }
        }

        if skip_count > 0 {
            result.push_str(&format!(
                "... [TokenSlim AI Mode: Skipped {} normal build lines] ...\n",
                skip_count
            ));
        }

        result
    }

    /// AI 模式下的 token 序列还原：将 Text/DictRef/Marker 各 token 变体拼接为文本，
    /// 字典引用使用 resolve_for_ai 解析（保留路径压缩、内联语义宏），供 AI 高效消费。
    pub fn rehydrate_tokens_for_ai<'a>(
        &self,
        tokens: &[Token<'a>],
    ) -> Result<String, RehydrationError> {
        self.rehydrate_tokens_with(tokens, true)
    }

    /// 普通模式下的 token 序列还原：将 Text/DictRef/Marker 各 token 变体拼接为完整文本，
    /// 字典引用使用 resolve_recursive 递归解析。
    ///
    /// P2-65：严格模式（`fallback_on_error=false`）下，`DictRef` 若在字典中
    /// 解析不到（`resolve_one_level` 为 `None`）则构造
    /// `DictResolutionFailed` 上传；宽松模式保持原样残留。
    pub fn rehydrate_tokens<'a>(&self, tokens: &[Token<'a>]) -> Result<String, RehydrationError> {
        self.rehydrate_tokens_with(tokens, false)
    }

    /// token 序列还原的统一实现（P2-16）：`rehydrate_tokens{,_for_ai}` 曾是
    /// 20 行逐字双份实现，仅 `resolve_recursive`/`resolve_for_ai` 与残留检查
    /// 的 `$D` 豁免之差，现收敛为 `ai_mode` 策略参数。
    fn rehydrate_tokens_with(
        &self,
        tokens: &[Token<'_>],
        ai_mode: bool,
    ) -> Result<String, RehydrationError> {
        let mut result = String::new();
        for token in tokens {
            match token {
                Token::Text(text) => result.push_str(text),
                Token::DictRef(dict_ref) => {
                    let resolved = if ai_mode {
                        self.dict.resolve_for_ai(dict_ref)
                    } else {
                        self.dict.resolve_recursive(dict_ref)
                    };
                    if !self.config.fallback_on_error {
                        if let Some(tok) = find_unresolved_dict_token(&resolved, ai_mode) {
                            return Err(RehydrationError::DictResolutionFailed(tok));
                        }
                    }
                    result.push_str(&resolved);
                }
                Token::Marker { kind: _, value } => {
                    result.push_str(value);
                }
            }
        }
        Ok(result)
    }

    /// 模糊去重还原的统一实现（P2-16）：`restore_fuzzy_dups{,_for_ai}` 曾是
    /// 30 行逐字双份实现，仅 base token 的解析策略之差，现收敛为 `ai_mode`
    /// 策略参数——修复 FUZZY_DUP 逻辑时不再需要同步两份。
    fn restore_fuzzy_dups_with(&self, text: &str, ai_mode: bool) -> String {
        let mut result = String::with_capacity(text.len());
        for line in text.lines() {
            if line.contains("// [FUZZY_DUP]") {
                if let Some(pos) = line.find("// [FUZZY_DUP]") {
                    let meta = &line[pos + 14..].trim();
                    let parts: Vec<&str> = meta.split(", ").collect();
                    let mut base_val = String::new();
                    let mut patch = "";

                    for p in parts {
                        if p.starts_with("base=") {
                            let token = &p[5..];
                            base_val = if ai_mode {
                                self.dict.resolve_for_ai(token)
                            } else {
                                self.dict.resolve_recursive(token)
                            };
                        } else if p.starts_with("patch=") {
                            patch = &p[6..];
                        }
                    }

                    if !base_val.is_empty() {
                        result.push_str(&self.apply_patch(&base_val, patch));
                        result.push('\n');
                        continue;
                    }
                }
            }
            result.push_str(line);
            result.push('\n');
        }
        result
    }

    /// 对基础文本应用词级补丁：patch 形如 "idx:old->new"，按空白切分 base 为词数组，
    /// 将指定索引的词替换为 new 后重建；补丁为空时原样返回 base。
    ///
    /// P2-14：重建时保留词间**原始空白**（行首缩进/Tab/连续空格），不再压成单空格——
    /// Python traceback、`kubectl get`/`ps` 等对齐输出的可读性依赖这些空白。
    fn apply_patch(&self, base: &str, patch: &str) -> String {
        if patch.is_empty() {
            return base.to_string();
        }
        // 切分为 (前导空白, 词) 序列：词索引语义与 split_whitespace 完全一致，
        // 空白原样记录以便重建；末尾纯空白单独留存。
        let mut toks: Vec<(String, String)> = Vec::new();
        let mut ws = String::new();
        let mut word = String::new();
        let mut trailing = String::new();
        for ch in base.chars() {
            if ch.is_whitespace() {
                if word.is_empty() {
                    ws.push(ch);
                } else {
                    toks.push((std::mem::take(&mut ws), std::mem::take(&mut word)));
                    ws.push(ch);
                }
            } else {
                word.push(ch);
            }
        }
        if !word.is_empty() {
            toks.push((ws, word));
        } else {
            trailing = ws;
        }

        let mut replacements: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        for p in patch.split(',') {
            let sub_parts: Vec<&str> = p.splitn(2, ':').collect();
            if sub_parts.len() == 2 {
                let idx: usize = sub_parts[0].parse().unwrap_or(9999);
                let change: Vec<&str> = sub_parts[1].split("->").collect();
                if change.len() == 2 && idx < toks.len() {
                    replacements.insert(idx, change[1].to_string());
                }
            }
        }

        // 按原空白重建，仅替换被补丁命中的词内容
        let mut result = String::with_capacity(base.len());
        for (i, (ws, w)) in toks.iter().enumerate() {
            result.push_str(ws);
            result.push_str(replacements.get(&i).map(String::as_str).unwrap_or(w));
        }
        result.push_str(&trailing);
        result
    }
}

/// 扫描文本中首个「字典键形态但未被解析」的 token 并返回。
///
/// 仅匹配压缩器实际产出的字典键形态（`$P\d+`/`$PK\d+`/`$D\d+`/`$M\d+`/`$C\d+`，
/// 数字后缀必需；`$FL` 非合法前缀，见 Q444 处置），不匹配 `$PATH`/`$HOME` 等
/// shell 变量字面量与 `${VAR}` 展开，避免把日志原文误判为残留。`$PL`（无数字）
/// 为管线级标记，由 `rehydrate`/`rehydrate_for_ai` 显式改写，不在此检测范围。
///
/// `ai_mode=true` 时跳过 `$D\d+`：AI 模式刻意保留目录 token（`resolve_for_ai`
/// 的 `$D` 分支无条件保留，配合导出侧 `[Directories]` 节消费），保留非失败。
fn find_unresolved_dict_token(text: &str, ai_mode: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let mut end = i + 1;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > i + 1 {
                let tok = &text[i..end];
                if is_dict_key_shape(tok) && !(ai_mode && tok.starts_with("$D")) {
                    return Some(tok.to_string());
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    None
}

/// 判断 token 是否为字典键形态：`$` + 前缀（PK 最长优先，Q442 教训）+ 非空纯数字。
fn is_dict_key_shape(tok: &str) -> bool {
    let body = match tok.strip_prefix('$') {
        Some(b) => b,
        None => return false,
    };
    let digits = if let Some(r) = body.strip_prefix("PK") {
        r
    } else if let Some(r) = body.strip_prefix('P') {
        r
    } else if let Some(r) = body.strip_prefix('D') {
        r
    } else if let Some(r) = body.strip_prefix('M') {
        r
    } else if let Some(r) = body.strip_prefix('C') {
        r
    } else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}
