//! content classifier 模型实现
//!
//! 实现纯标准库的多分类朴素贝叶斯分类器（`NaiveBayesClassifier`）。包含：
//! - [`Category`]：语义类别枚举，携带建议候选插件映射。
//! - [`NaiveBayesClassifier`]：基于词频 + 对数概率 + 拉普拉斯平滑的分类器。
//! - 置信度计算：将各类别对数后验做 softmax 归一化为概率分布，取最大值为置信度。

use crate::core::content_classifier::corpus_tokens;

use std::collections::HashMap;

/// 语义类别。对应各类命令输出 / 日志块的语义归属，用于路由到专用压缩插件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// cargo 构建 / 检查 / 运行输出（Rust 工具链）
    Cargo,
    /// gcc / g++ / make / ld 等 C/C++ 编译输出
    Gcc,
    /// 测试运行器输出（pytest / cargo test）
    Test,
    /// git diff / patch 输出
    GitDiff,
    /// docker / kubernetes(kubectl) 命令输出
    DockerK8s,
    /// node 生态输出（npm/yarn/pnpm 包管理、webpack/eslint/tsc 构建）、node 运行错误栈
    Node,
    /// 前端构建输出（webpack / vite，含 Vue/Svelte 等框架）
    Web,
    /// JVM 运行栈异常输出（Java 栈 trace）
    Java,
    /// Spring Boot 应用启动 / 运行日志（含内嵌异常栈）
    SpringBoot,
    /// Maven 构建 / 依赖 / 生命周期输出（JVM 构建链）
    Maven,
    /// PHP / Ruby（Rails/Laravel）脚本与框架运行时输出
    PhpRuby,
    /// .NET / MSBuild / C# 构建、测试运行与托管栈输出
    Dotnet,
    /// Helm chart 打包 / 安装 / 渲染 / 回滚输出
    Helm,
    /// Terraform 计划 / 应用 / 销毁 / 导入与状态输出
    Terraform,
    /// Go 构建 / 测试 / goroutine panic 栈输出
    Golang,
    /// HTTP 访问日志（apache/nginx/CDN/IIS，含方法、状态码、UA、referer 结构）
    WebLog,
    /// Python 解释器 traceback 异常栈输出
    PythonTraceback,
    /// Bazel 构建 / 分析 / 执行动作输出
    Bazel,
    /// Android Gradle 任务构建输出（`> Task :` 前缀 + BUILD SUCCESSFUL）
    Gradle,
    /// Xcode / Apple 构建输出（CompileC / DerivedData / arch）
    Xcode,
    /// Ansible playbook 执行输出（PLAY / TASK / PLAY RECAP）
    Ansible,
    /// Pulumi 资源编排输出（preview / up / Resources）
    Pulumi,
    /// AWS CloudFormation 栈事件输出（CREATE_*/ROLLBACK/DEPLOY）
    CloudFormation,
    /// SQL 查询 / 脚本执行输出（psql/mysql CLI，SELECT/INSERT/JOIN 谓词）
    Sql,
    /// 数据库服务器守护日志（mysqld/InnoDB/复制事件）
    DbLog,
    /// Unity / Unreal 引擎日志（LogTemp/LogGame 前缀 + Display/Warning/Error）
    UnityUnreal,
    /// 系统守护进程日志（RFC3164 前缀 `月 日 时:分:秒 hostname daemon[pid]:` + sshd/kernel/CRON）
    Syslog,
    /// CI/CD 流水线编排日志（GitHub Actions/GitLab/Jenkins/Azure/Azure DevOps/Buildkite/Travis/CircleCI/TeamCity）
    CiLog,
    /// 云平台日志（AWS CloudWatch/GCP/Azure/Aliyun/OCI/Tencent/Huawei/Cloudflare，logGroup/logName/logStream/RequestId）。
    /// 与 CiLog 同为「剥皮」类别：命中后由 cloud_log_plugin 剥外层，内嵌 HTTP/栈内容交内层插件。
    CloudLog,
    /// 通用文本（兜底，信息保持为主）
    GenericText,
}

