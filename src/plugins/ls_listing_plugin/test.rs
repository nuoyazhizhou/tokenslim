//! ls_listing 插件样本驱动测试（P3-206②）。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::ls_listing_plugin::LsListingPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file, read_sample_log};

/// 测试：真实 `aws s3 ls --recursive` 样本被识别。
#[test]
fn detects_s3_ls_recursive_listing() {
    let plugin = LsListingPlugin::new();
    let raw = read_sample_log("ls_listing_plugin", "case_001_s3_ls_recursive");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

/// 测试：纯填充规约口径——逐行 `datetime size fullpath`（单空格），路径/日期/
/// 尺寸零丢失，无组头（设计稿 §8.3 选项 1），且不得扩张。
#[test]
fn folds_listing_with_padding_normalization() {
    let plugin = LsListingPlugin::new();
    let raw = read_sample_log("ls_listing_plugin", "case_001_s3_ls_recursive");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

    let record_count = raw
        .lines()
        .skip(1)
        .filter(|l| regex::Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\s+\d+\s+\S").unwrap().is_match(l.trim_start()))
        .count();
    assert!(out.contains(&format!("[LS] {record_count} entries")), "必须产出与记录数一致的 entries 摘要头（{record_count}），实际输出前 300 字：{}", &out[..out.len().min(300)]);
    assert!(!out.contains("[LS] assets"), "方案 1 口径：不得产出目录组头");

    // 零丢失：输出侧逐行即 `<datetime> <size> <完整路径>`（单空格归一化），
    // 与输入侧（日期时间, 尺寸, 路径）三元组直接等价，无需字典/组头拼回。
    let entry_re =
        regex::Regex::new(r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) (\d+) (\S.*)$").unwrap();
    let header_re = regex::Regex::new(r"^\[LS\] \d+ entries$").unwrap();
    let mut out_triples: Vec<(String, String, String)> = Vec::new();
    for line in out.lines() {
        let line = line.trim_start();
        if header_re.is_match(line) {
            continue; // entries 摘要头
        }
        if let Some(c) = entry_re.captures(line) {
            out_triples.push((c[1].to_string(), c[2].to_string(), c[3].to_string()));
        }
    }

    let entry_re2 =
        regex::Regex::new(r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+(\d+)\s+(\S.*)$").unwrap();
    let mut in_triples: Vec<(String, String, String)> = Vec::new();
    for line in raw.lines().skip(1) {
        if let Some(c) = entry_re2.captures(line.trim_start()) {
            in_triples.push((c[1].to_string(), c[2].to_string(), c[3].trim_end().to_string()));
        }
    }
    assert_eq!(out_triples.len(), in_triples.len(), "记录数必须一致（零丢失）");
    let out_set: std::collections::BTreeSet<_> = out_triples.into_iter().collect();
    let in_set: std::collections::BTreeSet<_> = in_triples.into_iter().collect();
    let missing: Vec<_> = in_set.difference(&out_set).take(5).collect();
    assert!(out_set == in_set, "（日期时间, 尺寸, 路径）三元组必须逐条等价，缺失样例：{missing:?}");

    assert!(
        out.len() < raw.len(),
        "折叠后不得扩张：{} ≥ {}",
        out.len(),
        raw.len()
    );
}

/// 测试：非清单文本不被认领（负例守卫，防止抢 shell_session/generic_text 样本）。
#[test]
fn rejects_plain_text_without_columnar_structure() {
    let plugin = LsListingPlugin::new();
    for (dir, stem) in [
        ("cloud_log_plugin", "case_044_non_cloud_plain"),
        ("generic_text_plugin", "case_001_normal_text"),
    ] {
        let raw = read_sample_file(dir, &format!("{stem}.log"));
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "{dir}/{stem} 不应被 ls_listing 认领"
        );
    }
}
