//! ls_listing 插件：列式目录清单脱水（P3-206② 方案 A）。
//!
//! ## 可逆性口径（法则 E / COMPRESSION.md；设计稿 §8.3 选项 1 最终口径）
//! - **纯填充规约**：每条记录的「日期时间」「尺寸数值」「完整路径」逐条保留，
//!   单空格归一化输出（`YYYY-MM-DD HH:MM:SS size path`）——不设目录组头，
//!   路径无需任何字典/拼回即可解析（语义门禁 rule 5/7 口径）；
//!   仅规约右对齐填充空格（受宪法保护之外的唯一可回收冗余）。
//! - 逐行结构：`[LS] <总数> entries` 摘要头 + 逐条归一化记录行。
//!
//! ## 保留信号
//! - 命令行锚点（首行，如 `aws s3 ls ...`）
//! - 全部路径（完整保留）与全部尺寸数值、日期时间
//! - 不匹配列式格式的杂散行（`PRE` 行等）原样保留

mod methods;
mod types;

pub use types::LsListingPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