impl Category {
    /// 全部语义类别，用于训练阶段遍历。
    pub const ALL: [Category; 30] = [
        Category::Cargo,
        Category::Gcc,
        Category::Test,
        Category::GitDiff,
        Category::DockerK8s,
        Category::Node,
        Category::Web,
        Category::Java,
        Category::SpringBoot,
        Category::Maven,
        Category::PhpRuby,
        Category::Dotnet,
        Category::Helm,
        Category::Terraform,
        Category::Golang,
        Category::WebLog,
        Category::PythonTraceback,
        Category::Bazel,
        Category::Gradle,
        Category::Xcode,
        Category::Ansible,
        Category::Pulumi,
        Category::CloudFormation,
        Category::Sql,
        Category::DbLog,
        Category::UnityUnreal,
        Category::Syslog,
        Category::CiLog,
        Category::CloudLog,
        Category::GenericText,
    ];

    /// 类别的稳定名称（用于日志、审计与特征表索引）。
    pub fn name(&self) -> &'static str {
        match self {
            Category::Cargo => "cargo",
            Category::Gcc => "gcc",
            Category::Test => "test",
            Category::GitDiff => "git_diff",
            Category::DockerK8s => "docker_k8s",
            Category::Node => "node",
            Category::Web => "web",
            Category::Java => "java",
            Category::SpringBoot => "spring_boot",
            Category::Maven => "maven",
            Category::PhpRuby => "php_ruby",
            Category::Dotnet => "dotnet",
            Category::Helm => "helm",
            Category::Terraform => "terraform",
            Category::Golang => "golang",
            Category::WebLog => "web_log",
            Category::PythonTraceback => "python_traceback",
            Category::Bazel => "bazel",
            Category::Gradle => "gradle",
            Category::Xcode => "xcode",
            Category::Ansible => "ansible",
            Category::Pulumi => "pulumi",
            Category::CloudFormation => "cloudformation",
            Category::Sql => "sql",
            Category::DbLog => "db_log",
            Category::UnityUnreal => "unity_unreal",
            Category::Syslog => "syslog",
            Category::CiLog => "ci_log",
            Category::CloudLog => "cloud_log",
            Category::GenericText => "generic_text",
        }
    }

    /// 该类别「建议优先尝试」的候选插件名。
    ///
    /// 返回空切片表示无专用插件（如 `GenericText`），此时应由调用方回退到全量 detect。
    /// 插件名为空时，仅用于让调度器优先排前，不强制屏蔽其他候选。
    pub fn candidate_plugins(&self) -> &'static [&'static str] {
        match self {
            Category::Cargo => &["rust_go"],
            Category::Gcc => &["gcc_log"],
            Category::Test => &["pytest"],
            Category::GitDiff => &["git_diff"],
            Category::DockerK8s => &["kubernetes_docker"],
            Category::Node => &["nodejs", "node_error"],
            Category::Web => &["webpack_vite"],
            Category::Java => &["java_stack"],
            Category::SpringBoot => &["spring_boot"],
            Category::Maven => &["maven"],
            Category::PhpRuby => &["php_ruby"],
            Category::Dotnet => &["dotnet"],
            Category::Helm => &["helm"],
            Category::Terraform => &["terraform"],
            Category::Golang => &["rust_go"],
            Category::WebLog => &["web_log"],
            Category::PythonTraceback => &["python_traceback"],
            Category::Bazel => &["bazel"],
            Category::Gradle => &["android_gradle"],
            Category::Xcode => &["xcode_log"],
            Category::Ansible => &["ansible"],
            Category::Pulumi => &["pulumi"],
            Category::CloudFormation => &["cloudformation"],
            Category::Sql => &["sql"],
            Category::DbLog => &["db_log"],
            Category::UnityUnreal => &["unity_unreal"],
            Category::Syslog => &["syslog"],
            Category::CiLog => &["ci_log"],
            Category::CloudLog => &["cloud_log"],
            Category::GenericText => &[],
        }
    }

    /// 根据字符串名反查类别；未知名回退 `GenericText`。
    pub fn from_name(name: &str) -> Category {
        match name {
            "cargo" => Category::Cargo,
            "gcc" => Category::Gcc,
            "test" => Category::Test,
            "git_diff" => Category::GitDiff,
            "docker_k8s" => Category::DockerK8s,
            "node" => Category::Node,
            "web" => Category::Web,
            "java" => Category::Java,
            "spring_boot" => Category::SpringBoot,
            "maven" => Category::Maven,
            "php_ruby" => Category::PhpRuby,
            "dotnet" => Category::Dotnet,
            "helm" => Category::Helm,
            "terraform" => Category::Terraform,
            "golang" => Category::Golang,
            "web_log" => Category::WebLog,
            "python_traceback" => Category::PythonTraceback,
            "bazel" => Category::Bazel,
            "gradle" => Category::Gradle,
            "xcode" => Category::Xcode,
            "ansible" => Category::Ansible,
            "pulumi" => Category::Pulumi,
            "cloudformation" => Category::CloudFormation,
            "sql" => Category::Sql,
            "db_log" => Category::DbLog,
            "unity_unreal" => Category::UnityUnreal,
            "syslog" => Category::Syslog,
            "ci_log" => Category::CiLog,
            "cloud_log" => Category::CloudLog,
            _ => Category::GenericText,
        }
    }
}

