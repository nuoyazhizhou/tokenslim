//! 透明命令列表 — 不参与重写的命令
//!
//! 某些命令（如 ssh, mysql）不应该被重写，因为它们：
//! - 不透明：无法预测其内部行为
//! - 交互式：需要用户输入
//! - 远程执行：在其他机器上运行

/// 透明命令列表
///
/// 这些命令不会被重写引擎处理
const TRANSPARENT_COMMANDS: &[&str] = &[
    "ssh",
    "scp",
    "sftp",
    "rsync",
    "mysql",
    "psql",
    "mongo",
    "redis-cli",
    "sqlite3",
    "docker",
    "kubectl",
    "helm",
    "terraform",
    "ansible",
    "vagrant",
    "vim",
    "vi",
    "nano",
    "emacs",
    "less",
    "more",
    "man",
    "sudo",
    "su",
];

/// 检查命令是否为透明命令
///
/// 只检查命令的第一个词（程序名）
pub fn is_transparent_command(command: &str) -> bool {
    let first_word = command.split_whitespace().next().unwrap_or("");

    TRANSPARENT_COMMANDS.contains(&first_word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_is_transparent() {
        assert!(is_transparent_command("ssh user@host"));
        assert!(is_transparent_command("ssh -p 22 user@host"));
    }

    #[test]
    fn test_mysql_is_transparent() {
        assert!(is_transparent_command("mysql -u root -p"));
    }

    #[test]
    fn test_docker_is_transparent() {
        assert!(is_transparent_command("docker run nginx"));
    }

    #[test]
    fn test_regular_command_not_transparent() {
        assert!(!is_transparent_command("make test"));
        assert!(!is_transparent_command("cargo build"));
        assert!(!is_transparent_command("npm test"));
    }

    #[test]
    fn test_empty_command() {
        assert!(!is_transparent_command(""));
        assert!(!is_transparent_command("   "));
    }
}
