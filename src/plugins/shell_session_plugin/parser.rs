use regex::Regex;
use std::sync::OnceLock;

static BASH_ZSH_PROMPT: OnceLock<Regex> = OnceLock::new();
static POWERSHELL_PROMPT: OnceLock<Regex> = OnceLock::new();
static CMD_PROMPT: OnceLock<Regex> = OnceLock::new();
static ANSI_CLEANER: OnceLock<Regex> = OnceLock::new();

// Command specific regexes
static ENV_VAR_RE: OnceLock<Regex> = OnceLock::new();
static MULTI_SPACE_RE: OnceLock<Regex> = OnceLock::new();
static ROBOCOPY_FILE_RE: OnceLock<Regex> = OnceLock::new();
static CURL_PROGRESS_RE: OnceLock<Regex> = OnceLock::new();
static TAR_FILE_RE: OnceLock<Regex> = OnceLock::new();

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum CommandType {
    Unknown,
    Env,
    Ls,
    Robocopy,
    Curl,
    Xcopy,
    Tree,
    Ps,
    Top,
    Find,
    Tar,
    Df,
}

/// 解析命令行首词（剥离 `FOO=bar` 前缀）判别命令类型（Env/Ls/Robocopy/Curl/Tar/...），未知返回 Unknown。
pub fn parse_command_type(cmd_text: &str) -> CommandType {
    let mut cmd = cmd_text.trim();
    // Strip env prefixes like FOO=bar
    while let Some(idx) = cmd.find(' ') {
        let prefix = &cmd[..idx];
        if prefix.contains('=') && !prefix.starts_with('-') {
            cmd = cmd[idx..].trim();
        } else {
            break;
        }
    }

    let first_word = cmd.split_whitespace().next().unwrap_or("").to_lowercase();
    match first_word.as_str() {
        "env" | "set" | "export" => CommandType::Env,
        "ls" | "ll" | "la" | "dir" => CommandType::Ls,
        "robocopy" => CommandType::Robocopy,
        "curl" | "wget" => CommandType::Curl,
        "xcopy" => CommandType::Xcopy,
        "tree" => CommandType::Tree,
        "ps" | "get-process" => CommandType::Ps,
        "top" => CommandType::Top,
        "find" => CommandType::Find,
        "tar" | "zip" | "unzip" => CommandType::Tar,
        "df" | "du" => CommandType::Df,
        _ => CommandType::Unknown,
    }
}

