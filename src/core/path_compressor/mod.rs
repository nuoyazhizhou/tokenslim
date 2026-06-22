//! 路径压缩器
//!
//! # 功能
//!
//! 提取公共路径前缀，使用 $P1/$P2 方案压缩长路径
//!
//! # 示例
//!
//! 原始路径：
//! - `/jenkins/workspace/android_build/app/build/intermediates/classes`
//! - `/jenkins/workspace/android_build/lib/build/outputs/apk`
//!
//! 压缩后：
//! - `$P1/app/build/intermediates/classes`
//! - `$P2/lib/build/outputs/apk`
//!
//! 字典：
//! - `$P1` = `/jenkins/workspace/android_build`
//! - `$P2` = `/jenkins/workspace/android_build/lib`

pub mod methods;
pub mod types;

pub use types::*;
