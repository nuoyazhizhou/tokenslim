/// dotnet plugin 类型定义
use serde::{Deserialize, Serialize};

/// .NET 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotNetConfig {
    pub fold_stack_traces: bool,
    pub clean_msbuild_output: bool,
    pub extract_namespaces: bool,
}

impl Default for DotNetConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        DotNetConfig {
            fold_stack_traces: true,
            clean_msbuild_output: true,
            extract_namespaces: true,
        }
    }
}

/// .NET 插件结构
pub struct DotNetPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: DotNetConfig,
}
