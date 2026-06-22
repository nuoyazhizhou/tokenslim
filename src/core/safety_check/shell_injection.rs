use super::{SafetyCheck, SafetyWarning};

pub struct ShellInjectionCheck;
const W_SAFETY_SHELL_META: &str = "W_SAFETY_SHELL_META";

impl SafetyCheck for ShellInjectionCheck {
    fn name(&self) -> &'static str {
        "shell_injection"
    }

    fn check_config(&self, config_text: &str) -> Vec<SafetyWarning> {
        find_shell_meta(self.name(), config_text)
    }

    fn check_output(&self, raw: &str, filtered: &str) -> Vec<SafetyWarning> {
        let mut out = find_shell_meta(self.name(), raw);
        out.extend(find_shell_meta(self.name(), filtered));
        out
    }
}

fn find_shell_meta(check: &'static str, text: &str) -> Vec<SafetyWarning> {
    let patterns = ["&&", "||", ";", "|", ">", "<", "`", "$("];
    let mut out = Vec::new();
    for pat in patterns {
        if text.contains(pat) {
            out.push(SafetyWarning {
                check,
                message: format!("{W_SAFETY_SHELL_META}:{pat}"),
            });
        }
    }
    out
}
