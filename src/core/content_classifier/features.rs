//! content classifier 种子特征表
//!
//! 内置一组手工整理的「类别 → 单词 → 出现次数」特征表，作为朴素贝叶斯分类器的
//! 初始模型（`seed_model`）。特征词按语义类别挑选，侧重各类别的高区分度词；
//! 重叠词（如 error/warning）由分类器的平滑与 softmax 机制自动权衡。
//!
//! 除手工种子外，编译期特征聚合器（build.rs feature_builder，见计划 T-B）会扫描
//! `samples/` 纯类样板 `.log` 语料，聚合出补充特征并经 [`merge_generated_features`]
//! 以加法方式合入，增强分类器对真实语料高频词的感知；
//! 语料缺失/生成失败时自动降级为纯种子（`CORPUS_FEATURES_AVAILABLE = false`）。

use super::model::{Category, NaiveBayesClassifier};
use std::collections::HashMap;
use std::sync::OnceLock;

// 编译期（build.rs feature_builder，计划 T-B）聚合生成的语料特征。
// 文件内容由 `build.rs` 扫描 `samples/` 纯类样板 `.log` 语料生成；内容不存在或
// 生成失败时不报错，而是提供 `CORPUS_FEATURES_AVAILABLE = false` 的空桩，运行期退回纯种子。
include!(concat!(env!("OUT_DIR"), "/features_generated.rs"));

/// 内置种子模型：以默认拉普拉斯平滑系数构建朴素贝叶斯分类器，
/// 并在种子特征之上合入编译期聚合的语料特征（增强词表），
/// 最后合并运行期持久化的增量特征库（计划 T-E）。任一环节不可用即降级，不阻断初始化。
pub fn seed_model() -> NaiveBayesClassifier {
    let mut model = NaiveBayesClassifier::from_features(seed_features(), 1.0);
    // 合并增量特征库：读取失败/为空时静默跳过，保持 Fail-Soft。
    let lib = super::feature_reader::load(&super::feature_reader::default_feature_lib_path());
    match lib {
        Ok(table) => super::feature_reader::merge_into_model(&mut model, &table),
        Err(e) => {
            tracing::debug!("增量特征库合并跳过（{e}），使用种子+语料特征");
        }
    }
    // F-3：词表至此定型，一次性预计算对数似然查表，classify 走查表路径消除逐 token 的 ln()。
    model.build_likelihood_table();
    model
}

/// 全局惰性单例分类器（F-1 性能优化）。
///
/// 首次访问时由 [`seed_model`] 构建一次，之后全程只读共享，避免 `bayesian_fallback` /
/// `document_category` 每个 slice 都重建模型的高频开销。构建期已合入
/// 种子 + 语料 + 持久化增量特征，运行期主链路仅调用只读 [`NaiveBayesClassifier::classify`]，
/// 无运行期 `append_features`，故用只读 `OnceLock` 即可。
#[tracing::instrument(level = "trace", skip_all)]
pub fn classifier() -> &'static NaiveBayesClassifier {
    static CLASSIFIER: OnceLock<NaiveBayesClassifier> = OnceLock::new();
    CLASSIFIER.get_or_init(seed_model)
}

/// 构造种子特征表（`Category::GenericText` 为空表，作为兜底类别），并合并语料特征。
/// `pub(crate)` 供守护测试（P2-26）遍历两表对账。
pub(crate) fn seed_features() -> HashMap<Category, HashMap<String, f64>> {
    let mut table: HashMap<Category, HashMap<String, f64>> = HashMap::new();
    table.insert(Category::Cargo, cargo_words());
    table.insert(Category::Gcc, gcc_words());
    table.insert(Category::Test, test_words());
    table.insert(Category::GitDiff, git_diff_words());
    table.insert(Category::DockerK8s, docker_k8s_words());
    table.insert(Category::Node, node_words());
    table.insert(Category::Web, web_words());
    table.insert(Category::Java, java_words());
    table.insert(Category::SpringBoot, spring_boot_words());
    table.insert(Category::Maven, maven_words());
    table.insert(Category::PhpRuby, php_ruby_words());
    table.insert(Category::Dotnet, dotnet_words());
    table.insert(Category::Helm, helm_words());
    table.insert(Category::Terraform, terraform_words());
    table.insert(Category::Golang, golang_words());
    table.insert(Category::WebLog, web_log_words());
    table.insert(Category::PythonTraceback, python_traceback_words());
    table.insert(Category::Bazel, bazel_words());
    table.insert(Category::Gradle, gradle_words());
    table.insert(Category::Xcode, xcode_words());
    table.insert(Category::Ansible, ansible_words());
    table.insert(Category::Pulumi, pulumi_words());
    table.insert(Category::CloudFormation, cloudformation_words());
    table.insert(Category::Sql, sql_words());
    table.insert(Category::DbLog, db_log_words());
    table.insert(Category::UnityUnreal, unity_unreal_words());
    table.insert(Category::Syslog, syslog_words());
    table.insert(Category::CiLog, ci_log_words());
    table.insert(Category::CloudLog, cloud_log_words());
    table.insert(Category::GenericText, HashMap::new());
    merge_generated_features(&mut table);
    table
}

/// 将编译期聚合的语料特征以「加法计数」方式合入种子特征表。
///
/// 语义：语料特征作为在种子之上的补充证据（权重 1.5~8.0，与种子同量级），
/// 增强分类器对真实语料高频词的感知，同时保留种子特征对类别边界的约束，降低回归风险。
/// 生成不可用时（`CORPUS_FEATURES_AVAILABLE = false`）该合并为空操作。
fn merge_generated_features(table: &mut HashMap<Category, HashMap<String, f64>>) {
    if !CORPUS_FEATURES_AVAILABLE {
        return;
    }
    for (cat, word, weight) in corpus_generated_features() {
        let entry = table
            .entry(cat)
            .or_default()
            .entry(word.to_string())
            .or_insert(0.0);
        *entry += weight;
    }
}

