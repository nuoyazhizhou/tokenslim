//! CI/CD 外壳日志插件方法实现。

use super::types::CiLogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, DocumentSkin, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::{
    compact_spaces, contains_any, decompress_with_dict, fallback_if_anchor_only, keep_error_signal,
    push_anchor,
};
use bumpalo::Bump;
use std::borrow::Cow;

#[derive(Clone, Debug)]
struct CiStep {
    name: String,
    lines: usize,
    errors: usize,
    warnings: usize,
    test_passed: usize,
    test_failed: usize,
    failed: bool,
}

#[derive(Default)]
struct CiStats {
    provider: &'static str,
    status: &'static str,
    steps: Vec<CiStep>,
    error_total: usize,
    errors: Vec<String>,
    warnings: usize,
    artifacts: usize,
    caches: usize,
    retries: usize,
}

impl CiLogPlugin {
    /// 创建 CiLogPlugin 实例（名称 ci_log，优先级 40）。
    pub fn new() -> Self {
        Self {
            name: "ci_log",
            priority: 40,
        }
    }
}

impl Plugin for CiLogPlugin {
    /// 返回插件名称 "ci_log"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 解包外壳：剥离 GitHub Actions 时间戳前缀与 Jenkins [Pipeline] 前缀，有剥离时返回新文本。
    fn unwrap(&self, text: &str) -> Option<String> {
        let mut out = String::with_capacity(text.len());
        let mut has_unwrapped = false;

        for line in text.split('\n') {
            let mut current = line;

            // Strip GitHub Actions timestamps: e.g. "2024-04-08T01:01:12.1141231Z "
            // 时间戳微秒位数不固定（28 / 29 / 更多位），按固定 [..28]/[29..] 切片会失效，故改为
            // 定位 `Z ` 分隔符剥离并校验 ISO 形态（前 4 位数字 + `-` + 前方含 `T`），兼容变长精度（Q510 处置）。
            // Q510 处置补充：`current[..4]` 对短行（无时间戳的普通行）会越界 panic，先校验长度。
            if current.len() >= 5
                && current[..4].chars().all(|c| c.is_ascii_digit())
                && current.as_bytes().get(4) == Some(&b'-')
            {
                if let Some(zidx) = current.find('Z') {
                    if current.as_bytes().get(zidx + 1) == Some(&b' ')
                        && current[..zidx]
                            .as_bytes()
                            .iter()
                            .rev()
                            .take(20)
                            .any(|&b| b == b'T')
                    {
                        current = &current[zidx + 2..];
                        has_unwrapped = true;
                    }
                }
            }

            // Strip Jenkins [Pipeline] prefixes
            if let Some(rest) = current.strip_prefix("[Pipeline] ") {
                current = rest;
                has_unwrapped = true;
            }

            out.push_str(current);
            out.push('\n');
        }

        if has_unwrapped {
            out.pop(); // Remove the last extra newline added by split
            Some(out)
        } else {
            None
        }
    }

    /// 文档级剥皮：把 CI 外壳骨架（步骤/分组/状态/CI 级错误标记）与内层工具输出分离。
    ///
    /// - 外壳摘要：对「皮行」跑 `compact_ci_log`，生成 CI|SUMMARY/STEP/ERROR 摘要
    ///   （含命令锚点，满足法则 0）；皮行不含 CI 决策信号时自然退化为仅锚点摘要。
    /// - 内层正文：非皮行（工具真实输出，如 gradle `> Task :`、测试失败、堆栈）
    ///   拼接，交内层管线重新识别/切片/定向。
    /// - 无内层正文（纯 CI 编排日志）返回 `None`，回退现状单切片路径，避免行为漂移。
    #[tracing::instrument(level = "debug", skip_all)]
    fn peel_document(&self, text: &str) -> Option<DocumentSkin> {
        let mut skin_lines = Vec::new();
        let mut inner_lines = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            let is_skin = i == 0 // 首行命令锚点留在皮侧，保证外壳摘要含锚点（法则 0）
                || parse_step_name(trimmed).is_some()
                || is_step_end(trimmed)
                || is_ci_skin_marker(trimmed);
            if is_skin {
                skin_lines.push(line);
            } else {
                inner_lines.push(line);
            }
        }