/// 分类结果：得分类别、置信度（0~1 概率）与 `margin`（与次优类别的置信度差距）。
#[derive(Debug, Clone)]
pub struct ClassifyResult {
    pub category: Category,
    pub confidence: f32,
    /// 最高置信度与次高置信度之差，用于表示分类的可分性。
    pub margin: f32,
}

/// F-3：特征词表定型后一次性预计算的对数似然快照。
///
/// 保存「类别 → 词 → ln((count+α)/denom)」查表与未见词（OOV）的平滑对数 `ln(α/denom)`，
/// 供 [`NaiveBayesClassifier::classify`] 用查表累加替代逐 token 的 `ln()` 计算，消除推理期热点。
/// 快照与实时计算的表达式完全一致（同 `denom`、同 `α`），保证分类语义逐位不变。
struct LogLikelihood {
    /// 类别 → 词 → 预计算对数概率。
    table: HashMap<Category, HashMap<String, f64>>,
    /// 未见词平滑对数 `ln(α/denom)`（对所有类别通用，等价于 count=0 时的查表项）。
    oov: f64,
}

/// 极简多分类朴素贝叶斯分类器（纯 std）。
///
/// 存储每个类别下「单词 → 出现次数」的统计表，推理时对输入文本分词后，
/// 逐个类别累加词频对数概率，并叠加拉普拉斯平滑避免未见词归零。
pub struct NaiveBayesClassifier {
    /// 类别 → 单词 → 出现次数
    word_counts: HashMap<Category, HashMap<String, f64>>,
    /// 全类别单词出现次数总和（全局分母用）
    corpus_word_total: f64,
    /// 全局词汇表大小（所有类别去重后的不同单词数），用于拉普拉斯平滑分母
    vocab_size: f64,
    /// 拉普拉斯平滑系数 alpha
    smoothing: f64,
    /// F-3：对数似然预计算快照；`None` 表示尚未定型（回退实时计算，在线 append 场景安全）。
    log_likelihood: Option<LogLikelihood>,
}

impl Default for NaiveBayesClassifier {
    /// 默认分类器：使用 [`crate::core::content_classifier::features::seed_model`] 的内置种子特征表。
    fn default() -> Self {
        crate::core::content_classifier::features::seed_model()
    }
}

