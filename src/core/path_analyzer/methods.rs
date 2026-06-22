use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};

static UNIX_PATH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"/(?:[\w\.\-\+_~=@#]+/)*[\w\.\-\+_~=@#]+").unwrap());

static WINDOWS_PATH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[A-Za-z]:\\(?:[\w\.\-\+_~=@#]+\\)*[\w\.\-\+_~=@#]+").unwrap());

/// 提取文本中所有的路径（支持 Windows、Linux 和 macOS）。
/// 该方法会通过正则表达式扫描文本并返回唯一路径列表。
pub fn extract_all_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    // 匹配 Linux/macOS 路径：以 / 开头的合法路径字符
    for cap in UNIX_PATH_RE.captures_iter(text) {
        if let Some(path) = cap.get(0) {
            let path_str = path.as_str().to_string();
            if !seen.contains(&path_str) {
                paths.push(path_str.clone());
                seen.insert(path_str);
            }
        }
    }

    // 匹配 Windows 路径：以 C:\ 等盘符开头的合法路径字符
    for cap in WINDOWS_PATH_RE.captures_iter(text) {
        if let Some(path) = cap.get(0) {
            let path_str = path.as_str().to_string();
            if !seen.contains(&path_str) {
                paths.push(path_str.clone());
                seen.insert(path_str);
            }
        }
    }

    paths
}

/// 用于构建路径 Trie 树的目录树节点。
#[derive(Debug)]
struct TreeNode {
    /// 子节点映射（目录名 -> 节点）
    children: HashMap<String, Box<TreeNode>>,
    /// 经过该节点的路径计数（用于识别公共前缀）
    count: usize,
    /// 是否是完整路径的结尾
    is_leaf: bool,
}

impl TreeNode {
    /// 创建一个新的空节点。
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            count: 0,
            is_leaf: false,
        }
    }
}

/// 分析给定的一组路径，识别公共前缀并生成层级化的路径字典。
///
/// # 参数
/// - `paths`: 原始路径字符串切片。
///
/// # 返回
/// 返回一个元组，包含：
/// 1. 路径到 Token 的映射表。
/// 2. 识别出的主要公共路径列表（按长度排序）。
pub fn analyze_path_hierarchy(paths: &[String]) -> (HashMap<String, String>, Vec<String>) {
    if paths.is_empty() {
        return (HashMap::new(), Vec::new());
    }

    // 构建目录树
    let mut root = TreeNode::new();

    for path in paths {
        let parts: Vec<&str> = path
            .split(|c| c == '/' || c == '\\')
            .filter(|&p| !p.is_empty())
            .collect();
        if parts.is_empty() {
            continue;
        }

        // 将路径深度插入到树中
        let mut current = &mut root;
        for part in parts {
            current.count += 1;
            current = current
                .children
                .entry(part.to_string())
                .or_insert_with(|| Box::new(TreeNode::new()));
        }
        current.count += 1;
        current.is_leaf = true;
    }

    // 生成路径字典
    let mut dict = HashMap::new();
    let mut token_counter = 1;

    // 递归提取公共路径段并分配 Token
    extract_common_paths(&root, "", "/", &mut dict, &mut token_counter);

    // 生成分级字典非常慢 (O(N^2)) 且在流水线中最终会被 DictionaryEngine 覆盖忽略，因此移除此步骤。
    // generate_multi_level_dict(&mut dict, &mut token_counter);

    // 筛选出最具有代表性的公共路径
    let common_paths = extract_top_common_paths(&dict);

    (dict, common_paths)
}

/// 核心递归算法：从树中提取在多个路径中出现（count > 1）且长度超过阈值的子路径。
fn extract_common_paths(
    node: &TreeNode,
    current_path: &str,
    separator: &str,
    dict: &mut HashMap<String, String>,
    counter: &mut i32,
) {
    // 遍历当前节点的子节点
    for (part, child) in &node.children {
        // 剪枝：如果 count <= 1，说明该节点及以下所有路径均无公共共享，直接跳过，避免无效的字符串分配和递归
        if child.count <= 1 {
            continue;
        }

        let new_path = if current_path.is_empty() {
            part.to_string()
        } else {
            format!("{}{}{}", current_path, separator, part)
        };

        // 策略：只有被引用超过一次，且路径总长大于 20 字符，才值得分配 Token
        if new_path.len() > 20 {
            let token = format!("$P{}", counter);
            dict.insert(new_path.clone(), token);
            *counter += 1;
        }

        // 继续向下递归
        extract_common_paths(child, &new_path, separator, dict, counter);
    }
}

/// 尝试通过将现有路径 Token 与剩余后缀组合，生成更高层级的压缩字典。
#[allow(dead_code)]
fn generate_multi_level_dict(dict: &mut HashMap<String, String>, counter: &mut i32) {
    // 拉取当前已知的所有路径对
    let mut path_token_pairs: Vec<(String, String)> =
        dict.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

    // 优先处理较长的路径，以获取更高的压缩率
    path_token_pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    let mut new_entries = Vec::new();

    for (base_path, _base_token) in &path_token_pairs {
        // 检查是否有路径包含当前 base_path 作为前缀
        for (full_path, _) in &path_token_pairs {
            if full_path != base_path && full_path.starts_with(&format!("{}/", base_path)) {
                // 找到包含关系后，为更长的复合路径分配新的 Token
                let _sub_path = full_path.trim_start_matches(&format!("{}/", base_path));
                let new_token = format!("$P{}", counter);
                new_entries.push((full_path.clone(), new_token));
                *counter += 1;
            }
        }
    }

    // 合并新发现的层级 Token
    for (path, token) in new_entries {
        dict.insert(path, token);
    }
}

/// 从字典中提取最重要的（最长的）100 个路径条目。
fn extract_top_common_paths(dict: &HashMap<String, String>) -> Vec<String> {
    let mut paths: Vec<String> = dict.keys().cloned().collect();

    // 排序逻辑：路径越长，压缩价值通常越高
    paths.sort_by(|a, b| b.len().cmp(&a.len()));

    // 上限封顶 100 条
    paths.into_iter().take(100).collect()
}

/// 将文本中的所有已知路径根据字典替换为对应的 Token。
/// 该过程通过长度降序遍历字典，确保“长路径前缀匹配”优先。
pub fn replace_paths_with_tokens(text: &str, dict: &HashMap<String, String>) -> String {
    let mut result = text.to_string();

    // 构建一份排序后的字典键列表
    let mut paths: Vec<&String> = dict.keys().collect();
    paths.sort_by(|a, b| b.len().cmp(&a.len()));

    for path in paths {
        if let Some(token) = dict.get(path) {
            // 在替换时确保带上路径引导符，增加准确度并保持格式
            let path_with_sep = format!("/{}", path);
            result = result.replace(&path_with_sep, &format!("/{}", token));
        }
    }

    result
}