        if inner_lines.is_empty() {
            return None;
        }

        let skin_text = skin_lines.join("\n");
        Some(DocumentSkin {
            summary: compact_ci_log(&skin_text),
            inner_body: inner_lines.join("\n"),
        })
    }

    /// 返回插件优先级 40。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测切片是否含主流 CI 系统（GitHub/GitLab/Jenkins/Azure/CircleCI/Buildkite/TeamCity/Travis/自定义）特征，命中返回 0.94 置信度。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lower = slice.text.to_ascii_lowercase();
        contains_any(
            &lower,
            &[
                "::group::",
                "::endgroup::",
                "::error",
                "running with gitlab-runner",
                "section_start:",
                "section_end:",
                "[pipeline]",
                "##[section]",
                "##[error]",
                "circleci",
                "buildkite-agent",
                "##teamcity[",
                "travis_fold:",
                "travis_time:",
                "travis job",
                "[ci]",
                "[acme-ci]",
                "### step:",
                ">>> [",
                "process completed with exit code",
                "finished: failure",
                "finished: unstable",
                "error: job failed",
                "exited with code",
            ],
        )
        .then_some(0.94)
    }

    /// 压缩切片：剥离 ANSI 码，压缩 CI 日志为摘要行，保留错误信号并做 ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted = keep_error_signal(raw, compact_ci_log(&cleaned));
        let final_text = crate::core::utils::roi::prefer_non_expanding(raw, compacted);
        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：将压缩文本中的字典 token 用词典还原。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        decompress_with_dict(compressed, dict)
    }

    /// 返回后续插件列表（当前为空）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec![]
    }
}

/// 核心压缩逻辑：识别 provider、逐行统计 step/错误/警告/产物/缓存/重试，
/// 渲染 CI|SUMMARY/CI|STEP/!CI|ERROR 摘要行；压缩无收益时回退原文。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_ci_log(text: &str) -> String {
    let mut stats = CiStats {
        provider: detect_provider(text),
        status: "unknown",
        ..CiStats::default()
    };
    let mut current_step: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(step) = parse_step_name(trimmed) {
            current_step = Some(push_step(&mut stats.steps, step));
            continue;
        }

        if is_step_end(trimmed) {
            current_step = None;
            continue;
        }

        let idx = current_step.unwrap_or_else(|| push_step(&mut stats.steps, "job".to_string()));
        stats.steps[idx].lines += 1;

        // 测试结果行：提取通过/失败计数并记入当前步骤，随后跳过通用分类，
        // 避免其中 "N failed" 子串被误判为步骤失败（SAP-0002）。
        if let Some((passed, failed)) = parse_test_results(trimmed) {
            stats.steps[idx].test_passed = passed;
            stats.steps[idx].test_failed = failed;
            if failed > 0 {
                stats.steps[idx].failed = true;
                stats.error_total += 1;
                if stats.errors.is_empty() {
                    stats.errors.push(format!(
                        "s={} msg=test failed: passed={passed} failed={failed}",
                        stats.steps[idx].name
                    ));
                }
            }
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if is_cache_line(&lower) {
            stats.caches += 1;
        }
        if is_artifact_line(&lower) {
            stats.artifacts += 1;
        }
        if lower.contains("retrying")
            || lower.contains("re-running")
            || lower.contains("job retry")
            || lower.contains("retry attempt")
        {
            stats.retries += 1;
        }
        if is_warning_line(&lower) {
            stats.warnings += 1;
            stats.steps[idx].warnings += 1;
        }
        if is_error_line(&lower) {
            stats.error_total += 1;
            stats.steps[idx].errors += 1;
            stats.steps[idx].failed = true;
            if stats.errors.is_empty() {
                stats.errors.push(format!(
                    "s={} msg={}",
                    stats.steps[idx].name,
                    clean_ci_msg(trimmed)
                ));
            }
        }

        if let Some(status) = parse_status(&lower) {
            stats.status = status;
            if status == "failed" {
                stats.steps[idx].failed = true;
            }
        }
    }

    if stats.status == "unknown" {
        stats.status = if stats.error_total == 0 {
            "success"
        } else {
            "failed"
        };
    }

    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let step_count = stats.steps.len();
    lines.push(format!(
        "CI|SUMMARY|provider={} status={} steps={} errors={} warn={} art={} cache={} retry={}",
        stats.provider,
        stats.status,
        step_count,
        stats.error_total,
        stats.warnings,
        stats.artifacts,
        stats.caches,
        stats.retries
    ));

    let has_signal_steps = stats
        .steps
        .iter()
        .any(|step| step.failed || step.errors > 0 || step.warnings > 0);
    let selected_steps = stats
        .steps
        .iter()
        .filter(|step| !has_signal_steps || step.failed || step.errors > 0 || step.warnings > 0);
    for step in selected_steps.take(6) {
        let status = if step.failed { "failed" } else { "ok" };
        let mut line = format!(
            "CI|STEP|n={} st={} l={} e={} w={}",
            step.name, status, step.lines, step.errors, step.warnings
        );
        // 附上单个测试结果证据（通过/失败），支撑审计与问题定位（SAP-0002）。
        if step.test_passed > 0 || step.test_failed > 0 {
            line.push_str(&format!(" t={} f={}", step.test_passed, step.test_failed));
        }
        lines.push(line);
    }
    for error in stats.errors {
        lines.push(format!("!CI|ERROR|{}", error));
    }

    fallback_if_anchor_only(lines, text)
}

