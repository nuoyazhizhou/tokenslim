// 测试 fixture 中刻意的 mojibake 字符串（UTF-8 中文被误读为 Latin-1）含软连字符 U+00AD,
// 属不可见字符，clippy 默认 deny；此处按测试意图允许该 lint。
#![allow(clippy::invisible_characters)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tokenslim::core::encoding_fallback::{
    decode_and_repair_for_display, decode_with_fallback, evaluate_repair_confidence,
    is_probable_binary_bytes, write_utf8, write_utf8_bom,
};

const CHINESE: &str = "中文";

/// 构造带纳秒时间戳与进程 ID 的唯一临时目录路径（不创建目录）。
fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("tokenslim-{prefix}-{nanos}-{}", std::process::id()))
}

/// 去除文本末尾的 \r 与 \n（含 CRLF），用于对比子进程输出与期望值。
fn trim_line_endings(text: &str) -> &str {
    text.trim_end_matches(['\r', '\n'])
}

/// 去除字符串开头的 UTF-8 BOM（\u{feff}），无 BOM 时原样返回。
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// 解码字节流并返回 (文本, 识别编码名)，委托核心库的 decode_with_fallback。
fn decode_stdout(bytes: &[u8]) -> (String, &'static str) {
    decode_with_fallback(bytes)
}

/// 返回 encoding_fallback 样本目录相对路径。
fn sample_dir() -> PathBuf {
    PathBuf::from("samples/encoding_fallback")
}

/// 读取样本文件原始字节；样本缺失时 panic（测试前置条件不允许失败）。
fn read_sample(name: &str) -> Vec<u8> {
    fs::read(sample_dir().join(name)).expect("sample file should exist")
}

/// 读取以十六进制文本（空白分隔）存储的样本并解析为字节序列。
fn read_hex_sample(name: &str) -> Vec<u8> {
    let raw = fs::read_to_string(sample_dir().join(name)).expect("hex sample should exist");
    raw.split_whitespace()
        .map(|x| u8::from_str_radix(x, 16).expect("hex byte should parse"))
        .collect()
}

/// 依次探测候选命令（如 python/python3/py），返回首个可成功执行的命令名。
///
/// 用于测试对环境运行时的可选依赖：运行时缺失时返回 None，测试据此跳过。
fn runtime_available(candidates: &[&str], probe_args: &[&str]) -> Option<String> {
    for candidate in candidates {
        if let Ok(output) = Command::new(candidate).args(probe_args).output() {
            if output.status.success() {
                return Some((*candidate).to_string());
            }
        }
    }
    None
}

/// 探测可用的 Python 解释器命令名（python/python3/py）。
fn python_runtime() -> Option<String> {
    runtime_available(&["python", "python3", "py"], &["--version"])
}

/// 探测可用的 Node.js 命令名（node）。
fn node_runtime() -> Option<String> {
    runtime_available(&["node"], &["--version"])
}

/// 探测可用的 Java 运行时命令名（java）。
fn java_runtime() -> Option<String> {
    runtime_available(&["java"], &["--version"])
}

/// 探测可用的 Java 编译器命令名（javac）。
fn javac_runtime() -> Option<String> {
    runtime_available(&["javac"], &["--version"])
}

/// UTF-8 往返保真：write_utf8 写入中文后 read+decode 应原样还原且编码判为 utf-8。
#[test]
fn utf8_roundtrip_keeps_chinese_text_intact() {
    let dir = unique_temp_dir("utf8-roundtrip");
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("roundtrip.txt");
    write_utf8(&path, CHINESE).unwrap();

    let bytes = fs::read(&path).unwrap();
    let (decoded, encoding) = decode_stdout(&bytes);

    assert_eq!(encoding, "utf-8");
    assert_eq!(decoded, CHINESE);

    fs::remove_dir_all(&dir).ok();
}

/// Python 子进程输出中文经 fallback 解码为 utf-8（无 Python 时跳过）。
#[test]
fn python_subprocess_output_decodes_via_fallback() {
    let Some(python) = python_runtime() else {
        return;
    };

    let output = if python == "py" {
        Command::new(&python)
            .args(["-3", "-c", "print('中文')"])
            .env("PYTHONIOENCODING", "utf-8")
            .output()
            .unwrap()
    } else {
        Command::new(&python)
            .args(["-c", "print('中文')"])
            .env("PYTHONIOENCODING", "utf-8")
            .output()
            .unwrap()
    };

    assert!(output.status.success());

    let (decoded, encoding) = decode_stdout(&output.stdout);
    assert_eq!(encoding, "utf-8");
    assert_eq!(trim_line_endings(&decoded), CHINESE);
}

/// Node 子进程输出中文经 fallback 解码为 utf-8（无 node 时跳过）。
#[test]
fn node_subprocess_output_decodes_via_fallback() {
    let Some(node) = node_runtime() else {
        return;
    };

    let output = Command::new(&node)
        .args(["-e", "console.log('中文')"])
        .output()
        .unwrap();

    assert!(output.status.success());

    let (decoded, encoding) = decode_stdout(&output.stdout);
    assert_eq!(encoding, "utf-8");
    assert_eq!(trim_line_endings(&decoded), CHINESE);
}

