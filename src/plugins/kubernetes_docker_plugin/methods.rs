//! kubernetes docker plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::core::utils::json::extract_json_object;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

// Kubernetes Pod 名称通常是：name-deployment-hash-uuid
static K8S_POD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<ns>[\w\-]+)/(?P<pod>[\w\-]+-[a-z0-9]{5,10}-[a-z0-9]{5})").unwrap()
});
// Docker 容器 ID：64位或12位十六进制
static DOCKER_ID_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[0-9a-f]{12,64}\b").unwrap());
// 经典 docker build 输出中的 Dockerfile 步骤行：`Step 1/5 : FROM node:18`
static DOCKER_STEP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"Step \d+/\d+ : ").unwrap());
// P3-152（P3-127 家族扩展）：`normalize` 每次调用重建容器 ID / IP 抹除正则，
// 提升为进程级预编译（对照本文件既有 Lazy 范式）。
static ID_NORM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[a-f0-9]{12,64}\b").unwrap());
static IP_NORM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap());

impl KubernetesDockerPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        KubernetesDockerPlugin {
            name: "kubernetes_docker",
            priority: 130, // 优先级较高，因为前缀通常最先处理
            config: KubernetesDockerConfig::default(),
        }
    }

    /// 从一段 JSON 文本提取常见云平台日志的 message 字段（Q525 处置：空串候选跳过，
    /// 继续尝试下一字段，避免误丢弃更完整的正文）。
    fn message_field_of_json(raw: &str) -> Option<String> {
        let json = serde_json::from_str::<serde_json::Value>(raw).ok()?;
        for field in &["message", "log", "content", "msg"] {
            if let Some(msg) = json.get(*field).and_then(|v| v.as_str()) {
                if !msg.is_empty() {
                    return Some(msg.to_string());
                }
            }
        }
        None
    }

    /// 内部辅助函数：执行与 unwrap json if possible 相关的具体逻辑。
    ///
    /// P2-76 修复：此前对任意文本取「首个平衡 JSON 对象」的 message 整片替换——
    /// docker json-file 驱动 / 云日志逐行 JSON 流（每行一个对象）的第 2..N 行被静默丢弃。
    /// 现在分三条路径：
    /// - 单行文本 → 保持原语义（允许噪声前缀，extract 后整行替换，Q462 处置）；
    /// - 多行但整片恰为单个（pretty-print）JSON 对象 → 整片解包；
    /// - 多行 JSON 流 → 逐行解包，JSON 行替换为其 message，非 JSON 行原样保留。
    fn unwrap_json_if_possible<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if !self.config.unwrap_cloud_json {
            return Cow::Borrowed(text);
        }

        // 路径一：单行文本——保持原整片替换语义（含噪声前缀形态）。
        if !text.trim_end_matches(['\r', '\n']).contains('\n') {
            let Some(extracted) = extract_json_object(text) else {
                return Cow::Borrowed(text);
            };
            if let Some(msg) = Self::message_field_of_json(extracted.raw) {
                return Cow::Owned(msg);
            }
            return Cow::Borrowed(text);
        }

        // 路径二：多行但整片是一个 pretty-print JSON 对象——整片解包。
        let trimmed = text.trim();
        if trimmed.starts_with('{') {
            if let Some(extracted) = extract_json_object(text) {
                if extracted.raw.trim() == trimmed {
                    if let Some(msg) = Self::message_field_of_json(extracted.raw) {
                        return Cow::Owned(msg);
                    }
                }
            }
        }

        // 路径三：多行 JSON 流——逐行解包，行终止符（\n / \r\n）原样保留。
        let mut out = String::with_capacity(text.len());
        let mut changed = false;
        for seg in text.split_inclusive('\n') {
            let (line, term) = match seg.strip_suffix('\n') {
                Some(l) => (l, "\n"),
                None => (seg, ""),
            };
            let (line, term) = match line.strip_suffix('\r') {
                Some(l) => (l, "\r\n"),
                None => (line, term),
            };
            let mut unwrapped = false;
            if line.trim_start().starts_with('{') {
                if let Some(extracted) = extract_json_object(line) {
                    if let Some(msg) = Self::message_field_of_json(extracted.raw) {
                        out.push_str(&msg);
                        out.push_str(term);
                        changed = true;
                        unwrapped = true;
                    }
                }
            }
            if !unwrapped {
                out.push_str(seg);
            }
        }
        if changed {
            Cow::Owned(out)
        } else {
            Cow::Borrowed(text)
        }
    }
}

