use tokenslim::core::doctor_workspace::{
    collect_workspace_report, generate_context_file, run_workspace_doctor, WorkspaceReportFormat,
};

#[test]
fn test_workspace_report_has_core_fields() {
    let report = collect_workspace_report();
    assert!(!report.os.is_empty());
    assert!(!report.shell.is_empty());
    assert!(!report.project.primary.is_empty());
    assert!(!report.project.build.is_empty());
    assert!(!report.project.test.is_empty());
}

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

#[test]
fn test_generated_context_contains_vcs_guidance() {
    let content = generate_context_file().unwrap();
    assert!(content.contains("## VCS Plugin Config Guidance"));
    assert!(content.contains("config/vcs_plugin.json"));
    assert!(content.contains("scripts/generate_vcs_config.py"));
}

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