/// 小工具：从「词, 次数」元组数组构造词频映射。
fn make(words: &[(&str, f64)]) -> HashMap<String, f64> {
    words.iter().map(|(w, n)| (w.to_string(), *n)).collect()
}

/// cargo（Rust 工具链）输出特征词：构建进度动词 + Rust 诊断标示词。
fn cargo_words() -> HashMap<String, f64> {
    make(&[
        ("cargo", 10.0),
        ("compiling", 8.0),
        ("checking", 7.0),
        ("finished", 6.0),
        ("fresh", 4.0),
        ("generated", 3.0),
        ("expected", 6.0),
        ("found", 5.0),
        ("crate", 4.0),
        ("unused", 4.0),
        ("mut", 2.0),
        // src/target 为多种工具链共享的通用路径片段，权重压低以免压过 git_diff 等结构特征
        ("finished", 4.0),
        ("errno", 2.0),
        ("dependencies", 3.0),
        ("upgrade", 2.0),
    ])
}

/// gcc / g++ / make / ld（C/C++ 工具链）输出特征词：编译驱动与链接器标示词。
fn gcc_words() -> HashMap<String, f64> {
    make(&[
        ("gcc", 7.0),

        ("make", 6.0),
        ("undefined", 5.0),
        ("reference", 5.0),
        ("collect2", 4.0),
        ("included", 4.0),
        ("recipe", 3.0),
        ("makefile", 3.0),
        ("multiple", 3.0),
        ("definition", 3.0),
        ("ld", 3.0),
        ("relocation", 2.0),
        ("linker", 2.0),
        ("required", 3.0),
        ("restrict", 2.0),
        ("noexcept", 2.0),
        ("func", 2.0),
    ])
}

/// 测试运行器输出特征词（pytest / cargo test 等）。
fn test_words() -> HashMap<String, f64> {
    make(&[
        ("test", 10.0),
        ("tests", 9.0),
        ("passed", 7.0),
        ("assertion", 5.0),
        ("assert", 4.0),
        ("failures", 4.0),
        ("skipped", 4.0),
        ("collected", 3.0),
        ("session", 3.0),
        ("expected", 4.0),
        ("actual", 3.0),
        ("xfail", 2.0),
        ("deselected", 2.0),
        ])
}

/// git diff / patch 输出特征词：diff 头与文件操作标示词。
fn git_diff_words() -> HashMap<String, f64> {
    make(&[
        ("diff", 6.0),
        ("git", 5.0),
        ("index", 4.0),
        ("deleted", 4.0),
        ("similarity", 3.0),
        ("rename", 3.0),
        ("new", 4.0),
        ("mode", 3.0),
        ("binary", 2.0),
        ("modified", 2.0),
        ("changes", 2.0),
        ("commit", 2.0),
    ])
}

/// docker / kubernetes(kubectl) 命令输出特征词：容器编排与镜像标示词。
fn docker_k8s_words() -> HashMap<String, f64> {
    make(&[
        ("kubectl", 9.0),
        ("pod", 8.0),
        ("pods", 8.0),
        ("namespace", 7.0),
        ("deployment", 6.0),
        ("container", 6.0),
        ("image", 5.0),
        ("replicas", 4.0),
        ("docker", 15.0),
        ("compose", 12.0),
        ("building", 10.0),
        ("step", 9.0),
        ("service", 8.0),
        ("command", 3.0),
        ("buildkit", 4.0),
        ("buildx", 3.0),
        // BuildKit 构建阶段专属词：区分「docker build」内嵌 npm/test 子命令的场景，
        // 避免该输出被 node 类别（镜像内 RUN npm test 的词）抢走
        ("dockerfile", 8.0),
        ("solve", 7.0),
        ("transferring", 5.0),
        ("exporting", 5.0),
        ("metadata", 4.0),
        ("definition", 3.0),
        ("load", 3.0),
        // kubectl get 表格 / describe 输出的列头与状态字段：Ready/restarts/age 等
        // 为 k8s 专属信号，避免含 nginx 容器名的表格被 web_log 类别抢走
        ("ready", 6.0),
        ("restarts", 5.0),
        ("restart", 4.0),
        ("age", 4.0),
        ("rollout", 4.0),
        ("describe", 3.0),
        ("create", 4.0),
        ("stopped", 3.0),
        ("created", 3.0),
        ("namespaces", 3.0),
        // error/success 为跨类别通用词，权重压低以免干扰语义边界
        ("successfully", 3.0),
    ])
}

/// node 生态输出特征词：包管理器（npm/yarn/pnpm）、前端构建链（webpack/eslint/tsc 等）
/// 与 node 运行时错误栈特征。候选插件 nodejs_plugin 与 node_error_plugin 共用此类别。
fn node_words() -> HashMap<String, f64> {
    make(&[
        // 包管理器：node 生态强判别词
        ("npm", 9.0),
        ("yarn", 8.0),
        ("pnpm", 8.0),
        // lockfile 为包管理器 CI 专属词（package lock / frozen-lockfile 场景），
        // 用于稳固「pnpm install CI」这类依赖 pnpm 但词形常被分词器拆散的弱样本，
        // 避免其在语料改权后被 Test 等泛测试类别抢走
        ("lockfile", 9.0),
        // 构建 / 静态检查链
        ("webpack", 7.0),
        ("eslint", 7.0),
        ("tsc", 6.0),
        ("typescript", 6.0),
        ("bundle", 4.0),
        ("chunk", 4.0),
        // 运行时 / 模块（module/internal 等亦常见于 python traceback，降权以避跨语言误判）
        ("node", 6.0),
        ("package", 5.0),
        ("require", 4.0),
        ("module", 2.0),
        ("anonymous", 3.0),
        // node 错误栈行内特征
        ("throw", 4.0),
        ("internal", 2.0),
        // 跨类别通用词（error/warning/failed），压低以免破坏类别边界
        ])
}

