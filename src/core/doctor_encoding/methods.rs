use super::types::*;
use crate::core::sys_env::get_environment_info;
use crate::utils::i18n::{t, t1};
use std::process::Command;

const E_DOCTOR_ENCODING_SERIALIZE: &str = "E_DOCTOR_ENCODING_SERIALIZE";

/// 解析地域（locale）信号：依次回退取 LC_ALL → LANG → 传入的默认 locale，过滤空值，返回首个非空地域串，全空则返回 None。
fn detect_locale_signal(default_locale: &str) -> Option<String> {
    std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var("LANG").ok().filter(|v| !v.trim().is_empty()))
        .or_else(|| {
            if default_locale.trim().is_empty() {
                None
            } else {
                Some(default_locale.to_string())
            }
        })
}

/// 探测 PowerShell 运行时：优先 `pwsh --version`，失败则回退 `powershell -Command` 读取版本，返回 RuntimeSignal。
fn detect_powershell_runtime() -> RuntimeSignal {
    let powershell = detect_runtime("pwsh", &["--version"]);
    if powershell.detected {
        powershell
    } else {
        detect_runtime(
            "powershell",
            &["-Command", "$PSVersionTable.PSVersion.ToString()"],
        )
    }
}

/// 编码诊断入口：采集环境编码报告后，按请求格式（Json/Text）序列化为可读诊断输出。
pub fn run_encoding_doctor(format: DoctorReportFormat) -> Result<String, String> {
    let report = collect_encoding_report();
    match format {
        DoctorReportFormat::Json => serde_json::to_string_pretty(&report)
            .map_err(|e| format!("{E_DOCTOR_ENCODING_SERIALIZE}:{e}")),
        DoctorReportFormat::Text => Ok(render_text_report(&report)),
    }
}

/// 采集完整编码诊断报告：汇总 OS、shell、代码页、PowerShell/Python/Node/JDK 运行时信号，并分类风险等级、生成修复建议。
pub fn collect_encoding_report() -> EncodingDoctorReport {
    let env_info = get_environment_info();
    let locale = detect_locale_signal(&env_info.locale);
    let os_signal = OsSignal {
        name: env_info.os,
        version: env_info.os_version,
        locale,
    };

    let shell_signal = detect_shell();
    let codepage_signal = detect_codepage();
    let powershell = detect_powershell_runtime();
    let python = detect_python_runtime();
    let node = detect_runtime("node", &["-e", "console.log(process.version)"]);
    let jdk = detect_jdk_runtime();

    let mut report = EncodingDoctorReport {
        risk: EncodingRiskLevel::Ok, // Will be classified
        os: os_signal,
        shell: shell_signal,
        codepage: codepage_signal,
        powershell,
        python,
        node,
        jdk,
        supported_decoders: supported_decoders(),
        recommended_expansions: recommended_expansions(),
        repair_strategy_profile: repair_strategy_profile(),
        repair_confidence_profile: repair_confidence_profile(),
        recommendations: vec![], // Will be generated
    };

    report.risk = classify_risk(&report);
    report.recommendations = build_recommendations(&report);

    report
}

/// 返回本工具支持的字符解码器清单（utf-8 / utf-16 / gbk / big5 / shift_jis / chardetng 等），用于诊断报告展示。
fn supported_decoders() -> Vec<String> {
    vec![
        "utf-8".to_string(),
        "utf-16le/utf-16be (bom + no-bom heuristic)".to_string(),
        "utf-32le/utf-32be (bom + no-bom heuristic)".to_string(),
        "gbk/gb18030".to_string(),
        "big5(cp950)".to_string(),
        "shift_jis/windows-31j(cp932)/euc-jp".to_string(),
        "euc-kr(cp949 common windows path)".to_string(),
        "windows-1250/1251/1252/1253/1254/1255/1256/1258".to_string(),
        "windows-874".to_string(),
        "ibm866".to_string(),
        "mixed-by-lines (auto segment decode)".to_string(),
        "mixed-by-chunks (no-newline mixed text decode)".to_string(),
        "chardetng(auto-detect fallback)".to_string(),
    ]
}

