//! 字典管理器方法实现 - v6.0 终极前缀树折叠版

use crate::core::dictionary_engine::Dictionary;
use dashmap::DashMap;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 字典三分类
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum DictCategory {
    Structural, // A 类：路径、命令
    Semantic,   // B 类：错误消息、重要状态
    Noise,      // C 类：冗余信息
}

thread_local! {
    static PATH_ID_CACHE: Cell<(usize, usize)> = Cell::new((0, 0));
    static MACRO_ID_CACHE: Cell<(usize, usize)> = Cell::new((0, 0));
    static PKG_ID_CACHE: Cell<(usize, usize)> = Cell::new((0, 0));
}

/// 前缀树节点，用于统计和折叠目录
#[derive(Debug, Default)]
struct TrieNode {
    children: HashMap<String, TrieNode>,
    count: usize,
    is_end: bool,
}

impl TrieNode {
    fn insert(&mut self, segments: &[&str]) {
        self.count += 1;
        if segments.is_empty() {
            self.is_end = true;
            return;
        }
        let child = self.children.entry(segments[0].to_string()).or_default();
        child.insert(&segments[1..]);
    }
}

/// 字典管理器
#[derive(Debug)]
pub struct DictionaryManager {
    // 并行期极速字典：只存绝对路径
    pub(crate) path_dict: DashMap<String, (String, DictCategory)>,
    pub(crate) package_dict: DashMap<String, (String, DictCategory)>,
    pub(crate) macro_dict: DashMap<String, (String, DictCategory)>,
    pub(crate) command_dict: DashMap<String, (String, DictCategory)>,

    // 反向索引：内容 -> Token
    pub(crate) path_rev: DashMap<String, String>,
    pub(crate) macro_rev: DashMap<String, String>,
    pub(crate) package_rev: DashMap<String, String>,
    pub(crate) command_rev: DashMap<String, String>,

    next_path_id: AtomicUsize,
    next_package_id: AtomicUsize,
    next_macro_id: AtomicUsize,
    next_command_id: AtomicUsize,
}

impl DictionaryManager {
    pub fn new() -> Self {
        Self {
            path_dict: DashMap::new(),
            package_dict: DashMap::new(),
            macro_dict: DashMap::new(),
            command_dict: DashMap::new(),

            path_rev: DashMap::new(),
            macro_rev: DashMap::new(),
            package_rev: DashMap::new(),
            command_rev: DashMap::new(),

            next_path_id: AtomicUsize::new(1),
            next_package_id: AtomicUsize::new(1),
            next_macro_id: AtomicUsize::new(1),
            next_command_id: AtomicUsize::new(1),
        }
    }

    fn fetch_next_id(
        &self,
        atomic: &AtomicUsize,
        cache: &'static std::thread::LocalKey<Cell<(usize, usize)>>,
    ) -> usize {
        cache.with(|c| {
            let (curr, limit) = c.get();
            if curr >= limit {
                let batch_start = atomic.fetch_add(100, Ordering::Relaxed);
                c.set((batch_start + 1, batch_start + 100));
                batch_start
            } else {
                c.set((curr + 1, limit));
                curr
            }
        })
    }

    /// 极速路径记录：不做任何切分，直接返回 $P
    pub fn get_or_add_path(&self, path: &str) -> String {
        if path.starts_with('$') {
            return path.to_string();
        }
        if path.len() < 10 {
            return path.to_string();
        } // 忽略无意义短路径

        if let Some(token) = self.path_rev.get(path) {
            return token.value().clone();
        }

        let entry = self.path_rev.entry(path.to_string());
        match entry {
            dashmap::mapref::entry::Entry::Occupied(o) => o.get().clone(),
            dashmap::mapref::entry::Entry::Vacant(v) => {
                let id = self.fetch_next_id(&self.next_path_id, &PATH_ID_CACHE);
                let token = format!("$P{}", id);
                self.path_dict
                    .insert(token.clone(), (path.to_string(), DictCategory::Structural));
                v.insert(token.clone());
                token
            }
        }
    }

    pub fn get_or_add_package(&self, pkg: &str) -> String {
        if pkg.len() < 5 {
            return pkg.to_string();
        }
        if let Some(token) = self.package_rev.get(pkg) {
            return token.value().clone();
        }
        let id = self.fetch_next_id(&self.next_package_id, &PKG_ID_CACHE);
        let token = format!("$PK{}", id);
        self.package_dict
            .insert(token.clone(), (pkg.to_string(), DictCategory::Structural));
        self.package_rev.insert(pkg.to_string(), token.clone());
        token
    }

    pub fn get_or_add_macro(&self, m: &str) -> String {
        if m.len() < 10 {
            return m.to_string();
        }
        if let Some(token) = self.macro_rev.get(m) {
            return token.value().clone();
        }
        let id = self.fetch_next_id(&self.next_macro_id, &MACRO_ID_CACHE);
        let token = format!("$M{}", id);

        let category = if m.to_lowercase().contains("error") || m.to_lowercase().contains("fail") {
            DictCategory::Semantic
        } else {
            DictCategory::Noise
        };

        self.macro_dict
            .insert(token.clone(), (m.to_string(), category));
        self.macro_rev.insert(m.to_string(), token.clone());
        token
    }

