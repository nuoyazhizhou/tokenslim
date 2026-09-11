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
    ///
    /// P2-79 演进：R50 先按「诚实文档」路线将旧默认 `true` 修正为 `false`
    /// （当时 compress 全链无脱敏实现，true 构成假承诺）；现已实现真实脱敏——
    /// 开启后 compress 复用 privacy 插件内置凭证正则组，将 password/secret/
    /// api_key/token 等赋值与 Bearer/JWT/连接串凭证替换为 `[TS_*]` 不可逆占位符。
    /// 默认仍保持 `false`：脱敏属行为变更（产物内容改变），由调用方显式开启，
    /// 不做静默默认变更。
    pub obfuscate_sensitive: bool,
    /// 触发插件分析的最小 SQL 长度
    pub min_sql_length: usize,
}

impl Default for SqlConfig {
    /// 提供该插件类型的默认配置实现。
    /// Builds the conservative SQL compression defaults used by SqlPlugin.
    /// The defaults enable skeleton extraction, cap INSERT values, and ignore very short SQL-like text.
    /// P2-79: `obfuscate_sensitive` defaults to `false` — redaction is a behavior change
    /// (it alters compression output), so callers must opt in explicitly.
    fn default() -> Self {
        SqlConfig {
            extract_skeleton: true,
            max_insert_values_len: 200,
            obfuscate_sensitive: false,
            min_sql_length: 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P2-79 回归：obfuscate_sensitive 默认必须为 false——脱敏是产物内容变更
    /// （行为变更），必须由调用方显式开启，不得静默默认变更（R50 门控决策）。
    #[test]
    fn obfuscate_sensitive_defaults_to_false() {
        assert!(
            !SqlConfig::default().obfuscate_sensitive,
            "脱敏属行为变更，默认值必须保持 false（P2-79 R50 门控）"
        );
    }
}

/// SQL 插件结构
pub struct SqlPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: SqlConfig,
}