/// 前端构建输出特征词（webpack / vite，含 Vue/Svelte/React 等框架产物）。
/// 候选插件 webpack_vite_plugin 对应此类别。node 类别侧重包管理/静态检查链，
/// 此处侧重「构建产物 + 模块图 + HMR」等前端专属信号。
fn web_words() -> HashMap<String, f64> {
    make(&[
        // 构建驱动：前端构建器强判别词
        ("webpack", 11.0),
        ("vite", 11.0),
        // rollup 新一代打包器：与 webpack/vite 同属前端构建，token 极专属、不与 node 冲突
        ("rollup", 10.0),
        // 产物清单 / 编译结果
        ("asset", 8.0),
        ("dist", 6.0),
        ("emitted", 8.0),
        ("compiled", 7.0),
        ("transformed", 7.0),
        ("cacheable", 6.0),
        ("minimized", 6.0),
        ("chunk", 5.0),
        ("chunks", 5.0),
        // 模块装载链
        ("loader", 6.0),
        ("jsx", 6.0),
        ("tsx", 4.0),
        ("modules", 4.0),
        ("built", 4.0),
        // HMR 与产物结构
        ("hmr", 6.0),
        ("runtime", 3.0),
        ("bundle", 3.0),
        ("css", 3.0),
        // 跨类别通用词（error/build），压低以免破坏语义边界
        ])
}

/// JVM 运行栈异常输出特征词（Java 栈 trace 与异常头）。
/// 候选插件 java_stack_plugin 对应此类别。以「异常类型 + 栈定位」为核心判别词，
/// 注意 `at`/`main`/`example` 等被分词器判为噪声词，不计入特征表。
fn java_words() -> HashMap<String, f64> {
    make(&[
        // 运行时容器与异常标示
        ("java", 10.0),
        ("exception", 10.0),
        ("thread", 7.0),
        // 异常类型（高频尾部片段，分词后保留全拼或驼峰片段）
        ("nullpointerexception", 8.0),
        ("illegalargumentexception", 8.0),
        ("stackoverflowerror", 8.0),
        ("runtimeexception", 7.0),
        ("unsupportedoperationexception", 7.0),
        // 栈帧定位：包名片段 + 类名（小写后保留，`at` 已被滤除）
        ("com", 7.0),
        ("org", 4.0),
        ("app", 4.0),
        ("validator", 3.0),
        // 并发/线程池（常见于日志后端异常头）
        ("threadpool", 3.0),
        ("worker", 3.0),
        // 反射/通用 JVM 构建词（低频辅助）
        ("compiletimeexception", 4.0),
    ])
}

/// Spring Boot 应用启动 / 运行日志特征词。
/// 候选插件 spring_boot_plugin 对应此类别。以「框架名 + Tomcat + 应用生命周期」为强判别，
/// 与纯异常栈类别 java_stack 区分（spring 靠 spring/boot/tomcat/application 等框架专属词，
/// 而非 com/app/exception 等通用栈定位词）。
fn spring_boot_words() -> HashMap<String, f64> {
    make(&[
        // 框架标识（`spring-boot`、`springframework`、缩写 `o.s.b` 分词后均出 spring/boot）
        ("spring", 12.0),
        ("boot", 11.0),
        // 内嵌容器与运行时
        ("tomcat", 11.0),
        ("embedded", 6.0),
        ("catalina", 9.0),
        ("servlet", 8.0),
        ("port", 4.0),
        // 应用生命周期（Started / Starting 常出现于启动日志头）
        ("started", 7.0),
        ("starting", 7.0),
        ("application", 6.0),
        ("startup", 5.0),
        // 配置 / 依赖注入语境
        ("profile", 5.0),
        ("active", 4.0),
        ("bean", 4.0),
        ("autoconfiguration", 5.0),
        // 日志级别 / 线程 + 冒号结构（INFO/WARN 级别 token 保留）
        // 与 java_stack 共享的栈定位词，压低以避免纯异常栈被误判为 spring
        ("com", 3.0),
        ("exception", 3.0),
    ])
}

/// Maven 构建 / 依赖 / 生命周期输出特征词。
/// 候选插件 maven_plugin 对应此类别。以 `[INFO]` 前缀 + `BUILD` + `plugin` + `goal` +
/// `dependency` 为核心，printf 结构（`----`、坐标 `:`）分词后散落为辅证。
fn maven_words() -> HashMap<String, f64> {
    make(&[
        // 构建诊断前缀（[INFO]/[ERROR]/[WARNING]）与状态墙
        ("scanning", 5.0),
        // 插件 / 目标 / 生命周期
        ("plugin", 8.0),
        ("plugins", 6.0),
        ("goal", 6.0),
        ("resources", 5.0),
        ("recompile", 4.0),
        // 依赖 / 产物 / 仓库坐标（`jar`、`dependency`、Downloading/Downloaded）
        ("dependency", 7.0),
        ("artifact", 6.0),
        ("repository", 5.0),
        ("central", 4.0),
        ("jar", 4.0),
        ("downloading", 5.0),
        ("downloaded", 5.0),
        // 测试执行态（Tests run: / Failures:）
        ("tests", 5.0),
        ("failures", 4.0),
        // 其余 JVM 构建共同词，压低
        ("maven", 6.0),
    ])
}

