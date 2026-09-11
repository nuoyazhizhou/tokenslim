//! 字典管理器方法实现 - v6.0 终极前缀树折叠版

use crate::core::dictionary_engine::is_semantic_macro;
use crate::core::dictionary_engine::Dictionary;
use dashmap::DashMap;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};

/// 字典三分类
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum DictCategory {
    Structural, // A 类：路径、命令
    Semantic,   // B 类：错误消息、重要状态
    Noise,      // C 类：冗余信息
}

// P1-03：线程本地 ID 批发缓存携带「所属 manager 实例 ID」。
// 旧实现只看 (curr, limit)，rayon 长驻 worker 线程被多个 DictionaryManager
// 复用时，会把上一个实例的余量 ID 发给新实例，导致不同路径拿到同一 `$Pn`
// （path_dict 后写覆盖先写，解压时张冠李戴）且编号输出不确定。
thread_local! {
    static PATH_ID_CACHE: Cell<(u64, usize, usize)> = Cell::new((0, 0, 0));
    static MACRO_ID_CACHE: Cell<(u64, usize, usize)> = Cell::new((0, 0, 0));
    static PKG_ID_CACHE: Cell<(u64, usize, usize)> = Cell::new((0, 0, 0));
    static COMMAND_ID_CACHE: Cell<(u64, usize, usize)> = Cell::new((0, 0, 0));
}

/// 全局 manager 实例计数器：为每个 DictionaryManager 分配唯一 instance_id。
static NEXT_MANAGER_INSTANCE_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

/// 前缀树节点，用于统计和折叠目录
#[derive(Debug, Default)]
struct TrieNode {
    // P2-03：旧实现用 HashMap，`snapshot()`/`extract_directories` 按节点顺序 DFS
    // 分配 `$D{n}` 编号——HashMap 迭代顺序不确定，同一输入两次快照 $D 编号可能不同，
    // 产物不具可复现性。改 BTreeMap（键有序）保证遍历与编号确定性。
    children: BTreeMap<String, TrieNode>,
    count: usize,
    is_end: bool,
}

