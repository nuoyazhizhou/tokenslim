//! content classifier 单元测试
//!
//! 使用合成样本验证分类器能将 cargo / gcc / test / git_diff 各类输出正确分类到对应
//! 语义类别，并验证低置信度保护与候选插件映射。

use super::features::seed_model;
use super::model::{Category, NaiveBayesClassifier};

/// 构造分类器（内置种子模型）。
fn classifier() -> NaiveBayesClassifier {
    seed_model()
}

/// 读取 kubernetes_docker_plugin 目录下的物理样本文件（红线：测试必须加载真实样本，禁止手写 mock）。
fn read_docker_sample(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples/kubernetes_docker_plugin")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取样本 {name} 失败: {e}"))
}

/// 读取分类器测试专用合成样本（P3-202：原手写 mock 物理化为 samples/content_classifier/）。
fn read_classify_sample(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples/content_classifier")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取样本 {name} 失败: {e}"))
}

#[test]
fn classify_cargo_build_output() {
    let c = classifier();
    let sample = read_classify_sample("classify_cargo_build.log");
    let r = c.classify(&sample);
    assert_eq!(
        r.category,
        Category::Cargo,
        "cargo 输出应分类到 Cargo, got {:?}",
        r
    );
    assert!(
        r.confidence >= 0.5,
        "cargo 置信度应较高, got {}",
        r.confidence
    );
}

#[test]
fn classify_gcc_compile_output() {
    let c = classifier();
    let sample = read_classify_sample("classify_gcc_compile.log");
    let r = c.classify(&sample);
    assert_eq!(
        r.category,
        Category::Gcc,
        "gcc 输出应分类到 Gcc, got {:?}",
        r
    );
    assert!(
        r.confidence >= 0.4,
        "gcc 置信度应可接受, got {}",
        r.confidence
    );
}

#[test]
fn classify_test_output() {
    let c = classifier();
    let sample = read_classify_sample("classify_test_session.log");
    let r = c.classify(&sample);
    assert_eq!(
        r.category,
        Category::Test,
        "测试输出应分类到 Test, got {:?}",
        r
    );
    assert!(
        r.confidence >= 0.5,
        "测试置信度应较高, got {}",
        r.confidence
    );
}

#[test]
fn classify_git_diff_output() {
    let c = classifier();
    let sample = read_classify_sample("classify_git_diff.log");
    let r = c.classify(&sample);
    assert_eq!(
        r.category,
        Category::GitDiff,
        "git diff 输出应分类到 GitDiff, got {:?}",
        r
    );
    assert!(
        r.confidence >= 0.4,
        "git diff 置信度应可接受, got {}",
        r.confidence
    );
}

#[test]
fn classify_generic_text_fallback() {
    let c = classifier();
    // 无任何工具链特征的普通散文文本，应落到 GenericText 且置信度低。
    let sample = "the quick brown fox jumps over the lazy dog near the river bank";
    let r = c.classify(sample);
    assert_eq!(r.category, Category::GenericText);
    // 无特征词时 softmax 归一化后各类别近乎均匀，置信度应偏低。
    assert!(
        r.confidence <= 0.4 + 0.1,
        "通用文本置信度应偏低, got {}",
        r.confidence
    );
}

#[test]
fn classify_docker_k8s_output() {
    let c = classifier();
    let sample = read_classify_sample("classify_docker_k8s.log");
    let r = c.classify(&sample);
    assert_eq!(
        r.category,
        Category::DockerK8s,
        "docker/k8s 输出应分类到 DockerK8s, got {:?}",
        r
    );
    assert!(
        r.confidence >= 0.35,
        "docker/k8s 置信度应可接受, got {}",
        r.confidence
    );
}

#[test]
fn sweep_docker_k8s_samples() {
    // P3-205：手写扫描循环收敛到公共 helper sweep_samples_in_dir。
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/kubernetes_docker_plugin"
    );
    let (hits, total) = sweep_samples_in_dir(dir, Category::DockerK8s, "DockerK8s");
    assert!(hits >= 18, "docker/k8s 召回过低: {hits}/{total}");
}