impl Plugin for KubernetesDockerPlugin {
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
        let lower = text.to_ascii_lowercase();

        if K8S_POD_RE.is_match(text) {
            score += 0.5;
        }
        if DOCKER_ID_RE.is_match(text) {
            score += 0.3;
        }
        // docker ps 表格头：`CONTAINER ID   IMAGE   ...   NAMES`。容器短 ID（默认 6 位 hex）
        // 不满足 DOCKER_ID_RE（要求 12-64 位），故以表头为强锚点单独加分，
        // 否则 docker ps 输出 detect 得 0 分落到 smart_path 兜底。
        if lower.contains("container id")
            && lower.contains("image")
            && (lower.contains("names") || lower.contains("command"))
        {
            score += 0.5;
        }
        // 经典 docker build 输出（`Step 1/5 : FROM node:18`、`Sending build context to Docker daemon`、
        // `Successfully built`）：步骤行里的短 hex（`---> abc123`）为 6 位，不满足 DOCKER_ID_RE；
        // 且无 buildkit 的 `#N [internal]` 前缀，故须以 Step 行/构建摘要为锚点单独加分，
        // 否则 detect 得 0 分被 nodejs（`npm install` 等宽泛锚点）/smart_path 抢占。
        if DOCKER_STEP_RE.is_match(text)
            || lower.contains("sending build context to docker daemon")
            || lower.contains("successfully built")
        {
            score += 0.5;
        }
        if is_docker_ci_output(&lower) {
            score += 0.5;
        }
        if is_kubernetes_ci_output(&lower) {
            score += 0.5;
        }
        if text.trim_start().starts_with('{')
            && (text.contains("\"message\"") || text.contains("\"logGroup\""))
        {
            score += 0.6;
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
        let raw_text = slice.text.as_ref();

        // 1. 先尝试解包 JSON
        let unwrapped = self.unwrap_json_if_possible(raw_text);
        let mut text = unwrapped.into_owned();

        // 2. 提取 Kubernetes 元数据
        if self.config.extract_kubernetes_metadata {
            text = K8S_POD_RE
                .replace_all(&text, |caps: &regex::Captures| {
                    let ns = &caps["ns"];
                    let pod = &caps["pod"];
                    let ns_token = dict_engine.add_package(ns);
                    let pod_token = dict_engine.add_path_layered(pod);
                    format!("{}/{}", ns_token, pod_token)
                })
                .into_owned();
        }

        // 3. 清理 Docker ID
        if self.config.clean_container_ids {
            text = DOCKER_ID_RE
                .replace_all(&text, |caps: &regex::Captures| {
                    let id = caps.get(0).unwrap().as_str();
                    // 如果 ID 很长，将其字典化为 $D
                    dict_engine.add_path_layered(id)
                })
                .into_owned();
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 对文本进行归一化处理（用于日志比对）。消除时间戳、随机 Hash、乱序参数等 Diff噪音。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除 Docker 容器 ID
        let id_re = &*ID_NORM_RE;
        result = id_re.replace_all(&result, "[ID]").to_string();

        // 抹除 IP 地址
        let ip_re = &*IP_NORM_RE;
        result = ip_re.replace_all(&result, "[IP]").to_string();

        result
    }

    /// 执行反向的还原逻辑。利用字典引擎中存储的上下文，将压缩后的 Token 流重新展开为完整、人类可读的原始文本。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        // 由核心引擎统一还原 $D/$P/$PK
        compressed.to_string()
    }
}

/// 判断小写文本是否含 Docker CI 输出特征（docker build/buildx/compose 等）。
fn is_docker_ci_output(lower: &str) -> bool {
    [
        "docker build",
        "docker buildx",
        "docker compose",
        "docker-compose",
        "#1 [internal]",
        "=> [internal]",
        "writing image sha256:",
        "naming to docker.io/",
        "failed to solve:",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// 判断小写文本是否含 Kubernetes CI 输出特征（kubectl rollout/apply、deployment 等）。
fn is_kubernetes_ci_output(lower: &str) -> bool {
    [
        "kubectl rollout",
        "kubectl apply",
        "kubectl diff",
        "deployment.apps/",
        "statefulset.apps/",
        "daemonset.apps/",
        "service/",
        "configmap/",
        "waiting for deployment",
        "error: deployment",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}
