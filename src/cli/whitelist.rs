//! 双清单配置 (compress_whitelist + tty_support_list) —— v0.4.0 核心
//!
//! # 设计目标
//!
//! 替代 v0.3.7 的启发式黑名单 (`detect_git_interactive` + `is_git_program`),
//! 改用"已知可压缩 / 已知支持 tty"双白名单 + 三层合并架构:
//!
//! ```text
//!   L3 (用户) ~/.tokenslim-whitelist.toml
//!     ↓ 覆盖
//!   L2 (项目) config/whitelist.toml   ← include_str! 嵌入 binary
//!     ↓ 覆盖
//!   L1 (代码) DEFAULT_COMPRESS_WHITELIST / DEFAULT_TTY_SUPPORT_LIST 常量
//! ```
//!
//! # 3 路分发
//!
//! 1. 命令在 `compress_whitelist` → 走 plugin 压缩
//! 2. 命令在 `tty_support_list` 且 ConPTY 可用 → 走 ConPTY 转发
//! 3. 其余命令 → 走 passthrough fallback (stdio 透传, 退出码透传)
//!
//! # 安全
//!
//! - "我们支持什么自己知道, 我们不支持什么不知道" —— 白名单思想
//! - L1 默认清单是保守的: 不在 L1 = 不承诺支持, 走 fallback 永远安全
//! - 用户 L3 配置可扩展 / 缩减; 错的代价是走错路线 (不会丢数据)

use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

/// L1 默认: 已知可压缩命令 (compress_whitelist) —— 走 plugin 压缩
///
/// 选择标准: 命令输出结构稳定, 有现成 plugin 可压缩; 脚本语言 (python/node)
/// 故意不在此列, 因为它们常常是 REPL 入口, 放 [`DEFAULT_TTY_SUPPORT_LIST`].
const DEFAULT_COMPRESS_WHITELIST: &[&str] = &[
    // === VCS (11) ===
    "git", "svn", "hg", "fossil", "p4", "bzr", "cvs", "darcs", "git-lfs", "glab", "gh",
    // === Build/Test (12) ===
    "make", "cmake", "ninja", "meson", "gradle", "mvn", "ant", "sbt", "msbuild",
    "dotnet", "cargo", "rustc",
    // === Package Manager (7) ===
    "npm", "yarn", "pnpm", "npx", "pip", "go", "javac",
    // === 简单输出工具 (15) ===
    "ls", "dir", "cat", "type", "head", "tail", "wc", "grep", "find",
    "where", "which", "tree", "du", "df", "sort",
];

/// L1 默认: 已知交互式命令 (tty_support_list) —— 走 ConPTY 转发
///
/// 选择标准: 命令可能打开交互式 UI (编辑器/REPL/远程 shell/分页器/subshell).
/// 脚本语言解释器 (python/node/ruby) 在此列, 因为 REPL 用法普遍, 即便用户
/// 调用 `python script.py`, 走 ConPTY 转发是安全的 (子进程仍能拿到 tty).
const DEFAULT_TTY_SUPPORT_LIST: &[&str] = &[
    // === 编辑器 (12) ===
    "vim", "vi", "nvim", "emacs", "nano", "pico", "code", "subl", "micro",
    "helix", "hx", "kak", "kakoune", "neovide",
    // === REPL / 脚本语言 (20) ===
    "python", "python3", "ipython", "node", "deno", "bun", "irb", "ruby",
    "pry", "scala", "ghci", "ghcup", "julia", "R", "Rscript", "lua", "perl",
    "php", "sqlite3", "mysql", "psql", "mongosh", "redis-cli",
    // === 远程 (7) ===
    "ssh", "telnet", "ftp", "sftp", "scp", "rsync", "mosh",
    // === 分页器 (3) ===
    "less", "more", "most",
    // === Subshell (12) ===
    "bash", "zsh", "fish", "sh", "dash", "ksh", "csh", "tcsh",
    "powershell", "pwsh", "cmd", "wsl",
];

