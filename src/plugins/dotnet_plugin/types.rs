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
    /// 构造 DotNetConfig 默认配置：折叠堆栈跟踪、清理 MSBuild 输出、提取命名空间均开启。
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
