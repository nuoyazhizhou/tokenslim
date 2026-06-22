//! Xcode构建日志脱水：保留编译链接命令骨架，折叠/dev/null探针噪音，压缩路径参数。

//! ## 保留信号
//! - CompileC 行（编译命令）
//! - Linking 行（链接命令）
//! - clang 行（编译命令）
//! - Build succeeded/failed 行（构建结果）

//! ## 压缩目标
//! - /dev/null 探针行（clang/libtool，批量折叠为 $XC|PROBE|x）
//! - 编译命令中的路径参数（替换为字典令牌 $XC|C|）
//! - 编译命令中的源文件路径（压缩为 $XC|C| 的一部分）
mod methods;
mod types;
pub use types::XcodeLogPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
