//! plugin dispatcher 类型定义

use crate::core::compression::Token;
use crate::core::compression_context::CompressionContext;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::error_isolation::SafeExecutor;
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use serde::Serialize;
use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex};

/// 压缩结果，带有生命周期以支持 Arena 内存分配
#[derive(Debug, Clone)]
pub struct CompressResult<'a> {
    pub tokens: Vec<Token<'a>>,
    pub metadata: Option<HashMap<String, String>>,
    pub plugin_name: Option<&'static str>,
}

/// 文档级剥皮产物：把「外壳骨架」与「内层正文」分离，供两层化管线使用。
///
/// - `summary`：外壳摘要文本（决策信号留存，如 CI 步骤/错误统计、云 meta 等）。
///   必须由剥皮插件保证以原始命令锚点开头（压缩协议法则 0）；无可留信号时可留空，
///   此时管线仅保留内层 IR + 命令锚点单层。
/// - `inner_body`：内层干净工具输出，交由内层管线重新识别/切片/定向。
#[derive(Debug, Clone)]
pub struct DocumentSkin {
    pub summary: String,
    pub inner_body: String,
}

/// 插件接口定义
pub trait Plugin: Send + Sync + Any {
    /// 返回插件名称
    fn name(&self) -> &'static str;

    /// 返回优先级（越小越优先）
    fn priority(&self) -> u8;

    /// 探测切片是否匹配该插件
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32>;

    /// 执行压缩，引入 Arena 内存池
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a>;

    /// 执行压缩（带上下文能力）。默认复用 `compress`，插件可按需重写。
    fn compress_with_context<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
        _context: &mut CompressionContext,
    ) -> CompressResult<'a> {
        self.compress(slice, dict_engine, dedup_engine, arena)
    }

    /// 执行还原
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String;

    /// 推荐的后续处理插件
    fn next_plugins(&self) -> Vec<&'static str> {
        vec![]
    }

    /// 归一化处理（用于 Diff）
    fn normalize(&self, text: &str) -> String {
        text.to_string()
    }

    /// 尝试作为脱壳器 (Unwrapper) 剥离外壳，如果成功脱壳则返回内层纯净文本，否则返回 None。
    fn unwrap(&self, _text: &str) -> Option<String> {
        None
    }

    /// 文档级剥皮：当整个文档被判定为带皮类别（syslog/ci/cloud）时，把外层骨架
    /// 与内层正文分离，供两层化管线组合「外壳摘要 + 内层 IR」。
    ///
    /// 返回 `None` 表示该插件无皮可剥或无需两层化（如无内层正文的纯 CI 编排日志），
    /// 调用方应回退现状单切片路径。默认实现不剥皮。
    fn peel_document(&self, _text: &str) -> Option<DocumentSkin> {
        None
    }
}

/// 插件调度器配置
#[derive(Clone)]
pub struct DispatcherConfig {
    pub plugin_timeout_ms: u64,
}

impl Default for DispatcherConfig {
    /// 构造默认调度器配置：插件超时 1000ms。
    fn default() -> Self {
        Self {
            plugin_timeout_ms: 1000,
        }
    }
}

/// 插件执行错误
#[derive(Debug, thiserror::Error)]
pub enum PluginExecutionError {
    #[error("E_PLUGIN_EXECUTION_PANIC")]
    Panic,
    #[error("E_PLUGIN_EXECUTION_TIMEOUT:{0:?}")]
    Timeout(std::time::Duration),
    #[error("E_PLUGIN_EXECUTION_OTHER:{0}")]
    Other(String),
}

use crate::core::dictionary_manager::DictionaryManager;

/// 审计记录使用的、已注册插件描述。不包含用户输入或插件配置正文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginAuditDescriptor {
    pub plugin_id: String,
    pub priority: u8,
}

/// 单次压缩期间按插件汇总的真实执行贡献。
///
/// `output_estimated_bytes` 是插件返回 token 的估算大小，仅用于相对归因，
/// 不应被解释为精确的计费 token 数。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginAuditEffect {
    pub plugin_id: String,
    pub priority: u8,
    pub invocation_count: usize,
    pub input_bytes: usize,
    pub output_estimated_bytes: usize,
    pub changed: bool,
}
/// 插件调度器主结构
pub struct PluginDispatcher {
    pub(crate) plugins: Vec<Box<dyn Plugin>>,
    pub(crate) plugin_map: HashMap<String, usize>,
    /// 调度器配置（P2-65 接线）：`plugin_timeout_ms` 现实际传入
    /// [`SafeExecutor`] 的 `default_timeout`，不再 dead_code。
    pub(crate) config: DispatcherConfig,
    /// 错误隔离执行器：插件压缩调用的 panic 捕获统一经由 `SafeExecutor::catch_panic` 完成。
    pub(crate) executor: SafeExecutor,
    pub(crate) keyword_scanner: Arc<aho_corasick::AhoCorasick>,
    pub(crate) plugin_failures: Mutex<HashMap<String, u32>>,
    /// 本压缩窗口内按插件累计的 panic 次数（由 P1-10 panic 隔离点写入）。
    /// 管线在指标回写点排水进 `MetricsCollector::inc_plugin_panic`（修复 P2-63 恒 0）。
    pub(crate) plugin_panic_counts: Mutex<HashMap<String, u32>>,
    pub(crate) audit_effects: Mutex<Vec<PluginAuditEffect>>,
    pub(crate) audit_trace_active: AtomicBool,
    /// 本压缩窗口内 ANSI 剥离累计删除的字节数（含真 ANSI 序列与裸 CSI 残留）。
    /// 作为「裸码剥离红灯」的量化依据：输入含裸码但该值仍为 0 即为确定 bug。
    pub(crate) ansi_strip_bytes_removed: AtomicUsize,
}
