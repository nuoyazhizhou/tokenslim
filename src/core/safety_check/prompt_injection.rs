use super::{SafetyCheck, SafetyWarning};

pub struct PromptInjectionCheck;
const W_SAFETY_PROMPT_INJECTION: &str = "W_SAFETY_PROMPT_INJECTION";

impl SafetyCheck for PromptInjectionCheck {
    fn name(&self) -> &'static str {
        "prompt_injection"
    }

    fn check_config(&self, config_text: &str) -> Vec<SafetyWarning> {
        find_injection_patterns(self.name(), config_text)
    }

    fn check_output(&self, raw: &str, filtered: &str) -> Vec<SafetyWarning> {
        let mut out = find_injection_patterns(self.name(), raw);
        out.extend(find_injection_patterns(self.name(), filtered));
        out
    }
}

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
