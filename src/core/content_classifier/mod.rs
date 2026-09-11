//! content classifier 模块
//!
//! # 模块概述
//!
//! 本模块实现了一个纯 std 的极简**多分类朴素贝叶斯分类器**，用于在
//! `ContentAnalyzer.quick_analyze` 与 `PluginDispatcher` 规则/词表/扩展名
//! 均未命中时，依据文本内容推断其语义类别（如 cargo 构建输出、gcc 编译输出、
//! 测试运行器输出、git diff 等），从而避免诸如 cargo/gcc 输出被误路由到
//! `generic_text` 兜底插件、导致大量无效压缩的问题。
//!
//! ## 主要功能
//!
//! - [`Category`]：语义类别枚举，同时携带「建议候选插件名」，供插件调度裁剪使用。
//! - [`NaiveBayesClassifier`]：多分类朴素贝叶斯分类器，采用词频 + 对数概率 +
//!   拉普拉斯平滑，纯标准库实现，无外部依赖。
//! - [`classify`] 入口：输入文本块，输出（语义类别, 置信度, 与次优类别的差距）。
//!
//! ## 设计要点
//!
//! - 纯 std：分词、计数、对数与 softmax 全部基于标准库，便于嵌入任何链路。
//! - 低置信度保护：当最高类别置信度低于阈值时，调用方应回退到全量插件 detect，
//!   杜绝分类错误导致的信息丢失（安全优先）。
//! - 种子特征表见 [`features`]，后续可由编译期特征聚合器（feature_builder）覆盖。

mod corpus_tokens;
pub mod feature_reader;
pub mod features;
pub mod holdout;
mod model;

pub use features::classifier;
pub use model::{Category, ClassifyResult, NaiveBayesClassifier};

#[cfg(test)]
mod test;
