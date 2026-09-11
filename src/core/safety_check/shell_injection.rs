use super::{SafetyCheck, SafetyWarning};

pub struct ShellInjectionCheck;
const W_SAFETY_SHELL_META: &str = "W_SAFETY_SHELL_META";

impl SafetyCheck for ShellInjectionCheck {
    /// 返回 Shell 注入检查器的唯一名称 "shell_injection"。
    fn name(&self) -> &'static str {
        "shell_injection"
    }

    /// 对配置文本执行 Shell 元字符扫描，返回发现的警告列表。
    /// P2-31：`#`/`;` 开头的注释行（TOML 注释）不进入 shell 执行路径，豁免。
    fn check_config(&self, config_text: &str) -> Vec<SafetyWarning> {
        let mut out = Vec::new();
        for line in config_text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            out.extend(find_shell_meta(self.name(), line));
        }
        out
    }

    /// P2-31：对压缩产物/日志文本**不再**做 Shell 元字符扫描。
    ///
    /// 语义对齐说明：Shell 注入检查的合理对象是「将被 shell 执行的配置值」
    /// （check_config 用于 TOML 规则文件）。压缩产物本应原样保留 `>`/`<`/`|`/`;`
    /// 等字符——diff 内容行、泛型 `Foo<Bar>`、markdown 引用、模板字符串天然含
    /// 这些字符（实测 1122 个 log 样本中 `>` 35%、`<` 18%、`;` 13% 命中），
    /// 无上下文 contains 扫描使 `verify --safety` 对约 1/3 样本必然硬阻断，
    /// 安全门禁退化为「不可用于日志类插件的摆设开关」，反而诱导用户整体绕过。
    /// 产物**永远不会被本工具送入 shell 执行**，故返回空。
    fn check_output(&self, _raw: &str, _filtered: &str) -> Vec<SafetyWarning> {
        Vec::new()
    }
}

/// 扫描文本中的 Shell 元字符（&&、||、;、|、>、<、反引号、$()），
/// 每个命中字符产出一条警告。
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

#[cfg(test)]
mod tests {
    use super::*;

    /// P2-31 回归：check_output 必须返回空——压缩产物/日志文本本应保留 shell
    /// 元字符（diff 行、泛型、模板串），扫描它使 verify --safety 必然误报。
    #[test]
    fn check_output_never_flags_compressed_output() {
        let check = ShellInjectionCheck;
        let text =
            "diff --git a/main.rs b/main.rs\n-let x: Result<Vec<Foo<Bar>>, E> = a > b;\n$(`cmd`)\n";
        assert!(
            check.check_output(text, text).is_empty(),
            "产物扫描必须返回空（P2-31 核心缺陷）"
        );
    }

    /// P2-31 回归：check_config 豁免 TOML 注释行（`#`/`;` 开头）。
    #[test]
    fn check_config_exempts_comment_lines() {
        let check = ShellInjectionCheck;
        let config = "# 说明: a > b | c; 只是注释\n; 另一种注释 $()\ncommand = \"echo ok\"";
        assert!(
            check.check_config(config).is_empty(),
            "注释行中的元字符不得告警"
        );
        let active = "command = \"rm -rf /tmp && echo done\"";
        assert!(
            !check.check_config(active).is_empty(),
            "非注释行的真实元字符仍须告警"
        );
    }
}