    pub fn add_macros(&self, macros: Vec<String>) {
        for m in macros {
            self.get_or_add_macro(&m);
        }
    }

    pub fn add_compile_commands(&self, commands: Vec<String>) {
        for cmd in commands {
            if cmd.len() < 10 {
                continue;
            }
            if self.command_rev.get(&cmd).is_some() {
                continue;
            }
            let id = self.next_command_id.fetch_add(1, Ordering::Relaxed);
            let token = format!("$C{}", id);
            self.command_dict
                .insert(token.clone(), (cmd.clone(), DictCategory::Structural));
            self.command_rev.insert(cmd, token);
        }
    }

    pub fn get_path_by_token(&self, token: &str) -> Option<String> {
        self.path_dict.get(token).map(|e| e.value().0.clone())
    }

    // --- 树形优化核心逻辑 ---

    fn extract_directories(
        node: &TrieNode,
        parent_token: String,
        path_since_anchor: String,
        next_d_id: &mut usize,
        directories: &mut HashMap<String, String>,
    ) {
        for (seg, child) in &node.children {
            let current_path = if path_since_anchor.is_empty() {
                seg.clone()
            } else {
                format!("{}/{}", path_since_anchor, seg)
            };

            let mut current_token = parent_token.clone();

            // v6.0 核心：权重识别。只有“重度分支”才值得提取
            let significant_children = child.children.values().filter(|c| c.count > 10).count();

            // 触发条件：具有多个有价值的分支，或者是一个高频重复的叶子节点，并且路径积累足够长
            if (significant_children > 1 || (child.children.is_empty() && child.count > 10))
                && current_path.len() > 15
            {
                let new_token = format!("$D{}", next_d_id);
                *next_d_id += 1;

                let dict_value = if parent_token.is_empty() {
                    format!("/{}", current_path)
                } else {
                    format!("{}/{}", parent_token, current_path)
                };

                directories.insert(new_token.clone(), dict_value);

                current_token = new_token;
                self::DictionaryManager::extract_directories(
                    child,
                    current_token.clone(),
                    String::new(),
                    next_d_id,
                    directories,
                );
            } else {
                self::DictionaryManager::extract_directories(
                    child,
                    current_token,
                    current_path,
                    next_d_id,
                    directories,
                );
            }
        }
    }

    /// 生成字典的全局快照 (Snapshot)，用于序列化输出。
    ///
    /// 在生成快照时，会执行树形折叠 (Radix Trie 延迟路径提取)：
    /// 通过对 `path_dict` 中的绝对路径构建前缀树，提取出最大公共前缀
    /// (重度分支和高频叶子节点)，并将其替换为 `$D` (Directory) 锚点。
    /// 这种设计既保证了运行期并发插入时的高性能，又能在序列化前实现最优的路径层级压缩（Path Layering）。
    pub fn snapshot(&self) -> Dictionary {
        // 1. 构建前缀树
        let mut root = TrieNode::default();
        for e in self.path_dict.iter() {
            let path = &e.value().0;
            // 仅对绝对路径建树
            if path.starts_with('/') {
                let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
                root.insert(&segments);
            }
        }

        // 2. 提取 $D 锚点
        let mut next_d_id = 1;
        let mut directories = HashMap::new();
        Self::extract_directories(
            &root,
            String::new(),
            String::new(),
            &mut next_d_id,
            &mut directories,
        );

        // 3. 构建解析映射，准备替换 $P
        let mut resolved_dirs = HashMap::new();
        for (k, v) in &directories {
            let mut resolved = v.clone();
            if resolved.starts_with("$D") {
                let t: String = resolved.chars().take_while(|c| *c != '/').collect();
                if let Some(parent_val) = directories.get(&t) {
                    resolved = resolved.replace(&t, parent_val);
                }
            }
            resolved_dirs.insert(k.clone(), resolved);
        }

        let mut sorted_dirs: Vec<_> = resolved_dirs.into_iter().collect();
        sorted_dirs.sort_by(|a, b| b.1.len().cmp(&a.1.len())); // 从最长前缀开始匹配

        // 4. 重写 $P 字典
        let mut compressed_paths = HashMap::new();
        for e in self.path_dict.iter() {
            let token = e.key().clone();
            let original_path = e.value().0.clone();

            let mut best_match = original_path.clone();
            if original_path.starts_with('/') {
                for (d_token, resolved_path) in &sorted_dirs {
                    if original_path.starts_with(resolved_path) {
                        best_match =
                            format!("{}{}", d_token, &original_path[resolved_path.len()..]);
                        break;
                    }
                }
            }
            compressed_paths.insert(token, best_match);
        }

        Dictionary {
            paths: compressed_paths,
            packages: self
                .package_dict
                .iter()
                .map(|e| (e.key().clone(), e.value().0.clone()))
                .collect(),
            macros: self
                .macro_dict
                .iter()
                .map(|e| (e.key().clone(), e.value().0.clone()))
                .collect(),
            files: HashMap::new(),
            directories,
            flags: self
                .command_dict
                .iter()
                .map(|e| (e.key().clone(), e.value().0.clone()))
                .collect(),
            custom: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn shutdown(&self) {}
}

impl Default for DictionaryManager {
    fn default() -> Self {
        Self::new()
    }
}
