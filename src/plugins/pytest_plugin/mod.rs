//! pytest脱水：保留测试结果与摘要，折叠重复状态，压缩测试路径和 summary 标签。

//! ## 保留信号
//! - collected 行（测试收集信息）
//! - 测试结果行（如 test.py::test_func PASSED）
//! - summary 行（如 1 failed, 10 passed）
//! - 测试 session 开始行（如 test session starts）

//! ## 压缩目标
//! - 完整测试路径替换为 $PY 令牌字典
//! - 重复的测试结果状态合并计数
//! - summary 中为零计数的状态丢弃
//! - 状态标签缩写（如 passed->P, failed->F）
//! - 多余空格压缩
mod methods;
mod types;

pub use types::PytestPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
