use super::{SafetyCheck, SafetyWarning};

pub struct HiddenUnicodeCheck;
const W_SAFETY_HIDDEN_UNICODE: &str = "W_SAFETY_HIDDEN_UNICODE";

impl SafetyCheck for HiddenUnicodeCheck {
    /// 返回隐藏 Unicode 检查器的唯一名称 "hidden_unicode"。
    fn name(&self) -> &'static str {
        "hidden_unicode"
    }

    /// 对配置文本执行隐藏 Unicode 字符扫描，返回发现的警告列表。
    fn check_config(&self, config_text: &str) -> Vec<SafetyWarning> {
        scan_hidden_chars(self.name(), config_text)
    }

    /// 对原始输出与过滤后输出分别执行隐藏 Unicode 扫描，合并返回警告。
    fn check_output(&self, raw: &str, filtered: &str) -> Vec<SafetyWarning> {
        let mut out = scan_hidden_chars(self.name(), raw);
        out.extend(scan_hidden_chars(self.name(), filtered));
        out
    }
}

/// 扫描文本中的零宽空格、零宽连接符、RTL 覆盖符等可疑 Unicode 字符，
/// 每发现一种字符产出一条警告。
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