/// PHP / Ruby（Rails/Laravel）脚本与框架运行时输出特征词。
/// 候选插件 php_ruby_plugin 对应此类别。PHP 靠 `PHP Fatal`/`Uncaught`/`.php` 路径，
/// Ruby/Rails 靠 `rails`/`laravel`/`ActiveRecord`/`Controller`/`vendor` 框架词；
/// 跨语言共享错误头（Exception/Error）权重压低，避免与 java_stack / Test 抢。
fn php_ruby_words() -> HashMap<String, f64> {
    make(&[
        // 脚本语言标识（`.php`/`.rb` 分词后出现 php/rb，ruby 消息头亦带 Ruby）
        ("php", 11.0),
        ("fatal", 10.0),
        ("uncaught", 9.0),
        ("traceback", 8.0),
        // Ruby / Rails / Laravel 框架词
        ("ruby", 10.0),
        ("rails", 10.0),
        ("laravel", 11.0),
        ("activerecord", 8.0),
        ("controller", 5.0),
        ("vendor", 6.0),
        ("illuminate", 8.0),
        // 打包 / 依赖（Gemfile、bundle）
        ("gem", 6.0),
        ("bundle", 5.0),
        // 通用错误限权词
        ("exception", 4.0),
        ("runtimeerror", 7.0),
    ])
}

/// .NET / MSBuild / C# 构建、测试运行与托管栈输出特征词。
/// 候选插件 dotnet_plugin 对应此类别。以 `.csproj` / `.dll` / `dotnet` / `restore` /
/// `aspnetcore` 为专属信号；`build retorted`、`Version=` 等词与 gcc/cargo 共享但权重压。
fn dotnet_words() -> HashMap<String, f64> {
    make(&[
        // 项目与产物标识（`.csproj` / `.dll` 分词后出现 csproj / dll；`dotnet test` 命令头高权）
        ("csproj", 12.0),
        ("dll", 10.0),
        ("dotnet", 14.0),
        ("restore", 6.0),
        // F-2.3 复合判别标记（corpus_tokens 的 DISCRIMINATIVE_PHRASES 在分词时整体产出）：
        // `dotnet test` / `Test run for xxx.dll` / `Starting test execution` 是 dotnet 测试
        // 运行器的专属头部短语，拆成 unigram 后仅剩 test/run/dll 等泛词，会被 pytest 的
        // passed/failed/skipped 抢类（Test 类别）。本表显式收录，不依赖 build.rs 语料聚合
        //（dotnet 语料中这些头部短语频次低于 MIN_COUNT，聚合不会收录）。权重给足但不过高。
        ("dotnet test", 10.0),
        ("test run for", 6.0),
        ("starting test execution", 6.0),
        // F-2.4 负采样（dotnet/test 混淆对）：`duration` 是 dotnet 测试摘要行
        // `Passed! ... Total: N, Duration: X s` 的固有词，pytest 总结用 `in Xs` 无此
        // 字面。作为 dotnet 侧判别补强，尽量缩小与 Test 类别的 token 数量劣势；
        // 若仍无法反超（纯 unigram 结构性不可分），如实记录为已知弱项而非强行调权。
        ("duration", 5.0),
        // 框架与工具链
        ("aspnetcore", 7.0),
        ("msbuild", 11.0),
        ("framework", 4.0),
        // MSBuild 编译器 / 编译目标专属词：`csproj` 触发 `(CoreCompile target)` 的 MSBuild
        // 编译诊断输出（`CS0xxx` 错误码 + corecompile 目标 + `.cs` 源文件路径），靠这些
        // 与 gcc/cargo 共享的 failed(error)/warning 区分——否则 `failed=7` 的 Test 类别会
        // 把 .NET 构建失败输出抢走。`cs` 来自 `.cs` 源码后缀（长度≥2 可保留），`cs0` 前缀
        // 承载 CS 编号诊断码。
        ("corecompile", 8.0),
        ("cs", 5.0),
        ("csc", 5.0),
        // 构建生命周期词
        ("succeeded", 3.0),
        // 跨类别通用词，压低
        ])
}

/// Helm chart 打包 / 安装 / 渲染 / 回滚输出特征词。
/// 候选插件 helm_plugin 对应此类别。以 `helm` / `chart` / `yaml` / `values` / `templates`
/// 为专属信号；`configmap`/`deployment`/`namespace` 等与 docker_k8s 共享词也收录但
/// 权重让位于 helm 专属词，避免二者争夺。
fn helm_words() -> HashMap<String, f64> {
    make(&[
        // 工具与 chart 工作单元（最高权威判别）
        ("helm", 13.0),
        ("chart", 12.0),
        // manifest 渲染 / 校验
        ("yaml", 8.0),
        ("templates", 6.0),
        ("lint", 6.0),
        // 资源清单（与 k8s 共享但此处为 chart 内联输出）
        ("configmap", 6.0),
        ("release", 5.0),
        // 生命周期动作
        ("installed", 5.0),
        ("uninstalled", 5.0),
        ("upgrade", 4.0),
        ("revision", 4.0),
        ("deployed", 3.0),
        // k8s 领域共享词，让位 helm 专属词
        ("namespace", 3.0),
    ])
}

/// Terraform 计划 / 应用 / 销毁 / 导入与状态输出特征词。
/// 候选插件 terraform_plugin 对应此类别。以 `terraform` / `resource` / `apply` / `plan` /
/// `state` / `import` / `destroy` 为专属信号，特征几乎不与其他类别重叠，辨识度高。
fn terraform_words() -> HashMap<String, f64> {
    make(&[
        ("terraform", 14.0),
        ("resource", 8.0),
        ("resources", 6.0),
        // 生命周期动作
        ("apply", 7.0),
        ("plan", 7.0),
        ("destroy", 6.0),
        ("import", 5.0),
        // 状态 / 工作区
        ("state", 5.0),
        ("workspace", 5.0),
        ("refresh", 4.0),
        // 面向字段与输出
        ("created", 4.0),
        ("changed", 4.0),
        ("provider", 4.0),
        ("module", 3.0),
        ("known", 3.0),
    ])
}

