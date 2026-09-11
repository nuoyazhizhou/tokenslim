//! P3-206① 回归：≥2048B 大输入 cargo test verbose 输出必须跨行折叠为 [TEST] 摘要。
//!
//! 缺陷不在插件 `compress` 层（插件级整块压缩一直都能折叠），而在**插件选择（detect）层**：
//! `rust_go::detect` 的「前 15 行特征占比 ≥ 0.15」口径会被块首锚点周围的零贡献行稀释——
//!   · 段落切片路径（≥2048B）：锚点 `running N tests` 独占段落首行，但其后 verbose
//!     `test <path> ... ok` 正文行零贡献，锚点仅 2 分 → 2/15 ≈ 0.133 < 0.15；
//!   · 整块路径（<2048B）：锚点前还有 `Compiling`/`Finished`/`Running` 头行，
//!     同样稀释到阈值之下，且 sql 插件对该窗口并列给出 0.4（用例名撞 `\bINSERT\b`），
//!     调度平局按 priority 升序破平 → sql(110) 反抢 rust_go(185)，把数字抹成 `?`。
//! 两种情况 rust_go 都不被选中 → 产物 ≈ 原文（ratio ≈ 1.0）。
//!
//! 修复：`rust_go::detect` 把检测窗口内的块首锚点 `running N tests` 登记为**决定性短路**
//! （返回 1.0），不再参与比例稀释，也不再给对手留平局机会。
//!
//! 样本一律物理加载 `samples/rust_go_plugin/`（压缩协议致命红线 2：禁止手写 mock 字符串）。

use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};

/// 物理样本路径（相对 `CARGO_MANIFEST_DIR`）。
const SAMPLE: &str = "samples/rust_go_plugin/case_020_cargo_test_verbose_large.log";

/// 小输入整块阈值，与 `compression_pipeline::methods` 的 `SMALL_INPUT_WHOLE_THRESHOLD` 对齐。
const SMALL_INPUT_WHOLE_THRESHOLD: usize = 2048;

/// 物理加载样本（`std::fs::read_to_string`，禁止手写 mock）。
fn read_sample() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取物理样本 {} 失败: {e}", path.display()))
}

/// 构造禁用全部指标采集的 MetricsCollector（测试无采集开销）。
fn metrics() -> MetricsCollector {
    MetricsCollector::new(MetricsConfig {
        enabled: false,
        enable_module_timing: false,
        enable_plugin_stats: false,
        enable_error_logging: false,
        max_error_logs: 0,
    })
}

/// 用默认流水线配置 + 全量插件链压缩给定文本，返回拼接后的 Text token 与压缩率。
fn compress_with_full_chain(text: &str) -> (String, f32) {
    let config = PipelineConfig::default();
    let mut pipeline =
        CompressionPipeline::new(config, tokenslim::cli::get_plugins(), metrics());
    let out = pipeline.compress_str(text).expect("compress_str 失败");
    let joined: String = out
        .tokens
        .iter()
        .filter_map(|t| match t {
            tokenslim::core::compression::Token::Text(c) => Some(c.as_ref()),
            _ => None,
        })
        .collect();
    (joined, out.metadata.compression_ratio)
}

/// 按行边界截断到不超过 `limit` 字节（内容仍取自物理样本，非手写 mock）。
fn truncate_at_line_boundary(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if out.len() + line.len() + 1 > limit {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// 主回归：≥2048B 的物理样本必须经调度层折叠，而不是原样透传。
#[test]
fn large_cargo_test_verbose_sample_folds_through_pipeline() {
    let raw = read_sample();
    assert!(
        raw.len() >= SMALL_INPUT_WHOLE_THRESHOLD,
        "样本须为 ≥{SMALL_INPUT_WHOLE_THRESHOLD}B 大输入，实际 {}B",
        raw.len()
    );

    let (joined, ratio) = compress_with_full_chain(&raw);

    assert!(
        joined.contains("[TEST] Running 56 tests"),
        "≥2KB cargo test 必须折叠出 [TEST] 摘要（P3-206①），实际输出：{joined}"
    );
    assert!(
        joined.contains("55 passed"),
        "权威计数必须保留在摘要中，实际输出：{joined}"
    );
    assert!(
        !joined.contains("test arith::adds_two_positive_integers ... ok"),
        "逐行通过用例必须被折叠丢弃，实际输出：{joined}"
    );
    assert!(
        joined.contains("lexer::reports_unterminated_block_comment"),
        "失败用例名必须保留（Anti-Amnesia），实际输出：{joined}"
    );
    assert!(
        joined.contains("error: test failed"),
        "cargo 失败尾部信号必须保留，实际输出：{joined}"
    );
    assert!(
        ratio < 0.5,
        "折叠后压缩率须显著下降（修复前 ≈0.998），实际 {ratio}"
    );
}

/// 边界回归：小输入整块阈值两侧（2047/2048/2049B）都必须折叠。
///
/// 修复前两条路径都失效（各自被稀释到 detect 阈值之下，整块路径还被 sql 平局抢走），
/// 修复后两侧行为一致——锚点短路与路径无关，故阈值不构成行为分界。
#[test]
fn cargo_test_block_folds_on_both_sides_of_small_input_threshold() {
    let raw = read_sample();

    for limit in [
        SMALL_INPUT_WHOLE_THRESHOLD - 1,
        SMALL_INPUT_WHOLE_THRESHOLD,
        SMALL_INPUT_WHOLE_THRESHOLD + 1,
    ] {
        let input = truncate_at_line_boundary(&raw, limit);
        assert!(
            !input.is_empty(),
            "截断到 {limit}B 得到空输入，样本结构不满足本测试前提"
        );
        assert!(
            input.contains("running 56 tests"),
            "截断到 {limit}B 后须仍含块首锚点，否则该点未覆盖跨行折叠场景"
        );

        let (joined, _) = compress_with_full_chain(&input);
        assert!(
            joined.contains("[TEST] Running 56 tests"),
            "{limit}B 输入必须折叠（阈值两侧行为一致），实际输出：{joined}"
        );
    }
}