#[test]
fn sweep_node_samples() {
    // P3-205：手写扫描循环收敛到公共 helper（nodejs + node_error 双目录累加）。
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples");
    // nodejs（构建/包管理）与 node_error（运行错误栈）两个插件目录同属 Node 类别。
    let (h1, t1) = sweep_samples_in_dir(
        &std::path::Path::new(root).join("nodejs_plugin").display().to_string(),
        Category::Node,
        "Node/nodejs_plugin",
    );
    let (h2, t2) = sweep_samples_in_dir(
        &std::path::Path::new(root).join("node_error_plugin").display().to_string(),
        Category::Node,
        "Node/node_error_plugin",
    );
    let hits = h1 + h2;
    let total = t1 + t2;
    assert!(hits >= 25, "node 召回过低: {hits}/{total}");
}

#[test]
fn sweep_web_samples() {
    // P3-205：手写扫描循环收敛到公共 helper。
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/webpack_vite_plugin");
    let (hits, total) = sweep_samples_in_dir(dir, Category::Web, "Web");
    assert!(hits >= 8, "前端构建召回过低: {hits}/{total}");
}

#[test]
fn sweep_java_samples() {
    // P3-205：手写扫描循环收敛到公共 helper。
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/java_stack_plugin");
    let (hits, total) = sweep_samples_in_dir(dir, Category::Java, "Java");
    assert!(hits >= 10, "java 运行栈召回过低: {hits}/{total}");
}

/// 对某插件目录下所有 `.log` 样本做分类，统计命中目标类别的样本数并逐条打印。
/// 返回（命中, 总数），供各 sweep 门禁使用（避免重复扫描循环）。
fn sweep_samples_in_dir(dir: &str, target: Category, label: &str) -> (u32, u32) {
    let c = classifier();
    let mut hits = 0u32;
    let mut total = 0u32;
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("log"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let text = std::fs::read_to_string(e.path()).unwrap();
        let r = c.classify(&text);
        if r.category == target {
            hits += 1;
        }
        total += 1;
        eprintln!(
            "[sweep] {} {} conf={:.3}",
            e.file_name().to_string_lossy(),
            r.category.name(),
            r.confidence
        );
    }
    eprintln!("[sweep] {label} 命中 {hits}/{total}");
    (hits, total)
}

#[test]
fn sweep_spring_boot_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/spring_boot_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::SpringBoot, "SpringBoot");
    // spring_boot 类别精确识别「含 spring 框架专属词（spring/boot/tomcat/catalina/servlet/started）」
    // 的应用日志；无框架词的 JVM 异常栈样例被 java_stack 合理接管、maven/json 样例被对应类别接管，
    // 均属正确语义拆分。故门槛对齐「明确含框架信号」的样本数（6）。
    assert!(hits >= 6, "spring_boot 召回过低: {hits}/{total}");
}

#[test]
fn sweep_maven_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/maven_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Maven, "Maven");
    assert!(hits >= 11, "maven 召回过低: {hits}/{total}");
}

#[test]
fn sweep_php_ruby_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/php_ruby_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::PhpRuby, "PhpRuby");
    assert!(hits >= 9, "php_ruby 召回过低: {hits}/{total}");
}

#[test]
fn sweep_dotnet_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/dotnet_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Dotnet, "Dotnet");
    assert!(hits >= 8, "dotnet 召回过低: {hits}/{total}");
}

#[test]
fn sweep_helm_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/helm_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Helm, "Helm");
    assert!(hits >= 8, "helm 召回过低: {hits}/{total}");
}

#[test]
fn sweep_terraform_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/terraform_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Terraform, "Terraform");
    assert!(hits >= 9, "terraform 召回过低: {hits}/{total}");
}

