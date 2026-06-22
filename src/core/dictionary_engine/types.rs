use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DictType {
    Path,
    Package,
    Macro,
    File,
    Directory,
    Flag,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAliasRule {
    pub name: String,
    pub pattern: String,
    pub target_types: Vec<DictType>,
}

use crate::core::dictionary_manager::DictionaryManager;
use std::sync::Arc;

#[allow(dead_code)]
pub struct DictionaryEngine {
    pub(crate) paths: HashMap<String, String>,
    pub(crate) packages: HashMap<String, String>,
    pub(crate) macros: HashMap<String, String>,
    pub(crate) files: HashMap<String, String>,
    pub(crate) directories: HashMap<String, String>,
    pub(crate) flags: HashMap<String, String>,
    pub(crate) custom: HashMap<String, HashMap<String, String>>,
    pub(crate) custom_prefixes: HashMap<String, String>,
    pub(crate) next_ids: HashMap<DictType, usize>,
    #[allow(dead_code)]
    pub(crate) path_hierarchy_enabled: bool,
    pub(crate) semantic_aliases: HashMap<String, String>,
    #[allow(dead_code)]
    pub(crate) alias_rules: Vec<SemanticAliasRule>,
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
