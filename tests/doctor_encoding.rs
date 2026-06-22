use tokenslim::core::doctor_encoding::{
    classify_risk, collect_encoding_report, CodepageSignal, EncodingDoctorReport,
    EncodingRiskLevel, OsSignal, RuntimeSignal,
};
use tokenslim::core::doctor_encoding::{run_encoding_doctor, DoctorReportFormat};
use tokenslim::core::encoding_fallback::{
    decode_and_repair_for_display, evaluate_repair_confidence,
};

#[test]
fn test_risk_classification_ok_warn_fail() {
    let report_fail = EncodingDoctorReport {
        risk: EncodingRiskLevel::Ok,
        os: OsSignal {
            name: "Windows 10".to_string(),
            version: "10.0".to_string(),
            locale: Some("zh-CN".to_string()),
        },
        shell: None,
        codepage: Some(CodepageSignal {
            value: Some("936".to_string()),
            is_utf8: Some(false),
        }),
        powershell: RuntimeSignal {
            detected: false,
            version: None,
            note: None,
        },
        python: RuntimeSignal {
            detected: false,
            version: None,
            note: None,
        },
        node: RuntimeSignal {
            detected: false,
            version: None,
            note: None,
        },
        jdk: RuntimeSignal {
            detected: false,
            version: None,
            note: None,
        },
        supported_decoders: vec![],
        recommended_expansions: vec![],
        repair_strategy_profile: vec![],
        repair_confidence_profile: vec![],
        recommendations: vec![],
    };

    assert_eq!(classify_risk(&report_fail), EncodingRiskLevel::Fail);

    let report_warn = EncodingDoctorReport {
        python: RuntimeSignal {
            detected: true,
            version: Some("3.12 utf-8".to_string()),
            note: None,
        },
        ..report_fail.clone()
    };
    assert_eq!(classify_risk(&report_warn), EncodingRiskLevel::Warn);

    let report_ok = EncodingDoctorReport {
        os: OsSignal {
            name: "Linux".to_string(),
            version: "6.x".to_string(),
            locale: Some("en_US.UTF-8".to_string()),
        },
        codepage: Some(CodepageSignal {
            value: None,
            is_utf8: Some(true),
        }),
        powershell: RuntimeSignal {
            detected: false,
            version: None,
            note: None,
        },
        python: RuntimeSignal {
            detected: true,
            version: Some("3.12 utf-8".to_string()),
            note: None,
        },
        node: RuntimeSignal {
            detected: true,
            version: Some("v20".to_string()),
            note: None,
        },
        jdk: RuntimeSignal {
            detected: true,
            version: Some("17".to_string()),
            note: None,
        },
        ..report_fail
    };
    assert_eq!(classify_risk(&report_ok), EncodingRiskLevel::Ok);

    let actual_report = collect_encoding_report();
    assert!(!actual_report.os.name.is_empty());
}

#[test]
fn test_json_shape_contains_required_keys() {
    let json_str = run_encoding_doctor(DoctorReportFormat::Json).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert!(value.get("risk").is_some());
    assert!(value.get("os").is_some());
    assert!(value.get("shell").is_some());
    assert!(value.get("codepage").is_some());
    assert!(value.get("powershell").is_some());
    assert!(value.get("python").is_some());
    assert!(value.get("node").is_some());
    assert!(value.get("jdk").is_some());
    assert!(value.get("supported_decoders").is_some());
    assert!(value.get("recommended_expansions").is_some());
    assert!(value.get("repair_strategy_profile").is_some());
    assert!(value.get("repair_confidence_profile").is_some());
    assert!(value.get("recommendations").is_some());

    let supported = value
        .get("supported_decoders")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(supported.iter().any(|v| {
        v.as_str()
            .map(|s| s.contains("mixed-by-lines"))
            .unwrap_or(false)
    }));
}

#[test]
fn test_text_output_has_sections() {
    let text_str = run_encoding_doctor(DoctorReportFormat::Text).unwrap();

    assert!(text_str.contains("Overall Risk:") || text_str.contains("总体风险:"));
    assert!(text_str.contains("[Signals]") || text_str.contains("[环境信号]"));
    assert!(text_str.contains("[Decoder Support]") || text_str.contains("[解码器支持]"));
    assert!(text_str.contains("[Expansion Candidates]") || text_str.contains("[可扩展候选]"));
    assert!(text_str.contains("[Repair Strategy Tiers]") || text_str.contains("[修复策略分层]"));
    assert!(text_str.contains("[Repair Confidence]") || text_str.contains("[修复置信度]"));
    assert!(text_str.contains("mixed-by-lines"));
    assert!(text_str.contains("mixed-by-chunks"));
    assert!(text_str.contains("[Recommendations]") || text_str.contains("[建议]"));
}

#[test]
fn test_mixed_polluted_sample_and_repair_confidence() {
    // UTF-8 + mojibake fragment + BOM/CRLF pollution mixed in one payload.
    let mixed = "\u{feff}INFO start\r\nÃ¤Â¸Â­Ã¦â€“â€¡ segment\r\n正常UTF8段落";
    let (fixed, _enc, steps) = decode_and_repair_for_display(mixed.as_bytes());
    let (confidence, evidence) = evaluate_repair_confidence(mixed, &fixed, &steps);

    assert!(fixed.contains("segment"));
    assert!(fixed.contains("正常UTF8段落"));
    assert!(!fixed.contains('\u{feff}'));
    assert!(!fixed.contains("\r\n"));
    assert!(!steps.is_empty() || fixed != mixed);
    assert!(!confidence.is_empty());
    assert!(!evidence.is_empty());
}

#[test]
fn test_collect_encoding_report_profiles_and_risk_are_consistent() {
    let report = collect_encoding_report();
    assert!(!report.supported_decoders.is_empty());
    assert!(!report.repair_strategy_profile.is_empty());
    assert!(!report.repair_confidence_profile.is_empty());
    assert!(!report.recommendations.is_empty());
    assert_eq!(report.risk, classify_risk(&report));
}
