use super::types::*;
use crate::core::utils::json::extract_json_object;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// 默认的重排序规则库
static DEFAULT_RULES: Lazy<Vec<ReorderRule>> = Lazy::new(|| {
    vec![
        // 1. Xcode Task: CompileC, CopyPNGFile, etc.
        ReorderRule {
            name: "xcode_task",
            pattern: Regex::new(
                r"^(?P<task>[A-Z][a-zA-Z0-9_]+)\s+.*\(in target '(?P<target>[^']+)'",
            )
            .unwrap(),
            key_group: 2, // Group by target name
        },
        // 2. Make Task: make[4]: ...
        ReorderRule {
            name: "make_id",
            pattern: Regex::new(r"make\[(?P<id>\d+)\]").unwrap(),
            key_group: 1,
        },
        // 3. Shell Trace: + cd ..., + python3 ...
        ReorderRule {
            name: "shell_trace",
            pattern: Regex::new(r"^\+ (?P<cmd>[\w\-_]+)").unwrap(),
            key_group: 1,
        },
        // 4. Android Logcat: [PID:TID]
        ReorderRule {
            name: "android_log",
            pattern: Regex::new(r"^\d+-\d+\s+\d+:\d+:\d+\.\d+\s+(?P<pid>\d+)").unwrap(),
            key_group: 1,
        },
        // 5. Brackets Tag: [INFO], [DEBUG], [GIT]
        ReorderRule {
            name: "bracket_tag",
            pattern: Regex::new(r"^\[(?P<tag>[a-zA-Z0-9_\-]+)\]").unwrap(),
            key_group: 1,
        },
    ]
});

pub struct LogReorderer {
    config: ReorderConfig,
    rules: Vec<ReorderRule>,
    groups: HashMap<String, Vec<String>>,
    group_order: Vec<String>,
    current_context: String,
    total_lines_buffered: usize,
}

impl LogReorderer {
    /// 基于给定 [`ReorderConfig`] 创建日志重排序器，初始化规则表、分组缓冲与上下文状态。
    pub fn new(config: ReorderConfig) -> Self {
        // In a real implementation, we would load rules from config
        Self {
            config,
            rules: vec![], // Will use DEFAULT_RULES for now
            groups: HashMap::new(),
            group_order: Vec::new(),
            current_context: "default".to_string(),
            total_lines_buffered: 0,
        }
    }

