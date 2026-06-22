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
// 常见的 Kubernetes 日志前缀格式：[pod-name] [container-id]
#[allow(dead_code)]
static K8S_PREFIX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\[(?P<meta>.*?)\]\s+").unwrap());

impl KubernetesDockerPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        KubernetesDockerPlugin {
            name: "kubernetes_docker",
            priority: 130, // 优先级较高，因为前缀通常最先处理
            config: KubernetesDockerConfig::default(),
        }
    }

    /// 内部辅助函数：执行与 unwrap json if possible 相关的具体逻辑。
    fn unwrap_json_if_possible<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if !self.config.unwrap_cloud_json {
            return Cow::Borrowed(text);
        }

        let Some(extracted) = extract_json_object(text) else {
            return Cow::Borrowed(text);
        };

        // 尝试解析常见的云平台日志格式
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(extracted.raw) {
            // 检查 AWS CloudWatch / GCP / Aliyun SLS 常见的 message 字段
            for field in &["message", "log", "content", "msg"] {
                if let Some(msg) = json.get(*field).and_then(|v| v.as_str()) {
                    return Cow::Owned(msg.to_string());
                }
            }
        }
        Cow::Borrowed(text)
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
        let id_re = regex::Regex::new(r"\b[a-f0-9]{12,64}\b").unwrap();
        result = id_re.replace_all(&result, "[ID]").to_string();

        // 抹除 IP 地址
        let ip_re = regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
        result = ip_re.replace_all(&result, "[IP]").to_string();

        result
    }

    /// 执行反向的还原逻辑。利用字典引擎中存储的上下文，将压缩后的 Token 流重新展开为完整、人类可读的原始文本。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        // 由核心引擎统一还原 $D/$P/$PK
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<KubernetesDockerConfig>() {
            self.config = new_config.clone();
            Ok(())
        } else {
            Err("Invalid config type".to_string())
        }
    }
}

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
