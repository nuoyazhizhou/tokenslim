// filter_discover/classifier.rs
// 命令分类器 - 将命令分类为 AlreadyFiltered/Filterable/NoFilter

use super::types::{ClassifiedCommand, CommandClass, SessionCommand};
use regex::Regex;

/// 分类命令
///
/// # 参数
/// - `commands`: 待分类的命令列表
///
/// # 返回
/// - `Vec<ClassifiedCommand>`: 分类后的命令列表
#[tracing::instrument(level = "debug", skip_all)]
pub fn classify_commands(commands: &[SessionCommand]) -> Result<Vec<ClassifiedCommand>, String> {
    let mut classified = Vec::new();

    for cmd in commands {
        let class = classify_single_command(&cmd.command);
        classified.push(ClassifiedCommand {
            command: cmd.clone(),
            class,
        });
    }

    Ok(classified)
}

/// 分类单个命令
#[tracing::instrument(level = "trace", skip_all)]
fn classify_single_command(command: &str) -> CommandClass {
    let cmd = command.trim();

    // 1. 检查是否已被 tokenslim 包装
    if is_already_filtered(cmd) {
        return CommandClass::AlreadyFiltered;
    }

    // 2. 检查是否存在匹配的过滤器
    if let Some(filter_name) = find_matching_filter(cmd) {
        return CommandClass::Filterable { filter_name };
    }

    // 3. 无匹配过滤器
    CommandClass::NoFilter
}

/// 检查命令是否已被 tokenslim 包装
fn is_already_filtered(command: &str) -> bool {
    // 检查是否以 tokenslim 开头
    if command.starts_with("tokenslim ") {
        return true;
    }

    // 检查是否包含 tokenslim run
    if command.contains("tokenslim run ") {
        return true;
    }

    // 检查是否包含 SHELL=tokenslim
    if command.contains("SHELL=tokenslim") {
        return true;
    }

    // 检查是否包含 --shell tokenslim
    if command.contains("--shell tokenslim") {
        return true;
    }

    false
}

/// 查找匹配的过滤器
///
/// 返回过滤器名称（如果找到）
fn find_matching_filter(command: &str) -> Option<String> {
    // 提取命令的第一个词（程序名）
    let prog = extract_program_name(command)?;

    // VCS 工具
    if matches!(
        prog,
        "git" | "svn" | "hg" | "p4" | "cvs" | "bzr" | "fossil" | "darcs"
    ) {
        return Some(format!("vcs_{}", prog));
    }

    // GitHub CLI
    if prog == "gh" {
        return Some("vcs_github".to_string());
    }

    // GitLab CLI
    if prog == "glab" {
        return Some("vcs_gitlab".to_string());
    }

    // Azure DevOps CLI
    if prog == "az" && command.contains("repos") {
        return Some("vcs_azure".to_string());
    }

    // Bitbucket CLI
    if prog == "bb" {
        return Some("vcs_bitbucket".to_string());
    }

    // Repo tool
    if prog == "repo" {
        return Some("vcs_repo".to_string());
    }

    // Gerrit
    if prog == "gerrit" {
        return Some("vcs_gerrit".to_string());
    }

    // Rust 工具
    if matches!(prog, "cargo" | "rustc" | "rustup" | "rustfmt" | "clippy") {
        return Some("rust".to_string());
    }

    // Node.js 工具
    if matches!(prog, "npm" | "yarn" | "pnpm" | "node") {
        return Some("nodejs".to_string());
    }

    // Python 工具
    if matches!(
        prog,
        "python" | "python3" | "pip" | "pip3" | "pytest" | "poetry" | "uv"
    ) {
        return Some("python".to_string());
    }

    // Go 工具
    if prog == "go" {
        return Some("golang".to_string());
    }

    // Java 工具
    if matches!(prog, "java" | "javac" | "mvn" | "gradle") {
        return Some("java".to_string());
    }

    // C/C++ 工具
    if matches!(prog, "gcc" | "g++" | "clang" | "clang++" | "make" | "cmake") {
        return Some("cpp".to_string());
    }

    // Docker
    if prog == "docker" {
        return Some("docker".to_string());
    }

    // Kubernetes
    if prog == "kubectl" {
        return Some("kubernetes".to_string());
    }

    // Terraform
    if prog == "terraform" {
        return Some("terraform".to_string());
    }

    // 测试框架
    if matches!(prog, "jest" | "vitest" | "mocha" | "ava") {
        return Some("test_framework".to_string());
    }

    None
}

