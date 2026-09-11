use super::types::RustGoPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::keep_error_signal;
use bumpalo::Bump;
use regex::Regex;
use std::sync::{Arc, OnceLock};

/// 「cargo test --quiet」的进度点行正则：一串 `.`/状态字母（i=ignored/s=slow/u=E/F），
/// 可选尾随 ` N/M` 计数。如 `......i................. 87/1091` 或小套件的 `..`/`.`
/// （`{1,}` 允许单个点行，否则短点行导致 compress 早停、输出变长被 ROI 门控整体回退为原文）。
static DOT_PROGRESS_RE: OnceLock<Regex> = OnceLock::new();

/// 解析 `test result:` 权威计数（passed/failed/ignored）。
/// 如 `test result: ok. 1090 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 35.77s`
static TEST_RESULT_RE: OnceLock<Regex> = OnceLock::new();

/// `warning:` 诊断块的渲染层噪声行正则：纯 gutter 行（`|`）或 `|` + 纯 `^` 高亮行，
/// 如 `  |`、`  |                    ^^^^^^^^^^^^  ^^^^`。源码行（`5 | use ...`）因含代码不匹配。
static WARN_RENDER_NOISE_RE: OnceLock<Regex> = OnceLock::new();

/// Rust 编译错误码 `error[E0328]` 提取正则。`extract_error_code_stats` 热路径
/// 每次调用重建（P3-139，P3-127 家族扩展），提升为进程级预编译。
static ERROR_PATTERN_RE: OnceLock<Regex> = OnceLock::new();

#[inline]
fn warn_render_noise_re() -> &'static Regex {
    WARN_RENDER_NOISE_RE.get_or_init(|| {
        Regex::new(r"^\s*(\d+\s+)?\|[\s^]*$").expect("warn_render_noise 正则编译失败")
    })
}

#[inline]
fn dot_progress_re() -> &'static Regex {
    DOT_PROGRESS_RE.get_or_init(|| {
        Regex::new(r"^[.iIsSuUeEfF]{1,}(?:\s+\d+/\d+)?$").expect("dot_progress 正则编译失败")
    })
}

#[inline]
fn test_result_re() -> &'static Regex {
    TEST_RESULT_RE.get_or_init(|| {
        Regex::new(r"(\d+)\s+passed;\s+(\d+)\s+failed;\s+(\d+)\s+ignored")
            .expect("test_result 正则编译失败")
    })
}

/// 判断一行是否为「cargo test --quiet」的进度点行（reduce 点串密度，识别通过态测试输出）。
fn is_dot_progress(line: &str) -> bool {
    dot_progress_re().is_match(line.trim())
}

/// 从 `test result:` 行解析 (passed, failed, ignored) 计数，解析失败返回 None。
fn parse_test_result(line: &str) -> Option<(usize, usize, usize)> {
    let caps = test_result_re().captures(line)?;
    Some((
        caps.get(1)?.as_str().parse().ok()?,
        caps.get(2)?.as_str().parse().ok()?,
        caps.get(3)?.as_str().parse().ok()?,
    ))
}

/// 判断一行是否为 cargo/rust 测试块的**块首锚点** `running N tests`（复/单数皆可）。
///
/// 该行是权威的跨行块起点：其后必然跟随 N 行 `test <path> ... ok` 正文与 `test result:` 尾。
/// 提取为共享判定，供 [`RustGoPlugin::apply_advanced_compression`] 的折叠入口与
/// [`RustGoPlugin::detect`] 的锚点短路复用，避免两处口径漂移。
///
/// 要求紧随 `running ` 的是**十进制计数** + `tests`/`test`——与
/// [`RustGoPlugin::compress_cargo_test`] 的消费口径严格一致（后者同样 `strip_prefix("running ")`
/// 后按 `usize` 解析，解析失败即 `consumed = 0` 不消费）。旧实现用 `contains(" tests")` 宽松
/// 包含匹配，会把 `running unit tests for parser` 这类普通文本误判为块首；在锚点短路把命中
/// 提到满分后，该误判会直接抢走他插件样本（实证：`cloud_log_plugin/case_044_non_cloud_plain`）。
fn is_cargo_test_head(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("running ") else {
        return false;
    };
    let mut parts = rest.split_whitespace();
    let (Some(count), Some(unit)) = (parts.next(), parts.next()) else {
        return false;
    };
    matches!(unit, "tests" | "test") && count.parse::<usize>().is_ok()
}

