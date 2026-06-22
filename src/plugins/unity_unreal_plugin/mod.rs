//! Unity/Unreal构建日志脱水：保留Unreal和Unity的日志标签，折叠连续的资源加载噪音。

//! ## 保留信号
//! - LogUObject、LogHAL、LogLinker、FAndroidApp（Unreal引擎日志标签）
//! - Unloading、Building AssetBundle、Shader compilation（Unity构建消息）
//! - Loading .uasset/.prefab/.mat（通用资源加载信息，但可能被聚合）

//! ## 压缩目标
//! - 连续的Loading Object行（聚合为一条，计数量）
pub mod methods;
pub mod types;
pub use types::*;

#[cfg(test)]
mod showcase;
