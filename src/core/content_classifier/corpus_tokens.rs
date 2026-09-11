// 语料分词与噪声过滤（共享管线，T-B 特征聚合器 / T-E 增量回填共用）
//
// 本模块是「分词 + 无区分度噪声过滤」的唯一权威实现，同时被两处复用：
// - 运行期分类器（model 词表构建、feature_reader 误路由样本回填）；
// - 编译期特征聚合器（build.rs feature_builder）——因 build.rs 是独立编译单元，
//   无法直接引 crate，故用
//   `include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/core/content_classifier/corpus_tokens.rs"))`
//   原样内联本文件，保证两侧分词/滤噪逻辑永远一致，杜绝双份定义漂移。
//
// 一致性约束：修改本文件的 tokenize / is_noise_word 时，build.rs 亦自动生效，无需额外同步。
//
// 注意：头部文档注释故意用普通 `//` 而非 `//!`。因为该文件会被 build.rs 原样
// `include!`，若用 `//!` 内联到 build.rs 中（其前方已有 items），会触发 E0753
// 「inner doc comment 只能出现在 items 之前」。故此处一律用非文档注释。

/// 与运行期分类器一致的分词：非字母数字作分隔、保留长度 ≥ 2、**先小写再滤噪声**、返回词元。
/// 注意必须先 `to_ascii_lowercase()` 再做噪声判断，否则大写的功能词（如 "The"）会绕过
/// 大小写敏感的噪声匹配，被当作语料特征误收。
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = tokenize_unigram(text);
    // 判别性复合标记：在 unigram 之外附加命中的复合 token（F-2.3）。
    append_discriminative_phrases(text, &mut out);
    out
}

/// 基础 unigram 分词（见 [`tokenize`] 的整体语义）。
fn tokenize_unigram(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !is_noise_word(w))
        .collect()
}

/// 判别性复合标记：高信号、强类别专属的短语，分词时作为**整体 token** 追加产出。
///
/// 背景：朴素贝叶斯分类对 `Starting test execution` / `Updating (prod)` 这类复合
/// 判别信号，若只用 unigram 会被拆成 start/test/execution 等泛词或噪声词，导致跨类
/// 竞争失败（如 pulumi up 误判 cloud_log、dotnet test 误判 pytest）。本表把这类短语
/// 作为单一 token 并入词流，使 `build.rs` 聚合与运行期 `classify` 两端用同一分词即可
/// 命中对应类别的高信号复合词，从而以词表增加、不换寻址的方式提升判别力。
///
/// 选择约束：短语须强类别专属、跨类不冲突，避免污染其他类别或引发回归。
const DISCRIMINATIVE_PHRASES: &[&str] = &[
    // pulumi（up/refresh/preview 头部与类型前缀）
    "updating (",
    "previewing update",
    "pulumi:",
    // dotnet（dotnet test 命令与输出头部）
    "dotnet test",
    "test run for",
    "starting test execution",
];

/// 依原文（小写化）判定哪些复合标记命中，去重并追加到 token 流尾部。
fn append_discriminative_phrases(text: &str, out: &mut Vec<String>) {
    if text.is_empty() {
        return;
    }
    let lower = text.to_ascii_lowercase();
    for phrase in DISCRIMINATIVE_PHRASES {
        if lower.contains(phrase) && !out.iter().any(|t| t == *phrase) {
            out.push((*phrase).to_string());
        }
    }
}

/// 判定是否为「无区分度」噪声词，命中则不进特征表：
/// - 纯数字（行号/计数器，无语义）；
/// - 英文功能词（the/to/of 等，跨类别通用）；
/// - 常见路径根 / 通用构建词（home/usr/build/output 等，出现在各类工具输出，不具类别特异性）；
/// - 占位符标识符（foo/bar/baz 等，任何样本都可能出现）。
pub fn is_noise_word(word: &str) -> bool {
    if word.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    matches!(
        word,
        // 英文停用词/功能词
        "the" | "and" | "for" | "with" | "this" | "that" | "from" | "have" | "has" | "was"
            | "were" | "are" | "not" | "but" | "you" | "your" | "they" | "them" | "will"
            | "a" | "an" | "at" | "as" | "by" | "in" | "is" | "it" | "on" | "or" | "be"
            | "we" | "our" | "their" | "its" | "do" | "does" | "did" | "can" | "could"
            | "would" | "should" | "may" | "might" | "must" | "all" | "any" | "each"
            | "some" | "such" | "than" | "then" | "there" | "when" | "where" | "which"
            | "while" | "who" | "why" | "into" | "onto" | "upon" | "about" | "after"
            | "again" | "against" | "before" | "between" | "both" | "down" | "ever"
            | "every" | "few" | "how" | "just" | "more" | "most" | "much" | "now" | "only"
            | "over" | "own" | "same" | "so" | "too" | "under" | "until" | "up"
            | "very" | "what" | "yes" | "yet" | "off" | "out" | "via" | "e" | "g"
        // 常见路径根 / 通用构建词（跨工具链出现，无类别特异性）
            | "home" | "user" | "usr" | "root" | "var" | "etc" | "opt" | "tmp" | "bin"
            | "lib" | "sbin" | "dev" | "proc" | "sys" | "log" | "logs" | "file" | "files"
            | "path" | "paths" | "dir" | "dirs" | "directory" | "directories" | "folder"
            | "src" | "target" | "build" | "builds" | "output" | "outputs" | "cmd" | "args"
            | "argument" | "arguments" | "option" | "options" | "flag" | "flags" | "line"
            | "lines" | "number" | "code" | "info" | "debug" | "error" | "fail" | "failed"
            | "failure" | "errors" | "warn" | "warnings" | "warning" | "note" | "help"
            | "status" | "value" | "values" | "result" | "results" | "done" | "process"
            | "processes" | "start" | "stop" | "run" | "running" | "time" | "sec" | "min"
            | "total" | "count" | "main" | "default" | "example" | "examples" | "general"
        // 占位符 / 样例标识符
            | "foo" | "bar" | "baz" | "qux" | "hello" | "world" | "foobar" | "sample"
            | "samplee" | "dummy" | "testfoo"
    )
}
