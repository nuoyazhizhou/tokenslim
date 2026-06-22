// tree_restructure/trie.rs
// Trie 树数据结构与操作

use std::collections::HashMap;

/// Trie 节点
#[derive(Debug, Clone)]
pub struct TrieNode {
    /// 节点名称
    pub name: String,
    /// 子节点
    pub children: HashMap<String, TrieNode>,
    /// 是否为叶子节点（文件）
    pub is_leaf: bool,
    /// 装饰（如 git status 的 M, A, D 等）
    pub decoration: String,
    /// 尾部信息
    pub tail: String,
}

impl TrieNode {
    /// 创建新节点
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            children: HashMap::new(),
            is_leaf: false,
            decoration: String::new(),
            tail: String::new(),
        }
    }

    /// 是否为空目录
    pub fn is_empty_dir(&self) -> bool {
        !self.is_leaf && self.children.is_empty()
    }

    /// 是否只有一个子节点
    pub fn has_single_child(&self) -> bool {
        !self.is_leaf && self.children.len() == 1
    }

    /// 获取唯一的子节点
    pub fn get_single_child(&self) -> Option<&TrieNode> {
        if self.has_single_child() {
            self.children.values().next()
        } else {
            None
        }
    }

    /// 获取唯一的子节点（可变）
    pub fn get_single_child_mut(&mut self) -> Option<&mut TrieNode> {
        if self.has_single_child() {
            self.children.values_mut().next()
        } else {
            None
        }
    }
}

/// 插入路径到 Trie
///
/// # 参数
/// - `root`: 根节点
/// - `components`: 路径组件
/// - `decoration`: 装饰
/// - `tail`: 尾部信息
#[tracing::instrument(level = "trace", skip_all)]
pub fn insert_path(root: &mut TrieNode, components: &[String], decoration: &str, tail: &str) {
    if components.is_empty() {
        return;
    }

    let mut current = root;

    for (i, component) in components.iter().enumerate() {
        let is_last = i == components.len() - 1;

        current = current
            .children
            .entry(component.clone())
            .or_insert_with(|| TrieNode::new(component));

        if is_last {
            current.is_leaf = true;
            current.decoration = decoration.to_string();
            current.tail = tail.to_string();
        }
    }
}

/// 折叠单孩子目录
///
/// 例如: src/core/mod.rs 中，如果 src 只有一个子目录 core，
/// 则折叠为 src/core/mod.rs
#[tracing::instrument(level = "trace", skip_all)]
pub fn collapse_single_child(node: &mut TrieNode) {
    // 递归处理所有子节点
    for child in node.children.values_mut() {
        collapse_single_child(child);
    }

    // 跳过根节点（name 为空）
    if node.name.is_empty() {
        return;
    }

    // 如果当前节点只有一个子节点，且子节点不是叶子，则折叠
    while node.has_single_child() {
        // 获取子节点的键名（HashMap 中的键）
        let child_key = node.children.keys().next().unwrap().clone();
        let child = node.children.get(&child_key).unwrap();

        if child.is_leaf {
            break;
        }

        // 折叠：将子节点的名称合并到当前节点
        let child_name = child.name.clone();
        let child_children = node.children.remove(&child_key).unwrap().children;

        // 更新当前节点的名称
        if !node.name.is_empty() {
            node.name = format!("{}/{}", node.name, child_name);
        } else {
            node.name = child_name;
        }

        // 更新子节点
        node.children = child_children;
    }
}

/// 排序节点
///
/// 目录在前，文件在后，同类按字母排序
#[tracing::instrument(level = "trace", skip_all)]
pub fn sort_node(node: &mut TrieNode) {
    // 递归排序所有子节点
    for child in node.children.values_mut() {
        sort_node(child);
    }

    // 将子节点转换为 Vec 并排序
    let mut children: Vec<_> = node.children.drain().collect();
    children.sort_by(|a, b| {
        // 目录在前
        match (a.1.is_leaf, b.1.is_leaf) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.0.cmp(&b.0),
        }
    });

    // 重新插入
    node.children = children.into_iter().collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_path() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "main.rs".to_string()],
            "M",
            "",
        );

        assert_eq!(root.children.len(), 1);
        assert!(root.children.contains_key("src"));

        let src = &root.children["src"];
        assert_eq!(src.children.len(), 1);
        assert!(src.children.contains_key("main.rs"));

        let main_rs = &src.children["main.rs"];
        assert!(main_rs.is_leaf);
        assert_eq!(main_rs.decoration, "M");
    }

    #[test]
    fn test_collapse_single_child() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "core".to_string(), "mod.rs".to_string()],
            "M",
            "",
        );

        collapse_single_child(&mut root);

        // 根节点应该有一个子节点
        assert_eq!(root.children.len(), 1);

        // 获取第一个（也是唯一的）子节点
        let (key, collapsed) = root.children.iter().next().unwrap();

        // 键名应该还是 "src"
        assert_eq!(key, "src");

        // 但节点的 name 字段应该是 "src/core"（折叠后）
        assert_eq!(collapsed.name, "src/core");
        assert_eq!(collapsed.children.len(), 1);
        assert!(collapsed.children.contains_key("mod.rs"));
    }

    #[test]
    fn test_sort_node() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "main.rs".to_string()],
            "",
            "",
        );
        insert_path(
            &mut root,
            &["tests".to_string(), "test.rs".to_string()],
            "",
            "",
        );
        insert_path(&mut root, &["README.md".to_string()], "", "");

        sort_node(&mut root);

        let keys: Vec<_> = root.children.keys().cloned().collect();
        // 目录在前（src, tests），文件在后（README.md）
        // 但是 HashMap 的顺序不保证，所以我们需要检查目录和文件的分组
        let dirs: Vec<_> = keys
            .iter()
            .filter(|k| root.children[*k].children.len() > 0)
            .collect();
        let files: Vec<_> = keys.iter().filter(|k| root.children[*k].is_leaf).collect();

        assert_eq!(dirs.len(), 2); // src, tests
        assert_eq!(files.len(), 1); // README.md
    }

    #[test]
    fn test_has_single_child() {
        let mut node = TrieNode::new("test");
        assert!(!node.has_single_child());

        node.children
            .insert("child".to_string(), TrieNode::new("child"));
        assert!(node.has_single_child());

        node.children
            .insert("child2".to_string(), TrieNode::new("child2"));
        assert!(!node.has_single_child());
    }
}
