//! Azure DevOps CLI 输出脱水：保留命令锚点、关键K-V和项目信息，折叠空行、括号和噪音行，压缩URL为短形式。

//! ## 保留信号
//! - az repos show 等命令锚点行（保留命令本身）
//! - 警报行（通过 map_az_alert 保留）
//! - K-V 行（key:value 映射为 BR/URL/SS/REPO/PRJ/ID 等符号）
//! - 列表输出中的 name, id, defaultBranch, project 字段
//! - 创建结果中的 "Repository created:" 行（标记为 A:）
//! - 删除结果中的 "Repository deleted:" 行（标记为 D:）

//! ## 压缩目标
//! - 空行（跳过）
//! - JSON 数组括号 [ ] 和对象花括号 { }（过滤）
//! - 重复的 "az ..." 锚点行（仅保留第一个）
//! - 噪音行（通过 is_az_noise 过滤）
//! - 长 URL（remoteUrl, webUrl 缩写为短形式）
//! - 无冒号行视为项目名，标记为 PRJ:（show 函数中）
pub mod methods;
pub mod parser;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
