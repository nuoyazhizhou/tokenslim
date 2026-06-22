// tree_restructure/render.rs
// 树结构渲染引擎

use super::config::RenderStyle;
use super::trie::TrieNode;

/// 渲染树结构
///
/// # 参数
/// - `root`: 根节点
/// - `style`: 渲染风格
///
/// # 返回
/// - 渲染后的文本
#[tracing::instrument(level = "debug", skip_all)]
pub fn render_tree(root: &TrieNode, style: &RenderStyle) -> String {
    let mut output = String::new();
    render_node(root, "", true, &mut output, style);
    output
}

/// 渲染单个节点
fn render_node(
    node: &TrieNode,
    prefix: &str,
    is_last: bool,
    output: &mut String,
    style: &RenderStyle,
) {
    // 跳过根节点
    if !node.name.is_empty() {
        let (branch, continuation) = get_style_chars(style, is_last);

        // 输出当前节点
        let decoration = if !node.decoration.is_empty() {
            format!("{} ", node.decoration)
        } else {
            String::new()
        };

        let tail = if !node.tail.is_empty() {
            format!(" {}", node.tail)
        } else {
            String::new()
        };

        output.push_str(&format!(
            "{}{}{}{}{}\n",
            prefix, branch, decoration, node.name, tail
        ));

        // 更新前缀
        let new_prefix = format!("{}{}", prefix, continuation);

        // 渲染子节点
        let children: Vec<_> = node.children.values().collect();
        for (i, child) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            render_node(child, &new_prefix, is_last_child, output, style);
        }
    } else {
        // 根节点，直接渲染子节点
        let children: Vec<_> = node.children.values().collect();
        for (i, child) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            render_node(child, "", is_last_child, output, style);
        }
    }
}

/// 获取风格字符
fn get_style_chars(style: &RenderStyle, is_last: bool) -> (&'static str, &'static str) {
    match style {
        RenderStyle::Unicode => {
            if is_last {
                ("└─ ", "   ")
            } else {
                ("├─ ", "│  ")
            }
        }
        RenderStyle::Ascii => {
            if is_last {
                ("`- ", "   ")
            } else {
                ("|- ", "|  ")
            }
        }
        RenderStyle::Indent => ("  ", "  "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree_restructure::trie::insert_path;

    #[test]
    fn test_render_tree_unicode() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "main.rs".to_string()],
            "M",
            "",
        );
        insert_path(
            &mut root,
            &["src".to_string(), "lib.rs".to_string()],
            "A",
            "",
        );
        insert_path(
            &mut root,
            &["tests".to_string(), "test.rs".to_string()],
            "",
            "",
        );

        let output = render_tree(&root, &RenderStyle::Unicode);

        // 应该包含 Unicode 框线字符
        assert!(output.contains("├─"));
        assert!(output.contains("└─"));
        assert!(output.contains("│"));
    }

    #[test]
    fn test_render_tree_ascii() {
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

        let output = render_tree(&root, &RenderStyle::Ascii);

        // 应该包含 ASCII 框线字符
        assert!(output.contains("|-"));
        assert!(output.contains("`-"));
        assert!(output.contains("|"));
    }

    #[test]
    fn test_render_tree_indent() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "main.rs".to_string()],
            "",
            "",
        );

        let output = render_tree(&root, &RenderStyle::Indent);

        // 纯缩进风格，不应该包含框线字符
        assert!(!output.contains("├"));
        assert!(!output.contains("└"));
        assert!(!output.contains("│"));
        assert!(!output.contains("|"));
        assert!(!output.contains("`"));
    }

    #[test]
    fn test_render_with_decoration() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "main.rs".to_string()],
            "M",
            "modified",
        );

        let output = render_tree(&root, &RenderStyle::Unicode);

        // 应该包含装饰和尾部信息
        assert!(output.contains("M "));
        assert!(output.contains("modified"));
    }
}
