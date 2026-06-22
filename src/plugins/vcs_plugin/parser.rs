//! vcs_plugin 中央调度器 — 解析器 trait 定义 + 共享辅助函数
//!
//! 所有具体 VCS 工具解析器已迁移到独立微插件，本文件仅保留:
//! 1. VcsParser trait 定义
//! 2. 共享辅助函数 (helpers.rs)
#![allow(dead_code, private_interfaces)]

use super::ir::{VcsDocKind, VcsDocument, VcsRecord};
use super::methods::looks_like_vcs_path;
use super::types::VcsTool;
use std::collections::HashSet;

pub trait VcsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument>;
}

// 共享辅助函数 (path validation, generic parsers, etc.)
include!("parser/helpers.rs");