/// Java 编译运行探针子进程输出中文经 fallback 解码为 utf-8（无 java/javac 或编译失败时跳过）。
#[test]
fn java_subprocess_output_decodes_via_fallback() {
    let Some(java) = java_runtime() else {
        return;
    };
    let Some(javac) = javac_runtime() else {
        return;
    };

    let dir = unique_temp_dir("java-runtime");
    fs::create_dir_all(&dir).unwrap();

    let source = dir.join("EncodingProbe.java");
    fs::write(
        &source,
        r#"import java.nio.charset.StandardCharsets;

public class EncodingProbe {
    public static void main(String[] args) throws Exception {
        System.out.write("\u4e2d\u6587\n".getBytes(StandardCharsets.UTF_8));
        System.out.flush();
    }
}"#,
    )
    .unwrap();

    let compile = Command::new(&javac)
        .arg(&source)
        .current_dir(&dir)
        .output()
        .unwrap();
    if !compile.status.success() {
        fs::remove_dir_all(&dir).ok();
        return;
    }

    let classpath = dir.to_string_lossy().to_string();
    let output = Command::new(&java)
        .args(["-cp", classpath.as_str(), "EncodingProbe"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(output.status.success());

    let (decoded, encoding) = decode_stdout(&output.stdout);
    assert_eq!(encoding, "utf-8");
    assert_eq!(trim_line_endings(&decoded), CHINESE);

    fs::remove_dir_all(&dir).ok();
}

/// 跨语言一致性：可用运行时（python/node/java）输出的中文解码结果必须互相一致。
#[test]
fn multi_language_consistency_matches_across_available_runtimes() {
    let mut results = Vec::new();

    if let Some(python) = python_runtime() {
        let output = if python == "py" {
            Command::new(&python)
                .args(["-3", "-c", "print('中文')"])
                .env("PYTHONIOENCODING", "utf-8")
                .output()
                .unwrap()
        } else {
            Command::new(&python)
                .args(["-c", "print('中文')"])
                .env("PYTHONIOENCODING", "utf-8")
                .output()
                .unwrap()
        };
        let (decoded, encoding) = decode_stdout(&output.stdout);
        results.push(("python", trim_line_endings(&decoded).to_string(), encoding));
    }

    if let Some(node) = node_runtime() {
        let output = Command::new(&node)
            .args(["-e", "console.log('中文')"])
            .output()
            .unwrap();
        let (decoded, encoding) = decode_stdout(&output.stdout);
        results.push(("node", trim_line_endings(&decoded).to_string(), encoding));
    }

    if let (Some(java), Some(javac)) = (java_runtime(), javac_runtime()) {
        let dir = unique_temp_dir("multi-language-java");
        fs::create_dir_all(&dir).unwrap();

        let source = dir.join("EncodingProbe.java");
        fs::write(
            &source,
            r#"import java.nio.charset.StandardCharsets;

public class EncodingProbe {
    public static void main(String[] args) throws Exception {
        System.out.write("\u4e2d\u6587\n".getBytes(StandardCharsets.UTF_8));
        System.out.flush();
    }
}"#,
        )
        .unwrap();

        let compile = Command::new(&javac)
            .arg(&source)
            .current_dir(&dir)
            .output()
            .unwrap();

        if compile.status.success() {
            let classpath = dir.to_string_lossy().to_string();
            let output = Command::new(&java)
                .args(["-cp", classpath.as_str(), "EncodingProbe"])
                .current_dir(&dir)
                .output()
                .unwrap();
            let (decoded, encoding) = decode_stdout(&output.stdout);
            results.push(("java", trim_line_endings(&decoded).to_string(), encoding));
        }

        fs::remove_dir_all(&dir).ok();
    }

    assert!(!results.is_empty());
    let first = results[0].1.clone();
    let first_encoding = results[0].2;
    for (_, text, encoding) in &results {
        assert_eq!(text, &first);
        assert_eq!(*encoding, first_encoding);
    }
    assert_eq!(first, CHINESE);
}

/// GBK 编码字节（"中文"）经 fallback 处理不 panic 且产出非空结果。
#[test]
fn gbk_bytes_are_handled_without_panicking() {
    let gbk_bytes: &[u8] = &[0xd6, 0xd0, 0xce, 0xc4];
    let (decoded, encoding) = decode_stdout(gbk_bytes);

    assert!(!decoded.is_empty());
    assert!(!encoding.is_empty());
}

/// BOM 文件往返：write_utf8_bom 写入 BOM，读取后解码内容不含 BOM 泄漏。
#[test]
fn bom_prefixed_files_decode_without_leaking_bom_into_content() {
    let dir = unique_temp_dir("bom-roundtrip");
    fs::create_dir_all(&dir).unwrap();

    let path = dir.join("bom.txt");
    write_utf8_bom(&path, CHINESE).unwrap();

    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]));

    let (decoded, encoding) = decode_stdout(&bytes);
    assert_eq!(encoding, "utf-8");
    assert_eq!(strip_bom(&decoded), CHINESE);

    fs::remove_dir_all(&dir).ok();
}

