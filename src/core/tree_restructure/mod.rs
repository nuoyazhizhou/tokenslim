// tree_restructure/mod.rs
// 树结构重组模块 - 将文件列表重组为目录树结构

pub mod config;
pub mod render;
pub mod trie;

pub use config::*;
pub use render::*;
pub use trie::*;

use regex::Regex;

/// 将文本重组为树结构
///
/// # 参数
/// - `text`: 原始文本（如 git status 输出）
/// - `config`: 树结构配置
///
/// # 返回
/// - 重组后的树结构文本，如果不满足门控条件则返回原始文本
#[tracing::instrument(level = "debug", skip_all)]
pub fn restructure_as_tree(text: &str, config: &TreeConfig) -> String {
    // 1. 解析每一行，提取路径
    let mut matched_entries = Vec::new();
    let re = Regex::new(&config.path_pattern).unwrap_or_else(|_| {
        // 默认路径模式：匹配文件路径
        Regex::new(r"(?:^|\s)([a-zA-Z0-9_./\\-]+\.[a-zA-Z0-9]+)(?:\s|$)").unwrap()
    });

    for line in text.lines() {
        if let Some(entry) = parse_line(&re, line) {
            matched_entries.push(entry);
        }
    }

    // 2. 门控检查：最少匹配行数
    if matched_entries.len() < config.min_files {
        return text.to_string();
    }

    // 3. 计算共享深度
    let shared_depth = calculate_shared_depth(&matched_entries);

    // 4. 门控检查：最少共享深度
    if shared_depth < config.min_shared_depth {
        return text.to_string();
    }

    // 5. 构建 Trie
    let mut root = TrieNode::new("");
    for entry in &matched_entries {
        insert_path(&mut root, &entry.components, &entry.decoration, &entry.tail);
    }

    // 6. 折叠单孩子目录
    if config.collapse_single_child {
        collapse_single_child(&mut root);
    }

    // 7. 排序
    if config.sort {
        sort_node(&mut root);
    }

    // 8. 渲染
    render_tree(&root, &config.style)
}

/// 解析一行，提取路径信息
fn parse_line(re: &Regex, line: &str) -> Option<MatchedEntry> {
    let captures = re.captures(line)?;
    let path = captures.get(1)?.as_str();

    // 提取装饰（路径前的内容）
    let decoration = line[..captures.get(1)?.start()].trim().to_string();

    // 提取尾部（路径后的内容）
    let tail = line[captures.get(1)?.end()..].trim().to_string();

    // 分割路径为组件
    let components: Vec<String> = path
        .split(|c| c == '/' || c == '\\')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    if components.is_empty() {
        return None;
    }

    Some(MatchedEntry {
        path: path.to_string(),
        components,
        decoration,
        tail,
    })
}

/// 计算共享深度
fn calculate_shared_depth(entries: &[MatchedEntry]) -> usize {
    if entries.is_empty() {
        return 0;
    }

    let mut shared_depth = entries[0].components.len();

    for entry in entries.iter().skip(1) {
        let mut depth = 0;
        for (i, component) in entry.components.iter().enumerate() {
            if i >= entries[0].components.len() {
                break;
            }
            if component == &entries[0].components[i] {
                depth += 1;
            } else {
                break;
            }
        }
        shared_depth = shared_depth.min(depth);
    }

    shared_depth
}

/// 匹配的条目
#[derive(Debug, Clone)]
pub struct MatchedEntry {
    pub path: String,
    pub components: Vec<String>,
    pub decoration: String,
    pub tail: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line() {
        // 使用更精确的正则表达式，匹配包含路径分隔符的文件路径
        let re = Regex::new(r"([a-zA-Z0-9_]+[/\\][a-zA-Z0-9_./\\-]+)").unwrap();

        let entry = parse_line(&re, "M  src/main.rs").unwrap();
        assert_eq!(entry.path, "src/main.rs");
        assert_eq!(entry.components, vec!["src", "main.rs"]);
        assert_eq!(entry.decoration, "M");
        assert_eq!(entry.tail, "");

        let entry = parse_line(&re, "  modified:   tests/test.rs").unwrap();
        assert_eq!(entry.path, "tests/test.rs");
        assert_eq!(entry.components, vec!["tests", "test.rs"]);
    }

    #[test]
    fn test_calculate_shared_depth() {
        let entries = vec![
            MatchedEntry {
                path: "src/core/mod.rs".to_string(),
                components: vec!["src".to_string(), "core".to_string(), "mod.rs".to_string()],
                decoration: "".to_string(),
                tail: "".to_string(),
            },
            MatchedEntry {
                path: "src/core/types.rs".to_string(),
                components: vec![
                    "src".to_string(),
                    "core".to_string(),
                    "types.rs".to_string(),
                ],
                decoration: "".to_string(),
                tail: "".to_string(),
            },
            MatchedEntry {
                path: "src/lib.rs".to_string(),
                components: vec!["src".to_string(), "lib.rs".to_string()],
                decoration: "".to_string(),
                tail: "".to_string(),
            },
        ];

        assert_eq!(calculate_shared_depth(&entries), 1);
    }

    #[test]
    fn test_restructure_as_tree_with_gating() {
        let config = TreeConfig {
            path_pattern: r"([a-zA-Z0-9_./\\-]+)".to_string(),
            min_files: 10,
            min_shared_depth: 1,
            collapse_single_child: true,
            sort: true,
            style: RenderStyle::Unicode,
        };

        // 少于 min_files，应返回原始文本
        let text = "M  src/main.rs\nM  src/lib.rs";
        let result = restructure_as_tree(text, &config);
        assert_eq!(result, text);
    }
}