impl RustGoPlugin {
    /// 创建 RustGoPlugin 实例（名称 rust_go，优先级 185），预编译 Rust/Go 诊断正则。
    pub fn new() -> Self {
        Self {
            name: "rust_go",
            priority: 185,
            rust_compile_pattern: Arc::new(
                Regex::new(r"^(?P<prefix>\s*-->\s*)(?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+)$")
                    .unwrap(),
            ),
            go_panic_pattern: Arc::new(
                Regex::new(r"^goroutine\s+(?P<id>\d+)\s+\[(?P<state>[^\]]+)\]:$").unwrap(),
            ),
            go_frame_pattern: Arc::new(
                // 兼容制表符与空格缩进的 Go 栈帧。真实 Go 运行时用 `\t`，但切片/样例常保留下
                // 沉缩进的空格版本（如 8 空格）。原 `^\t` 只匹配制表符，空格缩进帧漏检导致
                // 纯 panic 栈仅观 header 一处命中、ratio 卡在 0.15 阈值下而不 detect。
                Regex::new(
                    r"^\s+(?P<file>[^:]+):(?P<line>\d+)(?:\s+\+(?P<offset>0x[0-9a-fA-F]+))?$",
                )
                .unwrap(),
            ),
        }
    }
}

impl Default for RustGoPlugin {
    /// Default 实现：等价于 new()。
    fn default() -> Self {
        Self::new()
    }
}

impl RustGoPlugin {
    /// 应用高级压缩功能（Cargo 输出、测试输出）
    /// 遵循压缩协议 V1 法则 E（零容忍废话），参见 docs/development/PLUGIN_DEVELOPMENT.md
    #[tracing::instrument(level = "debug", skip_all)]
    fn apply_advanced_compression(&self, text: &str) -> String {
        let mut result = String::new();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // 功能 1: 折叠 Cargo 编译输出
            if line.trim().starts_with("Compiling ") {
                let (folded, consumed) = self.fold_cargo_compiling(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&folded);
                    i += consumed;
                    continue;
                }
            }