/// Go 构建 / 测试 / goroutine panic 栈输出特征词。
/// 候选插件 rust_go_plugin 对应此类别。以 `goroutine` / `panic` 为最高权威判别
/// （Go 运行时独有），`.go` 路径 token `go` / 包段 `pkg` / `github` 为辅证。
/// 注意：其余 rust/cargo 编译输出仍归 Cargo 类别，本类别只承接 goroutine panic 栈
/// 与明显 `.go` 路径文本，避免与 Cargo 类别混抢。
fn golang_words() -> HashMap<String, f64> {
    make(&[
        ("goroutine", 14.0),
        ("panic", 11.0),
        ("pkg", 7.0),
        ("runtime", 6.0),
        ("github", 5.0),
        ("go", 4.0),
        ("created", 3.0),
        ])
}

/// HTTP 访问日志特征词（apache / nginx / CDN / IIS / Envoy 等）。
/// 候选插件 web_log_plugin 对应此类别。以 HTTP 方法与协议 token 为最高权威判别
/// （`GET`/`POST`/`PUT` 与 `HTTP/1.1` 的 `http`），辅以路径段、UA 与 Referer 品牌词；
/// 注意访问日志通常跨多种格式（combined / W3C / JSON / CSV），故本类别靠方法与 UA
/// 等不依赖格式的稳定信号，而非格式专用词。
fn web_log_words() -> HashMap<String, f64> {
    make(&[
        // HTTP 方法（访问日志每一行必有其一，最高语义盖然性）
        ("get", 10.0),
        ("post", 10.0),
        ("put", 8.0),
        ("head", 8.0),
        ("delete", 8.0),
        ("patch", 7.0),
        // 协议 / 版本（`HTTP/1.1` / `http2` 分词后含 http）
        ("http", 9.0),
        // 路径段与面向资源（/api/ 高频但 api 也常见于 build 标签 / 通用路径，降权防抢）
        ("api", 2.0),
        ("static", 5.0),
        // UA 与 Referer 品牌（curl / mozilla / chrome / python-requests）
        ("mozilla", 7.0),
        ("curl", 7.0),
        ("chrome", 6.0),
        ("requests", 6.0),
        ("referer", 5.0),
        // 常见源 / 转发标识（nginx 访问日志高频）
        ("nginx", 6.0),
        // 状态语义（负载均衡 Hit/Miss、HTTP status 配套词），权重低防撞
        ("hit", 3.0),
        ])
}

/// Python 解释器 traceback 异常栈输出特征词。
/// 候选插件 python_traceback_plugin 对应此类别。以 `Traceback` / `raise` 为最高权威判别
/// （每份 Python 异常必然出现），`File`/`line` 定位词与异常类型（ValueError/IndexError 等）
/// 为辅证。注意 `File`/`module`/`main` 亦见于 GitDiff/Node，故权重让位 traceback/raise。
fn python_traceback_words() -> HashMap<String, f64> {
    make(&[
        ("traceback", 13.0),
        ("raise", 11.0),
        // 异常类型（分词后保留全拼尾段）
        ("runtimeerror", 8.0),
        ("valueerror", 8.0),
        ("typeerror", 8.0),
        ("nameerror", 8.0),
        ("indexerror", 8.0),
        ("keyerror", 8.0),
        ("attributeerror", 8.0),
        // 调用帧（main 常出现在最外层帧名，module 为内层模块名）
        ("module", 3.0),
        // 脚本文件后缀（`app.py` 分词后出现 py）
        ("py", 3.0),
        ])
}

/// Bazel 构建 / 分析 / 执行动作输出特征词。
/// 候选插件 bazel_plugin 对应此类别。以 `bazel` / `Analyzed` / `targets` / `actions` /
/// `processes` / `sandbox` 为专属信号；`build`/`cache`/`target` 等更通用词降权以避抢。
fn bazel_words() -> HashMap<String, f64> {
    make(&[
        ("bazel", 13.0),
        ("analyzed", 9.0),
        ("targets", 8.0),
        ("actions", 7.0),
        ("action", 7.0),
        ("sandbox", 8.0),
        ("remote", 5.0),
        ("cache", 4.0),
        ("elapsed", 4.0),
        ("invocation", 4.0),
        ("critical", 3.0),
        // Bazel query 输出：查询输出多为 `//pkg:target` 标签列表，`api` 等段名易
        // 被 web_log 抢；query/deps 是 Bazel 查询专属词，权重拉高以稳归类
        ("query", 15.0),
        ("deps", 10.0),
        ("label", 4.0),
        // 通用构建词，降权防与其他工具链抢
        ("successfully", 3.0),
    ])
}

/// Android Gradle 任务构建输出特征词。
/// 候选插件 android_gradle_plugin 对应此类别。以 `> Task` 前缀的 `task` + `compile` /
/// `kotlin` / `javac` / `tasks` / `actionable` 为专属信号，`BUILD SUCCESSFUL` 的
/// `successful` 为辅证。`task` 与 Ansible 的 TASK 肩碰，靠 compile/kotlin/javac 区分。
fn gradle_words() -> HashMap<String, f64> {
    make(&[
        ("task", 9.0),
        ("compile", 9.0),
        ("kotlin", 8.0),
        ("javac", 7.0),
        ("tasks", 6.0),
        ("successful", 5.0),
        ("actionable", 4.0),
        ("executed", 4.0),
        ("manifest", 3.0),
        // 通用词，降权
        ("app", 4.0),
        ("resources", 3.0),
        ])
}

/// Xcode / Apple 构建输出特征词。
/// 候选插件 xcode_log_plugin 对应此类别。以 `CompileX` 系列工具名（CompileC/CompileSwift）
/// 分词后的 `compile` + `derived`（DerivedData）+ `object`（objects-*）+ `normal` +
/// `arm64` 为专属信号；`target`/`project`/`app` 为构建产物定位辅助词。
fn xcode_words() -> HashMap<String, f64> {
    make(&[
        ("compile", 10.0),
        ("derived", 7.0),
        // object 与 .NET 托管栈的 `Object` 关键词撞词，降权以免 .NET 栈被误判为 Xcode
        ("object", 3.0),
        ("normal", 6.0),
        ("arm64", 5.0),
        ("x86", 4.0),
        ("swift", 5.0),
        ("clang", 4.0),
        ("project", 6.0),
        ("succeeded", 4.0),
        // 定位辅助词，降权防与 gradle/cargo 抢
        ("app", 4.0),
        ])
}

