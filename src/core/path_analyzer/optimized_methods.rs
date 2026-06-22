use std::collections::{HashMap, HashSet};
use regex::Regex;

/// 提取文本中所有的路径（支持 Windows、Linux 和 macOS）
pub fn extract_all_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    
    // 匹配 Linux/macOS 路径：以 / 开头的路径
    let unix_path_re = Regex::new(r"/(?:[\w\.\-\+_~=@#]+/)*[\w\.\-\+_~=@#]+").unwrap();
    for cap in unix_path_re.captures_iter(text) {
        if let Some(path) = cap.get(0) {
            let path_str = path.as_str().to_string();
            if !seen.contains(&path_str) {
                paths.push(path_str.clone());
                seen.insert(path_str);
            }
        }
    }
    
    // 匹配 Windows 路径：以 C:\ 等盘符开头的路径
    let windows_path_re = Regex::new(r"[A-Za-z]:\\(?:[\w\.\-\+_~=@#]+\\)*[\w\.\-\+_~=@#]+").unwrap();
    for cap in windows_path_re.captures_iter(text) {
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

/// 目录树节点
#[derive(Debug)]
struct TreeNode {
    children: HashMap<String, Box<TreeNode>>,
    count: usize, // 引用计数
    is_leaf: bool,
}

impl TreeNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            count: 0,
            is_leaf: false,
        }
    }
}

/// 分析路径的公共前缀，生成层级化的路径字典
pub fn analyze_path_hierarchy(paths: &[String]) -> (HashMap<String, String>, Vec<String>) {
    if paths.is_empty() {
        return (HashMap::new(), Vec::new());
    }
    
    // 构建目录树
    let mut root = TreeNode::new();
    
    for path in paths {
        let parts: Vec<&str> = path.split(|c| c == '/' || c == '\\').filter(|&p| !p.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        
        // 将路径添加到树中
        let mut current = &mut root;
        for part in parts {
            current.count += 1;
            current = current.children.entry(part.to_string()).or_insert_with(|| Box::new(TreeNode::new()));
        }
        current.count += 1;
        current.is_leaf = true;
    }
    
    // 生成路径字典
    let mut dict = HashMap::new();
    let mut token_counter = 1;
    
    // 提取公共路径
    extract_common_paths(&root, "", "/", &mut dict, &mut token_counter);
    
    // 生成多级字典
    generate_multi_level_dict(&mut dict, &mut token_counter);
    
    // 提取主要的公共路径
    let common_paths = extract_top_common_paths(&dict);
    
    (dict, common_paths)
}

/// 提取公共路径（优化版本）
fn extract_common_paths(
    node: &TreeNode,
    current_path: &str,
    separator: &str,
    dict: &mut HashMap<String, String>,
    counter: &mut i32
) {
    // 优化：只提取长度超过20的路径
    for (part, child) in &node.children {
        let new_path = if current_path.is_empty() {
            part.to_string()
        } else {
            format!("{}{}{}", current_path, separator, part)
        };
        
        // 优化：增加最小路径长度阈值
        if child.count > 1 && new_path.len() > 20 {
            let token = format!("$P{}", counter);
            dict.insert(new_path.clone(), token);
            *counter += 1;
        }
        
        // 递归处理子节点
        extract_common_paths(child, &new_path, separator, dict, counter);
    }
}

/// 生成多级字典
fn generate_multi_level_dict(dict: &mut HashMap<String, String>, counter: &mut i32) {
    // 收集所有路径和token
    let mut path_token_pairs: Vec<(String, String)> = dict.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    
    // 按路径长度排序，长的在前
    path_token_pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    
    // 生成多级字典
    let mut new_entries = Vec::new();
    
    for (base_path, base_token) in &path_token_pairs {
        // 遍历所有路径，检查是否是base_path的子路径
        for (full_path, _) in &path_token_pairs {
            if full_path != base_path && full_path.starts_with(&format!("{}/", base_path)) {
                // 提取子路径部分
                let sub_path = full_path.trim_start_matches(&format!("{}/", base_path));
                // 生成多级token
                let multi_level_token = format!("{}/{}", base_token, sub_path);
                let new_token = format!("$P{}", counter);
                new_entries.push((full_path.clone(), new_token));
                *counter += 1;
            }
        }
    }
    
    // 添加新生成的多级字典条目
    for (path, token) in new_entries {
        dict.insert(path, token);
    }
}

/// 提取主要的公共路径
fn extract_top_common_paths(dict: &HashMap<String, String>) -> Vec<String> {
    let mut paths: Vec<String> = dict.keys().cloned().collect();
    
    // 按长度排序，长的在前
    paths.sort_by(|a, b| b.len().cmp(&a.len()));
    
    // 只返回前 100 个最长的公共路径
    paths.into_iter().take(100).collect()
}

/// 替换文本中的路径为 token
pub fn replace_paths_with_tokens(text: &str, dict: &HashMap<String, String>) -> String {
    let mut result = text.to_string();
    
    // 按路径长度排序，长的优先替换
    let mut paths: Vec<&String> = dict.keys().collect();
    paths.sort_by(|a, b| b.len().cmp(&a.len()));
    
    for path in paths {
        if let Some(token) = dict.get(path) {
            // 确保路径被正确替换，包括路径前后的分隔符
            let path_with_sep = format!("/{}", path);
            result = result.replace(&path_with_sep, &format!("/{}", token));
        }
    }
    
    result
}