impl NaiveBayesClassifier {
    /// 从特征表构建分类器。
    ///
    /// # 参数
    /// - `category_features`: 每个类别对应的「单词 → 出现次数」映射，`GenericText` 可为空表。
    /// - `smoothing`: 拉普拉斯平滑系数，默认取 1.0。
    ///
    /// # 返回
    /// 已计算的分类器模型。
    pub fn from_features(
        category_features: HashMap<Category, HashMap<String, f64>>,
        smoothing: f64,
    ) -> Self {
        let mut corpus_word_total = 0.0;
        let mut vocab: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (_, words) in &category_features {
            corpus_word_total += words.values().sum::<f64>();
            for w in words.keys() {
                vocab.insert(w.clone());
            }
        }
        // 平滑系数不能为负
        let smoothing = if smoothing < 0.0 { 1.0 } else { smoothing };
        // 分母需非零：全局单词数为 0 时退化为 1，避免除零
        let corpus_word_total = if corpus_word_total > 0.0 {
            corpus_word_total
        } else {
            1.0
        };

        NaiveBayesClassifier {
            word_counts: category_features,
            corpus_word_total,
            vocab_size: vocab.len() as f64,
            smoothing,
            // F-3：初始不预计算；词表经增量 append 定型后由外部调用 build_likelihood_table() 触发。
            log_likelihood: None,
        }
    }

    /// F-3：在特征词表定型后，一次性预计算「类别 → 词 → ln((count+α)/denom)」表与 OOV 常数。
    ///
    /// 仅在构建期词表不再变化后调用（例如 [`crate::core::content_classifier::features::seed_model`]
    /// 完成增量合并后）。之后 [`classify`](Self::classify) 走查表路径；若词表再被
    /// [`append_features`](Self::append_features) 修改，快照会失效并回退实时计算，保证安全。
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn build_likelihood_table(&mut self) {
        let denom = self.corpus_word_total + self.smoothing * self.vocab_size;
        let oov = (self.smoothing / denom).ln();
        let mut table = HashMap::with_capacity(self.word_counts.len());
        for (cat, words) in &self.word_counts {
            let mut inner = HashMap::with_capacity(words.len());
            for (w, count) in words {
                inner.insert(w.clone(), ((count + self.smoothing) / denom).ln());
            }
            table.insert(*cat, inner);
        }
        self.log_likelihood = Some(LogLikelihood { table, oov });
    }

