// filter_discover/classifier.rs
// 命令分类器 - 将命令分类为 AlreadyFiltered/Filterable/NoFilter

use super::filter_name::{derive_tracking_filter_name, is_vcs_routed, load_route_caps};
use super::types::{ClassifiedCommand, CommandClass, SessionCommand};
use crate::core::plugin_config_loader::RunRouteCapability;
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
    // P2-44：run 路由配置在批量分类前加载一次并逐命令复用，避免逐命令重复读盘（N+1 同族）。
    let caps = load_route_caps();
    let mut classified = Vec::new();

    for cmd in commands {
        let class = classify_single_command(&cmd.command, &caps);
        classified.push(ClassifiedCommand {
            command: cmd.clone(),
            class,
        });
    }

    Ok(classified)
}

/// 分类单个命令
#[tracing::instrument(level = "trace", skip_all)]
fn classify_single_command(command: &str, caps: &[RunRouteCapability]) -> CommandClass {
    let cmd = command.trim();

    // 1. 检查是否已被 tokenslim 包装
    if is_already_filtered(cmd) {
        return CommandClass::AlreadyFiltered;
    }

    // 2. 检查是否存在匹配的过滤器
    if let Some(filter_name) = find_matching_filter(cmd, caps) {
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
/// P2-44 修复：族判定（本命令是否属于值得过滤的已知族）与**命名**（归到哪个
/// filter 名）分离——族判定保留原有硬编码族表（discover 的启发式语义），
/// 命名则改走 [`super::filter_name::derive_tracking_filter_name`] 单一权威，
/// 与 tracking 写侧 `resolve_run_filter_name` 同规则。此前本函数硬编码产出
/// 13 个家族名（`vcs_git`/`rust`/`nodejs`/…），与写侧实际记录名零交集，
/// 历史 savings_pct 查询永远 miss。
///
/// `caps` 为 run 路由配置（调用方批量加载一次后传入，见 [`classify_commands`]）。
fn find_matching_filter(command: &str, caps: &[RunRouteCapability]) -> Option<String> {
    let cmd = skip_env_prefix(command);
    let mut parts = cmd.split_whitespace();
    let prog = parts.next()?;
    let args: Vec<String> = parts.map(|s| s.to_string()).collect();

    // 值得过滤的族判定（维持 discover 启发式语义：VCS/构建/测试/容器编排等已知族）
    if !is_discoverable_prog(prog, cmd) {
        return None;
    }

    // 命名权威：与 tracking 写侧同规则，保证 discover 组名可直接命中
    // tracker.get_by_filter() 的历史记录
    let vcs_routed = is_vcs_routed(caps, prog, &args);
    Some(derive_tracking_filter_name(prog, &args, vcs_routed))
}

/// 判定程序名是否属于 discover 关心的可过滤族（仅做族判定，不产出名称）
fn is_discoverable_prog(prog: &str, command: &str) -> bool {
    // VCS 工具
    if matches!(
        prog,
        "git" | "svn" | "hg" | "p4" | "cvs" | "bzr" | "fossil" | "darcs"
    ) {
        return true;
    }

    // GitHub/GitLab/Bitbucket/Repo/Gerrit CLI
    if matches!(prog, "gh" | "glab" | "bb" | "repo" | "gerrit") {
        return true;
    }

    // Azure DevOps CLI（限 repos 上下文）
    if prog == "az" && command.contains("repos") {
        return true;
    }

    // Rust 工具
    if matches!(prog, "cargo" | "rustc" | "rustup" | "rustfmt" | "clippy") {
        return true;
    }

    // Node.js 工具
    if matches!(prog, "npm" | "yarn" | "pnpm" | "node") {
        return true;
    }

    // Python 工具
    if matches!(
        prog,
        "python" | "python3" | "pip" | "pip3" | "pytest" | "poetry" | "uv"
    ) {
        return true;
    }

    // Go 工具
    if prog == "go" {
        return true;
    }

    // Java 工具
    if matches!(prog, "java" | "javac" | "mvn" | "gradle") {
        return true;
    }

    // C/C++ 工具
    if matches!(prog, "gcc" | "g++" | "clang" | "clang++" | "make" | "cmake") {
        return true;
    }

    // Docker / Kubernetes / Terraform
    if matches!(prog, "docker" | "kubectl" | "terraform") {
        return true;
    }

    // 测试框架
    if matches!(prog, "jest" | "vitest" | "mocha" | "ava") {
        return true;
    }

    false
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

    /// 验证 `is_already_filtered` 的识别：以 tokenslim 开头、含 "tokenslim run"/"SHELL=tokenslim"/"--shell tokenslim" 的均判为已过滤；普通 git/cargo 命令判为未过滤。
    #[test]
    /// 契约：四种 tokenslim 包装形态（`tokenslim run`/`tokenslim --preset`/`SHELL=tokenslim`/`--shell tokenslim`）均应识别为已过滤；普通命令不应误判。
    fn test_is_already_filtered() {
        assert!(is_already_filtered("tokenslim run git status"));
        assert!(is_already_filtered("tokenslim --preset ai -- git log"));
        assert!(is_already_filtered("make SHELL=tokenslim test"));
        assert!(is_already_filtered("just --shell tokenslim build"));
        assert!(!is_already_filtered("git status"));
        assert!(!is_already_filtered("cargo test"));
    }

    /// 验证 `find_matching_filter` 的命名契约（P2-44 单一命名权威）：
    /// VCS 命令经 run 路由判定归 `vcs_plugin`（与 tracking 写侧一致）；
    /// 非 VCS 命令取首个子命令参数（cargo test→test、npm test→test）；
    /// 无参数时兜底程序名（pytest→pytest）；族外命令返回 None。
    #[test]
    /// 契约：命名与 `derive_tracking_filter_name` 同规则，保证 discover 组名可命中 tracker 历史记录。
    fn test_find_matching_filter() {
        let caps = load_route_caps();
        assert_eq!(
            find_matching_filter("git status", &caps),
            Some("vcs_plugin".to_string())
        );
        assert_eq!(
            find_matching_filter("svn commit", &caps),
            Some("vcs_plugin".to_string())
        );
        assert_eq!(
            find_matching_filter("cargo test", &caps),
            Some("test".to_string())
        );
        assert_eq!(
            find_matching_filter("npm test", &caps),
            Some("test".to_string())
        );
        assert_eq!(
            find_matching_filter("pytest", &caps),
            Some("pytest".to_string())
        );
        assert_eq!(
            find_matching_filter("docker ps", &caps),
            Some("ps".to_string())
        );
        assert_eq!(
            find_matching_filter("kubectl get pods", &caps),
            Some("get".to_string())
        );
        assert_eq!(find_matching_filter("unknown-command", &caps), None);
    }

    /// 验证 `extract_program_name` 的程序名提取：普通命令取首词，带 "KEY=value" 环境变量前缀时跳过前缀取真实程序名，空串返回 None。
    #[test]
    /// 契约：应提取命令首词作为程序名；`KEY=value` 环境变量前缀应被跳过；空命令返回 `None`。
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

    /// 验证 `skip_env_prefix` 的前缀剥离：移除行首连续的 "KEY=value " 前缀，无前缀时原样返回。
    #[test]
    /// 契约：无前缀时原样返回；连续 `KEY=value` 前缀应全部剥离。
    fn test_skip_env_prefix() {
        assert_eq!(skip_env_prefix("git status"), "git status");
        assert_eq!(skip_env_prefix("RUST_LOG=debug cargo test"), "cargo test");
        assert_eq!(skip_env_prefix("A=1 B=2 C=3 npm test"), "npm test");
    }

    /// 验证 `classify_single_command` 的三态分类：tokenslim 包装→AlreadyFiltered、已知程序→Filterable(vcs_plugin)、未知→NoFilter。
    #[test]
    /// 契约：分类优先级为 AlreadyFiltered > Filterable > NoFilter；`tokenslim run` 包装命令归 AlreadyFiltered、`git status` 归 Filterable(vcs_plugin)、未知命令归 NoFilter。
    fn test_classify_single_command() {
        let caps = load_route_caps();
        // 已过滤
        assert_eq!(
            classify_single_command("tokenslim run git status", &caps),
            CommandClass::AlreadyFiltered
        );

        // 可过滤
        match classify_single_command("git status", &caps) {
            CommandClass::Filterable { filter_name } => {
                assert_eq!(filter_name, "vcs_plugin");
            }
            _ => panic!("Expected Filterable"),
        }

        // 无过滤器
        assert_eq!(
            classify_single_command("unknown-command", &caps),
            CommandClass::NoFilter
        );
    }

    /// 验证 `classify_commands` 批量分类：对 3 条命令分别正确产出 Filterable/AlreadyFiltered/NoFilter 分类结果。
    #[test]
    /// 契约：批量分类应逐条映射且保持顺序——`git status`→Filterable(vcs_plugin)、`tokenslim run`→AlreadyFiltered、未知→NoFilter。
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
                assert_eq!(filter_name, "vcs_plugin");
            }
            _ => panic!("Expected Filterable"),
        }

        assert_eq!(classified[1].class, CommandClass::AlreadyFiltered);
        assert_eq!(classified[2].class, CommandClass::NoFilter);
    }
}
