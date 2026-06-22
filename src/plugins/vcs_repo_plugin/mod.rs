//! Android Repo 命令输出脱水：保留命令锚点和项目/状态/推送信息，折叠进度条、URL 等噪音，压缩 diff 文件路径和 hunk 头。

//! ## 保留信号
//! - repo sync/status/upload 命令锚点（第一行）
//! - project 行：项目路径与状态/哈希
//! - 推送映射行：HEAD -> refs/...
//! - diff 文件路径（压缩为 D:前缀）
//! - hunk 头（压缩后格式）

//! ## 压缩目标
//! - 进度噪音行：Downloading..., Syncing: ..., Syncing done.
//! - SSH/HTTPS URL 行
//! - 重复的命令锚点（仅保留第一行）
//! - URL 行：ssh://, http://, https://
//! - diff 行中 a/ b/ 路径前缀（压缩为 D:）
//! - hunk 头中空格和上下文（压缩为 @@-a,b->c,d@@）
pub mod methods;
pub mod parser;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