    /// 对输入文本块做多分类，返回语义类别与置信度。
    ///
    /// # 算法
    /// 对每个类别计算对数后验：
    /// `log P(c|d) ∝ log prior_c + Σ_t log((count(c,t)+α) / (total_c + α·V))`
    /// 采用均匀先验（各类别相等），随后将各类别得分做 softmax 归一化为概率分布，
    /// 取最大概率作为类别与置信度，并计算与次优类别的差距 `margin`。
    ///
    /// # 参数
    /// - `text`: 待分类的文本块。
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn classify(&self, text: &str) -> ClassifyResult {
        let tokens = corpus_tokens::tokenize(text);
        let mut scores: Vec<(Category, f64)> = Vec::with_capacity(Category::ALL.len());
        let mut max_lp: f64 = f64::NEG_INFINITY;

        // 全局分母：所有类别共享，保证「未见词（OOV）」对所有类别贡献相同的平滑基线，
        // 从而让语义差异完全来自「特征词命中」，杜绝空类别（如 GenericText）被 OOV 抬升。
        let denom = self.corpus_word_total + self.smoothing * self.vocab_size;

        // F-3：词表定型后走「查表累加」路径（已预计算 ln((count+α)/denom)，避免逐 token 的 ln()）；
        // 否则回退「实时计算」路径，保证在线 append 增量场景语义一致。
        if let Some(ll) = &self.log_likelihood {
            for cat in Category::ALL {
                let inner = ll.table.get(&cat);
                // 均匀先验：每类 log(1/N) 相等，可忽略，仅保留词频部分。
                let mut lp: f64 = 0.0;
                for t in &tokens {
                    lp += inner.and_then(|m| m.get(t)).copied().unwrap_or(ll.oov);
                }
                scores.push((cat, lp));
                if lp > max_lp {
                    max_lp = lp;
                }
            }
        } else {
            for cat in Category::ALL {
                let cat_words = self.word_counts.get(&cat);
                // 均匀先验：每类 log(1/N) 相等，可忽略，仅保留词频部分。
                let mut lp: f64 = 0.0;
                for t in &tokens {
                    let count = cat_words.and_then(|m| m.get(t)).copied().unwrap_or(0.0);
                    lp += ((count + self.smoothing) / denom).ln();
                }
                scores.push((cat, lp));
                if lp > max_lp {
                    max_lp = lp;
                }
            }
        }

        // softmax 归一化（减 max 保证数值稳定）
        let probs: Vec<(Category, f64)> = scores
            .into_iter()
            .map(|(cat, lp)| (cat, (lp - max_lp).exp()))
            .collect();
        let sum: f64 = probs.iter().map(|(_, p)| p).sum();

        // 依概率降序排序，用于取榜首与次优；并列判定用相对容差 EPS 吸收浮点噪声。
        let mut ranked: Vec<(Category, f64)> = probs;
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if std::env::var("TS_CC_DBG").is_ok() {
            eprintln!("[cc-dbg] tokens={:?}", tokens);
            eprintln!(
                "[cc-dbg] probs={:?} denom={}",
                ranked
                    .iter()
                    .map(|(c, p)| format!("{}:{:.4}", c.name(), p / sum))
                    .collect::<Vec<_>>(),
                denom
            );
        }

        // 确定性 argmax：当榜首与 GenericText「几乎并列」时，优先 GenericText。
        // 排序不等价稳定，全类别无信号（概率均匀）时若直接取 ranked[0] 会挑到任意类别；
        // 而 GenericText 作为兜底/无特征类别，在无区分度时应胜出。
        const EPS: f64 = 1e-6;
        let top_p = ranked.first().map(|&(_, p)| p).unwrap_or(0.0);
        let generic_tied = ranked
            .iter()
            .any(|&(c, p)| c == Category::GenericText && (p - top_p).abs() <= EPS);

        let (category, top_prob) = if generic_tied {
            // 无信号 → 归入兜底类别，置信度即榜首分数（Uniform 下 ≈ 1/N）。
            (Category::GenericText, (top_p / sum) as f32)
        } else {
            let (cat, p) = ranked[0];
            (cat, (p / sum) as f32)
        };

        // 次优：取「非榜首」类别中的最高概率；兜底提升后取称手类别。
        let second_prob = ranked
            .iter()
            .find(|&&(c, _)| c != category)
            .map(|&(_, p)| (p / sum) as f32)
            .unwrap_or(0.0);

        ClassifyResult {
            category,
            confidence: top_prob,
            margin: (top_prob - second_prob).max(0.0),
        }
    }

    /// 分类入口辅助：当最高置信度低于 `threshold` 时，视为「不确定」。
    /// 供插件调度用于决定是否回退到全量 detect。
    pub fn is_confident(&self, text: &str, threshold: f32) -> bool {
        self.classify(text).confidence >= threshold
    }