#[test]
fn sweep_golang_samples() {
    // go 样例存放在 rust_go_plugin 目录（与 rust/cargo 混排），仅统计文件名带 `go` 标签的
    // 子集；rust/cargo 编译输出应归 Cargo，属不同语义类别，不在本测试承接范围。
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/rust_go_plugin");
    let mut hits = 0u32;
    let mut total = 0u32;
    let c = classifier();
    for e in std::fs::read_dir(root).unwrap().filter_map(|e| e.ok()) {
        let name = e.file_name();
        let name_s = name.to_string_lossy();
        if !name_s.starts_with("case_")
            || !name_s.contains("go_")
            || name_s.ends_with(".log") == false
        {
            continue;
        }
        let text = std::fs::read_to_string(e.path()).unwrap();
        let r = c.classify(&text);
        if r.category == Category::Golang {
            hits += 1;
        }
        total += 1;
        eprintln!(
            "[sweep] {} {} conf={:.3}",
            name_s,
            r.category.name(),
            r.confidence
        );
    }
    eprintln!("[sweep] Golang 命中 {hits}/{total}");
    assert!(hits >= 5, "golang 召回过低: {hits}/{total}");
}

#[test]
fn sweep_web_log_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/web_log_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::WebLog, "WebLog");
    // web_log 类别以 HTTP 方法与 UA 等不依赖格式的稳定信号识别，48 样例中除 empty/noise/
    // no_compress 等弱信号负样本外应大量命中，故设较高门槛。
    assert!(hits >= 34, "web_log 召回过低: {hits}/{total}");
}

#[test]
fn sweep_python_traceback_samples() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/python_traceback_plugin"
    );
    let (hits, total) = sweep_samples_in_dir(root, Category::PythonTraceback, "PythonTraceback");
    // Traceback/raise 为 Python 异常强判别词，16 样例除空文本负样本外应大量命中。
    assert!(hits >= 10, "python_traceback 召回过低: {hits}/{total}");
}

#[test]
fn sweep_bazel_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/bazel_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Bazel, "Bazel");
    assert!(hits >= 8, "bazel 召回过低: {hits}/{total}");
}

#[test]
fn sweep_gradle_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/android_gradle_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Gradle, "Gradle");
    // > Task : 前缀的 task/compile 为强判别，22 样例多数命中；个别非任务输出（如依赖下载）
    // 可能归 maven，属合法接管。
    assert!(hits >= 13, "gradle 召回过低: {hits}/{total}");
}

#[test]
fn sweep_xcode_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/xcode_log_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Xcode, "Xcode");
    assert!(hits >= 8, "xcode 召回过低: {hits}/{total}");
}

#[test]
fn sweep_ansible_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/ansible_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Ansible, "Ansible");
    assert!(hits >= 8, "ansible 召回过低: {hits}/{total}");
}

#[test]
fn sweep_pulumi_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/pulumi_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Pulumi, "Pulumi");
    // pulumi/previewing 为顶级专属词，12 样例除弱信号负样本外应大量命中。
    assert!(hits >= 8, "pulumi 召回过低: {hits}/{total}");
}

#[test]
fn sweep_cloudformation_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/cloudformation_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::CloudFormation, "CloudFormation");
    // cloudformation/stack/rollback/progress 为栈事件强判别词。
    assert!(hits >= 8, "cloudformation 召回过低: {hits}/{total}");
}

#[test]
fn sweep_sql_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/sql_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Sql, "Sql");
    // SELECT/INSERT/FROM/WHERE 为 SQL 强判别词，12 样例除 empty/no_compress 等弱信号外应大量命中。
    assert!(hits >= 8, "sql 召回过低: {hits}/{total}");
}

#[test]
fn sweep_db_log_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/db_log_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::DbLog, "DbLog");
    // mysqld/innodb/postgres/redis 等引擎名为强判别，20 样例除 empty 等负样本外应大量命中。
    assert!(hits >= 15, "db_log 召回过低: {hits}/{total}");
}

#[test]
fn sweep_unity_unreal_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/unity_unreal_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::UnityUnreal, "UnityUnreal");
    // LogTemp/LogGame/Unity/AssetBundle 为引擎强判别；case_008（显式“非游戏日志”）/
    // case_015（实为 syslog）为负样本应归 generic，不应计入命中。故门槛对齐真实正样本（9）。
    assert!(hits >= 9, "unity_unreal 召回过低: {hits}/{total}");
}

#[test]
fn sweep_syslog_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/syslog_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::Syslog, "Syslog");
    // sshd/kernel/CRON/systemd 等守护进程名为强判别；empty 等负样本除外。
    assert!(hits >= 9, "syslog 召回过低: {hits}/{total}");
}