/// 根据文本特征识别 CI 提供商（github_actions/gitlab_ci/jenkins/azure_pipelines/...）。
#[tracing::instrument(level = "debug", skip_all)]
fn detect_provider(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if contains_any(
        &lower,
        &["github actions", "::group::", "::error", "gh run view"],
    ) {
        "github_actions"
    } else if contains_any(&lower, &["running with gitlab-runner", "section_start:"]) {
        "gitlab_ci"
    } else if contains_any(&lower, &["[pipeline]", "finished: failure", "jenkins"]) {
        "jenkins"
    } else if contains_any(&lower, &["##[section]", "##[error]", "azure pipelines"]) {
        "azure_pipelines"
    } else if contains_any(&lower, &["circleci", "circleci received exit code"]) {
        "circleci"
    } else if contains_any(&lower, &["buildkite-agent", "buildkite", "^^^ +++"]) {
        "buildkite"
    } else if contains_any(&lower, &["act -j", "[build/"]) {
        "act"
    } else if contains_any(&lower, &["##teamcity["]) {
        "teamcity"
    } else if contains_any(&lower, &["travis_fold:", "travis_time:", "travis job"]) {
        "travis_ci"
    } else if contains_any(&lower, &["[acme-ci]", "### step:", ">>> [", "[ci]"]) {
        "custom_ci"
    } else {
        "ci"
    }
}

/// 将步骤加入列表：同名步骤复用已有条目（取最近一个），否则新建并返回索引。
#[tracing::instrument(level = "debug", skip_all)]
fn push_step(steps: &mut Vec<CiStep>, name: String) -> usize {
    if let Some((idx, _)) = steps
        .iter()
        .enumerate()
        .rev()
        .find(|(_, step)| step.name == name)
    {
        return idx;
    }
    steps.push(CiStep {
        name,
        lines: 0,
        errors: 0,
        warnings: 0,
        test_passed: 0,
        test_failed: 0,
        failed: false,
    });
    steps.len() - 1
}