/// Ansible playbook 执行输出特征词。
/// 候选插件 ansible_plugin 对应此类别。以 `PLAY` / `TASK` / `PLAY RECAP` 的 play/task/
/// recap + 主机结果 ok/changed/unreachable + `gathering`/`facts` 为专属信号。
fn ansible_words() -> HashMap<String, f64> {
    make(&[
        ("ansible", 11.0),
        ("play", 9.0),
        ("task", 8.0),
        ("recap", 8.0),
        ("unreachable", 7.0),
        ("ok", 6.0),
        ("changed", 7.0),
        ("gathering", 6.0),
        ("facts", 6.0),
        ("playbook", 5.0),
        // 主机/失败语境（失败计数列）
        ("hosts", 4.0),
        ("ignoring", 3.0),
        ])
}

/// Pulumi 资源编排输出特征词。
/// 候选插件 pulumi_plugin 对应此类别。以 `pulumi` / `Previewing` / `preview` 为最高权威
/// 判别；`stack`/`update`/`resources`/`plan` 与 Terraform 共享，但 Terraform 靠 terraform/
/// apply/state 顶级词，Pulumi 靠 pulumi/previewing/preview 顶级词区分。
fn pulumi_words() -> HashMap<String, f64> {
    make(&[
        ("pulumi", 12.0),
        ("previewing", 8.0),
        ("preview", 7.0),
        // F-2.3 复合判别标记（corpus_tokens 的 DISCRIMINATIVE_PHRASES 在分词时整体产出）：
        // `Updating (stack)` 头部与 `pulumi:` 资源类型前缀是 pulumi 专属复合信号。单独靠
        // unigram 的 update/up/stack 无法与 Terraform 区分（terraform 亦含 update/plan）；
        // 显式收录以确保 holdout 等未知输出在无聚合加持时也能命中。`updating (` 与
        // `pulumi:` 虽可能聚合，但 `updating (` 频次低于 MIN_COUNT 不一定收录，故种子兜底。
        ("updating (", 8.0),
        ("previewing update", 6.0),
        ("pulumi:", 6.0),
        // 编排通用词（与 Terraform 共享，权重让位于各自顶级词）
        ("stack", 5.0),
        ("update", 5.0),
        ("resources", 5.0),
        ("unchanged", 4.0),
        ("created", 3.0),
        ("plan", 3.0),
    ])
}

/// AWS CloudFormation 栈事件输出特征词。
/// 候选插件 cloudformation_plugin 对应此类别。以 `cloudformation` / `stack` /
/// `ROLLBACK` / `IN_PROGRESS` / `COMPLETE` / `Initiated` 为专属信号（栈事件生命周期）；
/// `resource`/`create`/`aws` 等与 Terraform/Pulumi 共享词降权，靠生命周期词区分。
fn cloudformation_words() -> HashMap<String, f64> {
    make(&[
        ("cloudformation", 13.0),
        ("stack", 9.0),
        ("rollback", 8.0),
        ("progress", 8.0),
        ("complete", 7.0),
        ("initiated", 6.0),
        ("deploy", 4.0),
        // 共享词，降权防抢
        ("aws", 5.0),
        ("resource", 3.0),
        ("resources", 3.0),
        ("create", 3.0),
        ])
}

/// SQL 查询 / 脚本执行输出特征词（psql/mysql/pgcli CLI 会话输出）。
/// 候选插件 sql_plugin 对应此类别。以 SQL 关键字（SELECT/INSERT/UPDATE/DELETE）与
/// 子句词（FROM/WHERE/JOIN/GROUP/ORDER/INTO/VALUES）为最高权威判别；`users`/`table`/
/// `syntax`/`row`/`rows` 为辅证。注意 SELECT/WHERE 等词与访问日志 HTTP 方法单词撞，
/// 但访问日志靠 GET/POST/HTTP 顶级词，本类靠候选 SQL 关键字与其组合辨识。
fn sql_words() -> HashMap<String, f64> {
    make(&[
        // SQL 关键指令（最高权威判别）
        ("select", 12.0),
        ("insert", 11.0),
        ("update", 11.0),
        ("delete", 8.0),
        ("truncate", 8.0),
        ("alter", 8.0),
        // 子句 / 结构词
        ("join", 9.0),
        ("group", 7.0),
        ("order", 7.0),
        ("inner", 5.0),
        ("left", 5.0),
        ("having", 5.0),
        // 目标对象 / 结果语义
        ("table", 5.0),
        ("database", 5.0),
        ("query", 5.0),
        ("syntax", 6.0),
        ("row", 4.0),
        ("rows", 4.0),
        // 执行语义 / 通用诊断，降权避免与 db_log 混淆
        ("executed", 4.0),
        ("statement", 3.0),
    ])
}