#[test]
fn sweep_ci_log_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/ci_log_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::CiLog, "CiLog");
    // jenkins/gitlab/buildkite/teamcity/travis/circleci 等平台词 + gh/group/section/checkout
    // 等编排锚点为强判别；纯包装构建输出（gradle/npm/pytest）可能被对应工具类别抢走，故门槛放中。
    assert!(hits >= 26, "ci_log 召回过低: {hits}/{total}");
}

#[test]
fn sweep_cloud_log_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/cloud_log_plugin");
    let (hits, total) = sweep_samples_in_dir(root, Category::CloudLog, "CloudLog");
    // 与 CiLog 同为「剥皮」类别，皮检率天然低：cloud_log 样本多为「云包装 + 内嵌第三方输出」，
    // 内嵌 HTTP access/java/python/node/syslog/db 会按语义被对应类别合法接管（不属漏检）。
    // 门槛只覆盖纯云基础设施包装（lambda/cloudtrail/flowlog/cloudwatch 表）这些稳定的皮阳性。
    assert!(hits >= 6, "cloud_log 召回过低: {hits}/{total}");
}

#[test]
fn classify_kubectl_error_edge_case() {
    let c = classifier();
    // 物理样本：Error from server (NotFound): pods "nonexistent" not found
    let sample = read_docker_sample("case_005_kubectl_error.log");
    let r = c.classify(&sample);
    assert_eq!(
        r.category,
        Category::DockerK8s,
        "kubectl 错误单行应分到 DockerK8s（pods 复数），got {:?}",
        r
    );
}

#[test]
fn classify_docker_compose_failure_edge_case() {
    let c = classifier();
    // 物理样本：compose 构建内嵌 RUN cargo test --lib 失败，docker 上下文应压制 test 词
    let sample = read_docker_sample("case_019_docker_compose_failure.log");
    let r = c.classify(&sample);
    assert_eq!(
        r.category,
        Category::DockerK8s,
        "compose 构建失败应分到 DockerK8s，got {:?}",
        r
    );
}

#[test]
fn candidate_plugin_mapping() {
    assert_eq!(Category::Cargo.candidate_plugins(), &["rust_go"]);
    assert_eq!(Category::Gcc.candidate_plugins(), &["gcc_log"]);
    assert_eq!(Category::Test.candidate_plugins(), &["pytest"]);
    assert_eq!(Category::GitDiff.candidate_plugins(), &["git_diff"]);
    assert_eq!(
        Category::DockerK8s.candidate_plugins(),
        &["kubernetes_docker"]
    );
    assert_eq!(
        Category::Node.candidate_plugins(),
        &["nodejs", "node_error"]
    );
    assert_eq!(Category::Web.candidate_plugins(), &["webpack_vite"]);
    assert_eq!(Category::Java.candidate_plugins(), &["java_stack"]);
    assert_eq!(Category::SpringBoot.candidate_plugins(), &["spring_boot"]);
    assert_eq!(Category::Maven.candidate_plugins(), &["maven"]);
    assert_eq!(Category::PhpRuby.candidate_plugins(), &["php_ruby"]);
    assert_eq!(Category::Dotnet.candidate_plugins(), &["dotnet"]);
    assert_eq!(Category::Helm.candidate_plugins(), &["helm"]);
    assert_eq!(Category::Terraform.candidate_plugins(), &["terraform"]);
    assert_eq!(Category::Golang.candidate_plugins(), &["rust_go"]);
    assert_eq!(Category::WebLog.candidate_plugins(), &["web_log"]);
    assert_eq!(
        Category::PythonTraceback.candidate_plugins(),
        &["python_traceback"]
    );
    assert_eq!(Category::Bazel.candidate_plugins(), &["bazel"]);
    assert_eq!(Category::Gradle.candidate_plugins(), &["android_gradle"]);
    assert_eq!(Category::Xcode.candidate_plugins(), &["xcode_log"]);
    assert_eq!(Category::Ansible.candidate_plugins(), &["ansible"]);
    assert_eq!(Category::Pulumi.candidate_plugins(), &["pulumi"]);
    assert_eq!(
        Category::CloudFormation.candidate_plugins(),
        &["cloudformation"]
    );
    assert_eq!(Category::Sql.candidate_plugins(), &["sql"]);
    assert_eq!(Category::DbLog.candidate_plugins(), &["db_log"]);
    assert_eq!(Category::UnityUnreal.candidate_plugins(), &["unity_unreal"]);
    assert_eq!(Category::Syslog.candidate_plugins(), &["syslog"]);
    assert_eq!(Category::CiLog.candidate_plugins(), &["ci_log"]);
    assert_eq!(Category::CloudLog.candidate_plugins(), &["cloud_log"]);
    assert!(Category::GenericText.candidate_plugins().is_empty());
}