/// 返回计划后续增强的解码能力清单（如 cp949 消歧、legacy DOS/Mac 代码页、按配置覆盖的领域解码器提示）。
fn recommended_expansions() -> Vec<String> {
    vec![
        "cp949(uhc) disambiguation improvements for korean windows corpora".to_string(),
        "legacy dos codepages(cp437/cp850) if real windows cmd archives appear".to_string(),
        "legacy mac-roman/mac-cyrillic datasets (if real samples appear)".to_string(),
        "domain-specific decoder hints via config override (per plugin/input source)".to_string(),
    ]
}

/// 返回修复置信度分级说明（high/medium/low），描述各级别对应的乱码修复把握。
fn repair_confidence_profile() -> Vec<String> {
    vec![
        "high: mojibake/replacement markers significantly reduced with explicit repair steps"
            .to_string(),
        "medium: text improved by heuristic or normalization but residual uncertainty remains"
            .to_string(),
        "low: no meaningful repair signal detected; keep original for manual review".to_string(),
    ]
}

/// 返回修复策略分级说明（cleanup_only / reencode_recover / manual_review），描述各类策略适用场景。
fn repair_strategy_profile() -> Vec<String> {
    vec![
        "cleanup_only: strip BOM / normalize newline / remove control chars when no risky re-decode is needed"
            .to_string(),
        "reencode_recover: mojibake chain reinterpretation applied (cp1252/gbk/shift-jis etc.) with measurable signal improvement"
            .to_string(),
        "manual_review: low-confidence or binary-like inputs; keep original semantics and require human validation"
            .to_string(),
    ]
}

#[derive(Default)]
struct ShellEnvSignals {
    shell: Option<String>,
    comspec: Option<String>,
    cmd_cmd_line: Option<String>,
    cmd_prompt: Option<String>,
    ps_dist: Option<String>,
    ps_exec_policy: Option<String>,
    ps_module_path: Option<String>,
    cmder_root: Option<String>,
    conemu_pid: Option<String>,
}

/// 取 shell 路径的 basename 并小写：将反斜杠规范为正斜杠后取末段、转 ASCII 小写，用于跨平台 shell 名归一化。
fn shell_basename(v: &str) -> String {
    let p = v.replace('\\', "/");
    p.rsplit('/').next().unwrap_or("").to_ascii_lowercase()
}

/// 由 shell 可执行路径推断 shell 名：匹配 cmd/bash/zsh/fish/csh/ksh/ash/powershell 等常见名（含 .exe），未知返回 None。
fn shell_from_path(path: &str) -> Option<String> {
    match shell_basename(path).as_str() {
        "cmd.exe" | "cmd" => Some("cmd".to_string()),
        "bash.exe" | "bash" => Some("bash".to_string()),
        "zsh.exe" | "zsh" => Some("zsh".to_string()),
        "fish.exe" | "fish" => Some("fish".to_string()),
        "csh.exe" | "csh" | "tcsh.exe" | "tcsh" => Some("csh".to_string()),
        "ksh.exe" | "ksh" | "ksh93" => Some("ksh".to_string()),
        "ash.exe" | "ash" | "dash.exe" | "dash" => Some("ash".to_string()),
        "powershell.exe" | "powershell" | "pwsh.exe" | "pwsh" => Some("powershell".to_string()),
        _ => None,
    }
}

/// 从环境信号识别宿主终端：若设置了 Cmder/ConEmu 相关变量，判定为 cmder，否则返回 None。
fn detect_shell_host(signals: &ShellEnvSignals) -> Option<String> {
    if signals.cmder_root.is_some() || signals.conemu_pid.is_some() {
        return Some("cmder".to_string());
    }
    None
}

