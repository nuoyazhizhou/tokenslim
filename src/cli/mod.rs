//! cli 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 cli 功能。
//!
//! ## 主要功能
//!
//! - 提供核心类型定义和接口
//! - 协调各子组件的工作流程
//! - 对外提供统一的 API 接口

pub mod commands;
pub mod common;
mod app;
pub mod conpty_probe;
pub mod pty_runner;
mod types;
pub mod whitelist;

pub use app::{get_plugins, run_cli};
pub(crate) use common::*;
pub use types::{
    CliArgs, CliError, CliMode, DoctorKind, DoctorOutputFormat, HookShell, InputSource, OutputFormat,
    OutputTarget, Preset,
};
