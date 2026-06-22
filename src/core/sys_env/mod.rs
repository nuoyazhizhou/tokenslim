//! sys env 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 sys env 功能。
//!
//! ## 主要功能
//!
//! - 提供核心类型定义和接口
//! - 协调各子组件的工作流程
//! - 对外提供统一的 API 接口

use sys_locale::get_locale;
use sysinfo::System;

/// 表示当前运行环境的系统信息结构。
#[derive(Debug, Clone)]
pub struct EnvironmentInfo {
    /// 操作系统名称（如 Windows, Linux）
    pub os: String,
    /// 操作系统内核或发行版版本
    pub os_version: String,
    /// 系统区域设置（Locale）
    pub locale: String,
    /// 系统中存在的各类文件系统类型列表
    pub file_systems: Vec<String>,
}

/// 获取并探测当前的系统运行环境信息。
///
/// 该函数会刷新系统信息缓存，并尝试识别操作系统版本、区域语言设置
/// 以及挂载的所有文件系统类型。
pub fn get_environment_info() -> EnvironmentInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let os = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());

    let locale = get_locale().unwrap_or_else(|| "Unknown".to_string());

    let mut file_systems = Vec::new();
    let disks = sysinfo::Disks::new_with_refreshed_list();
    for disk in disks.iter() {
        let fs = disk.file_system().to_string_lossy().into_owned();
        if !fs.is_empty() && !file_systems.contains(&fs) {
            file_systems.push(fs);
        }
    }

    EnvironmentInfo {
        os,
        os_version,
        locale,
        file_systems,
    }
}
