use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tokenslim::core::init_command::{run_init, InitOptions, InitResult};

#[cfg(test)]
mod tests {
    use super::*;

    static ISOLATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tokenslim-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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

    fn shell_config_path(home: &Path, shell: &str) -> PathBuf {
        match shell {
            "bash" => home.join(".bashrc"),
            "zsh" => home.join(".zshrc"),
            "fish" => home.join(".config/fish/config.fish"),
            "powershell" => home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1"),
            _ => home.join(".bashrc"),
        }
    }

    #[test]
    fn init_options_defaults_are_stable() {
        let opts = InitOptions::default();

        assert!(opts.install_hooks);
        assert_eq!(opts.hook_shell, None);
        assert!(!opts.dry_run);
        assert!(!opts.force);
    }

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
