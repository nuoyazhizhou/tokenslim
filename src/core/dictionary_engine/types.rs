use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::dictionary_manager::DictionaryManager;
use std::sync::Arc;

/// 字典引擎：本身不缓存任何词表，所有登记/查询/snapshot 一律委托
/// `DictionaryManager`（P3-01/P3-86：原 12 个本地映射字段均为死字段，已删除）。
pub struct DictionaryEngine {
    pub(crate) manager: Option<Arc<DictionaryManager>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Dictionary {
    pub paths: HashMap<String, String>,
    pub packages: HashMap<String, String>,
    pub macros: HashMap<String, String>,
    pub files: HashMap<String, String>,
    pub directories: HashMap<String, String>,
    pub flags: HashMap<String, String>,
    pub custom: HashMap<String, HashMap<String, String>>,
    pub aliases: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct HierarchicalNode {
    pub name: String,
    pub token: Option<String>,
    pub children: HashMap<String, HierarchicalNode>,
    pub is_essential: bool,
    pub alias: Option<String>,
}

impl HierarchicalNode {
    /// 构造一个空的层级节点：以给定名称初始化，token 与 alias 为 None，无子节点，且默认标记为非必要（is_essential=false）。
    #[allow(dead_code)]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            token: None,
            children: HashMap::new(),
            is_essential: false,
            alias: None,
        }
    }
}

#[allow(dead_code)]
pub enum TokenLevel {
    Essential,
    Contextual,
    Opaque,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PathHierarchyConfig {
    pub min_dir_length: usize,
    pub min_occurrences: usize,
    pub max_prefixes: usize,
}

#[allow(dead_code)]
impl Default for PathHierarchyConfig {
    /// 提供路径层级压缩的默认调优参数：目录最小长度 15、最小出现次数 2、最多保留前缀数 10；作为路径分层化策略的基线配置。
    fn default() -> Self {
        Self {
            min_dir_length: 15,
            min_occurrences: 2,
            max_prefixes: 10,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DictError {
    #[error("E_DICT_TOKEN_CONFLICT:{0}")]
    TokenConflict(String),
    #[error("E_DICT_TYPE_NOT_REGISTERED:{0}")]
    TypeNotRegistered(String),
    #[error("E_DICT_SERIALIZATION:{0}")]
    Serialization(#[from] serde_json::Error),
    #[error("E_DICT_IO:{0}")]
    Io(#[from] std::io::Error),
    #[error("E_DICT_ENTRY_NOT_FOUND:{0}")]
    NotFound(String),
    #[error("E_DICT_TYPE_NOT_FOUND:{0}")]
    TypeNotFound(String),
}