/// 乱码链样本修复：修复结果应为"中文"且修复置信度为 high、证据含 mojibake。
#[test]
fn sample_mojibake_chain_is_repaired_with_high_confidence() {
    let bytes = read_sample("case_001_mojibake_chain.log");
    let (decoded, enc) = decode_with_fallback(&bytes);
    let (repaired, _, steps) = decode_and_repair_for_display(&bytes);
    let (confidence, evidence) = evaluate_repair_confidence(&decoded, &repaired, &steps);

    assert!(!enc.is_empty());
    assert_eq!(decoded.trim(), "Ã¤Â¸Â­Ã¦â€“â€¡");
    assert_eq!(repaired.trim(), "中文");
    assert_eq!(confidence, "high");
    assert!(evidence.iter().any(|x| x.contains("mojibake")));
}

/// UTF-16 无 BOM 大小端样本：均解码为 "Hi!" 且编码识别正确。
#[test]
fn sample_utf16_no_bom_hex_cases_are_detected() {
    let le = read_hex_sample("case_002_utf16le_no_bom.hex");
    let be = read_hex_sample("case_003_utf16be_no_bom.hex");

    let (le_text, le_enc) = decode_with_fallback(&le);
    let (be_text, be_enc) = decode_with_fallback(&be);

    assert_eq!(le_text, "Hi!");
    assert_eq!(be_text, "Hi!");
    assert_eq!(le_enc, "UTF-16LE(no-bom)");
    assert_eq!(be_enc, "UTF-16BE(no-bom)");
}

/// 内联 BOM/NUL/CR 混合样本：修复为 "ab\n"，且修复步骤含 strip-inline-bom/strip-nul/normalize-cr。
#[test]
fn sample_inline_bom_nul_cr_hex_case_is_cleaned() {
    let bytes = read_hex_sample("case_004_inline_bom_nul_cr.hex");
    let (repaired, _, steps) = decode_and_repair_for_display(&bytes);

    assert_eq!(repaired, "ab\n");
    assert!(steps.iter().any(|x| x == "strip-inline-bom"));
    assert!(steps.iter().any(|x| x == "strip-nul"));
    assert!(steps.iter().any(|x| x == "normalize-cr"));
}

/// GBK 十六进制样本解码为简体中文"中文"。
#[test]
fn sample_gbk_hex_decodes_as_chinese() {
    let bytes = read_hex_sample("case_005_gbk_zh.hex");
    let (decoded, _) = decode_with_fallback(&bytes);
    assert_eq!(decoded, "中文");
}

/// Big5 十六进制样本解码为繁体中文"繁體中文測試"。
#[test]
fn sample_big5_hex_decodes_as_traditional_chinese() {
    let bytes = read_hex_sample("case_009_big5_zh_tw.hex");
    let (decoded, _) = decode_with_fallback(&bytes);
    assert_eq!(decoded, "繁體中文測試");
}

/// CP949 十六进制样本解码为韩文且编码识别为 euc-kr 或 windows-949 之一。
#[test]
fn sample_cp949_hex_decodes_as_korean() {
    let bytes = read_hex_sample("case_010_cp949_ko.hex");
    let (decoded, used_enc) = decode_with_fallback(&bytes);
    assert_eq!(decoded, "한국어테스트데이터");
    assert!(
        used_enc.eq_ignore_ascii_case("euc-kr") || used_enc.eq_ignore_ascii_case("windows-949"),
        "unexpected encoding: {used_enc}"
    );
}

/// UTF-32 无 BOM 大小端样本：均解码为 "Hi" 且编码识别正确。
#[test]
fn sample_utf32_no_bom_hex_cases_are_detected() {
    let le = read_hex_sample("case_006_utf32le_no_bom.hex");
    let be = read_hex_sample("case_007_utf32be_no_bom.hex");
    let (le_text, le_enc) = decode_with_fallback(&le);
    let (be_text, be_enc) = decode_with_fallback(&be);
    assert_eq!(le_text, "Hi");
    assert_eq!(be_text, "Hi");
    assert_eq!(le_enc, "UTF-32LE(no-bom)");
    assert_eq!(be_enc, "UTF-32BE(no-bom)");
}

/// 二进制特征样本：is_probable_binary_bytes 判定为二进制，且修复步骤含 binary-guard-skip-repair。
#[test]
fn sample_binary_like_hex_is_skipped_by_binary_guard() {
    let bytes = read_hex_sample("case_008_binary_like.hex");
    assert!(is_probable_binary_bytes(&bytes));
    let (_, _, steps) = decode_and_repair_for_display(&bytes);
    assert!(steps.iter().any(|x| x == "binary-guard-skip-repair"));
}
