// tree_restructure/render.rs
// 树结构渲染引擎

use super::config::RenderStyle;
use super::trie::TrieNode;

/// 渲染树结构
///
/// # 参数
/// - `root`: 根节点
/// - `style`: 渲染风格
/// - `sort_children`: 子节点渲染排序（目录在前/同类字母序）；`false` 时按 HashMap 原生序
///
/// # 返回
/// - 渲染后的文本
#[tracing::instrument(level = "debug", skip_all)]
pub fn render_tree(root: &TrieNode, style: &RenderStyle, sort_children: bool) -> String {
    let mut output = String::new();
    render_node(root, "", true, &mut output, style, sort_children);
    output
}

/// 渲染单个节点
fn render_node(
    node: &TrieNode,
    prefix: &str,
    is_last: bool,
    output: &mut String,
    style: &RenderStyle,
    sort_children: bool,
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
        let mut children: Vec<_> = node.children.iter().collect();
        // P2-60：HashMap 无序——排序承诺必须在渲染处落实（目录在前，同类按字母序），
        // 否则 `└─`/`├─` 框线分配随 RandomState 随机化，同输入两次运行输出不同。
        if sort_children {
            children.sort_by(|a, b| match (a.1.is_leaf, b.1.is_leaf) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => a.0.cmp(b.0),
            });
        }
        for (i, (_, child)) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            render_node(
                child,
                &new_prefix,
                is_last_child,
                output,
                style,
                sort_children,
            );
        }
    } else {
        // 根节点，直接渲染子节点
        let mut children: Vec<_> = node.children.iter().collect();
        if sort_children {
            children.sort_by(|a, b| match (a.1.is_leaf, b.1.is_leaf) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => a.0.cmp(b.0),
            });
        }
        for (i, (_, child)) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            render_node(child, "", is_last_child, output, style, sort_children);
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

    /// 测试：Unicode 风格渲染输出包含 ├─ / └─ / │ 框线字符。
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

        let output = render_tree(&root, &RenderStyle::Unicode, true);

        // 应该包含 Unicode 框线字符
        assert!(output.contains("├─"));
        assert!(output.contains("└─"));
        assert!(output.contains("│"));
    }

    /// 测试：ASCII 风格渲染输出包含 |- / `- 字符。
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

        let output = render_tree(&root, &RenderStyle::Ascii, true);

        // 应该包含 ASCII 框线字符
        assert!(output.contains("|-"));
        assert!(output.contains("`-"));
        assert!(output.contains("|"));
    }

    /// 测试：纯缩进风格渲染不包含任何框线字符。
    #[test]
    fn test_render_tree_indent() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "main.rs".to_string()],
            "",
            "",
        );

        let output = render_tree(&root, &RenderStyle::Indent, true);

        // 纯缩进风格，不应该包含框线字符
        assert!(!output.contains("├"));
        assert!(!output.contains("└"));
        assert!(!output.contains("│"));
        assert!(!output.contains("|"));
        assert!(!output.contains("`"));
    }

    /// 测试：渲染结果包含节点装饰（M）与尾部信息（modified）。
    #[test]
    fn test_render_with_decoration() {
        let mut root = TrieNode::new("");
        insert_path(
            &mut root,
            &["src".to_string(), "main.rs".to_string()],
            "M",
            "modified",
        );

        let output = render_tree(&root, &RenderStyle::Unicode, true);

        // 应该包含装饰和尾部信息
        assert!(output.contains("M "));
        assert!(output.contains("modified"));
    }

    /// P2-60 回归：`TrieNode.children` 是 HashMap（无序），排序承诺必须在渲染处落实。
    /// 同一输入渲染输出必须稳定，且子节点满足「目录在前、同类按字母序」——修复前
    /// `sort_node` 排序后又 re-collect 回 HashMap，排序被静默丢弃，框线分配随
    /// RandomState 随机化。
    #[test]
    fn test_render_tree_deterministic_and_sorted() {
        let mut root = TrieNode::new("");
        for (dir, file) in [
            ("zeta", "b.rs"),
            ("zeta", "a.rs"),
            ("src", "lib.rs"),
            ("src", "main.rs"),
            ("alpha", "x.rs"),
        ] {
            insert_path(&mut root, &[dir.to_string(), file.to_string()], "", "");
        }

        let first = render_tree(&root, &RenderStyle::Unicode, true);
        let second = render_tree(&root, &RenderStyle::Unicode, true);
        assert_eq!(
            first, second,
            "同一输入两次渲染输出必须逐字节一致（P2-60 确定性承诺）:\n{first}"
        );

        // 目录在前且按字母序：alpha < src < zeta
        let alpha = first.find("alpha").expect("alpha 目录应出现");
        let src = first.find("src").expect("src 目录应出现");
        let zeta = first.find("zeta").expect("zeta 目录应出现");
        assert!(alpha < src, "目录应按字母序渲染:\n{first}");
        assert!(src < zeta, "目录应按字母序渲染:\n{first}");

        // 同目录下文件按字母序：zeta/a.rs 先于 zeta/b.rs
        //（在 zeta 区段内查找，避免 "b.rs" 误匹配到 "lib.rs" 的子串）
        let zeta_section = &first[zeta..];
        let a_rs = zeta_section.find("a.rs").expect("a.rs 应出现");
        let b_rs = zeta_section.find("b.rs").expect("b.rs 应出现");
        assert!(a_rs < b_rs, "同目录文件应按字母序渲染:\n{first}");

        // last 框线分配：zeta 是最后一个目录，其子节点 b.rs 应获得 └─（收尾框线）
        let zeta_line_start = zeta;
        let b_rs_line = first[zeta_line_start..]
            .lines()
            .find(|l| l.contains("b.rs"))
            .expect("b.rs 行应存在");
        assert!(
            b_rs_line.contains("└─"),
            "末目录的最后文件应使用收尾框线 └─，实际: {b_rs_line}"
        );
    }
}