/// 将 shell 会话文本按提示符切分为命令块，并对敏感环境变量脱敏、折叠 robocopy/curl/tar 噪声行。
pub fn compress_shell_session_blocks(input: &str) -> Vec<String> {
    let bash_re = BASH_ZSH_PROMPT
        .get_or_init(|| Regex::new(r"^(?:[\w.-]+@[\w.-]+:?\s*[~/\w.-]*\s*[%#$>]+|\+)\s*").unwrap());
    let ps_re = POWERSHELL_PROMPT.get_or_init(|| Regex::new(r"^PS\s+[A-Z]:\\[^>]*>\s*").unwrap());
    let cmd_re = CMD_PROMPT.get_or_init(|| Regex::new(r"^[A-Z]:\\[^>]*>\s*").unwrap());
    let ansi_re =
        ANSI_CLEANER.get_or_init(|| Regex::new(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])").unwrap());

    let env_var_re =
        ENV_VAR_RE.get_or_init(|| Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)=(.*)$").unwrap());
    let multi_space_re = MULTI_SPACE_RE.get_or_init(|| Regex::new(r" {2,}").unwrap());
    let robocopy_file_re = ROBOCOPY_FILE_RE
        .get_or_init(|| Regex::new(r"^\s*\d+%\s+.*?(?:File|Dir)\s+\d+\s+.*$").unwrap());
    let curl_progress_re = CURL_PROGRESS_RE
        .get_or_init(|| Regex::new(r"^\s*\d+\s+\d[\d.KMGT]*\s+\d+\s+\d[\d.KMGT]*\s+.*$").unwrap());
    let tar_file_re = TAR_FILE_RE.get_or_init(|| Regex::new(r"^(?:x|Extracting)\s+.*$").unwrap());

    let mut blocks = Vec::new();
    let mut current_block = String::new();

    let mut empty_prompt_streak = 0;
    let mut last_prompt_line = String::new();
    let mut current_cmd = CommandType::Unknown;

    // Accumulators for noise reduction within blocks
    let mut skipped_robocopy_files = 0;
    let mut curl_progress_lines = 0;
    let mut tar_file_lines = 0;

    let flush_accumulators = |out: &mut String, rc: &mut i32, cp: &mut i32, tf: &mut i32| {
        if *rc > 0 {
            out.push_str(&format!("  [... skipped {} file lines ...]\n", *rc));
            *rc = 0;
        }
        if *cp > 0 {
            out.push_str(&format!(
                "  [... compressed {} progress bar updates ...]\n",
                *cp
            ));
            *cp = 0;
        }
        if *tf > 0 {
            out.push_str(&format!("  [... extracted {} files ...]\n", *tf));
            *tf = 0;
        }
    };

    let push_block = |blocks: &mut Vec<String>, current_block: &mut String| {
        if !current_block.is_empty() {
            blocks.push(current_block.clone());
            current_block.clear();
        }
    };

    for chunk in input.split_inclusive('\n') {
        let has_newline = chunk.ends_with('\n');
        let line_no_nl = chunk.strip_suffix('\n').unwrap_or(chunk);
        let line_no_cr = line_no_nl.strip_suffix('\r').unwrap_or(line_no_nl);

        let clean_line = ansi_re.replace_all(line_no_cr, "");

        let is_bash = bash_re.is_match(&clean_line);
        let is_ps = ps_re.is_match(&clean_line);
        let is_cmd = cmd_re.is_match(&clean_line);

        if is_bash || is_ps || is_cmd {
            flush_accumulators(
                &mut current_block,
                &mut skipped_robocopy_files,
                &mut curl_progress_lines,
                &mut tar_file_lines,
            );

            let without_prompt = if is_bash {
                bash_re.replace(&clean_line, "")
            } else if is_ps {
                ps_re.replace(&clean_line, "")
            } else {
                cmd_re.replace(&clean_line, "")
            };

            let cmd_text = without_prompt.trim();
            let is_empty_command = cmd_text.is_empty();

            if is_empty_command {
                empty_prompt_streak += 1;
                last_prompt_line = line_no_cr.to_string();
                continue;
            } else {
                if empty_prompt_streak > 0 {
                    if empty_prompt_streak == 1 {
                        current_block.push_str(&last_prompt_line);
                        current_block.push('\n');
                    } else {
                        current_block.push_str(&format!(
                            "{} [{} empty prompts skipped]\n",
                            last_prompt_line, empty_prompt_streak
                        ));
                    }
                    empty_prompt_streak = 0;
                }

                // We encountered a new non-empty command prompt.
                // This means the previous command block has finished.
                // We should push the current block and start a new one.
                push_block(&mut blocks, &mut current_block);
            }

            current_block.push_str(line_no_cr);
            if has_newline {
                current_block.push('\n');
            }

            current_cmd = parse_command_type(cmd_text);
        } else {
            // Not a prompt line.
            if empty_prompt_streak > 0 {
                if empty_prompt_streak == 1 {
                    current_block.push_str(&last_prompt_line);
                    current_block.push('\n');
                } else {
                    current_block.push_str(&format!(
                        "{} [{} empty prompts skipped]\n",
                        last_prompt_line, empty_prompt_streak
                    ));
                }
                empty_prompt_streak = 0;
            }

            // Windows DIR heuristic
            if clean_line.starts_with(" Volume in drive")
                || clean_line.starts_with(" Volume Serial Number")
            {
                continue;
            }
            let trimmed = clean_line.trim_start();
            if trimmed.starts_with(|c: char| c.is_ascii_digit())
                && (trimmed.contains(" File(s) ") || trimmed.contains(" Dir(s) "))
            {
                if current_cmd != CommandType::Robocopy && current_cmd != CommandType::Ls {
                    continue;
                }
            }

            // Command specific rules
            match current_cmd {
                CommandType::Env => {
                    if let Some(caps) = env_var_re.captures(&clean_line) {
                        let key = caps.get(1).unwrap().as_str();
                        let val = caps.get(2).unwrap().as_str();

                        let key_lower = key.to_lowercase();
                        let is_sensitive = key_lower.contains("secret")
                            || key_lower.contains("key")
                            || key_lower.contains("token")
                            || key_lower.contains("pass")
                            || key_lower.contains("auth")
                            || key_lower.contains("cert")
                            || val.len() > 30;

                        if is_sensitive && !val.is_empty() {
                            if val.len() > 10
                                && !key_lower.contains("secret")
                                && !key_lower.contains("token")
                                && !key_lower.contains("key")
                                && (val.starts_with('/') || val.contains(":\\"))
                            {
                                current_block.push_str(line_no_cr);
                            } else {
                                current_block.push_str(&format!(
                                    "{}={}[REDACTED]",
                                    key,
                                    val.chars().take(4).collect::<String>()
                                ));
                            }
                        } else {
                            current_block.push_str(line_no_cr);
                        }
                        if has_newline {
                            current_block.push('\n');
                        }
                        continue;
                    }
                }
                CommandType::Ls | CommandType::Ps | CommandType::Df => {
                    let compressed = multi_space_re.replace_all(&clean_line, " ");
                    current_block.push_str(&compressed);
                    if has_newline {
                        current_block.push('\n');
                    }
                    continue;
                }
                CommandType::Robocopy => {
                    if robocopy_file_re.is_match(&clean_line) {
                        skipped_robocopy_files += 1;
                        continue;
                    }
                }
                CommandType::Curl => {
                    if curl_progress_re.is_match(&clean_line) {
                        curl_progress_lines += 1;
                        continue;
                    }
                }
                CommandType::Tar => {
                    // P2-50：跳过判据收紧为「成功的提取行」——显式提取前缀（x /Extracting /inflating:）
                    // 且不含错误标志（tar:/error/warning/cannot/failed）。原先的 `contains('/')`
                    // 过宽：URL、`tar: /path: Cannot open ...` 等错误消息全被当提取行吞掉，
                    // tar/unzip 失败信号系统性丢失。
                    let lower_tar = clean_line.to_lowercase();
                    let looks_like_extraction = tar_file_re.is_match(&clean_line)
                        || clean_line.starts_with("inflating:")
                        || clean_line.starts_with("x ");
                    let has_error_marker = ["tar:", "error", "warning", "cannot", "failed"]
                        .iter()
                        .any(|k| lower_tar.contains(k));
                    if looks_like_extraction && !has_error_marker {
                        tar_file_lines += 1;
                        continue;
                    }
                    // 含 '/' 的非提取行落入下方常规处理：错误行由错误边界逻辑显式保留
                }
                _ => {}
            }

            // If we didn't continue above, flush accumulators before printing the normal line
            flush_accumulators(
                &mut current_block,
                &mut skipped_robocopy_files,
                &mut curl_progress_lines,
                &mut tar_file_lines,
            );

            // Exit status / error boundary modelling
            let lower_clean = clean_line.to_lowercase();
            if lower_clean.contains("command not found")
                || lower_clean.contains("syntax error")
                || lower_clean.contains("is not recognized as an internal or external command")
                || lower_clean.contains("exception")
                || (lower_clean.contains("error") && !lower_clean.contains("errorlevel"))
            {
                // Keep the error line explicitly
                current_block.push_str(line_no_cr);
                if has_newline {
                    current_block.push('\n');
                }
                continue;
            }

            // Fallback for normal lines
            current_block.push_str(line_no_cr);
            if has_newline {
                current_block.push('\n');
            }
        }
    }

    // Flush any pending accumulators at EOF
    flush_accumulators(
        &mut current_block,
        &mut skipped_robocopy_files,
        &mut curl_progress_lines,
        &mut tar_file_lines,
    );

    if empty_prompt_streak > 0 {
        if empty_prompt_streak == 1 {
            current_block.push_str(&last_prompt_line);
            current_block.push('\n');
        } else {
            current_block.push_str(&format!(
                "{} [{} empty prompts skipped]\n",
                last_prompt_line, empty_prompt_streak
            ));
        }
    }

    push_block(&mut blocks, &mut current_block);

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P2-50 回归：tar/unzip 的错误输出（`tar: ...: Cannot open ...`、`Exiting with
    /// failure status`）不得被当作提取进度行吞掉——修复前 `contains('/')` 判据把这些
    /// 含路径的错误行全部静默丢弃，失败信号系统性丢失。
    #[test]
    fn tar_error_lines_are_preserved_while_progress_is_collapsed() {
        let input = concat!(
            "user@host:~$ tar -xzf app.tar.gz\n",
            "x src/main.rs\n",
            "x src/lib.rs\n",
            "tar: src/missing.rs: Cannot open: No such file or directory\n",
            "tar: Exiting with failure status due to previous errors\n",
        );
        let blocks = compress_shell_session_blocks(input);
        let joined = blocks.join("\n");

        // 成功提取行仍正常折叠为计数提示
        assert!(
            joined.contains("[... extracted 2 files ...]"),
            "成功提取行应折叠为计数提示，实际输出:\n{joined}"
        );
        // 错误行必须原样保留
        assert!(
            joined.contains("tar: src/missing.rs: Cannot open: No such file or directory"),
            "tar 错误行不得被吞掉，实际输出:\n{joined}"
        );
        assert!(
            joined.contains("tar: Exiting with failure status due to previous errors"),
            "tar 失败汇总行不得被吞掉，实际输出:\n{joined}"
        );
    }

    /// P2-50 回归对照面：显式提取前缀（`inflating:`）在无错误标志时仍应折叠。
    #[test]
    fn unzip_success_progress_is_still_collapsed() {
        let input = concat!(
            "user@host:~$ unzip app.zip\n",
            "inflating: src/main.rs\n",
            "inflating: src/lib.rs\n",
        );
        let blocks = compress_shell_session_blocks(input);
        let joined = blocks.join("\n");

        assert!(
            joined.contains("[... extracted 2 files ...]"),
            "无错误标志的 inflating: 提取行应折叠，实际输出:\n{joined}"
        );
        assert!(
            !joined.contains("inflating:"),
            "提取进度行不应保留，实际输出:\n{joined}"
        );
    }
}