/// 从行中解析步骤名（支持 ::group::、##[group]、##[section]、section_start:、
/// ##teamcity[、travis_fold:、### Step: 等各 CI 系统的步骤标记）。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_step_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("::group::") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("##[group]") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("##[section]Starting:") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("##[section]") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("--- ") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("+++ ") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("[Pipeline] { (") {
        return Some(clean_step_name(rest.trim_end_matches(')')));
    }
    if trimmed.starts_with("section_start:") {
        let after_marker = trimmed
            .split_once(']')
            .map(|(_, rest)| rest)
            .unwrap_or(trimmed)
            .trim();
        if !after_marker.is_empty() {
            return Some(clean_step_name(after_marker));
        }
        let parts = trimmed.split(':').collect::<Vec<_>>();
        if parts.len() >= 3 {
            return Some(clean_step_name(
                parts[2].split('[').next().unwrap_or(parts[2]),
            ));
        }
    }
    if trimmed.starts_with("##teamcity[blockOpened") {
        if let Some(name) = extract_service_value(trimmed, "name") {
            return Some(clean_step_name(&name));
        }
    }
    if trimmed.starts_with("##teamcity[compilationStarted") {
        if let Some(name) = extract_service_value(trimmed, "compiler") {
            return Some(clean_step_name(&format!("compile {}", name)));
        }
        return Some("compile".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("travis_fold:start:") {
        return Some(clean_step_name(rest));
    }
    if let Some((_, rest)) = trimmed.split_once("### Step:") {
        return Some(clean_step_name(rest));
    }
    if let Some((_, rest)) = trimmed.split_once("[ci] step:") {
        return Some(clean_step_name(rest));
    }
    if let Some((_, rest)) = trimmed.split_once("[ACME-CI] STEP ") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix(">>> [") {
        if let Some((name, _)) = rest.split_once(']') {
            return Some(clean_step_name(name));
        }
    }
    if trimmed.starts_with("Run ") && trimmed.len() < 120 {
        return Some(clean_step_name(trimmed.trim_start_matches("Run ")));
    }
    None
}

/// 判断行是否为步骤结束标记（::endgroup::、section_end:、blockClosed 等）。
fn is_step_end(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "::endgroup::"
        || trimmed.starts_with("section_end:")
        || trimmed.starts_with("##teamcity[blockClosed")
        || trimmed.starts_with("##teamcity[compilationFinished")
        || trimmed.starts_with("travis_fold:end:")
        || trimmed.starts_with("##[endgroup]")
        || trimmed.starts_with("##[section]Finishing:")
        || trimmed == "[Pipeline] }"
}

/// 判断行是否为 CI 外壳骨架标记（编排结构，非工具输出）：步骤/分组/状态/CI 级错误标记。
///
/// 与 [`compact_ci_log`] 的步骤识别对齐，但仅捕获「编排骨架」——gradle `> Task :`、
/// 测试失败、堆栈等工具真实输出不在此列，从而剥皮后内层正文干净可定向。
/// 注意：本函数在 `unwrap` 前（原始文本）调用，Jenkins `[Pipeline]` 前缀仍存在。
fn is_ci_skin_marker(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower.starts_with("::") // GitHub ::group::/::error::/::warning::
        || lower.starts_with("##[") // Azure/GH ##[section]/##[group]/##[error]
        || lower.starts_with("section_start:")
        || lower.starts_with("section_end:")
        || lower.starts_with("##teamcity[")
        || lower.starts_with("travis_fold:")
        || lower.starts_with("travis_time:")
        || lower.starts_with("[pipeline]") // Jenkins 骨架（unwrapped 前的原始前缀）
        || lower.starts_with("finished:")
        || lower.starts_with("process completed with exit code")
        || lower.starts_with("error: script returned")
        || lower.starts_with("error: job failed")
        || lower.starts_with("running with gitlab-runner")
        || lower.starts_with("runner image:")
        || lower.starts_with("preparing workflow directory")
        || lower.starts_with("### step:")
        || lower.starts_with(">>> [")
        || lower.starts_with("[ci] step:")
        || lower.starts_with("[acme-ci]")
        || lower.starts_with("circleci")
        || lower.starts_with("buildkite-agent")
}

/// 判断行是否涉及缓存操作（restore/save/store cache 等）。
fn is_cache_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "restore cache",
            "restoring cache",
            "saving cache",
            "save cache",
            "cache hit",
            "cache miss",
            "setting up build cache",
            "store cache",
        ],
    )
}

/// 判断行是否涉及产物操作（upload/download/publish artifact 等）。
fn is_artifact_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "uploading artifact",
            "uploading artifacts",
            "download artifact",
            "downloading artifact",
            "store_artifacts",
            "artifacts uploaded",
            "publish artifacts",
            "publishing artifacts",
        ],
    )
}