/// 收集 shell 相关环境变量信号（SHELL/ComSpec/CMDCMDLINE/PROMPT/PowerShell 系列/Cmder/ConEmu），并过滤空值。
fn load_shell_env_signals() -> ShellEnvSignals {
    ShellEnvSignals {
        shell: std::env::var("SHELL").ok().filter(|v| !v.is_empty()),
        comspec: std::env::var("ComSpec").ok().filter(|v| !v.is_empty()),
        cmd_cmd_line: std::env::var("CMDCMDLINE").ok().filter(|v| !v.is_empty()),
        cmd_prompt: std::env::var("PROMPT").ok().filter(|v| !v.is_empty()),
        ps_dist: std::env::var("POWERSHELL_DISTRIBUTION_CHANNEL")
            .ok()
            .filter(|v| !v.is_empty()),
        ps_exec_policy: std::env::var("PSExecutionPolicyPreference")
            .ok()
            .filter(|v| !v.is_empty()),
        ps_module_path: std::env::var("PSModulePath").ok().filter(|v| !v.is_empty()),
        cmder_root: std::env::var("CMDER_ROOT").ok().filter(|v| !v.is_empty()),
        conemu_pid: std::env::var("ConEmuPID").ok().filter(|v| !v.is_empty()),
    }
}

/// 按优先级由环境信号推断 shell：PowerShell 痕迹（分发渠道/执行策略/模块路径）→ CMD（ComSpec+CMDCMDLINE/PROMPT）→ SHELL 变量 → ComSpec 兜底；无法判定返回 None。
fn detect_shell_from_signals(signals: &ShellEnvSignals) -> Option<ShellSignal> {
    let host = detect_shell_host(signals);

    if signals.ps_dist.is_some() || signals.ps_exec_policy.is_some() {
        let raw = format!(
            "POWERSHELL_DISTRIBUTION_CHANNEL={}; PSExecutionPolicyPreference={}",
            signals.ps_dist.as_deref().unwrap_or(""),
            signals.ps_exec_policy.as_deref().unwrap_or("")
        );
        return Some(ShellSignal {
            name: "powershell".to_string(),
            raw,
            host,
        });
    }

    let comspec_shell = signals
        .comspec
        .as_deref()
        .and_then(shell_from_path)
        .unwrap_or_default();
    if signals.cmd_cmd_line.is_some() || (comspec_shell == "cmd" && signals.cmd_prompt.is_some()) {
        return Some(ShellSignal {
            name: "cmd".to_string(),
            raw: format!(
                "ComSpec={}; CMDCMDLINE={}",
                signals.comspec.as_deref().unwrap_or(""),
                signals.cmd_cmd_line.as_deref().unwrap_or("")
            ),
            host,
        });
    }

    if let Some(shell) = signals.shell.as_deref() {
        let name = shell_from_path(shell).unwrap_or_else(|| "unknown".to_string());
        return Some(ShellSignal {
            name,
            raw: shell.to_string(),
            host,
        });
    }

    if let Some(v) = signals.ps_module_path.as_ref() {
        if v.to_ascii_lowercase().contains("powershell") {
            return Some(ShellSignal {
                name: "powershell".to_string(),
                raw: format!("PSModulePath={v}"),
                host,
            });
        }
    }

    if let Some(v) = signals.comspec.as_deref() {
        if let Some(name) = shell_from_path(v) {
            return Some(ShellSignal {
                name,
                raw: format!("ComSpec={v}"),
                host,
            });
        }
    }

    None
}

/// 采集环境信号后调用 detect_shell_from_signals 得到当前 shell 信号，是对外便捷的封装。
fn detect_shell() -> Option<ShellSignal> {
    let signals = load_shell_env_signals();
    detect_shell_from_signals(&signals)
}