    /// 归一化单行文本（针对 Diff 优化）。
    ///
    /// 核心逻辑：
    /// 1. 统一路径分隔符 (Windows -> Unix)。
    /// 2. 排序编译器标志 (-I, -L, -D)。
    /// 3. 递归排序 JSON/YAML 的 Key。
    /// 4. 抹除易变的内存地址 (0x...) 和构建哈希。
    pub fn normalize_line(&self, line: &str) -> String {
        // 1. 统一路径分隔符
        let mut result = line.replace('\\', "/");

        // 2. 抹除动态内存地址 (例如: at 0x00007ff71234 -> at 0x[ADDR])
        static ADDR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"0x[0-9a-fA-F]{8,16}").unwrap());
        result = ADDR_RE.replace_all(&result, "0x[ADDR]").to_string();

        // 3. 处理编译器标志位排序
        if result.contains(" -") {
            result = self.sort_compiler_flags(&result);
        }

        // 4. 尝试归一化行内的 JSON (针对 Web/Node 日志)
        if (result.contains('{') && result.contains('}'))
            || (result.contains('[') && result.contains(']'))
        {
            result = self.normalize_json_in_text(&result);
        }

        // 5. 抹除常见的随机构建哈希 (如 DerivedData 下的随机串, 32位或 40位 Hex)
        static HASH_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"/(?i)derivedData/[^/]+-[a-z0-9]+/|(?i)\b[0-9a-f]{32,40}\b").unwrap()
        });
        result = HASH_RE.replace_all(&result, "/[HASH]/").to_string();

        result
    }

    /// 内部辅助：排序编译器参数
    fn sort_compiler_flags(&self, text: &str) -> String {
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.len() < 2 {
            return text.to_string();
        }

        let mut base = Vec::new();
        let mut flags = Vec::new();
        let mut i = 0;
        while i < parts.len() {
            let p = parts[i];
            let is_prefix = p == "-I" || p == "-L" || p == "-D" || p == "-isystem" || p == "-F";
            if is_prefix && i + 1 < parts.len() {
                flags.push(format!("{}{}", p, parts[i + 1]));
                i += 2;
            } else if p.starts_with("-I")
                || p.starts_with("-L")
                || p.starts_with("-D")
                || p.starts_with("-W")
                || p.starts_with("-f")
                || p.starts_with("-std=")
                || p.starts_with("-O")
                || p.starts_with("-m")
            {
                flags.push(p.to_string());
            } else {
                base.push(p);
            }
            i += 1;
        }

        if flags.is_empty() {
            return text.to_string();
        }
        flags.sort();

        let mut out = base.join(" ");
        if !flags.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&flags.join(" "));
        }
        out
    }

    /// 内部辅助：尝试发现并排序文本中的 JSON key
    fn normalize_json_in_text(&self, text: &str) -> String {
        if let Some(extracted) = extract_json_object(text) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(extracted.raw) {
                // 如果解析成功，则利用 serde_json 默认的有序 map 特性（或手动排序）重新序列化
                // 默认情况下，serde_json 如果开启 preserve_order 特性会有序，不开启则默认按 Key 排序输出
                let normalized_json =
                    serde_json::to_string(&val).unwrap_or_else(|_| extracted.raw.to_string());
                let mut out = String::with_capacity(text.len());
                out.push_str(&text[..extracted.start]);
                out.push_str(&normalized_json);
                out.push_str(&text[extracted.end..]);
                return out;
            }
        }
        text.to_string()
    }

    /// 流式处理单行日志。
    ///
    /// 当缓冲区（`total_lines_buffered`）达到 `max_lines` 阈值时，
    /// 会返回被冲刷掉的最老上下文（Context）中的行，以释放内存。
    pub fn process_line(&mut self, line: String) -> Vec<String> {
        if !self.config.enabled {
            return vec![line];
        }

        let is_continuation = line.trim_start().starts_with('|')
            || line.trim_start().starts_with('^')
            || line.starts_with("    ");

        let mut found_key = None;
        let rules = if self.rules.is_empty() {
            &*DEFAULT_RULES
        } else {
            &self.rules
        };

        if !is_continuation {
            // Expanded rule for deterministic build targets (C/C++, MSVC, Linker, Java, Android, Rust, Go, Swift, etc.)
            let lower_line = line.to_lowercase();
            let is_gcc_clang = lower_line.contains("gcc")
                || lower_line.contains("g++")
                || lower_line.contains("clang")
                || lower_line.contains("cc")
                || lower_line.contains("c++");
            let is_msvc = lower_line.contains("cl.exe")
                || lower_line.contains("link.exe")
                || lower_line.contains("lib.exe");
            let is_linker_ar = lower_line.contains("ld ") || lower_line.contains("ar ");
            let is_java_android = lower_line.contains("javac")
                || lower_line.contains("d8")
                || lower_line.contains("r8")
                || lower_line.contains("aapt2")
                || lower_line.contains("jar ");
            let is_rust_go = lower_line.contains("rustc")
                || lower_line.contains("cargo ")
                || lower_line.contains("go build")
                || lower_line.contains("go run");
            let is_swift = lower_line.contains("swiftc");

            if is_gcc_clang || is_msvc || is_linker_ar || is_java_android || is_rust_go || is_swift
            {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let mut out_file = "";
                let mut src_file = "";

                for (i, p) in parts.iter().enumerate() {
                    let p_lower = p.to_lowercase();

                    // Output detection
                    if (*p == "-o" || *p == "--output" || *p == "-out") && i + 1 < parts.len() {
                        out_file = parts[i + 1];
                    } else if p_lower.starts_with("/out:") {
                        out_file = &p[5..];
                    } else if p_lower.starts_with("/fo") || p_lower.starts_with("/fe") {
                        if p.len() > 3 {
                            out_file = &p[3..];
                        }
                    } else if p.starts_with("-o") && p.len() > 2 {
                        out_file = &p[2..]; // e.g., -oTarget
                    } else if (parts[0] == "ar" || parts[0].ends_with("ar.exe"))
                        && (p.starts_with("cr") || p.starts_with("rc"))
                        && i + 1 < parts.len()
                    {
                        out_file = parts[i + 1];
                    } else if (parts[0] == "jar" || parts[0].ends_with("jar.exe"))
                        && (p.starts_with("cf") || p.starts_with("cvf"))
                        && i + 1 < parts.len()
                    {
                        out_file = parts[i + 1];
                    }

                    // Source detection
                    if p_lower.ends_with(".c")
                        || p_lower.ends_with(".cpp")
                        || p_lower.ends_with(".cc")
                        || p_lower.ends_with(".cxx")
                        || p_lower.ends_with(".m")
                        || p_lower.ends_with(".mm")
                        || p_lower.ends_with(".java")
                        || p_lower.ends_with(".kt")
                        || p_lower.ends_with(".o")
                        || p_lower.ends_with(".obj")
                        || p_lower.ends_with(".swift")
                        || p_lower.ends_with(".rs")
                        || p_lower.ends_with(".go")
                    {
                        if src_file.is_empty() {
                            src_file = p;
                        }
                    }
                }

                if !out_file.is_empty() {
                    if !src_file.is_empty() {
                        found_key = Some(format!("build_target:{}|{}", out_file, src_file));
                    } else {
                        found_key = Some(format!("build_target:{}", out_file));
                    }
                } else if !src_file.is_empty() {
                    // Fallback to source file if no output is explicitly matched (e.g., cl.exe default behavior)
                    found_key = Some(format!("build_target:{}", src_file));
                }
            }

            if found_key.is_none() {
                for rule in rules {
                    if let Some(caps) = rule.pattern.captures(&line) {
                        if let Some(m) = caps.get(rule.key_group) {
                            found_key = Some(format!("{}:{}", rule.name, m.as_str()));
                            break;
                        }
                    }
                }
            }
        }

        if let Some(key) = found_key {
            self.current_context = key;
        } else if is_continuation {
            // Keep current_context
        } else if !self.config.sticky_context {
            self.current_context = "default".to_string();
        }

        if !self.groups.contains_key(&self.current_context) {
            self.group_order.push(self.current_context.clone());
        }
        self.groups
            .entry(self.current_context.clone())
            .or_default()
            .push(line);
        self.total_lines_buffered += 1;

        let mut flushed = Vec::new();
        if self.total_lines_buffered > self.config.max_lines {
            let flush_count = (self.group_order.len() / 2).max(1);
            let mut flushed_groups: Vec<(String, Vec<String>)> = Vec::new();
            for _ in 0..flush_count {
                if self.group_order.is_empty() {
                    break;
                }
                let key = self.group_order.remove(0);
                if key == self.current_context {
                    self.group_order.push(key);
                    continue;
                }
                if let Some(mut group) = self.groups.remove(&key) {
                    self.total_lines_buffered -= group.len();
                    flushed_groups.push((key, group));
                }
            }
            // P2-55：中途冲刷与 flush 的组序策略保持一致——deterministic_sort 时对被冲刷
            // 子集按组名字母序输出，保证确定性承诺在大日志（超 max_lines）下不退化为
            // 「前半 FIFO + 后半 A-Z」的混合序。
            if self.config.deterministic_sort {
                flushed_groups.sort_by(|a, b| a.0.cmp(&b.0));
            }
            for (_, mut group) in flushed_groups {
                flushed.append(&mut group);
            }
        }

        flushed
    }

    /// 强制冲刷所有剩余缓冲的行，按记录顺序返回。
    ///
    /// 通常在文件读取结束时调用，以获取最后残留在缓冲区中的重排结果。
    pub fn flush(&mut self) -> Vec<String> {
        let mut result = Vec::with_capacity(self.total_lines_buffered);

        if self.config.deterministic_sort {
            self.group_order.sort();
        }

        for key in &self.group_order {
            if let Some(mut group) = self.groups.remove(key) {
                result.append(&mut group);
            }
        }
        self.group_order.clear();
        self.groups.clear();
        self.total_lines_buffered = 0;
        self.current_context = "default".to_string();
        result
    }

    /// 执行完整的重排序逻辑。输入为原始行列表，返回重排序后的行列表。
    ///
    /// （此方法为兼容老接口设计的封装，内部调用了流式处理逻辑）。
    pub fn reorder(&mut self, lines: Vec<String>) -> Vec<String> {
        if !self.config.enabled || lines.is_empty() {
            return lines;
        }

        let mut result = Vec::with_capacity(lines.len());
        for line in lines {
            let mut flushed = self.process_line(line);
            result.append(&mut flushed);
        }
        result.append(&mut self.flush());
        result
    }

    /// 对整个文本字符串执行重排序处理。
    pub fn reorder_text(&mut self, text: &str) -> String {
        let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
        let reordered = self.reorder(lines);
        reordered.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 `normalize_line` 能稳定处理含乱序 JSON 字段的单行：保留前缀与后缀，并规范化 JSON 键顺序。
    #[test]
    fn normalize_line_handles_noisy_json_segment() {
        let reorderer = LogReorderer::new(ReorderConfig {
            enabled: true,
            ..Default::default()
        });

        let input = "INFO prefix {\"z\":2,\"a\":1} tail";
        let normalized = reorderer.normalize_line(input);

        assert!(normalized.starts_with("INFO prefix "));
        assert!(normalized.ends_with(" tail"));
        assert!(normalized.contains("{\"a\":1,\"z\":2}"));
    }

    /// P2-55 回归：大日志触发中途冲刷（total_lines_buffered > max_lines）时，
    /// deterministic_sort 承诺不得退化为「前半 FIFO + 后半 A-Z」混合序——被冲刷的
    /// 组子集也必须按组名字母序输出。修复前中途冲刷直接按 group_order FIFO 顺序 append。
    #[test]
    fn mid_flush_respects_deterministic_sort_order() {
        let mut reorderer = LogReorderer::new(ReorderConfig {
            enabled: true,
            max_lines: 6,
            sticky_context: true,
            deterministic_sort: true,
        });

        // 依次创建 zulu/alpha/mike/bravo 四组（FIFO 组序故意与字母序相反），
        // 第 7 行触发中途冲刷：flush_count = 4/2 = 2，冲刷 zulu+alpha 两组。
        let mut mid_flushed: Vec<String> = Vec::new();
        for line in [
            "[zulu] 1",
            "[zulu] 2",
            "[alpha] 1",
            "[alpha] 2",
            "[mike] 1",
            "[mike] 2",
            "[bravo] 1",
        ] {
            mid_flushed.extend(reorderer.process_line(line.to_string()));
        }

        assert_eq!(mid_flushed.len(), 4, "中途冲刷应产出 zulu+alpha 共 4 行");
        assert!(
            mid_flushed[0].starts_with("[alpha]"),
            "中途冲刷子集须按字母序输出（alpha 先于 zulu），实际序: {mid_flushed:?}"
        );
        assert!(mid_flushed[1].starts_with("[alpha]"));
        assert!(mid_flushed[2].starts_with("[zulu]"));
        assert!(mid_flushed[3].starts_with("[zulu]"));

        // 收尾 flush：剩余 mike/bravo 组同样按字母序
        let rest = reorderer.flush();
        assert_eq!(rest.len(), 3);
        assert!(rest[0].starts_with("[bravo]"), "收尾 flush 序: {rest:?}");
        assert!(rest[1].starts_with("[mike]"));
        assert!(rest[2].starts_with("[mike]"));
    }
}
