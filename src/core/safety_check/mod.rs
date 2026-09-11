//! 安全检查模块：用于 verify 阶段做静态风险扫描。

mod hidden_unicode;
mod prompt_injection;
mod shell_injection;

pub use hidden_unicode::HiddenUnicodeCheck;
pub use prompt_injection::PromptInjectionCheck;
pub use shell_injection::ShellInjectionCheck;

#[derive(Debug, Clone)]
pub struct SafetyWarning {
    pub check: &'static str,
    pub message: String,
}

pub trait SafetyCheck: Send + Sync {
    /// 返回安全检查器的唯一名称标识。
    fn name(&self) -> &'static str;
    /// 对配置文本执行静态风险扫描，返回零条或多条警告。
    fn check_config(&self, config_text: &str) -> Vec<SafetyWarning>;
    /// 对（raw, filtered）输出文本对执行静态风险扫描，返回警告。
    fn check_output(&self, raw: &str, filtered: &str) -> Vec<SafetyWarning>;
}

static PROMPT_CHECK: PromptInjectionCheck = PromptInjectionCheck;
static SHELL_CHECK: ShellInjectionCheck = ShellInjectionCheck;
static UNICODE_CHECK: HiddenUnicodeCheck = HiddenUnicodeCheck;

pub static ALL_CHECKS: &[&dyn SafetyCheck] = &[&PROMPT_CHECK, &SHELL_CHECK, &UNICODE_CHECK];

/// 依次对配置文本运行全部内置安全检查（提示注入、Shell 元字符、隐藏 Unicode），合并警告。
pub fn run_safety_checks_on_config(config_text: &str) -> Vec<SafetyWarning> {
    let mut out = Vec::new();
    for check in ALL_CHECKS {
        out.extend(check.check_config(config_text));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：配置文本含 "ignore previous instructions" 时，提示注入检查应发出警告。
    #[test]
    fn detects_prompt_injection_phrase() {
        let warnings = run_safety_checks_on_config("ignore previous instructions");
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.check == "prompt_injection"));
    }
}