impl TrieNode {
    /// 将路径分段递归插入前缀树节点：每访问一层计数加一，末段标记为终点。
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

/// 统一路径分隔符为 `/` 并折叠重复分隔符，供盘符/正反斜杠混用路径归一。
/// 返回规范化后的字符串；非路径（空串）返回原值。
fn normalize_path_separators(path: &str) -> String {
    if path.is_empty() {
        return path.to_string();
    }
    // 统一 `\`→`/`（Windows 反斜杠），`//`→`/` 折叠，保留盘符前缀 `C:`。
    let mut norm = path.replace('\\', "/");
    while norm.contains("//") {
        norm = norm.replace("//", "/");
    }
    norm
}

/// 判断路径是否为绝对路径：Unix 的 `/` 打头，或 Windows 盘符 `C:/`（大小写不限）。
fn is_absolute_path(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

/// 判断单个分段是否为 Windows 盘符（单字母 + `:`，大小写不限）。
fn is_drive_name(seg: &str) -> bool {
    let bytes = seg.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
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
    /// 本实例唯一标识（P1-03）：线程本地批发缓存的归属校验依据。
    instance_id: u64,
}

impl DictionaryManager {
    /// 创建空字典管理器，初始化路径/包/宏/命令四条字典与反向索引，各分类 ID 计数器从 1 起。
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
            instance_id: NEXT_MANAGER_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// 批量预取下一个 ID（P1-03 修复版）：线程本地缓存携带实例归属，仅当
    /// 缓存属于当前 manager 且未耗尽时才继续发放；实例不匹配或耗尽时丢弃
    /// 余量、向本实例的原子计数器重新批发 100 个。正确性优先于余量利用率。
    fn fetch_next_id(
        &self,
        atomic: &AtomicUsize,
        cache: &'static std::thread::LocalKey<Cell<(u64, usize, usize)>>,
    ) -> usize {
        cache.with(|c| {
            let (owner, curr, limit) = c.get();
            if owner == self.instance_id && curr < limit {
                c.set((owner, curr + 1, limit));
                return curr;
            }
            let batch_start = atomic.fetch_add(100, Ordering::Relaxed);
            c.set((self.instance_id, batch_start + 1, batch_start + 100));
            batch_start
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

    /// 查询或新增包名到字典，返回 `$PKn` token；过短包名（<5 字符）原样返回。
    ///
    /// P2-07：与 `get_or_add_path` 对齐改用 `entry()` 原子登记——旧实现是
    /// 「先查反向索引、查不到即取号 + 双向 insert」的 check-then-act 序列，
    /// 并发下两个线程可能对同一包名各生成一个不同 `$PKn`（浪费 ID 且字典膨胀）。
    /// vacant 分支内只触碰 `package_dict`（不同的 map），与 path 实现同一安全范式。
    pub fn get_or_add_package(&self, pkg: &str) -> String {
        if pkg.len() < 5 {
            return pkg.to_string();
        }
        if let Some(token) = self.package_rev.get(pkg) {
            return token.value().clone();
        }
        let entry = self.package_rev.entry(pkg.to_string());
        match entry {
            dashmap::mapref::entry::Entry::Occupied(o) => o.get().clone(),
            dashmap::mapref::entry::Entry::Vacant(v) => {
                let id = self.fetch_next_id(&self.next_package_id, &PKG_ID_CACHE);
                let token = format!("$PK{}", id);
                self.package_dict
                    .insert(token.clone(), (pkg.to_string(), DictCategory::Structural));
                v.insert(token.clone());
                token
            }
        }
    }

    /// 查询或新增宏/消息到字典，返回 `$Mn` token；含 error/fail 的归入语义类，否则为噪声类。
    ///
    /// P2-07：与 `get_or_add_path` 对齐改用 `entry()` 原子登记（同 package，
    /// 消除 check-then-act 并发竞态）；vacant 分支内只触碰 `macro_dict`。
    pub fn get_or_add_macro(&self, m: &str) -> String {
        if m.len() < 10 {
            return m.to_string();
        }
        if let Some(token) = self.macro_rev.get(m) {
            return token.value().clone();
        }
        let entry = self.macro_rev.entry(m.to_string());
        match entry {
            dashmap::mapref::entry::Entry::Occupied(o) => o.get().clone(),
            dashmap::mapref::entry::Entry::Vacant(v) => {
                let id = self.fetch_next_id(&self.next_macro_id, &MACRO_ID_CACHE);
                let token = format!("$M{}", id);

                // P2-11：与解析侧共用同一语义/噪声谓词，登记类别与解析跳噪行为一致。
                let category = if is_semantic_macro(m) {
                    DictCategory::Semantic
                } else {
                    DictCategory::Noise
                };

                self.macro_dict
                    .insert(token.clone(), (m.to_string(), category));
                v.insert(token.clone());
                token
            }
        }
    }

    /// 批量将一组宏文本登记进宏字典（逐个调用 `get_or_add_macro`）。
    pub fn add_macros(&self, macros: Vec<String>) {
        for m in macros {
            self.get_or_add_macro(&m);
        }
    }

    /// 批量登记编译命令到命令字典，返回 `$Cn` token；过短或已存在者跳过。
    pub fn add_compile_commands(&self, commands: Vec<String>) {
        for cmd in commands {
            if cmd.len() < 10 {
                continue;
            }
            if self.command_rev.get(&cmd).is_some() {
                continue;
            }
            // P3-05：与 path/package/macro 三套机制统一走 fetch_next_id 批发缓存。
            let id = self.fetch_next_id(&self.next_command_id, &COMMAND_ID_CACHE);
            let token = format!("$C{}", id);
            self.command_dict
                .insert(token.clone(), (cmd.clone(), DictCategory::Structural));
            self.command_rev.insert(cmd, token);
        }
    }

    /// 根据路径 token（`$Pn`）反查原始绝对路径，不存在时返回 `None`。
    pub fn get_path_by_token(&self, token: &str) -> Option<String> {
        self.path_dict.get(token).map(|e| e.value().0.clone())
    }

    // --- 树形优化核心逻辑 ---

    /// 从前缀树递归提取 `$D` 目录锚点：仅对重度分支或高频叶子且路径足够长的节点折叠为锚点。
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
                    // P2-01：驱动路径根不进 leading `/`（Unix 进，Windows `C:` 不进）。
                    let drive_root = current_path.is_empty()
                        || is_drive_name(&current_path.split('/').next().unwrap_or(""));
                    if drive_root {
                        current_path
                    } else {
                        format!("/{}", current_path)
                    }
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
        // 1. 构建前缀树（P2-01：兼容 Windows 盘符路径）
        let mut root = TrieNode::default();
        for e in self.path_dict.iter() {
            let raw_path = &e.value().0;
            let path = normalize_path_separators(raw_path);
            // 仅对绝对路径建树（Unix `/` 或 Windows `C:/` 盘符）
            if is_absolute_path(&path) {
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

        // 4. 重写 $P 字典（P2-01：匹配同样走盘符/分隔符归一）
        let mut compressed_paths = HashMap::new();
        for e in self.path_dict.iter() {
            let token = e.key().clone();
            let original_path = e.value().0.clone();
            let norm_path = normalize_path_separators(&original_path);

            let mut best_match = original_path.clone();
            if is_absolute_path(&norm_path) {
                for (d_token, resolved_path) in &sorted_dirs {
                    // resolved_path 已归一为 `/` 分隔，与 norm_path 同坐标系。
                    if !norm_path.starts_with(resolved_path) {
                        continue;
                    }
                    let rest = &norm_path[resolved_path.len()..];
                    // 匹配到目录根：尾部即刻换锚点；否则保留剩余层级。
                    best_match = if rest.is_empty() {
                        d_token.clone()
                    } else {
                        format!("{}{}", d_token, rest)
                    };
                    break;
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

    /// 优雅关闭字典管理器（当前无后台资源，留作扩展钩子，空实现）。
    pub fn shutdown(&self) {}
}

impl Default for DictionaryManager {
    /// 返回字典管理器默认实例（等同于 `new`）。
    fn default() -> Self {
        Self::new()
    }
}