/// 数据库服务器守护日志特征词（mysqld / postgres / redis / mongod 等进程 stdout/stderr）。
/// 候选插件 db_log_plugin 对应此类别。以数据库引擎名（mysqld/innodb/mongodb/redis/
/// postgres/checkpoint/autovacuum/deadlock/replication）为专属判别；时间戳前缀 + 日志级别
/// （Note/Warning/ERROR/LOG:/INFO:/DEBUG:）为辅证，与 sql_plugin 的「SQL 语句本体」区分。
fn db_log_words() -> HashMap<String, f64> {
    make(&[
        // 引擎与存储组件（最高权威判别）
        ("mysqld", 13.0),
        ("innodb", 12.0),
        ("mongodb", 11.0),
        ("mongod", 10.0),
        ("redis", 10.0),
        ("postgres", 11.0),
        ("checkpoint", 8.0),
        ("autovacuum", 9.0),
        // 实例 / 连接 / 复制状态
        ("deadlock", 9.0),
        ("replication", 8.0),
        ("connection", 7.0),
        ("connections", 6.0),
        ("shutdown", 5.0),
        ("started", 5.0),
        ("ready", 4.0),
        // 存储 / 性能语境
        ("buffer", 6.0),
        ("tablespace", 5.0),
        ("cluster", 4.0),
        // 内存 / 守护错误语境（redis `OOM command not allowed when used memory > 'maxmemory'`
        // 与 `TCP backlog` 守护日志：maxmemory/oom 为 redis/内存类专属信号，防此类样本在
        // 特征近乎缺席时被 docker_k8s 的 command/running 等通用词抢走）
        ("oom", 8.0),
        ("maxmemory", 9.0),
        ("backlog", 6.0),
        // 语句与诊断（与 sql_plugin 共享，降权避免互相抢）
        ("statement", 5.0),
        ("query", 4.0),
        ("slow", 4.0),
        // 数据库客户端身份字段（常见于访问拒绝 / 中止连接日志）
        ("access", 4.0),
        ("denied", 4.0),
        ("aborted", 3.0),
    ])
}

/// Unity / Unreal 引擎构建与运行时日志特征词。
/// 候选插件 unity_unreal_plugin 对应此类别。Unreal 靠 `LogTemp`/`LogGame`/`LogUObject`
/// 等日志分类前缀 + `Display`/`Error` 级别 + `/Game/` 资产路径；Unity 靠 `Unity`/`AssetBundle`/
/// `Shader`/`MissingReferenceException`/`.cs` 栈尾。二者共享 Display/Warning/Error 级别词，
/// 靠引擎专属前缀与资产路径区分。
fn unity_unreal_words() -> HashMap<String, f64> {
    make(&[
        // Unreal 日志分类前缀（最高权威判别）
        ("logtemp", 13.0),
        ("loggame", 12.0),
        ("loginit", 10.0),
        ("loguobject", 10.0),
        ("loghal", 9.0),
        ("loglinker", 9.0),
        // Unreal 资产 / 构建语义（不收录松散 `game`：gameserver/syslog 亦含 game，
        // 会引入误判，仅用 LogGame 复合前缀承载引擎判别）
        ("umap", 8.0),
        ("blueprint", 7.0),
        ("asset", 6.0),
        ("material", 5.0),
        ("unreal", 7.0),
        // Unity 引擎专属
        ("unity", 8.0),
        ("assetbundle", 8.0),
        ("shader", 7.0),
        ("missingreference", 7.0),
        ("gameobject", 6.0),
        // Unity 异常 / 引擎栈帧专属词：`MissingReferenceException` 分词后为整词
        // missingreferenceexception（非 missingreference），`UnityEngine.*` 与
        // `InputSystem.PlayerInput` 为引擎栈帧头部专属全名。缺这些时此类样本只剩
        // assets 一个弱信号，会被 SQL 的 update()（栈中的 `Update()`）按 11.0 抢走。
        ("missingreferenceexception", 9.0),
        ("unityengine", 9.0),
        ("playerinput", 7.0),
        // 级别词（Display/Warning/Error 亦见于引擎日志，作为辅证而非最高判别）
        ("display", 5.0),
        // .cs 脚本栈尾（Unity 异常栈定位）
        ("assets", 5.0),
        ("compilation", 4.0),
    ])
}

/// 系统守护日志特征词（RFC3164 syslog：`月 日 时:分:秒 hostname daemon[pid]: msg`）。
/// 候选插件 syslog_plugin 对应此类别。以守护进程名 + 系统语义为强判别，时间戳前缀由
/// 语料聚合补强。`daemon[pid]:` 中的 pid 为数字、`[pid]` 符号被分词器剥离，故进程名是
/// 主身份；收录 sshd/kernel/CRON/systemd/journald 等专属词。
fn syslog_words() -> HashMap<String, f64> {
    make(&[
        // 常见 syslog 守护进程（最高权威判别）
        ("sshd", 12.0),
        ("kernel", 11.0),
        ("cron", 11.0),
        ("systemd", 10.0),
        ("journald", 10.0),
        ("rsyslogd", 9.0),
        ("pam", 8.0),
        ("cups", 9.0),
        ("dhcpd", 9.0),
        ("ntpd", 8.0),
        ("chronyd", 8.0),
        ("agetty", 7.0),
        // 认证 / 安全语义
        ("authenticated", 5.0),
        ("publickey", 5.0),
        ("password", 4.0),
        ("accept", 4.0),
        // 内核 / 网络语义
        ("link", 5.0),
        ("eth0", 6.0),
        ("interface", 4.0),
        // IP / 端口字段（辅证）
        ("port", 4.0),
        // 守护进程会话引导
        ("started", 4.0),
        ("stopping", 4.0),
    ])
}

