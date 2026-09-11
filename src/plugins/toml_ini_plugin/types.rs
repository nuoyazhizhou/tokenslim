//! toml_ini 插件类型定义

//! # 类型概述

//! 本模块定义了 toml_ini 插件所需的核心数据类型：配置结构与插件主结构。
//! 该插件面向「TOML / INI / 类 INI 键值配置」文件类文本：TOML 用 `[table]` +
//! `key = value`，INI 用 `[section]` + `key=value`，二者共享同一种行结构，故可收敛到
//! 单一插件，避免为两种几乎同构的格式各造一个插件。
use serde::{Deserialize, Serialize};

/// TOML / INI 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlIniConfig {
    /// 至少要出现几个 `key = value` 行才认定不是普通命令/脚本文本，防止把 shell
    /// `FOO=bar` 或日志行误判为配置。
    #[serde(default = "default_min_keyval")]
    pub min_keyval_lines: usize,
}

/// 返回键值行阈值默认值 3。
fn default_min_keyval() -> usize {
    3
}

impl Default for TomlIniConfig {
    /// 构造默认配置：键值行阈值 3。
    fn default() -> Self {
        Self {
            min_keyval_lines: default_min_keyval(),
        }
    }
}

/// TOML / INI 压缩插件主结构
pub struct TomlIniPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub config: TomlIniConfig,
}
