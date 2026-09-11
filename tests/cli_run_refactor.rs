use std::path::Path;
use std::process::Command;

/// 以给定参数运行 tokenslim 可执行文件并返回其输出（失败即 panic）。
///
/// 通过 CARGO_BIN_EXE_tokenslim 定位集成测试编译产物，验证 CLI 行为。
fn run_tokenslim(args: &[&str]) -> std::process::Output {
    let bin = env!("CARGO_BIN_EXE_tokenslim");
    Command::new(bin)
        .args(args)
        .output()
        .expect("failed to launch tokenslim")
}

/// 无参数运行时输出 USAGE 帮助且退出码为成功。
#[test]
fn cli_no_args_prints_usage_and_exits_success() {
    let output = run_tokenslim(&[]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // i18n 化后中文系统输出「用法:」，英文系统输出「USAGE:」，兼容两种语言
    assert!(stdout.contains("用法:") || stdout.contains("USAGE:"));
    assert!(stdout.contains("tokenslim"));
}

/// 隐式 run：不带 run 关键字直接传命令，命令输出应透传（run 拆分后兼容）。
#[test]
fn cli_implicit_run_still_works_after_run_cli_split() {
    let output = if cfg!(windows) {
        run_tokenslim(&["cmd", "/C", "echo", "hello_run"])
    } else {
        run_tokenslim(&["sh", "-lc", "printf hello_run"])
    };
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello_run"));
}

/// 显式 run：带 run 关键字执行命令，命令输出应透传（run 拆分后兼容）。
#[test]
fn cli_explicit_run_still_works_after_run_cli_split() {
    let output = if cfg!(windows) {
        run_tokenslim(&["run", "cmd", "/C", "echo", "hello_explicit"])
    } else {
        run_tokenslim(&["run", "sh", "-lc", "printf hello_explicit"])
    };
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello_explicit"));
}

/// workspace 诊断：`workspace --format json` 应输出可解析的 JSON，
/// 并包含 project.primary / os / risk 等核心字段（collect_workspace_report 经 CLI 触发的端到端验证）。
#[test]
fn cli_workspace_doctor_emits_valid_json_report() {
    let output = run_tokenslim(&["workspace", "--format", "json"]);
    assert!(
        output.status.success(),
        "workspace 诊断应以成功退出码结束: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    let report: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("workspace --format json 应输出合法 JSON");
    assert!(report.get("os").is_some(), "JSON 报告应包含 os 字段");
    assert!(report.get("risk").is_some(), "JSON 报告应包含 risk 字段");
    let primary = report
        .get("project")
        .and_then(|p| p.get("primary"))
        .expect("JSON 报告应包含 project.primary 字段");
    assert!(
        primary.is_string() && !primary.as_str().unwrap_or_default().is_empty(),
        "project.primary 应为非空字符串"
    );
}

/// workspace 诊断-文本格式：`workspace --format text` 应输出带 i18n 分区标题与分隔线的文本报告，
/// 端到端验证 render_workspace_text / detect_project 经 CLI 触发（与 --format json 对照补齐格式矩阵）。
#[test]
fn cli_workspace_doctor_emits_text_report_with_i18n_sections() {
    let output = run_tokenslim(&["workspace", "--format", "text"]);
    assert!(
        output.status.success(),
        "workspace 文本诊断应以成功退出码结束: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 文本报告应包含 i18n 报告标题与分区标题（测试与 CLI 同机同环境 locale，t() 解析一致）
    assert!(
        stdout.contains(tokenslim::utils::i18n::t("doctor_workspace_report_title")),
        "文本报告应包含报告标题"
    );
    assert!(
        stdout.contains(tokenslim::utils::i18n::t(
            "doctor_workspace_section_project"
        )),
        "文本报告应包含项目分区标题"
    );
    // 报告中应存在分隔线 "===="（语言无关锚点）
    assert!(stdout.contains("===="), "文本报告应包含分隔线");
    assert!(
        stdout.contains(tokenslim::utils::i18n::t("doctor_workspace_field_primary")),
        "文本报告应包含 primary 字段标签"
    );
}

/// workspace 上下文注入：`workspace --inject --dry-run` 仅预览注入审计，
/// 输出应标记 dry-run 并提示未写盘，不实际修改任何文件（inject_context_file 经 CLI 触发的端到端验证）。
#[test]
fn cli_workspace_inject_dry_run_previews_only() {
    let output = run_tokenslim(&["workspace", "--inject", "--dry-run"]);
    assert!(
        output.status.success(),
        "workspace --inject --dry-run 应以成功退出码结束: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("dry-run"), "注入审计应标记 dry-run 模式");
    assert!(
        stdout.contains("preview only, not written"),
        "dry-run 应明确提示仅预览、未写盘"
    );
    assert!(
        stdout.contains("Context Injection") || stdout.contains("context injection"),
        "审计输出应包含注入审计报告标题"
    );
}

/// encoding 诊断：`encoding --format json` 应输出可解析的 JSON，
/// 并包含 risk / os / supported_decoders 等核心字段（collect_encoding_report 经 CLI 触发的端到端验证）。
#[test]
fn cli_encoding_doctor_emits_valid_json_report() {
    let output = run_tokenslim(&["encoding", "--format", "json"]);
    assert!(
        output.status.success(),
        "encoding 诊断应以成功退出码结束: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    let report: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("encoding --format json 应输出合法 JSON");
    assert!(report.get("risk").is_some(), "JSON 报告应包含 risk 字段");
    assert!(report.get("os").is_some(), "JSON 报告应包含 os 信号");
    assert!(
        report
            .get("supported_decoders")
            .map(|v| v.is_array())
            .unwrap_or(false),
        "JSON 报告应包含 supported_decoders 数组"
    );
}

/// explain-plugin 命令链路：`explain-plugin --explain-command "<cmd>"` 仅解析命令字符串文本，
/// 输出为 JSON，应含 kind=plugin_selection / input_kind=command 与选中插件名
/// （handle_explain_plugin_action 命令分支端到端验证）。
#[test]
fn cli_explain_plugin_command_line_reports_selected_plugin() {
    let output = run_tokenslim(&["explain-plugin", "--explain-command", "git status"]);
    assert!(
        output.status.success(),
        "explain-plugin --explain-command 应以成功退出码结束: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    let out: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("explain-plugin --explain-command 应输出合法 JSON");
    assert_eq!(
        out.get("kind").and_then(|v| v.as_str()),
        Some("plugin_selection"),
        "顶层 kind 应为 plugin_selection"
    );
    assert_eq!(
        out.get("input_kind").and_then(|v| v.as_str()),
        Some("command"),
        "input_kind 应为 command"
    );
    // 应选中具体插件而非 none 回退
    let selected = out
        .get("fields")
        .and_then(|f| f.get("selected_plugin"))
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    assert!(
        !selected.is_empty() && selected != "none",
        "应选中具体插件而非 none 回退, 实际={selected}"
    );
}

/// 唯一临时目录（避免并发测试互相踩文件名）。
fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("tokenslim-{prefix}-{nanos}-{}", std::process::id()))
}

/// ANSI 剥壳端到端回归：把含真实 ESC 字节的物理样本经 `run --input <file> --audit-jsonl ...`
/// 喂进完整 run 链路，断言三件事：
///   1. 输出不含真实 ANSI 控制码（剥净）；
///   2. --audit-jsonl 落盘且 compression 事件含 ansi_strip_bytes_removed；
///   3. 构建/错误类插件 coverage>0（专用插件真实参与压缩，非 generic 兜底）。
///
/// 本测试刻意走 `run --input` 的「喂文本」路径（复用 run 上一级压缩函数）而非直接
/// 调函数，这样才能把 CLI 参数解析 → 流水线 → 插件调度 → 审计落盘的接线缺陷暴露出来，
/// 与 `scripts/audit_ansi_e2e_sweep.py` 巡检互为补充（脚本扫全量，测试固化红灯样本）。
#[test]
fn cli_run_input_feeds_ansi_sample_through_full_chain() {
    // 用物理样本而非手写 mock 字符串（红线：禁止手写 Mock）。
    // 选含真实 ESC 字节且走构建类插件的样本（bazel, coverage>0）。
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sample = manifest
        .join("samples")
        .join("bazel_plugin")
        .join("case_010_ansi.log");
    let raw = std::fs::read_to_string(&sample).expect("bazel ANSI 样本应可读取");
    // 确认样本确实含真实 ESC 字节，否则测不到剥壳链路
    assert!(
        raw.contains('\u{1b}'),
        "样本应含真实 ANSI ESC 字节, 实际={raw:?}"
    );

    let dir = unique_temp_dir("ansi-input");
    std::fs::create_dir_all(&dir).unwrap();
    let audit = dir.join("audit.jsonl");

    let output = run_tokenslim(&[
        "run",
        "--input",
        sample.to_str().unwrap(),
        "--audit-jsonl",
        audit.to_str().unwrap(),
        "bazel",
        "build",
    ]);
    assert!(
        output.status.success(),
        "run --input 应以成功退出码结束: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 断言 1：输出剥净 TAG ANSI 控制码
    assert!(
        !stdout.contains('\u{1b}'),
        "run --input 输出不应残留真实 ANSI 控制码"
    );
    // 断言 2：审计 JSONL 落盘且含剥壳遥测
    let audit_text = std::fs::read_to_string(&audit).expect("--audit-jsonl 应落盘审计文件");
    assert!(
        strike_metric_present(&audit_text),
        "--audit-jsonl 应含 ansi_strip_bytes_removed 遥测, 实际:\n{audit_text}"
    );
    // 断言 3：bazel 为构建类，coverage>0 证明专用插件真实参与
    let cov = parse_coverage_from_jsonl(&audit_text);
    assert!(
        cov > 0.0,
        "bazel 样本 coverage 应>0（专用插件真实参与）, 实际={cov}, audit:\n{audit_text}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// 从 audit JSONL 探测是否含 ansi_strip_bytes_removed 字段。
fn strike_metric_present(audit_text: &str) -> bool {
    audit_text.contains("ansi_strip_bytes_removed")
}

/// 从 audit JSONL 解析 compression 事件的 coverage 字段。
fn parse_coverage_from_jsonl(audit_text: &str) -> f64 {
    for line in audit_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(cov) = v.get("coverage").and_then(|c| c.as_f64()) {
                return cov;
            }
        }
    }
    -1.0
}
