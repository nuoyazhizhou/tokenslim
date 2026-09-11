//! smart path plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use std::borrow::Cow;

impl SmartPathPlugin {
    /// 判断文本是否含 cargo/rust 诊断定位特征，用于在 detect 阶段决定是否让位给专用诊断插件。
    ///
    /// 命中任一特征即视为诊断块：
    /// - 含 `-->`（Rust/Go 编译器的路径定位箭头行）
    /// - 含带诊断语义前缀的行：`warning:`、`note:`、`help:`、`error[`、`error:`
    fn has_diagnostic_anchor(text: &str) -> bool {
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("-->")
                || trimmed.starts_with("warning:")
                || trimmed.starts_with("note:")
                || trimmed.starts_with("help:")
                || trimmed.starts_with("error[")
                || trimmed.starts_with("error:")
            {
                return true;
            }
        }
        false
    }

    /// 判断文本是否命中「专用插件强特征」：此类块应交由 rust_go 等做结构化压缩，
    /// smart_path 应让位（降置信度）而不是抢占。命中任一特征即视为要让位：
    /// - Go 崩溃栈：`goroutine N [state]:` panic 头部（Go 堆栈折叠属 rust_go 职责）。
    /// - cargo 纯构建/测试：`Compiling `/`Finished ... target(s) in`/`running N tests`/`test result:`。
    ///   纯构建输出无 `error`/`warning` 等诊断前缀，不被 [`Self::has_diagnostic_anchor`] 覆盖，
    ///   故须单独识别，否则 smart_path 以 0.9 抢占、丢失 rust_go 的 [CARGO]/[TEST] 摘要。
    /// - docker/k8s 构建与调度：`docker build/buildx/compose`、buildkit `=> [#N] [internal]`、
    ///   `failed to solve:`、`kubectl <子命令>`、`deployment/statefulset/daemonset .apps/`、
    ///   容器表头 `CONTAINER ID`。此类块应交由 kubernetes_docker 做镜像/容器/pod 字典折叠，
    ///   否则 smart_path 以 0.9 抢占仅做轻量路径替换、丢失其结构摘要。
    ///   锚点刻意收敛到容器/编排专属特征，避免 `=>`（JS 箭头等）这类过宽匹配误伤普通文本。
    fn is_specialist_signature(text: &str) -> bool {
        for line in text.lines() {
            let t = line.trim_start();
            // Go panic 头部：`goroutine 1 [running]:`
            if t.starts_with("goroutine ") && t.contains('[') && t.contains(']') {
                return true;
            }
            // cargo 构建/测试强锚点
            if t.starts_with("Compiling ")
                || (t.starts_with("Finished ") && t.contains("] target(s) in"))
                || (t.starts_with("running ") && t.contains(" test"))
                || t.contains(" test result:")
            {
                return true;
            }
            // docker buildkit 专属构建输出（`=> [#N] [internal] ...`、`#N [internal] ...`）
            if t.starts_with("=> [internal]")
                || t.starts_with("failed to solve:")
                || t.starts_with("writing image sha256:")
                || t.starts_with("naming to docker.io/")
                || (t.starts_with('#')
                    && t.split_whitespace().next().is_some_and(|w| {
                        w.len() >= 2
                            && w.starts_with('#')
                            && w[1..].chars().all(|c| c.is_ascii_digit())
                    })
                    && t.contains("[internal]"))
            {
                return true;
            }
            // docker / compose / kubernetes CLI 强锚点
            if t.starts_with("CONTAINER ID")
                || t.starts_with("kubectl ")
                || t.starts_with("deployment.apps/")
                || t.starts_with("statefulset.apps/")
                || t.starts_with("daemonset.apps/")
                || t.starts_with("waiting for deployment")
                || t.starts_with("error: deployment")
                || t.contains("docker build")
                || t.contains("docker buildx")
                || t.contains("docker compose")
                || t.contains("docker-compose")
            {
                return true;
            }
            // Node 生态强特征：`node_modules`、npm/yarn/pnpm/npx 命令、jest/webpack/vitest/eslint/vite、
            // node 错误栈帧（`  at fn (file:line:col)`）。此类块应交由 nodejs/node_error 做结构化压缩，
            // 否则 smart_path 以 0.9 抢占仅做路径替换、丢失其 npm/vitest/webpack 折叠与栈去重。
            // 锚点收敛到 node 专属标记，避免裸 `Error:`（Rust/Python 也有）这类过宽命中误伤其他域。
            // node 栈帧：`  at fn (.../x.js:1:2)`
            let frame_colon_js = t.starts_with("at ")
                && t.contains('(')
                && (t.contains(".js:")
                    || t.contains(".cjs:")
                    || t.contains(".mjs:")
                    || t.contains(".ts:"));
            if t.starts_with("node_modules")
                || t.starts_with("npm ")
                || t.starts_with("npm_")
                || t.starts_with("pnpm ")
                || t.starts_with("yarn ")
                || t.starts_with("npx ")
                || t.contains("node:internal/")
                || t.contains("internal/modules/cjs/loader.js")
                || t.starts_with("at async ")
                || frame_colon_js
                || t.contains(" jest ") // jest 测试输出（含 PASS/FAIL 行）
                || t.starts_with("jest ") || t.starts_with("PASS ") || t.starts_with("FAIL ")
                || t.contains("eslint")
                || t.contains("webpack")
                || t.contains("vitest")
            {
                return true;
            }
        }
        false
    }

    /// 创建 SmartPathPlugin 实例（无状态插件，无配置字段）。
    pub fn new() -> Self {
        Self
    }
}