/// 探测当前系统活动代码页及是否 UTF-8：Windows 经 chcp 读取，非 Windows 由 LANG/LC_ALL 推断 locale 是否 UTF-8。
#[cfg(target_os = "windows")]
fn detect_codepage() -> Option<CodepageSignal> {
    // Try to get codepage via chcp
    if let Ok(output) = Command::new("cmd").args(&["/c", "chcp"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let cp_str = stdout
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>();
        if !cp_str.is_empty() {
            let is_utf8 = cp_str == "65001";
            return Some(CodepageSignal {
                value: Some(cp_str),
                is_utf8: Some(is_utf8),
            });
        }
    }
    None
}

/// 探测当前系统活动代码页及是否 UTF-8：Windows 经 chcp 读取，非 Windows 由 LANG/LC_ALL 推断 locale 是否 UTF-8。
#[cfg(not(target_os = "windows"))]
fn detect_codepage() -> Option<CodepageSignal> {
    // Codepage is mostly a Windows concept, on Unix we rely on locale
    let is_utf8 = std::env::var("LANG")
        .map(|v| v.to_lowercase().contains("utf-8"))
        .unwrap_or(false)
        || std::env::var("LC_ALL")
            .map(|v| v.to_lowercase().contains("utf-8"))
            .unwrap_or(false);

    Some(CodepageSignal {
        value: Some("n/a".to_string()),
        is_utf8: Some(is_utf8),
    })
}

/// 通用运行时探测：执行指定程序与参数，合并 stdout/stderr 首行作为版本信号；执行失败则标记未检测到。
fn detect_runtime(program: &str, args: &[&str]) -> RuntimeSignal {
    match Command::new(program).args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            // Java outputs version to stderr, Python sometimes stdout/stderr depending on script
            let combined = if stdout.is_empty() {
                stderr
            } else {
                stdout + " " + &stderr
            };
            let first_line = combined.lines().next().unwrap_or("").to_string();

            RuntimeSignal {
                detected: true,
                version: Some(first_line),
                note: None,
            }
        }
        Err(_) => RuntimeSignal {
            detected: false,
            version: None,
            note: None,
        },
    }
}

/// 探测 Python 运行时：依次尝试 `python`/`python3`，并以脚本同时输出版本与 stdout 编码。
fn detect_python_runtime() -> RuntimeSignal {
    let py_cmd = [
        "-c",
        "import sys; print(sys.version.split()[0] + ' ' + str(sys.stdout.encoding))",
    ];

    let python = detect_runtime("python", &py_cmd);
    if python.detected {
        return python;
    }

    detect_runtime("python3", &py_cmd)
}

/// 探测 JDK 运行时：执行 `java -version`，并通过 `-XshowSettings:properties` 解析 `file.encoding` 作为备注。
fn detect_jdk_runtime() -> RuntimeSignal {
    let mut signal = detect_runtime("java", &["-version"]);
    if !signal.detected {
        return signal;
    }

    if let Ok(output) = Command::new("java")
        .args(["-XshowSettings:properties", "-version"])
        .output()
    {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut encoding: Option<String> = None;
        for line in stderr.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("file.encoding =") {
                let v = rest.trim();
                if !v.is_empty() {
                    encoding = Some(v.to_string());
                    break;
                }
            }
        }

        if let Some(enc) = encoding {
            signal.note = Some(format!("file.encoding={enc}"));
        }
    }

    signal
}

/// 判断运行时信号中是否提及 UTF-8（版本或备注包含 "utf-8"/"utf8"，不区分大小写）。
fn runtime_mentions_utf8(signal: &RuntimeSignal) -> bool {
    signal
        .version
        .as_deref()
        .map(|v| {
            v.to_ascii_lowercase().contains("utf-8") || v.to_ascii_lowercase().contains("utf8")
        })
        .unwrap_or(false)
        || signal
            .note
            .as_deref()
            .map(|v| {
                v.to_ascii_lowercase().contains("utf-8") || v.to_ascii_lowercase().contains("utf8")
            })
            .unwrap_or(false)
}

/// 依据 OS、代码页、locale 与运行时信号分类编码风险等级（Ok/Warn/Fail）：Windows 非 UTF-8 代码页且运行时不明为 Fail，其它非 UTF-8 信号为 Warn。
pub fn classify_risk(report: &EncodingDoctorReport) -> EncodingRiskLevel {
    // Rule 1: Windows with non-UTF8 codepage is a FAIL if combined with unclear runtimes, WARN otherwise
    let is_windows = report.os.name.to_lowercase().contains("windows");
    let is_utf8_codepage = report
        .codepage
        .as_ref()
        .and_then(|c| c.is_utf8)
        .unwrap_or(true);
    let runtimes_detected = report.python.detected || report.node.detected || report.jdk.detected;

    if is_windows && !is_utf8_codepage {
        if runtimes_detected {
            return EncodingRiskLevel::Warn;
        } else {
            return EncodingRiskLevel::Fail;
        }
    }

    // Rule 2: Locale not utf8
    if let Some(locale) = &report.os.locale {
        if !locale.to_lowercase().contains("utf-8")
            && !locale.to_lowercase().contains("utf8")
            && locale != "C"
        {
            return EncodingRiskLevel::Warn;
        }
    }

    EncodingRiskLevel::Ok
}

