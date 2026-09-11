//! Android/Gradle 插件方法实现

use super::types::AndroidGradlePlugin;
use crate::core::dictionary_engine::DictionaryEngine;

impl AndroidGradlePlugin {
    /// 通用 Gradle 日志压缩：提取 `> Task ` 任务行、下载行与构建状态，折叠为紧凑的 `[GRADLE]` 摘要。
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn optimize_generic_gradle(&self, text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let task_lines: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|line| line.trim_start().starts_with("> Task "))
            .collect();
        if task_lines.len() < 5 {
            return text.to_string();
        }

        let mut failed = Vec::new();
        let mut up_to_date = 0usize;
        let mut from_cache = 0usize;
        let mut skipped = 0usize;
        let mut no_source = 0usize;
        let mut executed = 0usize;
        let mut actionable_summary = None;
        let mut actionable_total = None;
        for line in &task_lines {
            let trimmed = line.trim();
            if trimmed.contains(" FAILED") {
                failed.push(trimmed.to_string());
            } else if trimmed.contains(" UP-TO-DATE") {
                up_to_date += 1;
            } else if trimmed.contains(" FROM-CACHE") {
                from_cache += 1;
            } else if trimmed.contains(" SKIPPED") {
                skipped += 1;
            } else if trimmed.contains(" NO-SOURCE") {
                no_source += 1;
            } else {
                executed += 1;
            }
        }
        for line in &lines {
            if let Some(summary) = self.parse_actionable_summary(line.trim()) {
                actionable_summary = Some(summary);
                break;
            }
        }
        if let Some((summary_executed, summary_up_to_date, summary_from_cache, summary_skipped)) =
            actionable_summary
        {
            executed = summary_executed;
            up_to_date = summary_up_to_date;
            from_cache = summary_from_cache;
            skipped = summary_skipped;
            // 可见任务行数可能少于头行的 actionable 总数（部分任务未逐行打印）；
            // 以头行总数作为 `tasks=`，保证摘要与原文计数一致（SAP-0001）。
            actionable_total =
                Some(summary_executed + summary_up_to_date + summary_from_cache + summary_skipped);
        }

        let downloads = lines
            .iter()
            .filter(|line| line.trim_start().starts_with("Download "))
            .count();

