//! Terraform脱水：保留资源变更行与错误信号，折叠路径为$TF令牌，压缩计划摘要与已知后计算行。

//! ## 保留信号
//! - 资源变更行：'# 路径 will be 动作' 格式
//! - 错误信号行：包含error等关键词的行
//! - 计划摘要行：'Plan: X to add, Y to change, Z to destroy'
//! - 已知后计算行：'(known after apply)' 行统计

//! ## 压缩目标
//! - 长资源路径替换为$TF令牌字典
//! - 已知后计算行折叠为计数
//! - 计划摘要格式化为简洁摘要
//! - ANSI转义序列被清除
mod methods;
mod types;

pub use types::TerraformPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