/// 基于报告生成修复建议清单：针对非 UTF-8 代码页、PowerShell、Python/JDK 非 UTF-8 等逐项给出本地化提示。
fn build_recommendations(report: &EncodingDoctorReport) -> Vec<String> {
    let mut recs = Vec::new();

    if let Some(cp) = &report.codepage {
        if let Some(is_utf8) = cp.is_utf8 {
            if !is_utf8 {
                recs.push(t1(
                    "doctor_encoding_rec_codepage_non_utf8",
                    cp.value.as_deref().unwrap_or("unknown"),
                ));
                recs.push(t("doctor_encoding_rec_encoding_hint").to_string());
            }
        }
    }

    if let Some(shell) = &report.shell {
        if shell.name == "powershell" {
            recs.push(t("doctor_encoding_rec_powershell_utf8").to_string());
        }
    }

    if report.python.detected && !runtime_mentions_utf8(&report.python) {
        recs.push(t("doctor_encoding_rec_python_utf8").to_string());
    }

    if report.jdk.detected {
        let jdk_is_utf8 = runtime_mentions_utf8(&report.jdk);
        if !jdk_is_utf8 {
            recs.push(t("doctor_encoding_rec_jdk_utf8").to_string());
        }
    }

    if recs.is_empty() {
        recs.push(t("doctor_encoding_rec_no_action").to_string());
    }

    recs
}

/// 将运行时信号格式化为展示串：已检测显示版本（或本地化"已检测"），未检测显示本地化"未找到"。
fn runtime_display(signal: &RuntimeSignal) -> String {
    if signal.detected {
        signal
            .version
            .as_deref()
            .unwrap_or(t("doctor_encoding_runtime_detected"))
            .to_string()
    } else {
        t("doctor_encoding_runtime_not_found").to_string()
    }
}

/// 将数字代码页（936/950/932/949/1252/65001）映射为带可读名称的字符串（如 "936 GBK"），未知则原样返回。
fn format_codepage_name(cp: &str) -> String {
    match cp {
        "936" => format!("{} GBK", cp),
        "950" => format!("{} Big5", cp),
        "932" => format!("{} Shift-JIS", cp),
        "949" => format!("{} EUC-KR", cp),
        "1252" => format!("{} Windows-1252", cp),
        "65001" => format!("{} UTF-8", cp),
        _ => cp.to_string(),
    }
}

