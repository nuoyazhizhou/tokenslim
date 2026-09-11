//! rehydration pipeline 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 rehydration pipeline 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

use crate::core::dictionary_engine::Dictionary;
use crate::core::metrics::MetricsCollector;
use crate::core::plugin_dispatcher::Plugin;
use std::cell::RefCell;

/// 还原流水线配置（P2-65 接线）
#[derive(Clone, Debug)]
pub struct RehydrationConfig {
    /// 遇到无法还原的压缩 token 时的行为：
    /// `true`（默认）= 宽松降级，残留 token 原样保留（兼容旧版本压缩产物与
    /// 日志原文中的 `$PATH`/`$HOME` 等 shell 变量字面量）；
    /// `false` = 严格模式，在真实失败点构造 `RehydrationError` 上传给调用方
    /// （CLI `--strict-rehydrate`）。
    pub fallback_on_error: bool,
}

impl Default for RehydrationConfig {
    /// RehydrationConfig 默认值：fallback_on_error=true（宽松降级，与历史行为一致）。
    fn default() -> Self {
        Self {
            fallback_on_error: true,
        }
    }
}

/// 还原流水线主结构
pub struct RehydrationPipeline {
    pub(crate) dict: Dictionary,
    /// 有序插件表（P1-02）：按 `priority()` 升序稳定排序后线性遍历执行
    /// `decompress`。替代原 `HashMap` 随机迭代序——后者使多插件命中同一行时
    /// 解压结果随进程启动随机化，破坏冻结基线的逐字节可复现性。
    pub(crate) plugins: Vec<Box<dyn Plugin>>,
    /// 还原行为配置（P2-65 接线）：`fallback_on_error` 决定解压失败时
    /// 宽松降级还是构造 `RehydrationError` 上传。
    pub(crate) config: RehydrationConfig,
    /// 可选的指标采集器（P2-63）：解压路径接入 `record_plugin_decompress`，
    /// 使 `decompress_calls` 在真实解压场景下非 0。默认 `None`（不注入，
    /// 与既有构造保持兼容），由 CLI 层按需 `with_metrics` 注入。
    pub(crate) metrics: Option<RefCell<MetricsCollector>>,
}

/// 还原错误类型（P2-65 接线）。
///
/// 仅保留存在真实构造点的变体：`UnknownToken`（解压终态残留字典键形态
/// token）与 `DictResolutionFailed`（`Token::DictRef` 无法在字典中解析）。
/// 原 `PluginNotFound`/`PluginDecompressFailed`/`AstReconstructionFailed`
/// 三变体全仓零构造且无自然失败点（`Plugin::decompress` 按契约不出错、
/// 无 AST 重建环节），随本批删除；待插件解压契约改为可失败或引入
/// round-trip 校验器时再按需重立变体。
#[derive(Debug, thiserror::Error)]
pub enum RehydrationError {
    #[error("E_REHYDRATION_UNKNOWN_TOKEN:{0}")]
    UnknownToken(String),
    #[error("E_REHYDRATION_DICT_RESOLUTION_FAILED:{0}")]
    DictResolutionFailed(String),
}