            // 功能 5: 压缩 Cargo warning 诊断块（折叠 `|`/`^` 渲染层隔行，保留消息/定位/源码/note）
            if line.trim_start().starts_with("warning:") {
                let (compressed, consumed) = self.compress_cargo_warning(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 3: 压缩 Cargo test 输出（复/单数 "running N tests" 或 "running N test"）
            if is_cargo_test_head(line) {
                let (compressed, consumed) = self.compress_cargo_test(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 4: 压缩 Go test 输出
            if line.starts_with("=== RUN   ") {
                let (compressed, consumed) = self.compress_go_test(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            result.push_str(line);
            result.push('\n');
            i += 1;
        }

        result
    }

    /// 功能 1: 折叠 Cargo 编译输出
    /// 输入: "   Compiling libc v0.2.139\n   Compiling cfg-if v1.0.0\n..."
    /// 输出: "[CARGO] Compiling 25 crates (details suppressed)\n"
    #[tracing::instrument(level = "debug", skip_all)]
    fn fold_cargo_compiling(&self, lines: &[&str]) -> (String, usize) {
        let mut count = 0;
        let mut consumed = 0;
        let mut finished_line = String::new();

        for line in lines {
            if line.trim().starts_with("Compiling ") {
                count += 1;
                consumed += 1;
            } else if line.trim().starts_with("Finished ") {
                finished_line = line.to_string();
                consumed += 1;
                break;
            } else {
                break;
            }
        }

        if count == 0 {
            return (String::new(), 0);
        }

        let mut result = format!("[CARGO] Compiling {} crates (details suppressed)\n", count);
        if !finished_line.is_empty() {
            result.push_str(&format!("[CARGO] {}\n", finished_line.trim()));
        }

        (result, consumed)
    }

    /// 功能 5: 压缩 Cargo warning 诊断块
    /// 输入: "warning: unused imports: ...\n --> src\\cli\\commands\\benchmark.rs:5:32\n  |\n5 | use crate::...;\n  |  ^^^^  ^^^^  ^^^^\n  |\n  = note: `#[warn(...)]` ...\n"
    /// 输出: 保留消息 + `-->` 定位 + 源码行（连行号，便于 readline 精确定位）+ `= note`，
    ///       丢弃纯渲染层噪声行（`|` gutter 与 `^` 高亮），LLM 用不到高亮装饰且浪费 token。
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_cargo_warning(&self, lines: &[&str]) -> (String, usize) {
        if !lines
            .first()
            .map_or(false, |l| l.trim_start().starts_with("warning:"))
        {
            return (String::new(), 0);
        }

        let mut result = String::new();
        let mut notes: Vec<String> = Vec::new();
        let mut consumed = 0;

        // 保留 warning 消息本体（去尾部空白）
        result.push_str(lines[0].trim_end());
        result.push('\n');
        consumed += 1;

        // 遍历定位/源码/note 段：丢弃渲染层噪声，规整保留语义行
        for line in &lines[1..] {
            if line.trim().is_empty() {
                break; // 段落到空行结束（paragraph 切片内亦不会含空行）
            }
            consumed += 1;
            let trimmed = line.trim_end();
            let body = trimmed.trim_start();

            // 丢弃纯 gutter/高亮行（`|` 或 `| ^^^`），源码行因含代码不匹配而保留
            if warn_render_noise_re().is_match(trimmed) {
                continue;
            }
            // 位置锚点 ` --> path:l:c`
            if body.starts_with("--> ") {
                result.push_str(trimmed);
                result.push('\n');
                continue;
            }
            // 诊断说明 `= note: ...` / `= help: ...`（先收集，最后统一输出）
            if body.starts_with("= note") || body.starts_with("= help") {
                notes.push(body.trim_start_matches('=').trim_start().to_string());
                continue;
            }
            // 其余即源码行（`NN | code`），连行号保留，LLM 可直接 readline 精确定位
            result.push_str(trimmed);
            result.push('\n');
        }

        for note in notes {
            result.push_str("  = ");
            result.push_str(&note);
            result.push('\n');
        }

        (result, consumed)
    }

    /// 功能 2: 提取错误码统计
    /// 输入: 包含多个 "error[E0425]" 的文本
    /// 输出: "[ERROR_STATS] E0425 occurred 3 times, E0308 occurred 2 times\n"
    /// 注意: 仅当有重复错误码时才输出统计（单次出现的错误不统计）
    #[tracing::instrument(level = "debug", skip_all)]
    fn extract_error_code_stats(&self, text: &str) -> String {
        use std::collections::HashMap;

        let error_pattern =
            ERROR_PATTERN_RE.get_or_init(|| Regex::new(r"error\[(?P<code>E\d+)\]").unwrap());
        let mut error_counts: HashMap<String, usize> = HashMap::new();

        for line in text.lines() {
            if let Some(caps) = error_pattern.captures(line) {
                let code = caps.name("code").unwrap().as_str();
                *error_counts.entry(code.to_string()).or_insert(0) += 1;
            }
        }

        // 仅保留出现 2 次及以上的错误码（单次出现的不统计）
        let repeated_errors: HashMap<String, usize> = error_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .collect();

        if repeated_errors.is_empty() {
            return String::new();
        }

        // 按出现次数降序排序
        let mut sorted: Vec<_> = repeated_errors.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        let mut stats: Vec<String> = sorted
            .iter()
            .map(|(code, count)| format!("error[{}] occurred {} times", code, count))
            .collect();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("error:") || trimmed.starts_with("For more information") {
                stats.push(trimmed.to_string());
            }
        }

        format!("[ERROR_STATS] {}\n", stats.join("; "))
    }

    /// 功能 3: 压缩 Cargo test 输出
    /// 输入: "running 120 tests\ntest tests::test_foo ... ok\n..."（详细）或
    ///       "running 120 tests\n......... 100/120\ntest result: ok. ...\n"（--quiet 点进格式）
    /// 输出: "[TEST] Running 120 tests\n[TEST] 110 passed, 3 failed, 1 ignored (details below)\n"
    ///
    /// 同时支持两套格式：详细格式逐行 `test xxx ... ok/FAILED` 计数；安静点进格式跳过
    /// 点行并从权威的 `test result:` 行解析 (passed/failed/ignored)。失败用例名统一保留。
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_cargo_test(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut failed_tests: Vec<String> = Vec::new();
        let mut totals: Option<usize> = None;
        let mut outcome: Option<(usize, usize, usize)> = None; // (passed, failed, ignored)

        // 第一行: "running N tests" / "running N test"
        if let Some(first_line) = lines.first() {
            for (prefix, suffix) in [("running ", " tests"), ("running ", " test")] {
                if let Some(rest) = first_line
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_suffix(suffix))
                {
                    if let Ok(num) = rest.trim().parse::<usize>() {
                        totals = Some(num);
                        consumed += 1;
                    }
                    break;
                }
            }
        }

        // 收集结果：支持详细格式（`test foo ... ok`）与安静点进格式（`.` 点行 + 权威 test result）。
        for line in &lines[consumed..] {
            let trimmed = line.trim();
            if is_dot_progress(trimmed) {
                // cargo --quiet 的进度点行，无增量信息，仅计数消耗
                consumed += 1;
                continue;
            }
            if line.starts_with("test ") {
                // 详细格式：`test xxx ... FAILED` 记录失败用例名
                if line.contains(" ... FAILED") {
                    if let Some(name) = line
                        .strip_prefix("test ")
                        .and_then(|s| s.split(" ... ").next())
                    {
                        failed_tests.push(name.to_string());
                    }
                }
                consumed += 1;
                continue;
            }
            if line.starts_with("failures:") {
                consumed += 1;
                continue;
            }
            if line.starts_with("test result:") {
                outcome = parse_test_result(line);
                consumed += 1;
                break;
            }
            if trimmed.is_empty() {
                consumed += 1;
                continue;
            }
            // 失败详情段（`    thread 'foo' panicked...` 等缩进块），跳至 test result 摘要
            if line.starts_with("    ") || line.starts_with("thread") {
                consumed += 1;
                continue;
            }
            break;
        }

        // 缺省计数：以失败用例换算（兼容无失败段落时直接使用 test result 的权威计数）。
        let total = totals
            .or_else(|| outcome.map(|(p, f, i)| p + f + i))
            .unwrap_or(0);
        let (passed, failed, ignored) = outcome.unwrap_or_else(|| {
            let failed = failed_tests.len();
            (total.saturating_sub(failed), failed, 0)
        });

        // 摘要行仅在**确有失败明细**时才声明 `(details below)`：全通过场景没有明细可列，
        // 原实现无条件追加该后缀，构成「承诺了不存在的明细」的误导性文本
        // （P3-206① 语义审计捕获：G5 rule 5/9 判定其隐藏信息）。
        let has_details = !failed_tests.is_empty();
        let mut result = format!("[TEST] Running {} tests\n", total);
        result.push_str(&format!(
            "[TEST] {} passed, {} failed, {} ignored{}\n",
            passed,
            failed,
            ignored,
            if has_details { " (details below)" } else { "" }
        ));
        // 保留失败的测试名称
        for test_name in failed_tests {
            result.push_str(&format!("test {} ... FAILED\n", test_name));
        }

        (result, consumed)
    }

    /// 功能 4: 压缩 Go test 输出
    /// 输入: "=== RUN   TestFoo\n--- PASS: TestFoo (0.00s)\n..."
    /// 输出: "[GO TEST] 45 passed, 1 failed (0.123s)\n"
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_go_test(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut failed_tests = Vec::new();
        let mut total_time = String::new();

        for line in lines {
            if line.starts_with("=== RUN   ") {
                consumed += 1;
            } else if line.starts_with("--- PASS: ") {
                passed += 1;
                consumed += 1;
            } else if line.starts_with("--- FAIL: ") {
                failed += 1;
                // 提取测试名称
                if let Some(test_name) = line
                    .strip_prefix("--- FAIL: ")
                    .and_then(|s| s.split(' ').next())
                {
                    failed_tests.push(test_name.to_string());
                }
                consumed += 1;
            } else if line.starts_with("PASS") || line.starts_with("FAIL") {
                consumed += 1;
                break;
            } else if line.starts_with("ok  \t") {
                // 提取总时间
                if let Some(time_str) = line.split_whitespace().last() {
                    total_time = time_str.to_string();
                }
                consumed += 1;
                break;
            } else if line.trim().is_empty() || line.starts_with("    ") {
                consumed += 1;
            } else {
                break;
            }
        }

        let mut result = format!("[GO TEST] {} passed, {} failed", passed, failed);
        if !total_time.is_empty() {
            result.push_str(&format!(" ({})", total_time));
        }
        result.push('\n');

        // 保留失败的测试
        for test_name in failed_tests {
            result.push_str(&format!(
                "=== RUN   {}\n--- FAIL: {}\n",
                test_name, test_name
            ));
        }

        (result, consumed)
    }
}

impl Plugin for RustGoPlugin {
    /// 返回插件名称 "rust_go"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 185。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：切片前 15 行内出现 cargo test 块首锚点 `running N tests` 时直接命中（最高置信度，
    /// P3-206①）；否则按同窗口内 error[E/panic:/note:/Rust 编译/Go panic 帧匹配占比 ≥15% 判定。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lines: Vec<&str> = slice.text.lines().take(15).collect();
        if lines.is_empty() {
            return None;
        }

        // 决定性锚点短路（P3-206①）：检测窗口内出现 cargo test 块首 `running N tests` 即直接
        // 判定归属。该锚点无歧义（libtest 独有；go test 用 `=== RUN`、pytest 用 collected），
        // 但其周围两类行对特征零贡献（verbose `test <path> ... ok` 正文行、whole-input 路径下
        // 的 `Compiling`/`Finished`/`Running` 头），会把锚点的 2 分稀释进 15 行窗口：
        //   · 段落切片路径（≥2KB）：块首锚点独占段落首行，但正文 2/15≈0.133 仍低于 0.15 阈值；
        //   · 整块路径（<2KB）：锚点在 header 之后（如第 5 行），同样被稀释。
        // 两路径均落到 `ratio < 0.15` 而不选中 rust_go，跨行折叠整体失效（产物≈原文）。
        // 更凶险的是并列歧义：sql 插件对该窗口同样给出 0.4（`tests::arith::insert` 等
        // 用例名撞上其 `\bINSERT\b` 头动词），而调度平局按 priority **升序** 破平
        // （sql=110 胜 rust_go=185，见 `PluginDispatcher::detect_parallel`），于是 rust_go
        // 反被 sql 抢走并把数字字面量抹成 `?`。返回 1.0 既避开稀释、又确保严格高于 sql 上限。
        if lines.iter().any(|l| is_cargo_test_head(l)) {
            return Some(1.0);
        }

        let mut match_count = 0;
        for line in &lines {
            // 强信号：编译/运行错误、警告、panic
            if line.starts_with("error[E")
                || line.starts_with("warning: ")
                || line.starts_with("panic: ")
            {
                match_count += 2;
            }
            // 强信号：cargo CLI 报错（命令级参数/子命令错误，无 rustc 错误码方括号）。
            // 样本形如 `error: unexpected argument 'content_analyzer' found`，
            // 常伴随 `Usage:`/`tip:`/`help` 提示行；ruster CLI 报错归 rust 生态，
            // 不命中 rustc 的 `error[E` 分支，须单独识别，否则被 gcc/shell 抢用而漏检。
            if line.starts_with("error:")
                || line.starts_with("error[E")
                || line.starts_with("Usage:")
                || line.starts_with("tip:")
                || line.contains("unexpected argument")
                || (line.starts_with("cargo ") && line.contains(" test"))
            {
                match_count += 2;
            }
            // 强信号：Cargo test 头/尾（`running N tests` / `test result:`）——
            // 即使全部通过（无错误信号），也应命中以便把点进输出折叠为摘要。
            // 头部判定复用 [`is_cargo_test_head`]（与折叠入口、锚点短路同源），不再就地重写
            // `contains(" test")` 宽松式——否则 `running unit tests for parser` 一类普通文本
            // 仍会靠该分支凑出 0.667 而抢走他插件样本。
            if is_cargo_test_head(line) || line.contains(" test result:") {
                match_count += 2;
            }
            // 强信号：cargo 纯构建/运行进度（无诊断错误）。`Compiling ` 行不带 error/warning/note，
            // 原检测无任何命中而无缘进入候选，被 smart_path(0.9) 抢占做轻量路径替换、丢失 [CARGO] 摘要。
            // `Finished ... ] target(s) in` 是 cargo 构建完成行，需特征化避免「Finished reading ...」误命中。
            let t_start = line.trim_start();
            if t_start.starts_with("Compiling ")
                || (t_start.starts_with("Finished ") && t_start.contains("] target(s) in"))
            {
                match_count += 2;
            }
            // 中信号：`cargo test --quiet` 进度点行（一长串 `.` + 可选计数）
            if is_dot_progress(line) {
                match_count += 1;
            }
            // 强信号：go 测试逐步输出（`=== RUN` / `--- PASS` / `--- FAIL`）——纯 go test 全通过
            // 输出无 error/warning/panic/goroutine，原检测零命中被 smart_path(0.9) 抢占。
            if line.starts_with("=== RUN")
                || line.starts_with("--- PASS")
                || line.starts_with("--- FAIL")
                || line.starts_with("PASS")
            {
                match_count += 2;
            }
            // 弱信号：note 诊断行（cargo 的 note 块常被切成独立短段落）
            if line.starts_with("note:") {
                match_count += 1;
            }
            if self.rust_compile_pattern.is_match(line)
                || self.go_panic_pattern.is_match(line)
                || self.go_frame_pattern.is_match(line)
            {
                match_count += 1;
            }
        }

        let ratio = match_count as f32 / lines.len() as f32;
        // 阈值 0.15：cargo 的 error/warning/note 块常被段落切片切碎（5~15 行），
        // 30% 的强信号要求会漏判；放宽后 note 块（note + --> 两行命中 = 2/5）也能命中。
        if ratio >= 0.15 {
            Some(ratio.min(1.0))
        } else {
            None
        }
    }

    /// 压缩切片：折叠 Cargo 编译/测试与 Go 测试输出，Rust/Go 帧路径字典化，
    /// 错误码统计摘要，keep_error_signal 与 ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 先应用高级压缩功能
        let preprocessed = self.apply_advanced_compression(text);

        let mut tokens: Vec<Token<'a>> = Vec::new();

        // 法则 A.2.1 修复：去掉 IR 标签，直接使用路径字典 + 冒号分隔符
        // 原格式：` --> src/main.rs:5:9`（19B）
        // 新格式：` --> $Pn:5:9`（约 13B，取决于路径字典 token 长度）
        // 不再使用 `$RG|R| --> |$Pn|5|9`（27B）的 IR 标签格式
        for line in preprocessed.lines() {
            if let Some(caps) = self.rust_compile_pattern.captures(line) {
                let file_token = dict_engine.add_path_layered(caps.name("file").unwrap().as_str());
                // 直接输出：前缀 + 路径token + :行:列
                tokens.push(Token::Text(
                    format!(
                        "{}{}:{}:{}\n",
                        caps.name("prefix").unwrap().as_str(),
                        file_token,
                        caps.name("line").unwrap().as_str(),
                        caps.name("col").unwrap().as_str()
                    )
                    .into(),
                ));
                continue;
            }
            if let Some(caps) = self.go_panic_pattern.captures(line) {
                // Go panic 行保持简洁格式，去掉 IR 标签
                tokens.push(Token::Text(
                    format!(
                        "goroutine {} [{}]:\n",
                        caps.name("id").unwrap().as_str(),
                        caps.name("state").unwrap().as_str()
                    )
                    .into(),
                ));
                continue;
            }
            if let Some(caps) = self.go_frame_pattern.captures(line) {
                let file_token = dict_engine.add_path_layered(caps.name("file").unwrap().as_str());
                let line_num = caps.name("line").unwrap().as_str();
                // Go 栈帧格式：\t文件:行 +偏移
                if let Some(offset) = caps.name("offset") {
                    tokens.push(Token::Text(
                        format!("\t{}:{} +{}\n", file_token, line_num, offset.as_str()).into(),
                    ));
                } else {
                    tokens.push(Token::Text(
                        format!("\t{}:{}\n", file_token, line_num).into(),
                    ));
                }
                continue;
            }
            // 丢弃渲染层噪声行：cargo 诊断块里以 `|` 开头（首非空白字符是 `|`）的
            // 区位符 / `^` 高亮 / 续接标记行，是编译器终端图层渲染产物（含前导几百空格），
            // 对 LLM 无增量信息，整行丢弃。
            // 注意 `  7 | use crate::...` 这类「行号 | 源码」行首字符是数字，不受影响，保留。
            if line.trim_start().starts_with('|') {
                continue;
            }
            tokens.push(Token::Text(format!("{}\n", line).into()));
        }

        // 法则 A ROI 门控：去掉 IR 标签后，压缩格式更紧凑。
        // 但小样本 / 单行 / 无字典命中场景下仍可能扩张，
        // 整段 prefer_non_expanding 回退原文。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § A.2.1。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();

        // 功能 2: 错误码统计作为附加价值信息
        // 在 ROI 门控之后添加，不影响压缩率计算
        let error_stats = self.extract_error_code_stats(text);
        let mut final_text = keep_error_signal(
            text,
            crate::core::utils::roi::prefer_non_expanding(text, compacted),
        );

        // 如果有错误统计且文本中包含错误，优先采用统计摘要；摘要仍需通过 ROI 门控。
        if !error_stats.is_empty() && text.contains("error[E") {
            let candidate = keep_error_signal(text, error_stats);
            final_text = crate::core::utils::roi::prefer_non_expanding(text, candidate);
        }

        CompressResult {
            tokens: vec![Token::Text(final_text.into())],
            metadata: None,
            plugin_name: Some(self.name),
        }
    }

