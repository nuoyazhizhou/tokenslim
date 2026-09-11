use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::shell_session_plugin::parser::compress_shell_session_blocks;
use bumpalo::Bump;

#[derive(Debug, Clone, Default)]
pub struct ShellSessionPlugin {}

impl ShellSessionPlugin {
    /// 创建空的 Shell 会话插件实例（无状态）。
    pub fn new() -> Self {
        Self {}
    }
}

impl Plugin for ShellSessionPlugin {
    /// 返回插件标识名 `shell_session`。
    fn name(&self) -> &'static str {
        "shell_session"
    }

    /// 返回插件优先级（200）：低于具体工具插件，作为未识别通用 shell 输出的兜底。
    fn priority(&self) -> u8 {
        // Priority 200 is lower than specific tools (like rust_go at 185),
        // so shell_session_plugin acts as a fallback for unrecognized generic shell output.
        200
    }

    /// 检测文本是否含常见 shell 提示符（`$`/`#`/`>`/`PS`/`~$`/`C:\`），命中返回 0.4 置信度。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if text.contains("$ ")
            || text.contains("# ")
            || text.contains("> ")
            || text.contains("PS ")
            || text.contains("~$")
            || text.contains("C:\\")
        {
            Some(0.4) // Higher than generic text but lower than dedicated plugins
        } else {
            None
        }
    }

    /// 按提示符切分 shell 会话为多块，逐块转成 `Text` token 并返回压缩结果。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let blocks = compress_shell_session_blocks(slice.text.as_ref());

        use crate::core::compression::Token;
        use std::borrow::Cow;

        let tokens = blocks
            .into_iter()
            .map(|block| Token::Text(Cow::Owned(block)))
            .collect();

        CompressResult {
            tokens,
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压占位实现：shell 会话压缩为可逆文本重写，直接返回原串。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    /// 返回应转交的具体原生插件清单（rust_go/git_diff/kubernetes_docker/pytest/nodejs/terraform/maven/bazel）。
    fn next_plugins(&self) -> Vec<&'static str> {
        // Yield to specific native plugins if they match a shell command block
        vec![
            "rust_go",
            "git_diff",
            "kubernetes_docker",
            "pytest",
            "nodejs",
            "terraform",
            "maven",
            "bazel",
        ]
    }
}
