/// kubernetes docker plugin 类型定义
use serde::{Deserialize, Serialize};

/// Kubernetes/Docker 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesDockerConfig {
    /// 是否提取 Kubernetes Pod 名称和命名空间
    pub extract_kubernetes_metadata: bool,
    /// 是否清理 Docker 容器 ID
    pub clean_container_ids: bool,
    /// 是否解包云平台日志（如 AWS CloudWatch）的 JSON 壳
    pub unwrap_cloud_json: bool,
}

impl Default for KubernetesDockerConfig {
    /// 构造 KubernetesDockerConfig 默认配置：提取 Pod 元数据、清理容器 ID、
    /// 解包云平台 JSON 壳均开启。
    fn default() -> Self {
        KubernetesDockerConfig {
            extract_kubernetes_metadata: true,
            clean_container_ids: true,
            unwrap_cloud_json: true,
        }
    }
}

/// Kubernetes/Docker 插件结构
pub struct KubernetesDockerPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: KubernetesDockerConfig,
}