#[test]
fn margin_reflects_separability() {
    let c = classifier();
    let cargo = c.classify("Compiling foo v1.0.0 error[E0308] expected u32 found &str");
    let generic = c.classify("the quick brown fox jumps over the lazy dog");
    // 明确类别样本应比无特征样本 margin 更大（可分性更强）
    assert!(
        cargo.margin > generic.margin,
        "cargo margin {} 应大于 generic margin {}",
        cargo.margin,
        generic.margin
    );
}

// =====================================================================
// holdout 泛化门禁（out-of-distribution）
// =====================================================================
//
// 与 `sweep_*`（samples/ 自洽回归）不同：本组测试用 `classifier_holdout/` ——与训练
// 语料 `samples/` 不同源的独立盲测语料根——评估分类器对**未知输出**的泛化能力。
// 分工语义见 `classifier_holdout/README.md` 与 `.trae/documents/classifier_holdout_blindtest.md`。
//
// 基线（2026-09 blind 盲测，`cargo run --bin classifier_holdout`）：
//   bayesian 整体 top1=0.867（52/60），平均 margin=0.855；structured 强断言 12/12 命中。
//   已知弱类别（Phase4 三方向修复目标，本轮不设高门槛虚惊）：
//     dotnet=0.000（误归 test）、generic_text=0.000（兜底类别，设计上几乎不"获胜"，预期）、
//     web=0.500（webpack→test 混）、pulumi=0.500（→cloud_log）、db_log=0.500（→docker_k8s）、
//     unity_unreal=0.500（→sql）。门槛值取基线留出余量，避免合法数据扩展时的体检式红灯。
#[cfg(test)]
mod holdout_gate {
    use super::classifier;
    use crate::cli::get_plugins;
    use crate::core::content_classifier::holdout::{
        bayesian_metrics, detect_with, holdout_root, load_bayesian_spec, load_structured_spec,
        run_bayesian_case,
    };
    use crate::core::plugin_dispatcher::Plugin;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    /// 读取盲测物理样本（AGENTS 红线：禁止 inline mock）。
    fn read_case(path: &Path) -> String {
        fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("读取盲测样本 {} 失败: {e}", path.display()))
    }

    /// 递归收集 `classifier_holdout/` 下全部文件路径（规范化分隔符）。
    fn all_holdout_files(root: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = fs::read_dir(root)
            .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", root.display()))
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                all_holdout_files(&p, out);
            } else {
                out.push(p.to_string_lossy().replace('\\', "/").to_lowercase());
            }
        }
    }

    /// 根因隔离 guard：`classifier_holdout/**` 必须与 `samples/` 路径不相交，
    /// 否则会重新引入「训练/测试同源泄漏」，sweep 自评与泛化门禁同时失准。
    #[test]
    fn holdout_corpus_isolation() {
        let root = holdout_root();
        assert!(root.is_dir(), "holdout 语料根缺失: {}", root.display());
        let mut files = Vec::new();
        all_holdout_files(&root, &mut files);
        assert!(!files.is_empty(), "holdout 语料为空，无法做隔离校验");
        for f in &files {
            assert!(
                !f.contains("/samples/") && !f.contains("samples/"),
                "holdout 样本不得位于 samples/ 训练语料下，block: {f}"
            );
        }
    }

    /// 贝叶斯泛化门禁：整体 top1 准确率阈值（基线 0.867，取 0.80 留余量，见模块头注释）。
    /// 拒绝「整体泛化能力退化」，但不过度收紧导致合法数据扩展虚惊。
    #[test]
    fn holdout_bayesian_recall_gate() {
        let root = holdout_root();
        let c = classifier();
        let spec = load_bayesian_spec(&root);
        assert!(!spec.is_empty(), "bayesian 盲测语料为空，无法评估泛化");
        let mut results = Vec::new();
        for case in &spec {
            let text = read_case(&case.path);
            results.push(run_bayesian_case(&c, case.expected.clone(), &text));
        }
        let rep = bayesian_metrics(&results);
        // 门槛：0.80（基线 0.867）。数据合理扩展时可按体检式调整门槛，但须同步注释。
        const TOP1_GATE: f64 = 0.80;
        assert!(
            rep.top1_acc >= TOP1_GATE,
            "bayesian 泛化 top1={:.3} 低于门禁 {:.3}（弱类别见模块头注释）",
            rep.top1_acc,
            TOP1_GATE
        );
        // 供调试：打印逐类 recall，便于定位回归类别。
        let mut rows: Vec<_> = rep.recall.iter().collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));
        for (name, rc) in rows {
            let v = if rc.is_nan() { f64::NAN } else { *rc };
            println!("[holdout] bayesian recall {name:<18} {v:.3}");
        }
    }

    /// 结构化泛化门禁：`structured/<fmt>/*`（非 gap_probes）的目标插件 `detect()` 必须命中。
    /// 0/1 布尔断言，无阈值脆弱性；基线 12/12（2026-09）。
    #[test]
    fn holdout_structured_detect_gate() {
        let root = holdout_root();
        let plugins = get_plugins();
        let mut map: HashMap<String, &dyn Plugin> = HashMap::new();
        for p in &plugins {
            map.insert(p.name().to_string(), p.as_ref());
        }
        let spec = load_structured_spec(&root);
        let mut asserted = 0usize;
        let mut passed = 0usize;
        for case in &spec {
            if case.probe {
                continue; // gap_probes：只记录、不判定（已知缺口待 Phase4 处置）
            }
            asserted += 1;
            let text = read_case(&case.path);
            let hit = detect_with(&map, &case.expected_plugin, &text).is_some();
            if hit {
                passed += 1;
            } else {
                eprintln!(
                    "[holdout] 结构化漏检 {} 期望插件={}",
                    case.path.display(),
                    case.expected_plugin
                );
            }
        }
        assert!(
            asserted > 0,
            "无结构化强断言样本（checker 路径 or gap_probes 误入）"
        );
        assert_eq!(
            passed, asserted,
            "结构化 detect 强断言通过率 {passed}/{asserted}，应全数命中"
        );
    }
}