    /// 运行期增量追加特征（计划 T-E 增量学习）。
    ///
    /// 将给定类别下的「词 → 权重」合并进模型，并随之累加全局分母
    /// `corpus_word_total` 与全局词表大小 `vocab_size`，使新特征参与后续
    /// 的拉普拉斯平滑与 softmax 分类。供 [`crate::core::content_classifier::features::seed_model`]
    /// 在构建时回填外部特征库，以及 long-lived 实例在线学习使用。
    ///
    /// `weight` 为正时视为追加证据（累加）；传入仅用于覆盖/削权的零值词会略过
    /// 词表增量，但极简场景不建议用本方法削权。
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn append_features<I>(&mut self, category: Category, terms: I)
    where
        I: IntoIterator<Item = (String, f64)>,
    {
        // F-3：增量会改变词表与平滑分母，已预计算的对数似然快照随之失效；
        // 置 None 使 classify 回退实时计算。构建期可于全部 append 后再次 build_likelihood_table()。
        self.log_likelihood = None;
        let cat_counts = self.word_counts.entry(category).or_default();
        let mut delta_total: f64 = 0.0;
        for (word, weight) in terms {
            if weight <= 0.0 {
                continue;
            }
            let old = cat_counts.get(&word).copied().unwrap_or(0.0);
            let new = old + weight;
            if old == 0.0 {
                // 新增词：扩大全局词表，参与平滑分母
                self.vocab_size += 1.0;
            }
            cat_counts.insert(word, new);
            delta_total += weight;
        }
        if delta_total > 0.0 {
            self.corpus_word_total += delta_total;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个覆盖「命中词 / OOV / 空类别」的小模型，返回两实例：
    /// - `live`：不预计算，classify 走实时计算路径；
    /// - `lookup`：词表定型后调用 build_likelihood_table，classify 走查表路径。
    fn make_pair() -> (NaiveBayesClassifier, NaiveBayesClassifier) {
        use std::collections::HashMap;

        let mut gcc = HashMap::new();
        gcc.insert("error".to_string(), 12.0);
        gcc.insert("cc1".to_string(), 8.0);
        gcc.insert("undefined".to_string(), 9.0);
        gcc.insert("reference".to_string(), 7.0);

        let mut test = HashMap::new();
        test.insert("test".to_string(), 10.0);
        test.insert("passed".to_string(), 7.0);
        test.insert("failed".to_string(), 7.0);
        test.insert("skipped".to_string(), 4.0);

        let mut generic = HashMap::new();

        let table: HashMap<Category, HashMap<String, f64>> = [
            (Category::Gcc, gcc),
            (Category::Test, test),
            (Category::GenericText, generic),
        ]
        .into_iter()
        .collect();

        let mut live = NaiveBayesClassifier::from_features(table.clone(), 1.0);
        let mut lookup = NaiveBayesClassifier::from_features(table, 1.0);
        lookup.build_likelihood_table();
        (live, lookup)
    }

    /// F-3 核心保证：预计算查表路径与实时计算路径对同一输入产出逐位一致的分类结果。
    /// 样本覆盖强命中（gcc/test）、大量 OOV（novel log）、以及空类别兜底兜底，
    /// 确保两路径的平滑分母、OOV 基线、softmax 数值完全对齐。
    #[test]
    fn precomputed_lookup_bitwise_matches_live_computation() {
        let (live, lookup) = make_pair();
        let samples = [
            // 命中 gcc 特征词
            "error: undefined reference to `main' in file.c",
            // 命中 test 摘要特征词
            "test session passed 2 failed 1 skipped 3",
            // 大量 OOV：平滑基线/分母必须一致
            "novel foo bar baz qux zebra alpha beta gamma a",
            // 全部 OOV 且极短（空类别 GenericText 兜底路径）
            "hello world",
            // gcc 与 test 混排，验证 softmax 归一化数值一致
            "error: test failed in compile phase passed none",
        ];
        for sample in samples {
            let a = live.classify(sample);
            let b = lookup.classify(sample);
            assert_eq!(
                a.category,
                b.category,
                "类别不一致 at `{sample}`：live={} lookup={}",
                a.category.name(),
                b.category.name()
            );
            assert_eq!(
                a.confidence.to_bits(),
                b.confidence.to_bits(),
                "confidence 逐位不一致 at `{sample}`"
            );
            assert_eq!(
                a.margin.to_bits(),
                b.margin.to_bits(),
                "margin 逐位不一致 at `{sample}`"
            );
        }
    }

    /// F-3 安全护栏：词表经 append_features 增量后，快照必须失效并回退实时计算，
    /// 新词能即时参与分类（防「快照过时」导致在线学习失效）。
    #[test]
    fn append_invalidates_snapshot_and_new_word_takes_effect() {
        let (mut live, mut lookup) = make_pair();
        lookup.build_likelihood_table();
        // 在线增量：给 Test 加入新判别词 `assertion`，两实例同步追加。
        // 若 lookup 的快照未被置失效，将仍按旧词表（无 assertion）分类而与 live 分道扬镳。
        let terms = [("assertion".to_string(), 20.0)];
        live.append_features(Category::Test, terms.clone());
        lookup.append_features(Category::Test, terms);
        // assert 仅作为这条 "assertion" 的唯一强特征，应主导分类为 Test，
        // 证明增量词已即时参与、快照未残留旧值。
        let b = lookup.classify("assertion assertion assertion");
        assert_eq!(
            b.category,
            Category::Test,
            "增量词未参与分类，快照未正确失效"
        );
    }
}