/// 提取程序名
fn extract_program_name(command: &str) -> Option<&str> {
    // 跳过环境变量前缀（如 RUST_LOG=debug）
    let cmd = skip_env_prefix(command);

    // 提取第一个词
    cmd.split_whitespace().next()
}

/// 跳过环境变量前缀
fn skip_env_prefix(command: &str) -> &str {
    // 简单实现：跳过所有 KEY=value 形式的前缀
    let re = Regex::new(r"^(\w+=\S+\s+)+").unwrap();
    if let Some(m) = re.find(command) {
        &command[m.end()..]
    } else {
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_already_filtered() {
        assert!(is_already_filtered("tokenslim run git status"));
        assert!(is_already_filtered("tokenslim --preset ai -- git log"));
        assert!(is_already_filtered("make SHELL=tokenslim test"));
        assert!(is_already_filtered("just --shell tokenslim build"));
        assert!(!is_already_filtered("git status"));
        assert!(!is_already_filtered("cargo test"));
    }

    #[test]
    fn test_find_matching_filter() {
        assert_eq!(
            find_matching_filter("git status"),
            Some("vcs_git".to_string())
        );
        assert_eq!(
            find_matching_filter("svn commit"),
            Some("vcs_svn".to_string())
        );
        assert_eq!(find_matching_filter("cargo test"), Some("rust".to_string()));
        assert_eq!(find_matching_filter("npm test"), Some("nodejs".to_string()));
        assert_eq!(find_matching_filter("pytest"), Some("python".to_string()));
        assert_eq!(
            find_matching_filter("docker ps"),
            Some("docker".to_string())
        );
        assert_eq!(
            find_matching_filter("kubectl get pods"),
            Some("kubernetes".to_string())
        );
        assert_eq!(find_matching_filter("unknown-command"), None);
    }

    #[test]
    fn test_extract_program_name() {
        assert_eq!(extract_program_name("git status"), Some("git"));
        assert_eq!(extract_program_name("cargo test --all"), Some("cargo"));
        assert_eq!(
            extract_program_name("RUST_LOG=debug cargo test"),
            Some("cargo")
        );
        assert_eq!(
            extract_program_name("KEY=value KEY2=value2 npm test"),
            Some("npm")
        );
        assert_eq!(extract_program_name(""), None);
    }

    #[test]
    fn test_skip_env_prefix() {
        assert_eq!(skip_env_prefix("git status"), "git status");
        assert_eq!(skip_env_prefix("RUST_LOG=debug cargo test"), "cargo test");
        assert_eq!(skip_env_prefix("A=1 B=2 C=3 npm test"), "npm test");
    }

    #[test]
    fn test_classify_single_command() {
        // 已过滤
        assert_eq!(
            classify_single_command("tokenslim run git status"),
            CommandClass::AlreadyFiltered
        );

        // 可过滤
        match classify_single_command("git status") {
            CommandClass::Filterable { filter_name } => {
                assert_eq!(filter_name, "vcs_git");
            }
            _ => panic!("Expected Filterable"),
        }

        // 无过滤器
        assert_eq!(
            classify_single_command("unknown-command"),
            CommandClass::NoFilter
        );
    }

    #[test]
    fn test_classify_commands() {
        let commands = vec![
            SessionCommand {
                command: "git status".to_string(),
                input_bytes: None,
                output_bytes: None,
                input_tokens: None,
                output_tokens: None,
                timestamp: None,
            },
            SessionCommand {
                command: "tokenslim run cargo test".to_string(),
                input_bytes: None,
                output_bytes: None,
                input_tokens: None,
                output_tokens: None,
                timestamp: None,
            },
            SessionCommand {
                command: "unknown-command".to_string(),
                input_bytes: None,
                output_bytes: None,
                input_tokens: None,
                output_tokens: None,
                timestamp: None,
            },
        ];

        let classified = classify_commands(&commands).unwrap();
        assert_eq!(classified.len(), 3);

        match &classified[0].class {
            CommandClass::Filterable { filter_name } => {
                assert_eq!(filter_name, "vcs_git");
            }
            _ => panic!("Expected Filterable"),
        }

        assert_eq!(classified[1].class, CommandClass::AlreadyFiltered);
        assert_eq!(classified[2].class, CommandClass::NoFilter);
    }
}
