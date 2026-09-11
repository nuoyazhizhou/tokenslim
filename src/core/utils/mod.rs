pub mod json;
pub mod roi;

use regex::Regex;
use std::sync::OnceLock;

/// 剥离终端 ANSI 转义序列，供非 VCS 插件在入口第一步统一净化。
///
/// P3-192（C-4）：本函数是全库唯一权威实现，`plugin_dispatcher` 的切片级剥离
/// 与各插件入口的预处理统一收敛至此，消除「简单版/完整版」双实现漂移。
/// 语义 = 真实 ESC 序列（CSI + 单字符控制码）+ 裸 CSI 残留（`[Nm`，去 ESC 后的
/// 字面色码）双通道剥离，且保护 `path/to/[2m]odule.rs` 这类以 `]` 收尾的合法路径。
pub fn strip_ansi(text: &str) -> String {
    // 无真实 ESC 字节 → 快速路径直接跳过正则扫描；裸 CSI（`[Nm`）仍需处理，
    // 故仅当「既无 ESC 也无 `[`」才可整串透传。
    if !text.as_bytes().contains(&0x1b) && !text.contains("[") {
        return text.to_string();
    }
    let cleaned = ansi_re().replace_all(text, "").into_owned();
    strip_naked_csi(&cleaned)
}

/// 真 ANSI 序列正则：CSI（`ESC [ ... final`）+ 单字符控制码（`ESC X`，如
/// `ESC 7/8` 存/取光标、`ESC D/E/M` 索引、`ESC ( )` 字符集选择）。
static ANSI_RE: OnceLock<Regex> = OnceLock::new();

#[inline]
fn ansi_re() -> &'static Regex {
    ANSI_RE.get_or_init(|| {
        Regex::new(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])").expect("ANSI 剥离正则编译失败")
    })
}

/// 裸 CSI 残留（`[Nm`）正则：去真实 ESC 字节后的字面色码字面量（cargo 报错样本）。
static NAKED_CSI_RE: OnceLock<Regex> = OnceLock::new();

#[inline]
fn naked_csi_re() -> &'static Regex {
    NAKED_CSI_RE.get_or_init(|| Regex::new(r"\[[0-9;]+m").expect("裸 CSI 剥离正则编译失败"))
}