    /// 解压：将压缩文本中的路径 token 用词典还原为原始 Rust/Go 格式。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut result = String::new();
        for line in compressed.lines() {
            // 新格式不再使用 IR 标签，直接是原始格式
            // 只需要解析路径 token 即可
            // 格式：` --> $Pn:5:9` 或 `\t$Pn:42 +0x123`

            // Rust 编译路径格式：` --> $Pn:line:col`
            if let Some(caps) = self.rust_compile_pattern.captures(line) {
                let file_str = caps.name("file").unwrap().as_str();
                let file = dict.resolve_or_self(file_str);
                result.push_str(&format!(
                    "{}{}:{}:{}\n",
                    caps.name("prefix").unwrap().as_str(),
                    file,
                    caps.name("line").unwrap().as_str(),
                    caps.name("col").unwrap().as_str()
                ));
                continue;
            }

            // Go panic 格式：goroutine N [state]:
            if let Some(caps) = self.go_panic_pattern.captures(line) {
                result.push_str(&format!(
                    "goroutine {} [{}]:\n",
                    caps.name("id").unwrap().as_str(),
                    caps.name("state").unwrap().as_str()
                ));
                continue;
            }

            // Go 栈帧格式：\t$Pn:line +offset
            if let Some(caps) = self.go_frame_pattern.captures(line) {
                let file_str = caps.name("file").unwrap().as_str();
                let file = dict.resolve_or_self(file_str);
                if let Some(offset) = caps.name("offset") {
                    result.push_str(&format!(
                        "\t{}:{} +{}\n",
                        file,
                        caps.name("line").unwrap().as_str(),
                        offset.as_str()
                    ));
                } else {
                    result.push_str(&format!(
                        "\t{}:{}\n",
                        file,
                        caps.name("line").unwrap().as_str()
                    ));
                }
                continue;
            }

            result.push_str(line);
            result.push('\n');
        }
        result
    }

    /// 返回后续插件列表（smart_path）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}
