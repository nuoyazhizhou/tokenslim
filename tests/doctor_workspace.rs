use tokenslim::core::doctor_workspace::{
    collect_workspace_report, generate_context_file, run_workspace_doctor, WorkspaceReportFormat,
};

/// 工作区报告核心字段非空：OS/shell/项目主语言/构建/测试命令均须检出。
#[test]
fn test_workspace_report_has_core_fields() {
    let report = collect_workspace_report();
    assert!(!report.os.is_empty());
    assert!(!report.shell.is_empty());
    assert!(!report.project.primary.is_empty());
    assert!(!report.project.build.is_empty());
    assert!(!report.project.test.is_empty());
}

/// LLM 紧凑格式 JSON 结构契约：r/enc_risk/enc_mixed/os/proj/act/ide/repo(v,b) 键齐全。
#[test]
fn test_workspace_llm_format_is_compact_json() {
    let llm = run_workspace_doctor(WorkspaceReportFormat::Llm, false).unwrap();
    let v: serde_json::Value = serde_json::from_str(&llm).unwrap();
    assert!(v.get("r").is_some());
    assert!(v.get("enc_risk").is_some());
    assert_eq!(v.get("enc_mixed").and_then(|x| x.as_bool()), Some(true));
    assert!(v.get("os").is_some());
    assert!(v.get("proj").is_some());
    assert!(v.get("act").is_some());
    assert!(v.get("ide").is_some());
    assert!(v.get("ide").unwrap().is_array());
    assert!(v.get("repo").is_some());
    assert!(v.get("repo").unwrap().is_object());
    assert!(v.get("repo").and_then(|r| r.get("v")).is_some());
    assert!(v.get("repo").and_then(|r| r.get("b")).is_some());
}

/// 生成的上下文文件须含 VCS 插件配置引导（vcs_plugin.json/生成脚本路径）。
#[test]
fn test_generated_context_contains_vcs_guidance() {
    let content = generate_context_file().unwrap();
    assert!(content.contains("## VCS Plugin Config Guidance"));
    assert!(content.contains("config/vcs_plugin.json"));
    assert!(content.contains("scripts/generate_vcs_config.py"));
}

/// 上下文文件须将检测到的构建/测试命令包装为 tokenslim run 形式供 AI 工具使用。
#[test]
fn test_generated_context_wraps_detected_commands_for_ai_tools() {
    let content = generate_context_file().unwrap();
    assert!(content.contains("## TokenSlim Command Policy"));
    assert!(content.contains("tokenslim run"));
    assert!(content.contains("- Raw Build:"));
    assert!(content.contains("- Raw Test:"));
    assert!(!content.contains("- Use the detected build/test commands.\n"));
    assert!(content.contains("- Use the detected build/test commands through `tokenslim run`."));
}