/// 渲染文本版诊断报告：汇总风险等级、各信号与运行时、修复建议，输出本地化可读文本。
fn render_text_report(report: &EncodingDoctorReport) -> String {
    let mut out = String::new();
    out.push_str(t("doctor_encoding_report_title"));
    out.push_str("\n================================\n\n");

    let risk_icon = match report.risk {
        EncodingRiskLevel::Ok => t("doctor_encoding_risk_ok"),
        EncodingRiskLevel::Warn => t("doctor_encoding_risk_warn"),
        EncodingRiskLevel::Fail => t("doctor_encoding_risk_fail"),
    };
    out.push_str(&t1("doctor_encoding_report_risk", risk_icon));
    out.push_str("\n\n");

    out.push_str(t("doctor_encoding_section_signals"));
    out.push('\n');
    out.push_str(&t1(
        "doctor_encoding_signal_os",
        format!("{} ({})", report.os.name, report.os.version),
    ));
    out.push('\n');
    out.push_str(&t1(
        "doctor_encoding_signal_locale",
        report
            .os
            .locale
            .as_deref()
            .unwrap_or(t("doctor_encoding_unknown")),
    ));
    out.push('\n');

    if let Some(shell) = &report.shell {
        if let Some(host) = shell.host.as_deref() {
            out.push_str(&t1(
                "doctor_encoding_signal_shell",
                format!("{} ({}) [host={}]", shell.name, shell.raw, host),
            ));
            out.push('\n');
        } else {
            out.push_str(&t1(
                "doctor_encoding_signal_shell",
                format!("{} ({})", shell.name, shell.raw),
            ));
            out.push('\n');
        }
    } else {
        out.push_str(&t1(
            "doctor_encoding_signal_shell",
            t("doctor_encoding_unknown"),
        ));
        out.push('\n');
    }

    if let Some(cp) = &report.codepage {
        let cp_value = cp.value.as_deref().unwrap_or(t("doctor_encoding_unknown"));
        let display_value = format_codepage_name(cp_value);
        out.push_str(&t1(
            "doctor_encoding_signal_codepage",
            format!("{} (UTF-8: {})", display_value, cp.is_utf8.unwrap_or(false)),
        ));
        out.push('\n');
    } else {
        out.push_str(&t1(
            "doctor_encoding_signal_codepage",
            t("doctor_encoding_unknown"),
        ));
        out.push('\n');
    }

    out.push_str(&t1(
        "doctor_encoding_signal_powershell",
        runtime_display(&report.powershell),
    ));
    out.push('\n');
    out.push_str(&t1(
        "doctor_encoding_signal_python",
        runtime_display(&report.python),
    ));
    out.push('\n');
    out.push_str(&t1(
        "doctor_encoding_signal_node",
        runtime_display(&report.node),
    ));
    out.push('\n');
    let jdk_display = if report.jdk.detected {
        let base = report
            .jdk
            .version
            .as_deref()
            .unwrap_or("detected")
            .lines()
            .next()
            .unwrap_or("detected");
        if let Some(note) = report.jdk.note.as_deref() {
            format!("{} ({})", base, note)
        } else {
            base.to_string()
        }
    } else {
        t("doctor_encoding_runtime_not_found").to_string()
    };
    out.push_str(&t1("doctor_encoding_signal_jdk", jdk_display));
    out.push('\n');

    out.push('\n');
    out.push_str(t("doctor_encoding_section_decoder_support"));
    out.push('\n');
    for decoder in &report.supported_decoders {
        out.push_str(&format!("- {decoder}\n"));
    }

    out.push('\n');
    out.push_str(t("doctor_encoding_section_expansion_candidates"));
    out.push('\n');
    for expansion in &report.recommended_expansions {
        out.push_str(&format!("- {expansion}\n"));
    }

    out.push('\n');
    out.push_str(t("doctor_encoding_section_repair_strategy"));
    out.push('\n');
    for strategy in &report.repair_strategy_profile {
        out.push_str(&format!("- {strategy}\n"));
    }

    out.push('\n');
    out.push_str(t("doctor_encoding_section_repair_confidence"));
    out.push('\n');
    for conf in &report.repair_confidence_profile {
        out.push_str(&format!("- {conf}\n"));
    }

    out.push('\n');
    out.push_str(t("doctor_encoding_section_recommendations"));
    out.push('\n');
    for (i, rec) in report.recommendations.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, rec));
    }

    out
}