        let mut result = Vec::new();
        for line in self.leading_semantic_context(&lines) {
            self.push_unique(&mut result, line);
        }
        result.push(self.gradle_task_summary(
            actionable_total.unwrap_or(task_lines.len()),
            executed,
            up_to_date,
            from_cache,
            skipped,
            no_source,
            failed.len(),
        ));
        if downloads > 0 {
            result.push(format!("[GRADLE] downloads={downloads}"));
        }
        for line in failed {
            self.push_unique(&mut result, &line);
        }
        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with("> Task ") || trimmed.starts_with("Download ") {
                continue;
            }
            if trimmed.starts_with("BUILD ")
                || trimmed.starts_with("FAILURE:")
                || trimmed.starts_with("* What went wrong:")
                || trimmed.starts_with("Execution failed")
                || trimmed.contains(" actionable tasks:")
                || trimmed.starts_with("Gradle build daemon")
                || self.is_diagnostic_detail(trimmed)
                || self.is_ci_gradle_signal(trimmed)
            {
                self.push_unique(&mut result, trimmed);
            }
        }

        let compacted = result.join("\n");
        crate::core::utils::roi::prefer_non_expanding(text, compacted)
    }

    /// 将任务统计（执行/最新/缓存/跳过/无源/失败）格式化为单行 `[GRADLE] ...` 摘要串。
    #[tracing::instrument(level = "trace", skip_all)]
    fn gradle_task_summary(
        &self,
        tasks: usize,
        executed: usize,
        up_to_date: usize,
        from_cache: usize,
        skipped: usize,
        no_source: usize,
        failed: usize,
    ) -> String {
        let mut parts = vec![format!("tasks={tasks}"), format!("executed={executed}")];
        if up_to_date > 0 {
            parts.push(format!("up_to_date={up_to_date}"));
        }
        if from_cache > 0 {
            parts.push(format!("from_cache={from_cache}"));
        }
        if skipped > 0 {
            parts.push(format!("skipped={skipped}"));
        }
        if no_source > 0 {
            parts.push(format!("no_source={no_source}"));
        }
        if failed > 0 {
            parts.push(format!("failed={failed}"));
        }
        format!("[GRADLE] {}", parts.join(" "))
    }

    /// 从 `actionable tasks:` 行用正则解析出 executed/up-to-date/from cache/skipped 四项计数。
    #[tracing::instrument(level = "trace", skip_all)]
    fn parse_actionable_summary(&self, line: &str) -> Option<(usize, usize, usize, usize)> {
        if !line.contains(" actionable tasks:") {
            return None;
        }
        let mut executed = 0usize;
        let mut up_to_date = 0usize;
        let mut from_cache = 0usize;
        let mut skipped = 0usize;
        let tail = line.split_once(':')?.1;
        let pattern = regex::Regex::new(
            r"(?P<count>\d+)\s+(?P<label>executed|up-to-date|from cache|skipped)",
        )
        .ok()?;
        for cap in pattern.captures_iter(tail) {
            let count = cap.name("count")?.as_str().parse::<usize>().ok()?;
            match cap.name("label")?.as_str() {
                "executed" => executed = count,
                "up-to-date" => up_to_date = count,
                "from cache" => from_cache = count,
                "skipped" => skipped = count,
                _ => {}
            }
        }
        Some((executed, up_to_date, from_cache, skipped))
    }

    /// 提取首条 `> Task ` 之前的前导语义上下文行（跳过空行与下载行）作为压缩保留头。
    #[tracing::instrument(level = "trace", skip_all)]
    fn leading_semantic_context<'a>(&self, lines: &'a [&'a str]) -> Vec<&'a str> {
        let mut context = Vec::new();
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("Download ") {
                continue;
            }
            if trimmed.starts_with("> Task ") {
                if context.is_empty() {
                    context.push(trimmed);
                }
                break;
            }
            context.push(trimmed);
        }
        context
    }

    /// 将一行追加到结果集，仅当该行尚未存在（去重，避免重复信号行）。
    #[tracing::instrument(level = "trace", skip_all)]
    fn push_unique(&self, result: &mut Vec<String>, line: &str) {
        if !result.iter().any(|existing| existing == line) {
            result.push(line.to_string());
        }
    }

    /// 判断一行是否为诊断细节（warning/java./org./at / 各类错误与异常签名等），需保留。
    #[tracing::instrument(level = "trace", skip_all)]
    fn is_diagnostic_detail(&self, line: &str) -> bool {
        let lower = line.to_ascii_lowercase();
        line.starts_with("warning:")
            || line.starts_with("e: ")
            || line.starts_with("java.")
            || line.starts_with("org.")
            || line.starts_with("at ")
            || line.starts_with("com.")
            || line.starts_with("Error in ")
            || line.starts_with("Signed APK:")
            || line.starts_with("Zip aligning ")
            || line.starts_with("Zip aligned APK:")
            || line.starts_with("> A failure occurred while executing ")
            || line.starts_with("D8:") // D8 dex 编译冲突（如 Program type already present）为决定性诊断信息，必须保留
            || line.starts_with("Deprecated Gradle features")
            || line.starts_with("You can use '--warning-mode")
            || line.starts_with("Publishing build scan")
            || line.starts_with("http://")
            || line.starts_with("https://")
            || line.contains("AssertionError")
            || line.contains("AssertionFailedError")
            || lower.contains("expected")
            || lower.contains("unresolved reference")
            || lower.contains("type mismatch")
            || lower.contains("not found")
    }

    /// 判断一行是否为 CI/Gradle 信号（GitHub Actions/GitLab/Jenkins/Azure/Bitrise/Buildkite/CircleCI 等 runner 与失败标记）。
    #[tracing::instrument(level = "trace", skip_all)]
    fn is_ci_gradle_signal(&self, line: &str) -> bool {
        let lower = line.to_ascii_lowercase();
        line.starts_with("Run ./gradlew")
            || line.starts_with("$ ./gradlew")
            || line.starts_with("GitHub Actions runner ")
            || line.starts_with("Current runner version:")
            || line.starts_with("Running with gitlab-runner ")
            || line.starts_with("Preparing the ")
            || line.starts_with("Using Docker image ")
            || line.starts_with("Azure Pipelines hosted agent ")
            || line.starts_with("Bitrise step ")
            || line.starts_with("::error")
            || line.starts_with("ERROR: Job failed")
            || line.starts_with("Error: Process completed with exit code")
            || line.starts_with("There were failing tests.")
            || line.starts_with("See the report at:")
            || line.starts_with("Buildkite agent ")
            || line.starts_with("CircleCI received job ")
            || (line.starts_with("Starting ") && lower.contains(" tests on "))
            || (line.starts_with("Finished ") && lower.contains(" tests on "))
            || line.contains(" FAILED")
            || line.contains("FAILURE")
            || lower.contains("tests failed")
            || lower.contains("failed test")
            || lower.contains("connectedandroidtest")
            || lower.contains("test report")
    }

    /// 折叠 Jenkins 环境变量块：仅保留关键变量，整段超过 8 行时摘要为 `[ENV_BLOCK]`。
    pub fn optimize_jenkins_env(&self, text: &str, _dict: &mut DictionaryEngine) -> String {
        let pattern = regex::Regex::new(r"^([A-Z0-9_]+)=(.*)$").unwrap();
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return text.to_string();
        }
        let mut result = String::with_capacity(text.len());
        let mut i = 0;
        let key_vars = [
            "WORKSPACE",
            "BUILD_NUMBER",
            "JOB_NAME",
            "GIT_BRANCH",
            "ANDROID_HOME",
            "BUILD_TIMESTAMP",
        ];
        while i < lines.len() {
            let line = lines[i];
            if let Some(cap) = pattern.captures(line) {
                let mut env_block = Vec::new();
                env_block.push(cap);
                let mut j = i + 1;
                while j < lines.len() {
                    if let Some(next_cap) = pattern.captures(lines[j]) {
                        env_block.push(next_cap);
                        j += 1;
                        continue;
                    }
                    break;
                }
                if env_block.len() > 8 {
                    let full_env_only = i == 0 && j == lines.len();
                    let mut important = Vec::new();
                    for cap in &env_block {
                        let key = cap.get(1).unwrap().as_str();
                        if full_env_only || key_vars.contains(&key) {
                            let val = cap.get(2).unwrap().as_str();
                            important.push(format!("{}={}", key, val));
                        }
                    }
                    result.push_str(&format!(
                        "[ENV_BLOCK: {} vars (key_info: {})]\n",
                        env_block.len(),
                        important.join(", ")
                    ));
                } else {
                    for cap in env_block {
                        let key = cap.get(1).unwrap().as_str();
                        let val = cap.get(2).unwrap().as_str();
                        result.push_str(&format!("{}={}\n", key, val));
                    }
                }
                i = j;
            } else {
                result.push_str(line);
                result.push('\n');
                i += 1;
            }
        }
        result
    }

    /// 聚合 `warn: removing resource ... without default value` 同类告警，超过 5 条时折叠为 `[RES_WARN_AGG]` 摘要。
    pub fn optimize_resource_warnings(
        &self,
        text: &str,
        _dict: &mut DictionaryEngine,
        _arena: &bumpalo::Bump,
    ) -> String {
        let pattern = regex::Regex::new(r"(?P<pre>warn: removing resource )(?P<pkg>[\w\.]+):(?P<type>\w+)/(?P<name>\w+)(?P<post> without default value\.)").unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let mut result = String::with_capacity(text.len());
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if let Some(caps) = pattern.captures(line) {
                let pkg = caps["pkg"].to_string();
                let mut names = Vec::new();
                names.push(caps["name"].to_string());
                let mut j = i + 1;
                while j < lines.len() {
                    if let Some(n_caps) = pattern.captures(lines[j]) {
                        if n_caps["pkg"] == pkg {
                            names.push(n_caps["name"].to_string());
                            j += 1;
                            continue;
                        }
                    }
                    break;
                }
                if names.len() > 5 {
                    let first = names.first().unwrap();
                    let last = names.last().unwrap();
                    result.push_str(&format!(
                        "[RES_WARN_AGG: warn removing resource without default value; {}: [{}, ..., {}] (total {})]\n",
                        pkg,
                        first,
                        last,
                        names.len()
                    ));
                } else {
                    for line in &lines[i..j] {
                        result.push_str(line);
                        result.push('\n');
                    }
                }
                i = j;
            } else {
                result.push_str(line);
                result.push('\n');
                i += 1;
            }
        }
        result
    }
}
