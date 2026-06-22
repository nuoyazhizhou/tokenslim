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

pub fn compress_shell_session_blocks(input: &str) -> Vec<String> {
    let bash_re = BASH_ZSH_PROMPT.get_or_init(|| {
        Regex::new(r"^(?:[\w.-]+@[\w.-]+:?\s*[~/\w.-]*\s*[%#$>]+|\+)\s*").unwrap()
    });
    let ps_re = POWERSHELL_PROMPT.get_or_init(|| {
        Regex::new(r"^PS\s+[A-Z]:\\[^>]*>\s*").unwrap()
    });
    let cmd_re = CMD_PROMPT.get_or_init(|| {
        Regex::new(r"^[A-Z]:\\[^>]*>\s*").unwrap()
    });
    let ansi_re = ANSI_CLEANER.get_or_init(|| {
        Regex::new(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])").unwrap()
    });

    let env_var_re = ENV_VAR_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)=(.*)$").unwrap()
    });
    let multi_space_re = MULTI_SPACE_RE.get_or_init(|| {
        Regex::new(r" {2,}").unwrap()
    });
    let robocopy_file_re = ROBOCOPY_FILE_RE.get_or_init(|| {
        Regex::new(r"^\s*\d+%\s+.*?(?:File|Dir)\s+\d+\s+.*$").unwrap()
    });
    let curl_progress_re = CURL_PROGRESS_RE.get_or_init(|| {
        Regex::new(r"^\s*\d+\s+\d[\d.KMGT]*\s+\d+\s+\d[\d.KMGT]*\s+.*$").unwrap()
    });
    let tar_file_re = TAR_FILE_RE.get_or_init(|| {
        Regex::new(r"^(?:x|Extracting)\s+.*$").unwrap()
    });

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
            out.push_str(&format!("  [... compressed {} progress bar updates ...]\n", *cp));
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
            flush_accumulators(&mut current_block, &mut skipped_robocopy_files, &mut curl_progress_lines, &mut tar_file_lines);

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
                        current_block.push_str(&format!("{} [{} empty prompts skipped]\n", last_prompt_line, empty_prompt_streak));
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
                    current_block.push_str(&format!("{} [{} empty prompts skipped]\n", last_prompt_line, empty_prompt_streak));
                }
                empty_prompt_streak = 0;
            }

            // Windows DIR heuristic
            if clean_line.starts_with(" Volume in drive") || clean_line.starts_with(" Volume Serial Number") {
                continue;
            }
            let trimmed = clean_line.trim_start();
            if trimmed.starts_with(|c: char| c.is_ascii_digit()) && 
               (trimmed.contains(" File(s) ") || trimmed.contains(" Dir(s) ")) {
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
                        let is_sensitive = key_lower.contains("secret") || key_lower.contains("key") 
                            || key_lower.contains("token") || key_lower.contains("pass") 
                            || key_lower.contains("auth") || key_lower.contains("cert")
                            || val.len() > 30;

                        if is_sensitive && !val.is_empty() {
                            if val.len() > 10 && !key_lower.contains("secret") && !key_lower.contains("token") && !key_lower.contains("key") && (val.starts_with('/') || val.contains(":\\")) {
                                current_block.push_str(line_no_cr);
                            } else {
                                current_block.push_str(&format!("{}={}[REDACTED]", key, val.chars().take(4).collect::<String>()));
                            }
                        } else {
                            current_block.push_str(line_no_cr);
                        }
                        if has_newline { current_block.push('\n'); }
                        continue;
                    }
                },
                CommandType::Ls | CommandType::Ps | CommandType::Df => {
                    let compressed = multi_space_re.replace_all(&clean_line, " ");
                    current_block.push_str(&compressed);
                    if has_newline { current_block.push('\n'); }
                    continue;
                },
                CommandType::Robocopy => {
                    if robocopy_file_re.is_match(&clean_line) {
                        skipped_robocopy_files += 1;
                        continue;
                    }
                },
                CommandType::Curl => {
                    if curl_progress_re.is_match(&clean_line) {
                        curl_progress_lines += 1;
                        continue;
                    }
                },
                CommandType::Tar => {
                    if tar_file_re.is_match(&clean_line) || clean_line.contains('/') || clean_line.starts_with("inflating:") {
                        tar_file_lines += 1;
                        continue;
                    }
                },
                _ => {}
            }

            // If we didn't continue above, flush accumulators before printing the normal line
            flush_accumulators(&mut current_block, &mut skipped_robocopy_files, &mut curl_progress_lines, &mut tar_file_lines);

            // Exit status / error boundary modelling
            let lower_clean = clean_line.to_lowercase();
            if lower_clean.contains("command not found") 
                || lower_clean.contains("syntax error") 
                || lower_clean.contains("is not recognized as an internal or external command") 
                || lower_clean.contains("exception")
                || (lower_clean.contains("error") && !lower_clean.contains("errorlevel")) {
                
                // Keep the error line explicitly
                current_block.push_str(line_no_cr);
                if has_newline { current_block.push('\n'); }
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
    flush_accumulators(&mut current_block, &mut skipped_robocopy_files, &mut curl_progress_lines, &mut tar_file_lines);

    if empty_prompt_streak > 0 {
        if empty_prompt_streak == 1 {
            current_block.push_str(&last_prompt_line);
            current_block.push('\n');
        } else {
            current_block.push_str(&format!("{} [{} empty prompts skipped]\n", last_prompt_line, empty_prompt_streak));
        }
    }

    push_block(&mut blocks, &mut current_block);

    blocks
}
