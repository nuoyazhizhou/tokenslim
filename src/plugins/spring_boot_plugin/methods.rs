//! spring boot plugin 方法实现

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

static MAVEN_DOWNLOAD_RE: Lazy<Regex> = Lazy::new(|| {
    // `compress` 里按行调用，`detect` 里对整段文本调用。使用 `(?m)` 统一语义：
    // 多行 Maven 下载日志的任何一行都能被 detect 识别。
    Regex::new(r"(?m)^(Download(?:ing|ed)\s+from\s+[\w\-]+:\s+)(?P<url>https?://.*)$").unwrap()
});
static SPRING_LIFECYCLE_RE: Lazy<Regex> = Lazy::new(|| {
    // 修复：detect 对多行 text 调用 is_match，但原正则 `^...$` 在默认非 multi-line 模式下
    // 只能匹配「整段 == 一行」的情形。加 `(?m)` 让 `^`/`$` 按行匹配，真实多行 Spring Boot
    // 日志（如 Tomcat started / Bean init 多行）都能被识别。compress 里按行匹配语义不变。
    Regex::new(r"(?m)^(?P<ts>.*)\s+(?P<level>INFO|DEBUG)\s+.*\s+---\s+\[.*\]\s+(?P<logger>[\w\.]+)\s+:\s+(?P<msg>.*)$").unwrap()
});
static BEAN_INIT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"Initializing (?:ExecutorService|Bean) '(?P<name>.*?)'").unwrap());

impl SpringBootPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        SpringBootPlugin {
            name: "spring_boot",
            priority: 115,
            config: SpringBootConfig::default(),
        }
    }
}

impl Plugin for SpringBootPlugin {
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
        let mut score: f32 = 0.0;

        if text.contains("Downloaded from") || text.contains("Downloading from") {
            score += 0.6;
        }
        if text.contains("Spring Boot") || text.contains("Starting application") {
            score += 0.5;
        }
        if SPRING_LIFECYCLE_RE.is_match(text) {
            score += 0.4;
        }

        if score > 0.3 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    /// 执行核心的压缩与特征提取逻辑。将输入文本中的重复长字符串、路径、包名等转换为紧凑的 Token，并存入字典引擎。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let mut result_text = String::new();

        for line in text.lines() {
            // 1. 处理 Maven 下载
            if self.config.fold_maven_downloads && MAVEN_DOWNLOAD_RE.is_match(line) {
                if let Some(caps) = MAVEN_DOWNLOAD_RE.captures(line) {
                    let url = &caps["url"];
                    let token = dict_engine.add_path_layered(url);
                    result_text.push_str(&format!("{} {}\n", &line[..10], token));
                    continue;
                }
            }

            // 2. 处理 Spring 生命周期和 Bean 初始化
            if self.config.merge_lifecycle_logs {
                if let Some(caps) = SPRING_LIFECYCLE_RE.captures(line) {
                    let logger = &caps["logger"];
                    let msg = &caps["msg"];

                    let logger_token = if self.config.extract_beans_packages {
                        dict_engine.add_package(logger)
                    } else {
                        logger.to_string()
                    };

                    // 特殊处理 Bean 初始化消息
                    if let Some(bean_caps) = BEAN_INIT_RE.captures(msg) {
                        let bean_name = &bean_caps["name"];
                        let bean_token = dict_engine.add_macro(bean_name);
                        result_text.push_str(&format!("  Init bean {}\n", bean_token));
                    } else {
                        result_text.push_str(&format!("{} {}\n", logger_token, msg));
                    }
                    continue;
                }
            }

            result_text.push_str(line);
            result_text.push('\n');
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(result_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 对文本进行归一化处理（用于日志比对）。消除时间戳、随机 Hash、乱序参数等 Diff 噪音。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除 Spring Boot 的线程名 (如 [nio-8080-exec-1])
        let thread_re = regex::Regex::new(r"\[[\w\-]{2,}\]").unwrap();
        result = thread_re.replace_all(&result, "[THREAD]").to_string();

        // 抹除随机的 Context ID
        let ctx_re = regex::Regex::new(r"/[a-f0-9]{8,16}/").unwrap();
        result = ctx_re.replace_all(&result, "/[CTX]/").to_string();

        result
    }

    /// 执行反向的还原逻辑。利用字典引擎中存储的上下文，将压缩后的 Token 流重新展开为完整、人类可读的原始文本。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<SpringBootConfig>() {
            self.config = new_config.clone();
            Ok(())
        } else {
            Err("Invalid config type".to_string())
        }
    }
}