/// P2-26 守护测试：种子特征表与噪声表不得再漂移——`tokenize_unigram` 先滤噪声
/// 再入表，任何 `is_noise_word` 命中的种子词都是永远匹配不上的死权重（曾实存
/// 67 条 / 死权重 290，拉大平滑分母收窄对比度）。遍历全部种子词断言：
/// ① 非噪声词；② `tokenize` 能产出该词自身。
#[test]
fn p2_26_seed_features_contain_no_noise_words() {
    use super::corpus_tokens::{is_noise_word, tokenize};
    use super::features::seed_features;

    let seeds = seed_features();
    assert!(!seeds.is_empty(), "种子特征表不得为空");
    let mut checked = 0usize;
    for (category, table) in seeds.iter() {
        // GenericText 是刻意空表的兜底类别（见 seed_features 注释），跳过。
        if *category == Category::GenericText {
            continue;
        }
        assert!(!table.is_empty(), "类别 {category:?} 的特征表不得为空");
        for word in table.keys() {
            assert!(
                !is_noise_word(word),
                "种子特征 {category:?}::{word} 是噪声词——永远匹配不上的死权重，禁止入表"
            );
            assert!(
                tokenize(word).iter().any(|t| t == word),
                "种子特征 {category:?}::{word} 无法经 tokenize 产出（分词器漂移）"
            );
            checked += 1;
        }
    }
    assert!(checked >= 400, "种子特征词条数异常（checked={checked}），两表对账可能失真");
}
