//! Android/Gradle 插件方法实现

use super::types::AndroidGradlePlugin;
use crate::core::dictionary_engine::DictionaryEngine;

impl AndroidGradlePlugin {
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn optimize_gradle_tasks(&self, text: &str, _dict: &mut DictionaryEngine) -> String {
        text.to_string()
    }

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
            task_lines.len(),
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

    #[tracing::instrument(level = "trace", skip_all)]
    fn push_unique(&self, result: &mut Vec<String>, line: &str) {
        if !result.iter().any(|existing| existing == line) {
            result.push(line.to_string());
        }
    }

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

    pub fn optimize_build_paths(&self, text: &str, dict: &mut DictionaryEngine) -> String {
        let pattern =
            regex::Regex::new(r"([\w-]+)/build/(intermediates|outputs|tmp)/([\w./-]+)").unwrap();
        let mut result = text.to_string();
        for cap in pattern.captures_iter(text) {
            if let (Some(module), Some(dir_type), Some(path)) = (cap.get(1), cap.get(2), cap.get(3))
            {
                let full_path = format!(
                    "{}/build/{}/{}",
                    module.as_str(),
                    dir_type.as_str(),
                    path.as_str()
                );
                let token = dict.add_path_layered(&full_path);
                result = result.replace(&full_path, &token);
            }
        }
        result
    }

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