/// 剥离裸 CSI 残留（`[Nm`），但保留「紧跟 `]` 且该 `]` 后是路径/词语字符」的合法字面量。
///
/// 脱色日志的色码是「去 ESC 后的字面量」，须无条件剥；而 `path/to/[2m]odule.rs`、
/// `test [0m] ok`、`build/[3m]/lib.rs` 等合法目录名都以 `]` 紧跟裸码、其后衔接路径字符，
/// 若误剥会砍坏 smart_path 依赖的路径。故遍历每个裸码命中：若后跟 `]` **且**该 `]` 后不是
/// 另一个 `[Nm`（即 `]` 后是路径/词语字符），判定为合法字面量保留；否则判为色码残留丢弃。
/// 后者覆盖 cargo Usage 的 `[1m[96m][0m`——`][0m` 是相邻色码链，`]` 只是被着色的字面括号。
fn strip_naked_csi(text: &str) -> String {
    let re = naked_csi_re();
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for m in re.find_iter(text) {
        out.push_str(&text[last..m.start()]);
        let end = m.end();
        let rest = &text[end..];
        // 仅当「紧跟 `]` 且该 `]` 后不是另一个 `[Nm`」才视为合法字面量；否则是色码残留
        let keep_literal = rest.starts_with(']') && !rest[1..].starts_with('[');
        if keep_literal {
            out.push_str(&text[m.start()..end]);
        }
        last = end;
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    /// 脱色日志：色码是脱掉 ESC 后的字面裸码，必须无条件剥干净。
    /// 这就是 302 字节 cargo 报错样本（真实输入无任何 `\x1B` 字节）的核心回归。
    #[test]
    fn test_naked_ansi_text_is_stripped() {
        assert_eq!(
            strip_ansi("[1m[91merror:[0m unexpected argument '[1m[93mcontent_analyzer[0m' found"),
            "error: unexpected argument 'content_analyzer' found"
        );
        assert_eq!(
            strip_ansi("[1m[92mUsage:[0m [1m[96mcargo.exe test[0m [36m[OPTIONS][0m ..."),
            "Usage: cargo.exe test [OPTIONS] ..."
        );
        assert_eq!(
            strip_ansi("For more information, try '[1m[96m--help[0m'."),
            "For more information, try '--help'."
        );
        // 真实 cargo 完整 Usage 行：`[-- [ARGS]...[96m]` 里的 `]` 是被着色的字面括号，
        // `[96m` 必须剥（不能因「紧跟 `]`」被保留，否则 `...][96m]` 泄漏进输出）。
        assert_eq!(
            strip_ansi("[1m[92mUsage:[0m [1m[96mcargo.exe test[0m [36m[OPTIONS][0m [36m[TESTNAME][0m [1m[96m[--[0m [36m[ARGS]...[0m[1m[96m][0m"),
            "Usage: cargo.exe test [OPTIONS] [TESTNAME] [-- [ARGS]...]"
        );
    }

    /// 以 `]` 收尾的裸码是合法字面量（路径/上下文），必须原样保留，绝不误伤。
    #[test]
    fn test_literal_bracket_residues_preserved() {
        assert_eq!(strip_ansi("path/to/[2m]odule.rs"), "path/to/[2m]odule.rs");
        assert_eq!(strip_ansi("test [0m] ok"), "test [0m] ok");
        // warning 前缀前的字符不能被吃掉，否则 `warning:` 前缀匹配失配
        assert_eq!(
            strip_ansi("level[3m] warning: foo"),
            "level[3m] warning: foo"
        );
        // 目录名左括号 `[2m]` 与路径字符相邻 → 保留；但紧跟另一裸码的 `]` 是被着色的
        // 字面括号（cargo Usage 的 `[1m[96m][0m`），不构成合法目录，须剥。
        assert_eq!(strip_ansi("build/[3m]/lib.rs"), "build/[3m]/lib.rs");
        // 链中 `[1m`/`[2m` 后是另一裸码（着色的字面括号）必须剥，其间的 `]` 作为字面括号留
        // 下；`[3m]` 后是空格（非色码字符），是合法字面量保留——与 `build/[3m]/lib.rs` 一致。
        assert_eq!(
            strip_ansi("progress [1m][2m][3m] done"),
            "progress ]][3m] done"
        );
        // 含字母的方括号文本不会被裸码正则误判为 SGR
        assert_eq!(strip_ansi("[OPTIONS]"), "[OPTIONS]");
    }

    /// 含真实 ESC 字节（曾彩色化）的文本：真 ANSI 序列无条件剥掉，
    /// 脱色后残留的裸 CSI（`[Nm`）也一并清干净。
    #[test]
    fn test_colored_with_residue_is_cleaned() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m[1m[96m"), "red");
        assert_eq!(strip_ansi("\u{1b}[1m[96mtext and [0m"), "text and ");
    }

    /// 单字符 ESC 控制码（ESC D 索引 / ESC M 反向索引）剥离；类外字符（如数字、`=`）不剥。
    #[test]
    fn test_single_char_esc_sequences_stripped() {
        assert_eq!(strip_ansi("a\u{1b}Db"), "ab");
        assert_eq!(strip_ansi("x\u{1b}My"), "xy");
        // 类外单字符（0x3D `=`、数字）不属于标准 ESC 序列，保持原样
        assert_eq!(strip_ansi("\u{1b}="), "\u{1b}=");
        assert_eq!(strip_ansi("v\u{1b}7w"), "v\u{1b}7w");
    }

    /// 空串与不含任何色码的纯净文本保持零改动。
    #[test]
    fn test_trivial_input_unchanged() {
        assert_eq!(strip_ansi(""), "");
        assert_eq!(strip_ansi("plain"), "plain");
    }
}
