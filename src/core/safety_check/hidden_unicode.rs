use super::{SafetyCheck, SafetyWarning};

pub struct HiddenUnicodeCheck;
const W_SAFETY_HIDDEN_UNICODE: &str = "W_SAFETY_HIDDEN_UNICODE";

impl SafetyCheck for HiddenUnicodeCheck {
    fn name(&self) -> &'static str {
        "hidden_unicode"
    }

    fn check_config(&self, config_text: &str) -> Vec<SafetyWarning> {
        scan_hidden_chars(self.name(), config_text)
    }

    fn check_output(&self, raw: &str, filtered: &str) -> Vec<SafetyWarning> {
        let mut out = scan_hidden_chars(self.name(), raw);
        out.extend(scan_hidden_chars(self.name(), filtered));
        out
    }
}

fn scan_hidden_chars(check: &'static str, text: &str) -> Vec<SafetyWarning> {
    let suspicious = [
        ('\u{200B}', "ZERO WIDTH SPACE"),
        ('\u{200C}', "ZERO WIDTH NON-JOINER"),
        ('\u{200D}', "ZERO WIDTH JOINER"),
        ('\u{FEFF}', "ZERO WIDTH NO-BREAK SPACE"),
        ('\u{202E}', "RIGHT-TO-LEFT OVERRIDE"),
    ];
    let mut out = Vec::new();
    for (ch, label) in suspicious {
        if text.contains(ch) {
            out.push(SafetyWarning {
                check,
                message: format!("{W_SAFETY_HIDDEN_UNICODE}:{label}"),
            });
        }
    }
    out
}
