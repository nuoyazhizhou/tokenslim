use super::{SafetyCheck, SafetyWarning};

pub struct PromptInjectionCheck;
const W_SAFETY_PROMPT_INJECTION: &str = "W_SAFETY_PROMPT_INJECTION";

impl SafetyCheck for PromptInjectionCheck {
    /// 返回提示注入检查器的唯一名称 "prompt_injection"。
    fn name(&self) -> &'static str {
        "prompt_injection"
    }

    /// 对配置文本执行提示注入短语扫描，返回发现的警告列表。
    fn check_config(&self, config_text: &str) -> Vec<SafetyWarning> {
        find_injection_patterns(self.name(), config_text)
    }

    /// 对原始输出与过滤后输出分别执行提示注入扫描，合并返回警告。
    fn check_output(&self, raw: &str, filtered: &str) -> Vec<SafetyWarning> {
        let mut out = find_injection_patterns(self.name(), raw);
        out.extend(find_injection_patterns(self.name(), filtered));
        out
    }
}

/// 小写化文本后逐一匹配提示注入特征短语（如 "ignore previous instructions"），
/// 每个命中短语产出一条警告。
fn find_injection_patterns(check: &'static str, text: &str) -> Vec<SafetyWarning> {
    let patterns = [
        "ignore previous instructions",
        "ignore all previous instructions",
        "disregard previous",
        "you are now",
        "system prompt",
        "developer instructions",
    ];
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for pat in patterns {
        if lower.contains(pat) {
            out.push(SafetyWarning {
                check,
                message: format!("{W_SAFETY_PROMPT_INJECTION}:{pat}"),
            });
        }
    }
    out
}
