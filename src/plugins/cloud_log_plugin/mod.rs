//! 云日志脱水：保留命令提示和记录结构，折叠元数据字段，压缩长值和时间。

//! ## 保留信号
//! - command_lines（命令提示行，如aws logs tail）
//! - records中的消息文本
//! - passthrough行（未被识别的行）
//! - CSV输出或访问摘要（如成功渲染）

//! ## 压缩目标
//! - 长结构化字段值截断为<TRUNCATED>（超过360字符）
//! - 示例字段（samples）压缩为摘要格式[pos:shop:price eta= rating=]
//! - 时间字段精简为'日期 时间'（去掉毫秒和时区）
//! - 资源路径缩短为前两部分+后8字符（如a/b/c...）
//! - 多余空白字符合并为单个空格
pub mod methods;
pub mod types;

pub use types::CloudLogPlugin;

#[cfg(test)]
mod showcase;

#[cfg(test)]
mod test;