/// CI/CD 流水线编排行特征词（GitHub Actions / GitLab CI / Jenkins / Azure Pipelines /
/// Buildkite / Travis / CircleCI / TeamCity）。
/// 候选插件 ci_log_plugin 对应此类别。平台名（jenkins/gitlab/buildkite/teamcity/travis/
/// circleci/actions/runner/pipeline/worker）为其专属判别；`::group::`/`##[section]`/
/// `section_start:` 等控制符不参与分词（冒号/方括号被剥离），仅靠平台词与 `artifact`/
/// `matrix`/`workflow`/`stage` 等编排语义补足。
///
/// **GH Actions 补强**：GitHub Actions 样本的命令头是 `gh run view <n> --log`，外壳骨架是
/// `::group::`/`::endgroup::` 折叠段、`Process completed with exit code` 步脚注与
/// `::error/::warning/::notice file=...::` 注解；而 error/file/line/code/process/run 等
/// 骨架 token 全是噪声词被分词器丢弃，仅剩 `group`/`exit`/`gh` 等少量弱信号，导致内层
/// test/node 内容（PASS/FAILED/pytest/npm）把类别抢走。故补 `endgroup`（`::endgroup::`
/// 为 GH Actions 专属）、`view`（`gh run view` 锚点）、`completed`（步脚注）、`upload`
/// （Artifact/SARIF 上传）与 `notice`（`::notice::` 注解）强化外壳判定。
fn ci_log_words() -> HashMap<String, f64> {
    make(&[
        // CI 平台名（最高权威判别；`actions` 走复合词 `github`/`actions` 双保险）
        ("jenkins", 12.0),
        ("gitlab", 12.0),
        ("buildkite", 12.0),
        ("teamcity", 12.0),
        ("travis", 11.0),
        ("circleci", 11.0),
        ("github", 9.0),
        ("actions", 6.0),
        ("glab", 8.0),
        ("azure", 6.0),
        // CLI 锚点：部分样本命令头不带平台名（gh/az/act），需靠 CLI 名识别。
        // `gh run view`（GitHub Actions）、`az pipelines`（Azure）、`act -j`（本地 runner）。
        // `view` 是 `gh run view` 专属子命令，权重压低避免与 kubectl get/web 等通用词冲突。
        ("gh", 8.0),
        ("pipelines", 8.0),
        ("act", 6.0),
        ("view", 4.0),
        // 分组 / 折叠控制符：GitHub `::group::` 与 GitLab `section_start:`/Azure `##[section]`
        // 每条样本重复多次，是嵌入的第三方构建输出之外的稳定编排信号。
        // `endgroup` 出自 `::endgroup::`，为 GH Actions 折叠段专属收尾符，权重最高防内层
        // test/node 内容抢类（冒号/方括号被分词器剥离后 group/endgroup 独立成 token）。
        ("group", 5.0),
        ("endgroup", 10.0),
        ("section", 5.0),
        // 编排会话语义：checkout 拉码 / cache 缓存，跨 GitHub/GitLab/Jenkins/CircleCI/Buildkite。
        ("checkout", 5.0),
        ("cache", 4.0),
        // 执行器 / 编排语义
        ("runner", 8.0),
        ("pipeline", 8.0),
        ("worker", 6.0),
        ("executor", 6.0),
        // 编排产物 / 生命周期
        ("artifact", 6.0),
        ("artifacts", 6.0),
        ("matrix", 6.0),
        ("workflow", 6.0),
        ("stage", 5.0),
        // GH Actions 步生命周期：`Process completed with exit code` 步脚注（每条样本每个
        // 步骤出现一次）与 `Artifact upload complete`/`Uploading SARIF` 产物上传语。
        // `completed`/`upload` 权重压低，避免与 gcc/build 等「Build completed」通用词冲突。
        ("completed", 4.0),
        ("upload", 4.0),
        ("notice", 5.0),
        // CI 状态语义
        ("job", 4.0),
        ("succeeded", 4.0),
        ("finished", 4.0),
        ("exit", 4.0),
    ])
}

/// 云平台日志特征词（AWS CloudWatch / GCP / Azure / Aliyun / OCI / Tencent / Huawei / Cloudflare）。
/// 候选插件 cloud_log_plugin 对应此类别。与 CiLog 同为「剥皮」类别：命中后 plugin 剥外层云包装，
/// 内嵌 HTTP/栈内容再交内层插件。分词器把 camelCase 字段折叠成单 token（logGroup→`loggroup` 等），
/// 故收录折叠后的字段 token + 各厂商 CLI/平台名；`log`/`logs` 本身是噪声词，但 `loggroup`/`logname`/
/// `logstream`/`requestid` 等复合字段为强判别。**刻意不聚合语料**：同 CiLog 一样，cloud_log 样本内嵌
/// 大量第三方输出（HTTP access→web_log、java/python/node、syslog/db），聚合会触发 SHARE_THRESHOLD
/// 全局改权殃及其他类别（实测 node 召回跌穿）。皮检率天然低于 CiLog——多数样本被内嵌类别接管属合法。
fn cloud_log_words() -> HashMap<String, f64> {
    make(&[
        // 云平台名 / CLI（最高权威判别；Log 字段跨厂商共享，平台词负责定厂）
        ("aws", 8.0),
        ("cloudwatch", 10.0),
        ("cloudtrail", 10.0),
        ("flowlogs", 9.0),
        ("lambda", 7.0),
        ("gcloud", 10.0),
        ("gcp", 8.0),
        ("azure", 7.0),
        ("az", 5.0),
        ("aliyun", 10.0),
        ("sls", 7.0),
        ("oci", 9.0),
        ("ocid", 8.0),
        ("tencent", 9.0),
        ("huawei", 8.0),
        ("hcloud", 9.0),
        ("cloudflare", 10.0),
        ("wrangler", 9.0),
        ("logpush", 7.0),
        // 云日志共享字段（camelCase 折叠后；多行样本重复累积，为跨厂商主身份）
        ("loggroup", 8.0),
        ("logstream", 8.0),
        ("logname", 7.0),
        ("requestid", 12.0),
        ("insertid", 6.0),
        ("jsonpayload", 7.0),
        ("severity", 5.0),
        // Azure / Tencent 等厂商专属字段
        ("timegenerated", 7.0),
        ("resourceid", 6.0),
        ("subscriptions", 6.0),
        ("workspace", 5.0),
        ("logsetname", 8.0),
        ("rayid", 7.0),
        ("scriptname", 5.0),
        // 通用云编排语义（权重压低，避免与 DockerK8s/web_log 抢）
        ("cloud", 4.0),
        ("stream", 3.0),
    ])
}
