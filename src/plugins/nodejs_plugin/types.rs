//! Node.js 插件类型定义

use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use std::borrow::Cow;

/// Node.js 日志与错误分析插件
pub struct NodeJsPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}

impl NodeJsPlugin {
    /// 创建 NodeJsPlugin 实例（名称 nodejs，优先级 90）。
    pub fn new() -> Self {
        Self {
            name: "nodejs",
            priority: 90,
        }
    }

    /// 判断文本是否被 docker/k8s 构建与调度输出主导。docker build/compose、kubectl、
    /// buildkit、kubernetes 资源、经典 `Step N/M` 等虽常内嵌 npm/yarn/jest 文本（如
    /// `RUN npm test`），但整块应归属 kubernetes_docker，故命中任一强容器/编排信号即让位
    /// （返回 None），避免本插件的宽泛 `npm `/`jest` 等锚点以 0.85 抢占 docker 构建日志。
    fn is_docker_k8s_dominated(text: &str) -> bool {
        let lower = text.to_ascii_lowercase();
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("Step ") && t.contains(" : ")
                || t.starts_with("CONTAINER ID")
                || t.starts_with("=> [internal]")
                || t.starts_with("kubectl ")
                || t.starts_with("deployment.apps/")
                || t.starts_with("statefulset.apps/")
                || t.starts_with("daemonset.apps/")
                || t.starts_with("waiting for deployment")
            {
                return true;
            }
        }
        lower.contains("docker build")
            || lower.contains("docker buildx")
            || lower.contains("docker compose")
            || lower.contains("docker-compose")
            || lower.contains("sending build context to docker daemon")
            || lower.contains("successfully built")
            || lower.contains("successfully tagged")
            || lower.contains("failed to solve:")
            || lower.contains("writing image sha256:")
            || lower.contains("naming to docker.io/")
    }

    /// 判断文本是否是「node 错误栈」主导（前若干行密集出现 node 栈帧 ` at fn (.../x.js:line:col)`）。
    /// 此类块应交由 node_error 做栈级去重/精简，nodejs 返回 None 让位；否则 nodejs 0.85 > node_error 0.8，
    /// 会抢走 node 错误栈、使 node_error 形同虚设。npm install/webpack/vitest/jest 等构建输出不含此类帧，不受影响。
    fn is_node_error_dominated(text: &str) -> bool {
        let mut frame_count = 0usize;
        // 只扫前 30 行，栈样例头部即密集出帧
        for line in text.lines().take(30) {
            let t = line.trim_start();
            if t.starts_with("at ")
                && t.contains('(')
                && (t.contains(".js:")
                    || t.contains(".cjs:")
                    || t.contains(".mjs:")
                    || t.contains(".ts:"))
            {
                frame_count += 1;
            }
        }
        frame_count >= 2
    }
}

impl Plugin for NodeJsPlugin {
    /// 返回插件名称 "nodejs"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 90。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：含 node_modules/Error:/at Module. 或 pnpm/yarn/npm/jest/eslint/webpack 等特征得 0.85。
    fn detect<'a>(&self, slice: &Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let lower = text.to_ascii_lowercase();
        // docker/k8s 构建与调度输出整体归 kubernetes_docker（常内嵌 npm/jest 文本），让位
        if Self::is_docker_k8s_dominated(text) || Self::is_node_error_dominated(text) {
            return None;
        }
        if text.contains("node_modules")
            || text.contains("Error:")
            || text.contains("at Module.")
            || lower.contains("pnpm ")
            || lower.contains("yarn ")
            || lower.contains("npm ")
            || lower.contains("jest")
            || lower.contains("eslint")
            || lower.contains("webpack")
            || lower.contains("typescript")
            || lower.contains("error ts")
            || lower.contains("vite")
            || lower.contains("vitest")
        {
            return Some(0.85);
        }
        None
    }

    /// 压缩切片：先应用高级压缩（npm/yarn/pnpm/tsc/eslint/webpack/jest），再做路径压缩。
    fn compress<'a>(
        &self,
        slice: &Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // Step 1: 应用高级压缩（npm install、TypeScript、ESLint、Webpack、Jest）
        let advanced_compressed = self.apply_advanced_compression(text);

        // Step 2: 路径压缩
        let optimized = crate::core::path_compressor::methods::replace_paths_in_text(
            &advanced_compressed,
            dict_engine,
        );

        // P2-74：法则 A ROI 门控收口——eslint/webpack/vitest/jest 等压缩器无内部
        // 门控（P3-135「门控靠插件自觉」家族最重实例：单 `PASS x` 行 24B 会被
        // 重写成零统计 `[JEST]` 头 46B），与 maven/gcc/java_stack 一致在出口
        // 统一兜底：压缩结果比原文扩张即回退原文整段透传。
        let optimized = crate::core::utils::roi::prefer_non_expanding(
            text,
            optimized.into_owned(),
        );

        CompressResult {
            tokens: vec![crate::core::compression::Token::Text(Cow::Owned(
                optimized,
            ))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：原文透传。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}

impl Clone for NodeJsPlugin {
    /// 克隆插件实例：复制名称与优先级。
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
        }
    }
}