impl Default for SmartPathPlugin {
    /// Default 实现：等价于 new()。
    fn default() -> Self {
        Self
    }
}

impl Plugin for SmartPathPlugin {
    /// 返回插件名称 "smart_path"。
    fn name(&self) -> &'static str {
        "smart_path"
    }
    /// 返回插件优先级 250。
    fn priority(&self) -> u8 {
        250
    }

    /// 检测：文本含 / 或 \ 路径分隔符得 0.9。
    ///
    /// 诊断让位：当切片同时满足「含路径」与「含 cargo/rust 诊断定位特征」时，返回较低置信度
    /// 0.2，把路由让给 rust_go 等专用诊断插件，避免 smart_path 抢占诊断块而只做轻量路径替换
    /// （把 `|`/`^`/note 等渲染层与语义行原样留下）。纯路径/普通日志文本仍走 0.9 正常压缩。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        if !(slice.text.contains('/') || slice.text.contains('\\')) {
            return None;
        }
        // cargo/rust 诊断块特征：`-->` 路径定位行，或带 `warning:`/`note:`/`error[` 语义前缀。
        // 此类块应交由 rust_go 做结构化诊断压缩，smart_path 降权让位。
        // 同理：Go 崩溃栈 / cargo 纯构建测试签名由 rust_go 结构化压缩，smart_path 也让位，
        // 否则以 0.9 抢占后仅做轻量路径替换、丢失 [CARGO]/[TEST]/Go 栈折叠摘要。
        if Self::has_diagnostic_anchor(&slice.text) || Self::is_specialist_signature(&slice.text) {
            Some(0.15)
        } else {
            Some(0.9)
        }
    }

    /// 压缩切片：用路径压缩器替换文本中的路径为字典 token。
    ///
    /// 对 cargo/rust 诊断锚点块（含 `-->` / `warning:` / `note:` 等定位特征），先逐行丢弃
    /// `|`/`^` 渲染层噪声行（区位符、高亮标记，含前导几百空格），再对剩余语义行做路径替换，
    /// 与 rust_go 的丢弃规则保持一致，避免诊断定位块被本插件兜底时仍残留渲染噪声。
    /// 普通路径文本保持原行为：仅做路径替换。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        let normalized: Cow<'_, str> = if Self::has_diagnostic_anchor(text) {
            let mut filtered = String::with_capacity(text.len());
            for line in text.lines() {
                // 与 rust_go 一致：丢弃首非空白字符是 `|` 的渲染层行（`|`/`^` 区位符、续接标记）
                if line.trim_start().starts_with('|') {
                    continue;
                }
                filtered.push_str(line);
                filtered.push('\n');
            }
            Cow::Owned(filtered)
        } else {
            Cow::Borrowed(text)
        };
        // 将 normalized 投射进 arena，解耦局部变量生命周期，使后续路径压缩返回的 Cow 与切片同样长寿。
        let normalized_ref = arena.alloc_str(&normalized);

        let optimized = crate::core::path_compressor::methods::replace_paths_in_text_scoped(
            normalized_ref,
            dict_engine,
            Some(arena),
        );

        CompressResult {
            tokens: vec![Token::Text(optimized)],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：原文透传（由核心引擎统一还原 $P token）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}

impl Clone for SmartPathPlugin {
    /// 克隆插件实例：无状态，返回新实例。
    fn clone(&self) -> Self {
        SmartPathPlugin
    }
}
