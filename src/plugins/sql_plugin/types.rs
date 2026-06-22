/// SQL 插件类型定义
use serde::{Deserialize, Serialize};

/// SQL 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlConfig {
    /// 是否提取 SQL 语法骨架（隐藏具体数值）
    pub extract_skeleton: bool,
    /// INSERT 语句 VALUES 部分的最大字符数，超出则截断
    pub max_insert_values_len: usize,
    /// 是否混淆敏感词汇（如密码、秘钥等字段的值）
    pub obfuscate_sensitive: bool,
    /// 触发插件分析的最小 SQL 长度
    pub min_sql_length: usize,
}

impl Default for SqlConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        SqlConfig {
            extract_skeleton: true,
            max_insert_values_len: 200,
            obfuscate_sensitive: true,
            min_sql_length: 20,
        }
    }
}

/// SQL 插件结构
pub struct SqlPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: SqlConfig,
}
