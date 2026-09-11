//! CLI 基础测试模块。
//!
//! 覆盖不依赖外部文件、进程或环境变量的 CLI 参数规范化和解析边界；
//! 端到端命令测试应继续沿命令入口到执行器的调用链补充。
#[cfg(test)]
mod tests {
    use super::super::HookShell;

    /// 验证 shell 名称在命令行边界的规范化、别名映射和拒绝语义。
    #[test]
    fn parses_hook_shell_aliases_and_canonical_names() {
        assert_eq!(
            HookShell::parse(&['B', 'A', 'S', 'H'].into_iter().collect::<String>()),
            Some(HookShell::Bash)
        );
        assert_eq!(
            HookShell::parse(&['p', 'w', 's', 'h'].into_iter().collect::<String>()),
            Some(HookShell::PowerShell)
        );
        assert_eq!(
            HookShell::PowerShell.as_str().chars().collect::<Vec<_>>(),
            vec!['p', 'o', 'w', 'e', 'r', 's', 'h', 'e', 'l', 'l']
        );
        assert_eq!(
            HookShell::parse(&['c', 'm', 'd'].into_iter().collect::<String>()),
            None
        );
    }
}