/// Generate executable fix commands based on current environment diagnosis.
/// This does NOT execute anything - it only outputs commands the user can copy.
pub fn generate_fix_commands() -> Result<String, String> {
    let report = collect_encoding_report();
    let mut cmds = Vec::new();

    cmds.push(t("doctor_encoding_fix_title").to_string());
    cmds.push(t("doctor_encoding_fix_note").to_string());
    cmds.push(String::new());

    let is_windows = report.os.name.to_lowercase().contains("windows");

    // Codepage fix for Windows
    if is_windows {
        if let Some(cp) = &report.codepage {
            if cp.is_utf8 == Some(false) {
                cmds.push(t("doctor_encoding_fix_step1").to_string());
                cmds.push("chcp 65001".to_string());
                cmds.push(String::new());
                cmds.push(t("doctor_encoding_fix_step2").to_string());
                cmds.push(t("doctor_encoding_fix_step2_cmd").to_string());
                cmds.push(String::new());
            }
        }
    }

    // PowerShell profile fix
    if report
        .shell
        .as_ref()
        .map(|s| s.name == "powershell")
        .unwrap_or(false)
    {
        cmds.push(t("doctor_encoding_fix_step3").to_string());
        cmds.push(t("doctor_encoding_fix_step3_cmd").to_string());
        cmds.push("[Console]::OutputEncoding = [System.Text.Encoding]::UTF8".to_string());
        cmds.push(String::new());
    }

    // Python encoding fix
    if report.python.detected && !runtime_mentions_utf8(&report.python) {
        cmds.push(t("doctor_encoding_fix_step4").to_string());
        if is_windows {
            cmds.push("set PYTHONIOENCODING=utf-8".to_string());
            cmds.push("set PYTHONUTF8=1".to_string());
        } else {
            cmds.push("export PYTHONIOENCODING=utf-8".to_string());
            cmds.push("export PYTHONUTF8=1".to_string());
        }
        cmds.push(String::new());
    }

    // JDK encoding fix
    if report.jdk.detected && !runtime_mentions_utf8(&report.jdk) {
        cmds.push(t("doctor_encoding_fix_step5").to_string());
        cmds.push(t("doctor_encoding_fix_step5_cmd").to_string());
        cmds.push(t("doctor_encoding_fix_step5_example").to_string());
        cmds.push(String::new());
    }

    // Git autocrlf fix for cross-platform
    if is_windows {
        cmds.push(t("doctor_encoding_fix_step6").to_string());
        cmds.push("git config --global core.autocrlf true".to_string());
        cmds.push(String::new());
    }

    if cmds.len() <= 3 {
        cmds.push(t("doctor_encoding_fix_no_issues").to_string());
    }

    Ok(cmds.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单测：当存在 PowerShell 分发渠道信号时，即使同时有 ComSpec/CMD 信号也优先判定为 powershell。
    #[test]
    fn detect_shell_from_signals_prefers_powershell_hints() {
        let signals = ShellEnvSignals {
            ps_dist: Some("VSCode".to_string()),
            comspec: Some("C:\\Windows\\System32\\cmd.exe".to_string()),
            cmd_cmd_line: Some("cmd /d /k".to_string()),
            ..Default::default()
        };
        let shell = detect_shell_from_signals(&signals).expect("shell expected");
        assert_eq!(shell.name, "powershell");
    }

    /// 单测：ComSpec=cmd.exe 且 CMDCMDLINE/PROMPT 存在时，正确判定为 cmd。
    #[test]
    fn detect_shell_from_signals_detects_cmd() {
        let signals = ShellEnvSignals {
            comspec: Some("C:\\Windows\\System32\\cmd.exe".to_string()),
            cmd_cmd_line: Some("cmd /d /k".to_string()),
            cmd_prompt: Some("$P$G".to_string()),
            ..Default::default()
        };
        let shell = detect_shell_from_signals(&signals).expect("shell expected");
        assert_eq!(shell.name, "cmd");
    }

    /// 单测：SHELL 变量（/bin/zsh）优先于 PSModulePath 中的 powershell 路径，判定为 zsh。
    #[test]
    fn detect_shell_from_signals_prefers_shell_env_over_psmodulepath() {
        let signals = ShellEnvSignals {
            shell: Some("/bin/zsh".to_string()),
            ps_module_path: Some(
                "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\Modules".to_string(),
            ),
            ..Default::default()
        };
        let shell = detect_shell_from_signals(&signals).expect("shell expected");
        assert_eq!(shell.name, "zsh");
        assert_eq!(shell.raw, "/bin/zsh");
    }

    /// 单测：仅提供 ComSpec=cmd.exe 时回退判定为 cmd，且 raw 含 ComSpec=。
    #[test]
    fn detect_shell_from_signals_falls_back_to_comspec() {
        let signals = ShellEnvSignals {
            comspec: Some("C:\\Windows\\System32\\cmd.exe".to_string()),
            ..Default::default()
        };
        let shell = detect_shell_from_signals(&signals).expect("shell expected");
        assert_eq!(shell.name, "cmd");
        assert!(shell.raw.contains("ComSpec="));
    }

    /// 单测：信号全空时 detect_shell_from_signals 返回 None。
    #[test]
    fn detect_shell_from_signals_returns_none_when_empty() {
        let signals = ShellEnvSignals::default();
        assert!(detect_shell_from_signals(&signals).is_none());
    }
}