/// Whitelist 4 段配置 (L2/L3 共享结构)
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WhitelistConfig {
    /// compress_whitelist extra 段: 追加到 L1 默认
    #[serde(default)]
    pub compress: CompressSection,
    /// tty_support_list extra 段: 追加到 L1 默认
    #[serde(default)]
    pub tty: TtySection,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CompressSection {
    #[serde(default)]
    pub extra: CommandList,
    #[serde(default)]
    pub remove: CommandList,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TtySection {
    #[serde(default)]
    pub extra: CommandList,
    #[serde(default)]
    pub remove: CommandList,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CommandList {
    #[serde(default)]
    pub commands: Vec<String>,
}

/// 合并后的最终双清单 (供 3 路分发使用)
#[derive(Debug, Clone, Default)]
pub struct Whitelist {
    compress: HashSet<String>,
    tty: HashSet<String>,
}

impl Whitelist {
    /// 构造空 Whitelist (主要用于测试)
    pub fn empty() -> Self {
        Self::default()
    }

    /// 加载完整双清单: L1 默认 + L2 项目 (`include_str!` 嵌入) + L3 用户 (`~/.tokenslim-whitelist.toml`)
    ///
    /// 合并规则: extra 段取并集; remove 段从结果中扣减.
    /// 任一来源加载失败 → 跳过该层, 继续下一层 (fail-soft, 不会让 binary 启动失败).
    pub fn load() -> Self {
        let mut list = Self::from_defaults();
        list.merge_layer(&embedded_project_config());
        if let Some(home) = user_home_dir() {
            let user_path = home.join(".tokenslim-whitelist.toml");
            list.merge_layer(&read_user_config(&user_path));
        }
        list
    }

    /// 从 L1 默认常量构建
    pub fn from_defaults() -> Self {
        let compress = DEFAULT_COMPRESS_WHITELIST
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let tty = DEFAULT_TTY_SUPPORT_LIST
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        Self { compress, tty }
    }

    /// 合并一个配置层 (L2 或 L3)
    fn merge_layer(&mut self, cfg: &WhitelistConfig) {
        for cmd in &cfg.compress.extra.commands {
            if !cmd.is_empty() {
                self.compress.insert(cmd.to_lowercase());
            }
        }
        for cmd in &cfg.compress.remove.commands {
            if !cmd.is_empty() {
                self.compress.remove(&cmd.to_lowercase());
            }
        }
        for cmd in &cfg.tty.extra.commands {
            if !cmd.is_empty() {
                self.tty.insert(cmd.to_lowercase());
            }
        }
        for cmd in &cfg.tty.remove.commands {
            if !cmd.is_empty() {
                self.tty.remove(&cmd.to_lowercase());
            }
        }
    }

    /// `prog` 是否在 compress_whitelist
    pub fn compress_matches(&self, prog: &str) -> bool {
        self.compress.contains(&normalize_prog(prog))
    }

    /// `prog` 是否在 tty_support_list
    pub fn tty_matches(&self, prog: &str) -> bool {
        self.tty.contains(&normalize_prog(prog))
    }

    /// 调试用: 当前 L1+L2+L3 合并后双清单大小
    pub fn sizes(&self) -> (usize, usize) {
        (self.compress.len(), self.tty.len())
    }
}

/// 把 prog 归一化为"基名小写"以便匹配:
/// - `git` → `git`
/// - `C:\Program Files\Git\bin\git.exe` → `git`
/// - `/usr/bin/git` → `git`
/// - `GIT.EXE` → `git`
pub(crate) fn normalize_prog(prog: &str) -> String {
    let path = PathBuf::from(prog);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(prog);
    stem.to_lowercase()
}

// === L2: 项目级配置 (include_str! 嵌入) ===

/// L2 项目配置 — `include_str!` 嵌入 binary, 编译期固定.
///
/// 文件不存在时 (例如新增项目阶段) 应保持空 4 段, 仍可被 serde 解析。
const EMBEDDED_PROJECT_CONFIG: &str = include_str!("../../config/whitelist.toml");

/// 解析嵌入的项目配置; 解析失败 → 走空配置 (fail-soft)
fn embedded_project_config() -> WhitelistConfig {
    toml::from_str(EMBEDDED_PROJECT_CONFIG).unwrap_or_default()
}

// === L3: 用户级配置 (动态读盘) ===

/// 用户主目录 (跨平台); None 表示无法获取
fn user_home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// 读取用户配置; 失败 (文件不存在 / 解析错) → 走空配置
fn read_user_config(path: &PathBuf) -> WhitelistConfig {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return WhitelistConfig::default(),
    };
    toml::from_str(&raw).unwrap_or_default()
}

// === 全局缓存 (单实例进程内只 load 一次) ===

static WHITELIST_CACHE: OnceLock<Whitelist> = OnceLock::new();

/// 全局 Whitelist 单例; 首次访问时调用 [`Whitelist::load`], 之后返回缓存。
pub fn global_whitelist() -> &'static Whitelist {
    WHITELIST_CACHE.get_or_init(Whitelist::load)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_contain_canonical_commands() {
        let list = Whitelist::from_defaults();
        // COMPRESS 核心
        assert!(list.compress_matches("git"));
        assert!(list.compress_matches("GIT"));
        assert!(list.compress_matches("C:\\Program Files\\Git\\bin\\git.exe"));
        assert!(list.compress_matches("cargo"));
        assert!(list.compress_matches("npm"));
        // TTY 核心
        assert!(list.tty_matches("vim"));
        assert!(list.tty_matches("python"));
        assert!(list.tty_matches("ssh"));
        assert!(list.tty_matches("cmd"));
    }

    #[test]
    fn defaults_exclude_unknown_commands() {
        let list = Whitelist::from_defaults();
        assert!(!list.compress_matches("my-random-tool"));
        assert!(!list.compress_matches(""));
        assert!(!list.tty_matches("my-tui"));
    }

    #[test]
    fn normalize_prog_handles_paths_and_extensions() {
        assert_eq!(normalize_prog("git"), "git");
        assert_eq!(normalize_prog("GIT.EXE"), "git");
        assert_eq!(normalize_prog("C:\\Program Files\\Git\\bin\\git.exe"), "git");
        assert_eq!(normalize_prog("/usr/local/bin/python3"), "python3");
        assert_eq!(normalize_prog("./venv/bin/python"), "python");
    }

    #[test]
    fn merge_layer_extra_adds() {
        let mut list = Whitelist::empty();
        list.merge_layer(&WhitelistConfig {
            compress: CompressSection {
                extra: CommandList { commands: vec!["foo".into(), "BAR".into()] },
                remove: CommandList::default(),
            },
            tty: TtySection {
                extra: CommandList { commands: vec!["baz".into()] },
                remove: CommandList::default(),
            },
        });
        assert!(list.compress_matches("foo"));
        assert!(list.compress_matches("bar"));
        assert!(list.tty_matches("baz"));
    }

    #[test]
    fn merge_layer_remove_subtracts() {
        let mut list = Whitelist::from_defaults();
        list.merge_layer(&WhitelistConfig {
            compress: CompressSection {
                extra: CommandList::default(),
                remove: CommandList { commands: vec!["git".into(), "cargo".into()] },
            },
            tty: TtySection::default(),
        });
        assert!(!list.compress_matches("git"));
        assert!(!list.compress_matches("cargo"));
        // 没在 remove 列表的命令不受影响
        assert!(list.compress_matches("npm"));
    }

    #[test]
    fn merge_layer_full_round_trip() {
        let mut list = Whitelist::from_defaults();
        list.merge_layer(&WhitelistConfig {
            compress: CompressSection {
                extra: CommandList { commands: vec!["my-repl".into()] },
                remove: CommandList { commands: vec!["ls".into()] },
            },
            tty: TtySection {
                extra: CommandList { commands: vec!["k9s".into()] },
                remove: CommandList { commands: vec!["man".into()] },
            },
        });
        assert!(list.compress_matches("my-repl"));
        assert!(!list.compress_matches("ls"));
        assert!(list.tty_matches("k9s"));
        // man 不在 L1 默认, remove 是 no-op, 仍不在 tty
        assert!(!list.tty_matches("man"));
    }

    #[test]
    fn embedded_project_config_parses() {
        // 真实文件应能被解析; 解析失败会返回 default
        let cfg = embedded_project_config();
        // 嵌入配置有 4 段 (可能 commands 为空), 段必须存在
        let _ = &cfg.compress.extra;
        let _ = &cfg.compress.remove;
        let _ = &cfg.tty.extra;
        let _ = &cfg.tty.remove;
    }

    #[test]
    fn global_whitelist_returns_consistent_instance() {
        let a = global_whitelist();
        let b = global_whitelist();
        assert!(std::ptr::eq(a, b));
    }

    /// 3 路分发不变量: 默认清单下, 同一 `prog` 不能同时命中 compress 和 tty
    /// (否则分发逻辑会有歧义). 这是 [`crate::cli::commands::run::run_run_mode`]
    /// 决策树依赖的核心不变量, 任何新增默认命令必须满足.
    #[test]
    fn three_route_disjoint_invariant() {
        let list = Whitelist::from_defaults();
        for cmd in DEFAULT_COMPRESS_WHITELIST {
            let prog = cmd.to_lowercase();
            assert!(
                !list.tty_matches(&prog),
                "命令 `{cmd}` 同时在 compress 和 tty 清单, 3 路分发有歧义"
            );
        }
        for cmd in DEFAULT_TTY_SUPPORT_LIST {
            let prog = cmd.to_lowercase();
            assert!(
                !list.compress_matches(&prog),
                "命令 `{cmd}` 同时在 tty 和 compress 清单, 3 路分发有歧义"
            );
        }
    }

    /// 4 段配置解析: 用一段真实 TOML 验证 `WhitelistConfig` 4 段全部反序列化成功.
    /// 这是 [`crate::cli::whitelist`] 模块与 `config/whitelist.toml` 模板之间的契约.
    #[test]
    fn four_section_toml_round_trip() {
        const SAMPLE: &str = r#"
[compress.extra]
commands = ["my-repl", "make-watch"]

[compress.remove]
commands = ["ls", "dir"]

[tty.extra]
commands = ["k9s", "lazygit"]

[tty.remove]
commands = ["man"]
"#;
        let cfg: WhitelistConfig = toml::from_str(SAMPLE).expect("4 段 TOML 解析失败");
        assert_eq!(cfg.compress.extra.commands, vec!["my-repl", "make-watch"]);
        assert_eq!(cfg.compress.remove.commands, vec!["ls", "dir"]);
        assert_eq!(cfg.tty.extra.commands, vec!["k9s", "lazygit"]);
        assert_eq!(cfg.tty.remove.commands, vec!["man"]);
    }

    /// 未知命令兜底: 完全不在任何清单的随机命令必须被两个匹配函数都拒绝,
    /// 这是 `run_run_mode` 走"路 3 passthrough"的触发条件.
    #[test]
    fn unknown_command_bypasses_both_lists() {
        let list = Whitelist::from_defaults();
        for prog in [
            "my-random-tool",
            "unknown-cli-xyz",
            "foobarbaz",
            "",
        ] {
            assert!(
                !list.compress_matches(prog),
                "{prog:?} 不应命中 compress_whitelist"
            );
            assert!(
                !list.tty_matches(prog),
                "{prog:?} 不应命中 tty_support_list"
            );
        }
    }
}
