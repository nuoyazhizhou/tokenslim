//! 初始化命令类型定义

/// 初始化命令的配置选项
#[derive(Debug, Clone)]
pub struct InitOptions {
    /// 是否安装 shell hooks
    pub install_hooks: bool,
    /// shell 类型 (None = 自动探测)
    pub hook_shell: Option<String>,
    /// 仅打印计划变更，不写文件
    pub dry_run: bool,
    /// 强制覆盖已存在的配置文件
    pub force: bool,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            install_hooks: true,
            hook_shell: None,
            dry_run: false,
            force: false,
        }
    }
}

/// 初始化结果
#[derive(Debug, Clone)]
pub struct InitResult {
    /// 配置文件是否已创建
    pub config_created: bool,
    /// 配置文件路径
    pub config_path: String,
    /// 检测到的项目类型
    pub project_type: String,
    /// 检测到的框架 (如果有)
    pub framework: Option<String>,
    /// 检测到的包管理器 (如果有)
    pub package_manager: Option<String>,
    /// Shell hooks 是否已安装
    pub hooks_installed: bool,
    /// 状态消息
    pub message: String,
}
