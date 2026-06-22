//! Node.js plugin methods

use super::types::NodeJsPlugin;
use crate::core::dictionary_engine::DictionaryEngine;
use std::collections::HashMap;

impl NodeJsPlugin {
    /// 应用高级压缩功能（npm install、TypeScript、ESLint、Webpack、Jest）
    /// 遵循压缩协议 V1 法则 E（零容忍废话），参见 docs/development/PLUGIN_DEVELOPMENT.md §8
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn apply_advanced_compression(&self, text: &str) -> String {
        let mut result = String::new();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();

            if (!trimmed.starts_with("npm WARN")
                && trimmed.contains("WARN")
                && trimmed.to_ascii_lowercase().contains("deprecated"))
                || trimmed.starts_with("Packages: ")
            {
                let (compressed, consumed) = self.compress_pnpm_install(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            if trimmed.starts_with("yarn install")
                || trimmed.starts_with("[1/4] Resolving packages")
                || trimmed.starts_with("warning ")
            {
                let (compressed, consumed) = self.compress_yarn_install(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 1: 压缩 npm install 输出
            if line.contains("npm WARN deprecated")
                || line.contains("added ") && line.contains(" packages")
            {
                let (compressed, consumed) = self.compress_npm_install(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 2: 压缩 TypeScript 编译输出
            if line.contains(" - error TS") || line.contains(" - warning TS") {
                let (compressed, consumed) = self.compress_tsc_output_v2(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 3: 压缩 ESLint 输出
            if line.starts_with("/")
                && line.contains(".js")
                && i + 1 < lines.len()
                && lines[i + 1].trim().starts_with(char::is_numeric)
            {
                let (compressed, consumed) = self.compress_eslint_output(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 4: 压缩 Webpack 构建输出
            if line.starts_with("asset ") || line.contains("webpack ") && line.contains(" compiled")
            {
                let (compressed, consumed) = self.compress_webpack_output(&lines[i..]);
                if consumed > 0 {
                    result.push_str(&compressed);
                    i += consumed;
                    continue;
                }
            }

            // 功能 5: 压缩 Jest 测试输出
            if line.trim().starts_with("PASS ") || line.trim().starts_with("FAIL ") {
                let (compressed, consumed) = self.compress_jest_output(&lines[i..]);
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

    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_pnpm_install(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut original_size = 0;
        let mut deprecated_count = 0usize;
        let mut packages_line = String::new();
        let mut progress_line = String::new();
        let mut done_line = String::new();

        for line in lines {
            let trimmed = line.trim();
            original_size += line.len() + 1;
            if trimmed.contains("WARN") && trimmed.to_ascii_lowercase().contains("deprecated") {
                deprecated_count += 1;
                consumed += 1;
            } else if trimmed.starts_with("Packages: ") {
                packages_line = trimmed.to_string();
                consumed += 1;
            } else if trimmed.starts_with("Progress: ") {
                progress_line = trimmed.to_string();
                consumed += 1;
            } else if trimmed.starts_with("Done in ") {
                done_line = trimmed.to_string();
                consumed += 1;
                break;
            } else if trimmed.starts_with("Lockfile is up to date")
                || trimmed.starts_with("Already up to date")
                || trimmed.starts_with("Resolution step")
                || trimmed.starts_with("Packages are")
                || trimmed.is_empty()
            {
                consumed += 1;
            } else {
                break;
            }
        }

        if packages_line.is_empty() && deprecated_count == 0 {
            return (String::new(), 0);
        }

        let mut result = format!(
            "[PNPM] install deprecated={}{}\n",
            deprecated_count,
            if packages_line.is_empty() {
                String::new()
            } else {
                format!(" {}", packages_line)
            }
        );
        if !progress_line.is_empty() {
            result.push_str(&progress_line);
            result.push('\n');
        }
        if !done_line.is_empty() {
            result.push_str(&done_line);
            result.push('\n');
        }

        if result.len() >= original_size {
            return (String::new(), 0);
        }
        (result, consumed)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_yarn_install(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut original_size = 0;
        let mut warnings = 0usize;
        let mut steps = 0usize;
        let mut done_line = String::new();

        for line in lines {
            let trimmed = line.trim();
            original_size += line.len() + 1;
            if trimmed.starts_with("yarn install") {
                consumed += 1;
            } else if trimmed.starts_with("warning ") {
                warnings += 1;
                consumed += 1;
            } else if trimmed.starts_with("[") && trimmed.contains("/4]") {
                steps += 1;
                consumed += 1;
            } else if trimmed.starts_with("success ") || trimmed.starts_with("info ") {
                consumed += 1;
            } else if trimmed.starts_with("Done in ") {
                done_line = trimmed.to_string();
                consumed += 1;
                break;
            } else if trimmed.is_empty() {
                consumed += 1;
            } else {
                break;
            }
        }

        if warnings == 0 && steps == 0 {
            return (String::new(), 0);
        }

        let mut result = format!("[YARN] install warnings={} steps={}\n", warnings, steps);
        if !done_line.is_empty() {
            result.push_str(&done_line);
            result.push('\n');
        }

        if result.len() >= original_size {
            return (String::new(), 0);
        }
        (result, consumed)
    }

    /// 功能 1: 压缩 npm install 输出
    /// 输入: "npm WARN deprecated...\nadded 120 packages..."
    /// 输出: "[NPM] Installing 120 packages (2 deprecation warnings suppressed)\n"
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_tsc_output_v2(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut original_size = 0;
        let mut errors = 0usize;
        let mut warnings = 0usize;
        let mut shown = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            if line.contains(" - error TS") || line.contains(" - warning TS") {
                if line.contains(" - error TS") {
                    errors += 1;
                } else {
                    warnings += 1;
                }
                if shown.len() < 5 {
                    shown.push(line.to_string());
                }
                original_size += line.len() + 1;
                consumed += 1;
                i += 1;
                while i < lines.len() {
                    let next = lines[i];
                    if next.contains(" - error TS")
                        || next.contains(" - warning TS")
                        || next.starts_with("Found ")
                        || next.starts_with("error Command")
                    {
                        break;
                    }
                    original_size += next.len() + 1;
                    consumed += 1;
                    i += 1;
                }
            } else if line.starts_with("Found ") && line.contains(" errors") {
                original_size += line.len() + 1;
                consumed += 1;
                break;
            } else {
                break;
            }
        }

        if errors == 0 && warnings == 0 {
            return (String::new(), 0);
        }

        let mut result = format!("[TSC] {} errors, {} warnings\n", errors, warnings);
        for line in shown {
            result.push_str(&line);
            result.push('\n');
        }

        if result.len() >= original_size {
            return (String::new(), 0);
        }
        (result, consumed)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_npm_install(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut deprecated_count = 0;
        let mut packages_added = 0;
        let mut install_time = String::new();
        let mut vulnerabilities = String::new();
        let mut original_size = 0;

        for line in lines {
            original_size += line.len() + 1; // +1 for newline

            if line.contains("npm WARN deprecated") {
                deprecated_count += 1;
                consumed += 1;
            } else if line.starts_with(">") {
                // 跳过 postinstall 脚本输出
                consumed += 1;
            } else if line.starts_with("added ") && line.contains(" packages") {
                // 提取包数量和时间
                if let Some(num_str) = line.split_whitespace().nth(1) {
                    if let Ok(num) = num_str.parse::<usize>() {
                        packages_added = num;
                    }
                }
                if let Some(time_part) = line.split(" in ").nth(1) {
                    install_time = time_part.trim().to_string();
                }
                consumed += 1;
            } else if line.contains("packages are looking for funding") {
                consumed += 1;
            } else if line.contains("run `npm fund`") {
                consumed += 1;
            } else if line.contains("found ") && line.contains(" vulnerabilities") {
                vulnerabilities = line.trim().to_string();
                consumed += 1;
                break;
            } else if line.trim().is_empty() {
                consumed += 1;
            } else {
                break;
            }
        }

        if packages_added == 0 {
            return (String::new(), 0);
        }

        let mut result = String::new();

        // 只在有多个废弃警告且能节省空间时才添加摘要
        if deprecated_count >= 3 {
            result.push_str(&format!(
                "[NPM] {} deprecation warnings suppressed\n",
                deprecated_count
            ));
        }

        result.push_str(&format!("added {} packages", packages_added));
        if !install_time.is_empty() {
            result.push_str(&format!(" in {}", install_time));
        }
        result.push('\n');

        if !vulnerabilities.is_empty() {
            result.push_str(&vulnerabilities);
            result.push('\n');
        }

        // ROI 门控：如果压缩后更长，返回原始内容
        if result.len() >= original_size {
            return (String::new(), 0);
        }

        (result, consumed)
    }

    /// 功能 2: 压缩 TypeScript 编译输出
    /// 输入: "src/foo.ts:10:5 - error TS2304: Cannot find name 'foo'.\n..."
    /// 输出: "[TSC] 2 errors, 10 warnings (first 5 shown)\n..."
    #[tracing::instrument(level = "debug", skip_all)]
    #[allow(dead_code)]
    fn compress_tsc_output(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut errors = 0;
        let mut warnings = 0;
        let mut error_lines = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            if line.contains(" - error TS") {
                errors += 1;
                if errors <= 5 {
                    error_lines.push(line.to_string());
                }
                consumed += 1;
                i += 1;
                // 跳过错误详情行（空行和缩进行）
                while i < lines.len()
                    && (lines[i].trim().is_empty()
                        || lines[i].starts_with(" ") && !lines[i].contains(" - "))
                {
                    consumed += 1;
                    i += 1;
                }
            } else if line.contains(" - warning TS") {
                warnings += 1;
                consumed += 1;
                i += 1;
                // 跳过警告详情行
                while i < lines.len()
                    && (lines[i].trim().is_empty()
                        || lines[i].starts_with(" ") && !lines[i].contains(" - "))
                {
                    consumed += 1;
                    i += 1;
                }
            } else if line.starts_with("Found ") && line.contains(" errors") {
                consumed += 1;
                break;
            } else {
                break;
            }
        }

        if errors == 0 && warnings == 0 {
            return (String::new(), 0);
        }

        let mut result = format!("[TSC] {} errors, {} warnings", errors, warnings);
        if errors > 5 {
            result.push_str(&format!(
                " (first 5 errors shown, {} suppressed)",
                errors - 5
            ));
        }
        result.push('\n');

        // 保留前 5 个错误
        for error_line in error_lines {
            result.push_str(&error_line);
            result.push('\n');
        }

        (result, consumed)
    }

    /// 功能 3: 压缩 ESLint 输出
    /// 输入: "/path/to/foo.js\n  10:5  error  'foo' is not defined  no-undef\n..."
    /// 输出: "[ESLINT] 45 problems (20 errors, 25 warnings)\n[ESLINT] Top errors: no-undef (15), no-unused-vars (10)\n..."
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_eslint_output(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut errors = 0;
        let mut warnings = 0;
        let mut error_rules: HashMap<String, usize> = HashMap::new();
        let mut first_errors = Vec::new();

        for line in lines {
            if line.starts_with("/") && line.contains(".js") {
                // 文件路径行
                if errors + warnings < 5 {
                    first_errors.push(line.to_string());
                }
                consumed += 1;
            } else if line.trim().starts_with(char::is_numeric) {
                // 错误/警告行：  10:5  error  'foo' is not defined  no-undef
                if line.contains("  error  ") {
                    errors += 1;
                    // 提取规则名称
                    if let Some(rule) = line.split_whitespace().last() {
                        *error_rules.entry(rule.to_string()).or_insert(0) += 1;
                    }
                } else if line.contains("  warning  ") {
                    warnings += 1;
                }
                if errors + warnings <= 10 {
                    first_errors.push(line.to_string());
                }
                consumed += 1;
            } else if line.trim().is_empty() {
                consumed += 1;
            } else if line.starts_with("✖") && line.contains(" problems") {
                consumed += 1;
                break;
            } else {
                break;
            }
        }

        if errors == 0 && warnings == 0 {
            return (String::new(), 0);
        }

        let mut result = format!(
            "[ESLINT] {} problems ({} errors, {} warnings)\n",
            errors + warnings,
            errors,
            warnings
        );

        // 添加 top 错误规则统计
        if !error_rules.is_empty() {
            let mut sorted_rules: Vec<_> = error_rules.iter().collect();
            sorted_rules.sort_by(|a, b| b.1.cmp(a.1));
            let top_rules: Vec<String> = sorted_rules
                .iter()
                .take(2)
                .map(|(rule, count)| format!("{} ({})", rule, count))
                .collect();
            result.push_str(&format!("[ESLINT] Top errors: {}\n", top_rules.join(", ")));
        }

        // 保留前几个错误示例
        if errors + warnings > 10 {
            result.push_str(&format!(
                "(first 5 errors shown, {} suppressed)\n",
                errors + warnings - 10
            ));
        }
        for (i, error_line) in first_errors.iter().take(10).enumerate() {
            if i < 5 {
                result.push_str(error_line);
                result.push('\n');
            }
        }

        (result, consumed)
    }

    /// 功能 4: 压缩 Webpack 构建输出
    /// 输入: "asset main.js 2.5 MiB...\nwebpack 5.75.0 compiled successfully in 12345 ms"
    /// 输出: "[WEBPACK] Built 120 modules in 12.345s (details suppressed)\n[WEBPACK] Main assets: main.js (2.5 MiB), vendor.js (1.2 MiB)\n"
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_webpack_output(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut main_assets = Vec::new();
        let mut module_count = 0;
        let mut build_time = String::new();

        for line in lines {
            if line.starts_with("asset ") {
                // 提取主要资产（前 2 个）
                if main_assets.len() < 2 {
                    if let Some(parts) = line.strip_prefix("asset ") {
                        let parts: Vec<&str> = parts.split_whitespace().collect();
                        if parts.len() >= 2 {
                            main_assets.push(format!("{} ({})", parts[0], parts[1]));
                        }
                    }
                }
                consumed += 1;
            } else if line.contains(" modules") {
                // 统计模块数
                if let Some(num_str) = line.split_whitespace().next() {
                    if let Ok(num) = num_str.parse::<usize>() {
                        module_count += num;
                    }
                }
                consumed += 1;
            } else if line.contains("webpack ") && line.contains(" compiled") {
                // 提取构建时间
                if let Some(time_part) = line.split(" in ").nth(1) {
                    if let Some(time_str) = time_part.split_whitespace().next() {
                        build_time = time_str.to_string();
                    }
                }
                consumed += 1;
                break;
            } else if line.trim().is_empty() || line.starts_with("  ") {
                consumed += 1;
            } else {
                break;
            }
        }

        if main_assets.is_empty() && module_count == 0 {
            return (String::new(), 0);
        }

        let mut result = String::new();
        if module_count > 0 {
            result.push_str(&format!("[WEBPACK] Built {} modules", module_count));
            if !build_time.is_empty() {
                // 转换 ms 到 s
                if let Ok(ms) = build_time.trim_end_matches("ms").parse::<f64>() {
                    result.push_str(&format!(" in {:.3}s", ms / 1000.0));
                }
            }
            result.push_str(" (details suppressed)\n");
        }

        if !main_assets.is_empty() {
            result.push_str(&format!(
                "[WEBPACK] Main assets: {}\n",
                main_assets.join(", ")
            ));
        }

        (result, consumed)
    }

    /// 功能 5: 压缩 Jest 测试输出
    /// 输入: " PASS  tests/foo.test.js\n PASS  tests/bar.test.js\n..."
    /// 输出: "[JEST] Tests: 313 passed, 5 failed, 2 skipped (320 total, 12.345s)\n[JEST] Failed suites: tests/fail1.test.js (see details below)\n..."
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_jest_output(&self, lines: &[&str]) -> (String, usize) {
        let mut consumed = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut failed_suites = Vec::new();
        let mut in_failure_details = false;
        let mut failure_details = Vec::new();

        for line in lines {
            if line.trim().starts_with("PASS ") {
                passed += 1;
                consumed += 1;
            } else if line.trim().starts_with("FAIL ") {
                failed += 1;
                // 提取失败的测试套件名称
                if let Some(suite_name) = line
                    .trim()
                    .strip_prefix("FAIL ")
                    .and_then(|s| s.split_whitespace().next())
                {
                    failed_suites.push(suite_name.to_string());
                }
                consumed += 1;
                in_failure_details = true;
            } else if in_failure_details {
                // 收集失败详情
                if line.starts_with("Test Suites:") {
                    consumed += 1;
                    break;
                }
                failure_details.push(line.to_string());
                consumed += 1;
            } else if line.starts_with("Test Suites:") {
                consumed += 1;
                break;
            } else if line.trim().is_empty() {
                consumed += 1;
            } else {
                break;
            }
        }

        // 解析最后的统计行
        let mut total_tests = 0;
        let mut tests_passed = 0;
        let mut tests_failed = 0;
        let mut tests_skipped = 0;
        let mut test_time = String::new();

        if consumed < lines.len() {
            for line in &lines[consumed..consumed + 5.min(lines.len() - consumed)] {
                if line.starts_with("Tests:") {
                    // 解析: Tests:       5 failed, 2 skipped, 313 passed, 320 total
                    for part in line.split(',') {
                        if part.contains(" passed") {
                            if let Some(num_str) =
                                part.split_whitespace().find(|s| s.parse::<usize>().is_ok())
                            {
                                tests_passed = num_str.parse().unwrap_or(0);
                            }
                        } else if part.contains(" failed") {
                            if let Some(num_str) =
                                part.split_whitespace().find(|s| s.parse::<usize>().is_ok())
                            {
                                tests_failed = num_str.parse().unwrap_or(0);
                            }
                        } else if part.contains(" skipped") {
                            if let Some(num_str) =
                                part.split_whitespace().find(|s| s.parse::<usize>().is_ok())
                            {
                                tests_skipped = num_str.parse().unwrap_or(0);
                            }
                        } else if part.contains(" total") {
                            if let Some(num_str) =
                                part.split_whitespace().find(|s| s.parse::<usize>().is_ok())
                            {
                                total_tests = num_str.parse().unwrap_or(0);
                            }
                        }
                    }
                    consumed += 1;
                } else if line.starts_with("Time:") {
                    if let Some(time_str) = line.split_whitespace().nth(1) {
                        test_time = time_str.to_string();
                    }
                    consumed += 1;
                } else if line.starts_with("Snapshots:") || line.starts_with("Ran all") {
                    consumed += 1;
                } else if !line.trim().is_empty() {
                    break;
                }
            }
        }

        if passed == 0 && failed == 0 {
            return (String::new(), 0);
        }

        let mut result = format!(
            "[JEST] Tests: {} passed, {} failed, {} skipped ({} total",
            tests_passed, tests_failed, tests_skipped, total_tests
        );
        if !test_time.is_empty() {
            result.push_str(&format!(", {}", test_time));
        }
        result.push_str(")\n");

        if !failed_suites.is_empty() {
            result.push_str(&format!(
                "[JEST] Failed suites: {} (see details below)\n",
                failed_suites.join(", ")
            ));
        }

        // 保留失败详情
        for detail_line in failure_details {
            result.push_str(&detail_line);
            result.push('\n');
        }

        (result, consumed)
    }
}

// 旧的辅助函数（保留向后兼容）
impl NodeJsPlugin {
    /// 内部辅助函数：执行与 optimize node modules 相关的具体逻辑。
    pub fn optimize_node_modules(&self, text: &str, dict: &mut DictionaryEngine) -> String {
        let pattern = regex::Regex::new(r"node_modules/([\w@./_-]+)").unwrap();
        let mut result = text.to_string();
        for cap in pattern.captures_iter(text) {
            if let Some(module_path) = cap.get(1) {
                let full_path = format!("node_modules/{}", module_path.as_str());
                let token = dict.add_path_layered(&full_path);
                result = result.replace(&full_path, &token);
            }
        }
        result
    }

    /// 内部辅助函数：执行与 optimize npm yarn 相关的具体逻辑。
    pub fn optimize_npm_yarn(&self, text: &str, dict: &mut DictionaryEngine) -> String {
        let mut result = text.to_string();
        let npm_err_pattern = regex::Regex::new(r"npm\s+ERR!\s+([\w-]+)").unwrap();
        for cap in npm_err_pattern.captures_iter(text) {
            if let Some(code) = cap.get(1) {
                let full_err = format!("npm ERR! {}", code.as_str());
                let token = dict.add_macro(&full_err);
                result = result.replace(&full_err, &token);
            }
        }
        let yarn_pattern =
            regex::Regex::new(r"yarn\s+(install|build|run|add|remove)\s+v?([\d.]+)").unwrap();
        for cap in yarn_pattern.captures_iter(text) {
            if let (Some(cmd), Some(version)) = (cap.get(1), cap.get(2)) {
                let full_cmd = format!("yarn {} v{}", cmd.as_str(), version.as_str());
                let token = dict.add_macro(&full_cmd);
                result = result.replace(&full_cmd, &token);
            }
        }
        result
    }

    /// 内部辅助函数：执行与 optimize webpack 相关的具体逻辑。
    pub fn optimize_webpack(&self, text: &str, dict: &mut DictionaryEngine) -> String {
        let mut result = text.to_string();
        let patterns = [
            ("Hash:", r"Hash:\s*[a-f0-9]+"),
            ("Version:", r"Version:\s*[\d.]+"),
            ("Time:", r"Time:\s*\d+ms"),
            ("Built at:", r"Built at:\s*[\d/]+\s+[APM]+"),
        ];
        for (label, pattern_str) in &patterns {
            let pattern = regex::Regex::new(pattern_str).unwrap();
            for cap in pattern.captures_iter(text) {
                if let Some(full_match) = cap.get(0) {
                    let token = dict.add_macro(label);
                    result = result.replace(full_match.as_str(), &token);
                }
            }
        }
        result
    }

    /// 内部辅助函数：执行与 optimize typescript 相关的具体逻辑。
    pub fn optimize_typescript(&self, text: &str, dict: &mut DictionaryEngine) -> String {
        let pattern = regex::Regex::new(r"TS\d+:\s*[\w\s()]+").unwrap();
        let mut result = text.to_string();
        for cap in pattern.captures_iter(text) {
            if let Some(ts_error) = cap.get(0) {
                let token = dict.add_macro(ts_error.as_str());
                result = result.replace(ts_error.as_str(), &token);
            }
        }
        result
    }

    /// 内部辅助函数：执行与 optimize pipeline 相关的具体逻辑。
    pub fn optimize_pipeline(&self, text: &str, _dict: &mut DictionaryEngine) -> String {
        // Simplified implementation to avoid encoding issues
        text.to_string()
    }
}
