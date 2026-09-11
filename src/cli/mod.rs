//! cli 妯″潡
//!
//! # 妯″潡姒傝堪
//!
//! 鏈ā鍧楀疄鐜颁簡 TokenSlim 鐨?cli 鍔熻兘銆?//!
//! ## 涓昏鍔熻兘
//!
//! - 鎻愪緵鏍稿績绫诲瀷瀹氫箟鍜屾帴鍙?//! - 鍗忚皟鍚勫瓙缁勪欢鐨勫伐浣滄祦绋?//! - 瀵瑰鎻愪緵缁熶竴鐨?API 鎺ュ彛

mod app;
pub mod commands;
pub mod common;
pub mod conpty_probe;
pub mod pty_runner;
#[cfg(test)]
mod test;
mod types;
pub mod whitelist;

pub use app::{get_plugins, run_cli};
pub(crate) use common::*;
pub use types::{
    CliArgs, CliError, CliMode, DoctorKind, DoctorOutputFormat, HookShell, InputSource,
    OutputFormat, OutputTarget, Preset,
};