/// 判断行是否为警告（::warning、##[warning]、warning: 等）。
fn is_warning_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "::warning",
            "##[warning]",
            "warning:",
            "warn ",
            "status='warning'",
        ],
    )
}

/// 判断行是否为错误：先排除 "0 failed"/"errors: 0" 等无错误统计行，
/// 再匹配 ::error、failed、exception、exit code 1 等错误特征。
fn is_error_line(lower: &str) -> bool {
    if contains_any(
        lower,
        &[
            "failures: 0",
            "failure: 0",
            "errors: 0",
            "failed: 0",
            "0 failed",
            "0 failures",
            "0 errors",
        ],
    ) {
        return false;
    }
    contains_any(
        lower,
        &[
            "::error",
            "##[error]",
            "error:",
            "failed",
            "failure",
            "exception",
            "exited with code",
            "exit code 1",
            "finished: failure",
            "buildproblem",
            "status='error'",
            "status='failure'",
            "errored",
            "the command",
        ],
    )
}

/// 从 `test result: ok. N passed; M failed; ...` 行解析测试通过数与失败数。
/// 未命中或非测试结果行返回 None；其中的 "N failed" 是计数而非状态，需单独处理（SAP-0002）。
#[tracing::instrument(level = "trace", skip_all)]
fn parse_test_results(line: &str) -> Option<(usize, usize)> {
    let lower = line.to_ascii_lowercase();
    let start = lower.find("test result:")?;
    let rest = &lower[start + "test result:".len()..];
    let passed = parse_count_before_keyword(rest, "passed")?;
    let failed = parse_count_before_keyword(rest, "failed").unwrap_or(0);
    Some((passed, failed))
}

/// 取关键字前最近的一个十进制整数，用于解析 "N passed" / "M failed" 形式的计数。
#[tracing::instrument(level = "trace", skip_all)]
fn parse_count_before_keyword(text: &str, keyword: &str) -> Option<usize> {
    let pos = text.find(keyword)?;
    text[..pos]
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse::<usize>().ok())
        .next_back()
}

/// 从行中解析任务最终状态（failed/success/unstable/canceled），未命中返回 None。
fn parse_status(lower: &str) -> Option<&'static str> {
    if contains_any(
        lower,
        &[
            "process completed with exit code 1",
            "error: job failed",
            "finished: failure",
            "failed",
            "exited with code 1",
            "exit status 1",
            "travis job failed",
            "buildstatus status='failure'",
            "status='failure'",
        ],
    ) {
        Some("failed")
    } else if contains_any(
        lower,
        &[
            "process completed with exit code 0",
            "finished: success",
            "job succeeded",
            "success",
            "travis job succeeded",
            "buildstatus status='success'",
            "status='success'",
        ],
    ) {
        Some("success")
    } else if contains_any(lower, &["finished: unstable", "unstable"]) {
        Some("unstable")
    } else if contains_any(lower, &["canceled", "cancelled"]) {
        Some("canceled")
    } else {
        None
    }
}

/// 清理步骤名：压缩连续空白、去除引号/括号/冒号等包裹符，空名回退 "step" 并截断。
fn clean_step_name(name: &str) -> String {
    let cleaned = compact_spaces(name)
        .trim_matches(|ch| matches!(ch, ':' | '"' | '\'' | '[' | ']' | '(' | ')'))
        .to_string();
    if cleaned.is_empty() {
        "step".to_string()
    } else {
        truncate(&cleaned, 64)
    }
}

/// 清理错误消息：剥离 ::error::、##[error]、ERROR: 等前缀并截断。
fn clean_ci_msg(line: &str) -> String {
    let mut msg = line
        .replace("::error::", "")
        .replace("##[error]", "")
        .replace("ERROR:", "")
        .replace("Error:", "");
    if let Some(text) = extract_service_value(&msg, "text") {
        msg = text;
    }
    if let Some((_, rest)) = msg.split_once("::") {
        msg = rest.to_string();
    }
    truncate(&compact_spaces(&msg), 64)
}

/// 从 TeamCity 服务消息中提取 key='value' 形式的属性值。
#[tracing::instrument(level = "debug", skip_all)]
fn extract_service_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("{}='", key);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

/// 将文本截断至指定字符数，超长时追加省略号。
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}
