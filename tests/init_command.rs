use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tokenslim::core::init_command::{run_init, InitOptions, InitResult};

#[cfg(test)]
mod tests {
    use super::*;

    static ISOLATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    /// 在系统临时目录下创建带唯一后缀（纳秒时间戳）的隔离测试目录。
    ///
    /// 命名格式为 `tokenslim-{name}-{unix_nanos}`，避免并发测试间目录冲突，
    /// 目录创建失败时直接 panic（测试前置条件不允许失败）。
    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tokenslim-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 在隔离环境下执行闭包：临时切换 cwd 与 HOME/USERPROFILE，执行后恢复。
    ///
    /// 通过全局互斥锁串行化所有用例，避免并行测试互相污染进程级环境变量；
    /// home 为 None 时移除 HOME/USERPROFILE（模拟无家目录环境）。
    fn with_isolation<T>(cwd: &Path, home: Option<&Path>, f: impl FnOnce() -> T) -> T {
        let _guard = ISOLATION_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        let original_home = std::env::var_os("HOME");
        let original_userprofile = std::env::var_os("USERPROFILE");

        std::env::set_current_dir(cwd).unwrap();
        match home {
            Some(home) => unsafe {
                std::env::set_var("HOME", home);
                std::env::set_var("USERPROFILE", home);
            },
            None => unsafe {
                std::env::remove_var("HOME");
                std::env::remove_var("USERPROFILE");
            },
        }

        let result = f();

        std::env::set_current_dir(original_cwd).unwrap();
        match original_home {
            Some(val) => unsafe { std::env::set_var("HOME", val) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        match original_userprofile {
            Some(val) => unsafe { std::env::set_var("USERPROFILE", val) },
            None => unsafe { std::env::remove_var("USERPROFILE") },
        }

        result
    }

    /// 返回指定 shell 在 home 目录下的配置文件路径。
    ///
    /// 支持 bash/zsh/fish/powershell 四种 shell；未知 shell 回退到 .bashrc，
    /// 保证 hook 注入测试对任意 shell 值都能定位落点文件。
    fn shell_config_path(home: &Path, shell: &str) -> PathBuf {
        match shell {
            "bash" => home.join(".bashrc"),
            "zsh" => home.join(".zshrc"),
            "fish" => home.join(".config/fish/config.fish"),
            "powershell" => home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1"),
            _ => home.join(".bashrc"),
        }
    }

    /// 验证 InitOptions 默认值稳定：默认安装 hooks、无 shell、非 dry-run、非 force。
    #[test]
    fn init_options_defaults_are_stable() {
        let opts = InitOptions::default();

        assert!(opts.install_hooks);
        assert_eq!(opts.hook_shell, None);
        assert!(!opts.dry_run);
        assert!(!opts.force);
    }

    /// 验证 InitResult 各字段可正常访问且值正确（纯结构体读取，不触发 IO）。
    #[test]
    fn init_result_fields_are_accessible() {
        let result = InitResult {
            config_created: true,
            config_path: ".tokenslim.toml".to_string(),
            project_type: "rust".to_string(),
            framework: Some("tauri".to_string()),
            package_manager: Some("cargo".to_string()),
            hooks_installed: false,
            message: "TokenSlim initialized successfully!".to_string(),
        };

        assert!(result.config_created);
        assert_eq!(result.config_path, ".tokenslim.toml");
        assert_eq!(result.project_type, "rust");
        assert_eq!(result.framework.as_deref(), Some("tauri"));
        assert_eq!(result.package_manager.as_deref(), Some("cargo"));
        assert!(!result.hooks_installed);
        assert!(result.message.contains("initialized"));
    }

    /// 端到端验证 dry-run 下项目类型检测：rust/node/python 三类工程
    /// 分别识别出对应 project_type 与框架/包管理器（node 检出 nextjs+pnpm）。
    #[test]
    fn detect_project_type_identifies_rust_node_and_python() {
        let rust_dir = temp_dir("rust");
        fs::write(rust_dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();

        let node_dir = temp_dir("node");
        fs::write(node_dir.join("package.json"), "{\"name\":\"demo\"}").unwrap();
        fs::write(node_dir.join("pnpm-lock.yaml"), "lock").unwrap();
        fs::write(node_dir.join("next.config.js"), "module.exports = {};").unwrap();

        let python_dir = temp_dir("python");
        fs::write(python_dir.join("requirements.txt"), "fastapi\nuvicorn\n").unwrap();

        let rust_result = with_isolation(&rust_dir, None, || {
            run_init(InitOptions {
                install_hooks: false,
                hook_shell: None,
                dry_run: true,
                force: false,
            })
            .unwrap()
        });

        let node_result = with_isolation(&node_dir, None, || {
            run_init(InitOptions {
                install_hooks: false,
                hook_shell: None,
                dry_run: true,
                force: false,
            })
            .unwrap()
        });

        let python_result = with_isolation(&python_dir, None, || {
            run_init(InitOptions {
                install_hooks: false,
                hook_shell: None,
                dry_run: true,
                force: false,
            })
            .unwrap()
        });

        assert_eq!(rust_result.project_type, "rust");
        assert_eq!(rust_result.framework, None);
        assert_eq!(rust_result.package_manager, None);

        assert_eq!(node_result.project_type, "node");
        assert_eq!(node_result.framework.as_deref(), Some("nextjs"));
        assert_eq!(node_result.package_manager.as_deref(), Some("pnpm"));

        assert_eq!(python_result.project_type, "python");
        assert_eq!(python_result.framework.as_deref(), Some("fastapi"));
        assert_eq!(python_result.package_manager, None);
    }

    /// 验证 dry-run 模式不落盘：不创建 .tokenslim.toml，且消息含 "dry-run" 字样。
    #[test]
    fn dry_run_leaves_config_unwritten() {
        let project_dir = temp_dir("dry-run");
        fs::write(
            project_dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();

        let result = with_isolation(&project_dir, None, || {
            run_init(InitOptions {
                install_hooks: false,
                hook_shell: None,
                dry_run: true,
                force: false,
            })
            .unwrap()
        });

        assert!(!result.config_created);
        assert!(result.message.to_ascii_lowercase().contains("dry-run"));
        assert!(!project_dir.join(".tokenslim.toml").exists());
    }

    /// 验证 run_init 真实写盘：生成可被 toml 解析的 .tokenslim.toml，
    /// 且 general 段写入 node/yarn 等检测结果（package.json + yarn.lock）。
    #[test]
    fn run_init_writes_valid_toml_config() {
        let project_dir = temp_dir("config");
        fs::write(project_dir.join("package.json"), "{\"name\":\"demo\"}").unwrap();
        fs::write(project_dir.join("yarn.lock"), "lock").unwrap();

        let result = with_isolation(&project_dir, None, || {
            run_init(InitOptions {
                install_hooks: false,
                hook_shell: None,
                dry_run: false,
                force: true,
            })
            .unwrap()
        });

        let config_path = project_dir.join(".tokenslim.toml");
        let config = fs::read_to_string(&config_path).unwrap();
        let value: toml::Value = toml::from_str(&config).expect("valid toml");

        assert!(result.config_created);
        assert_eq!(result.project_type, "node");
        assert_eq!(result.framework, None);
        assert_eq!(result.package_manager.as_deref(), Some("yarn"));
        assert_eq!(value["general"]["project_type"].as_str(), Some("node"));
        assert_eq!(value["general"]["framework"].as_str(), Some(""));
        assert_eq!(value["general"]["package_manager"].as_str(), Some("yarn"));
    }

    /// 验证四种 shell（bash/zsh/fish/powershell）的 hook 注入：
    /// 在隔离 home 下运行 init 后，各 shell 配置文件应包含预期的
    /// ts/ts-run/ts-compress/ts-doctor 别名及 go/kubectl/terraform/pytest 包装函数。
    #[test]
    fn run_init_generates_shell_hook_aliases() {
        let shells = [
            (
                "bash",
                vec![
                    "alias ts='tokenslim'",
                    "alias ts-run='tokenslim run'",
                    "alias ts-compress='tokenslim compress'",
                    "alias ts-doctor='tokenslim workspace'",
                    "function go() { tokenslim run go \"$@\"; }",
                    "function kubectl() { tokenslim run kubectl \"$@\"; }",
                    "function terraform() { tokenslim run terraform \"$@\"; }",
                    "function pytest() { tokenslim run pytest \"$@\"; }",
                ],
            ),
            (
                "zsh",
                vec![
                    "alias ts='tokenslim'",
                    "alias ts-run='tokenslim run'",
                    "alias ts-compress='tokenslim compress'",
                    "alias ts-doctor='tokenslim workspace'",
                    "function go() { tokenslim run go \"$@\"; }",
                    "function kubectl() { tokenslim run kubectl \"$@\"; }",
                    "function terraform() { tokenslim run terraform \"$@\"; }",
                    "function pytest() { tokenslim run pytest \"$@\"; }",
                ],
            ),
            (
                "fish",
                vec![
                    "alias ts 'tokenslim'",
                    "alias ts-run 'tokenslim run'",
                    "alias ts-compress 'tokenslim compress'",
                    "alias ts-doctor 'tokenslim workspace'",
                    "function go; tokenslim run go $argv; end",
                    "function kubectl; tokenslim run kubectl $argv; end",
                    "function terraform; tokenslim run terraform $argv; end",
                    "function pytest; tokenslim run pytest $argv; end",
                ],
            ),
            (
                "powershell",
                vec![
                    "function ts { tokenslim @args }",
                    "function ts-run { tokenslim run @args }",
                    "function ts-compress { tokenslim compress @args }",
                    "function ts-doctor { tokenslim workspace @args }",
                    "function go { tokenslim run go @args }",
                    "function kubectl { tokenslim run kubectl @args }",
                    "function terraform { tokenslim run terraform @args }",
                    "function pytest { tokenslim run pytest @args }",
                ],
            ),
        ];

        for (shell, expected_snippets) in shells {
            let project_dir = temp_dir(shell);
            let home_dir = temp_dir(&format!("home-{shell}"));

            with_isolation(&project_dir, Some(&home_dir), || {
                run_init(InitOptions {
                    install_hooks: true,
                    hook_shell: Some(shell.to_string()),
                    dry_run: false,
                    force: true,
                })
                .unwrap();
            });

            let hook_path = shell_config_path(&home_dir, shell);
            let hook_content = fs::read_to_string(&hook_path).unwrap();
            for snippet in expected_snippets {
                assert!(
                    hook_content.contains(snippet),
                    "missing {snippet} for {shell}"
                );
            }
        }
    }
}
