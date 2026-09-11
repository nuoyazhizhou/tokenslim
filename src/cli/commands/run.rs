//! cli run 子命令

use crate::cli::app::{is_tokenslim_builtin_command, render_run_command_hint};
use crate::cli::commands::compress::merge_compression_outputs;
use crate::cli::common::*;
use crate::cli::get_plugins;
use crate::cli::types::*;
use crate::core::compression::CompressionOutput;
use crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use crate::core::metrics::{MetricsCollector, MetricsConfig};
use crate::core::path_optimizer::methods::{
    optimize_path_dictionary_blocks_with_options, PathDictionaryOptions,
};
use crate::core::path_optimizer::token_boundary::{
    is_path_token_boundary_next, replace_path_token_boundary,
};
use crate::core::plugin_config_loader::{self, RunRouteCapability};
use crate::core::plugin_dispatcher::Plugin;
use crate::utils::i18n::{render_user_facing_terminal_message, t, t1, t2, UserFacingMessage};
use serde::Serialize;
use serde_json::json;
use std::io::{self, IsTerminal, Read};

/// 从 `run_command` 中拆分出「被代理的外部程序」与「传给它的参数」。
///
/// 返回 `(prog, args)`，其中 `prog` 是 `run_command[0]`，`args` 是其余部分。
///
/// 两类拒绝场景（均返回 [`CliError::InvalidArgs`]，并附带本地化提示）：
/// - `run_command` 为空 → `E_CLI_RUN_EMPTY`；
/// - 首个 token 以 `-` 开头 → `E_CLI_RUN_INVALID_TARGET`。此时会额外判断去掉前导 `-`
///   后是否为 TokenSlim 内置命令（如 `--gain`），命中则提示用户误把内置命令当外部命令执行。
///
/// `program` 仅用于渲染提示文案中的自身可执行名，不参与解析逻辑。
pub(crate) fn parse_run_target<'a>(
    program: &str,
    run_command: &'a [String],
) -> Result<(&'a str, &'a [String]), CliError> {
    if run_command.is_empty() {
        let hint = render_run_command_hint(program);
        return Err(CliError::InvalidArgs(format!(
            "{}\n\n{}",
            format_invalid_args_message(
                "E_CLI_RUN_EMPTY",
                "未提供要执行的外部命令。",
                "No external command was provided for run mode.",
                Some(format!("{program} run git status 或 {program} git status")),
                Some(format!("{program} run git status or {program} git status")),
            ),
            hint
        )));
    }
    let prog = run_command[0].as_str();
    if prog.starts_with('-') {
        let normalized = prog.trim_start_matches('-');
        let (hint_zh, hint_en) = if normalized.eq_ignore_ascii_case("gain") {
            (
                Some(format!(
                    "检测到内置命令: `{program} gain`。若要执行外部命令，请使用 `{program} run <command>`。"
                )),
                Some(format!(
                    "Detected built-in command: `{program} gain`. For external commands, use `{program} run <command>`."
                )),
            )
        } else if is_tokenslim_builtin_command(normalized) {
            (
                Some(format!(
                    "检测到内置命令: `{program} {normalized}`。若要执行外部命令，请使用 `{program} run <command>`。"
                )),
                Some(format!(
                    "Detected built-in command: `{program} {normalized}`. For external commands, use `{program} run <command>`."
                )),
            )
        } else {
            (
                Some(format!(
                    "请改为: {program} run git status 或 {program} git status"
                )),
                Some(format!(
                    "Try: {program} run git status or {program} git status"
                )),
            )
        };
        let hint = render_run_command_hint(program);
        return Err(CliError::InvalidArgs(format!(
            "{}\n\n{}",
            format_invalid_args_message(
                "E_CLI_RUN_INVALID_TARGET",
                format!("`{prog}` 不是可执行命令。"),
                format!("`{prog}` is not a valid executable command."),
                hint_zh,
                hint_en,
            ),
            hint
        )));
    }
    Ok((prog, &run_command[1..]))
}

/// 判断 `prog` 是否是 `git` 可执行 (兼容绝对/相对路径与 Windows 扩展名)。
///
/// 例如全部返回 `true`:
/// - `"git"`
/// - `"/usr/bin/git"`
/// - `"C:\\Program Files\\Git\\bin\\git.exe"`
pub(crate) fn is_git_program(prog: &str) -> bool {
    let lower = prog.to_ascii_lowercase();
    if lower == "git" {
        return true;
    }
    // 取 basename (兼容 / 与 \)
    let basename = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    // 去掉 Windows 可执行扩展名 (.exe / .cmd / .bat)
    let stem = basename
        .strip_suffix(".exe")
        .or_else(|| basename.strip_suffix(".cmd"))
        .or_else(|| basename.strip_suffix(".bat"))
        .unwrap_or(basename);
    stem == "git"
}

/// 检测 `git <subcmd> [args...]` 是否需要交互式输入 (vim/merge-tool/hunk 选择器等)。
///
/// 命中后调用方应放弃 stdout 压缩, 直接透传 stdio 给原生命令, 否则子进程会卡死。
///
/// 黑名单规则 (与 `git --help` 行为对齐):
/// - `commit` 无 `-m` / `-F` / `--file` / `--message` / `--no-edit` → 打开 vim
/// - `rebase` 含 `-i` / `--interactive` → 打开 todo list 编辑器
/// - `tag` 含 `-a` / `--annotate` 且无 `-m` / `-F` → 打开 vim
/// - `add` 含 `-p` / `--patch` → hunk 选择器
/// - `checkout` / `restore` / `rm` 含 `-p` / `--patch` → hunk 选择器
/// - `clean` 含 `-i` / `--interactive` → 文件选择器
///
/// **不**进黑名单 (无冲突/无 flag 时不进入交互):
/// - `merge` / `pull` / `cherry-pick` / `stash` — 无冲突时无 tty 需求
/// - `push` — 协议层 (HTTP/SSH agent) 处理认证
/// - `branch` / `log` / `diff` / `show` / `fetch` / `clone`
///
/// 注意: 这是启发式检测, 别名/外部 `git-foo` 工具可能漏判。漏判的最坏后果是
/// 用户再次卡住, 不会损坏数据。
pub(crate) fn detect_git_interactive(prog: &str, args: &[String]) -> bool {
    if !is_git_program(prog) {
        return false;
    }
    let sub = match args.first().map(String::as_str) {
        Some(s) => s,
        None => return false, // 裸 `git` 本身是 help, 不交互
    };

    // 通用 flag 命中检测: 完全相等 / 短/长 flag / 带 `=` 的形式
    let has = |flag: &str| -> bool {
        let eq_form = format!("{flag}=");
        args.iter().any(|a| a == flag || a.starts_with(&eq_form))
    };

    match sub {
        "commit" => {
            // `-m` / `-F` / `--file` / `--message` / `--no-edit` 都能跳过 vim
            !(has("-m") || has("-F") || has("--file") || has("--message") || has("--no-edit"))
        }
        "rebase" => has("-i") || has("--interactive"),
        "tag" => (has("-a") || has("--annotate")) && !(has("-m") || has("-F")),
        "add" => has("-p") || has("--patch"),
        "checkout" | "restore" | "rm" => has("-p") || has("--patch"),
        "clean" => has("-i") || has("--interactive"),
        _ => false,
    }
}

/// 透传 stdio 跑外部命令 (无压缩, 无 tty 转发)。
///
/// 用于 `git` 交互式子命令的 fallback: 不接管 stdout/stderr/stdin, 让子进程
/// 看到真实的 tty, vim/merge-tool 等能正常工作。退出码透传给调用方。
pub(crate) fn run_external_command_passthrough(
    prog: &str,
    cmd_args: &[String],
) -> Result<std::process::ExitStatus, CliError> {
    use std::process::Stdio;
    let mut child = std::process::Command::new(prog);
    child
        .args(cmd_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = child
        .spawn()
        .map_err(CliError::Io)?
        .wait()
        .map_err(CliError::Io)?;
    Ok(status)
}

/// 执行外部命令并捕获其全部输出，供后续压缩使用（run 模式的取数入口）。
///
/// 行为要点：
/// - Windows 下经 `cmd /C <prog> <args>` 启动，以便解析 `.cmd` / `.bat` 包装器
///   （如 `npm`）；其他平台直接 spawn `prog`。
/// - stdout 与 stderr 各起一条读取线程并发消费，避免管道缓冲区写满导致子进程死锁。
/// - `passthrough` 为真时把两路原始字节实时回显到**本进程 stderr**（不是 stdout），
///   以保证 stdout 只承载压缩后的结果，便于下游继续管道处理。
/// - `tee_file` 非空时把原始字节同步落盘（自动创建父目录），用于事后对照压缩前后的差异。
///
/// 返回 `(退出状态, 合并文本)`。两路字节各自经
/// `encoding_fallback::decode_and_repair_for_display` 解码修复后，以
/// `stdout + "\n" + stderr` 顺序拼接（任一为空则只取另一路）。
pub(crate) fn run_external_command_capture(
    prog: &str,
    cmd_args: &[String],
    passthrough: bool,
    tee_file: Option<&std::path::Path>,
) -> Result<(std::process::ExitStatus, String), CliError> {
    use std::fs::File;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    let mut child = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C");
        c.arg(prog);
        c.args(cmd_args);
        c
    } else {
        let mut c = std::process::Command::new(prog);
        c.args(cmd_args);
        c
    };

    let mut child = child
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(CliError::Io)?;

    let child_stdout = child.stdout.take().ok_or_else(|| {
        CliError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to capture stdout",
        ))
    })?;
    let child_stderr = child.stderr.take().ok_or_else(|| {
        CliError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to capture stderr",
        ))
    })?;

    // 初始化 tee 物理输出文件句柄
    let tee_writer = if let Some(path) = tee_file {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = File::create(path).map_err(CliError::Io)?;
        Some(Arc::new(Mutex::new(file)))
    } else {
        None
    };

    let stdout_bytes = Arc::new(Mutex::new(Vec::new()));
    let stderr_bytes = Arc::new(Mutex::new(Vec::new()));

    // 开启线程 1 并发读取 stdout 流
    let stdout_bytes_clone = stdout_bytes.clone();
    let tee_writer_clone = tee_writer.clone();
    let stdout_handle = std::thread::spawn(move || {
        let mut reader = child_stdout;
        let mut buf = [0u8; 8192];
        loop {
            match std::io::Read::read(&mut reader, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    stdout_bytes_clone.lock().unwrap().extend_from_slice(chunk);

                    if passthrough {
                        let mut stderr = std::io::stderr();
                        let _ = stderr.write_all(chunk);
                        let _ = stderr.flush();
                    }
                    if let Some(ref file_arc) = tee_writer_clone {
                        if let Ok(mut file) = file_arc.lock() {
                            let _ = file.write_all(chunk);
                            let _ = file.flush();
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    // 开启线程 2 并发读取 stderr 流
    let stderr_bytes_clone = stderr_bytes.clone();
    let tee_writer_clone = tee_writer.clone();
    let stderr_handle = std::thread::spawn(move || {
        let mut reader = child_stderr;
        let mut buf = [0u8; 8192];
        loop {
            match std::io::Read::read(&mut reader, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    stderr_bytes_clone.lock().unwrap().extend_from_slice(chunk);

                    if passthrough {
                        let mut stderr = std::io::stderr();
                        let _ = stderr.write_all(chunk);
                        let _ = stderr.flush();
                    }
                    if let Some(ref file_arc) = tee_writer_clone {
                        if let Ok(mut file) = file_arc.lock() {
                            let _ = file.write_all(chunk);
                            let _ = file.flush();
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    // 等待双路读取完毕
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    let status = child.wait().map_err(CliError::Io)?;

    let out_buf = stdout_bytes.lock().unwrap().clone();
    let err_buf = stderr_bytes.lock().unwrap().clone();

    let (out_str, _out_enc, _out_fix_steps) =
        crate::core::encoding_fallback::decode_and_repair_for_display(&out_buf);
    let (err_str, _err_enc, _err_fix_steps) =
        crate::core::encoding_fallback::decode_and_repair_for_display(&err_buf);

    let combined = if err_str.is_empty() {
        out_str.to_string()
    } else if out_str.is_empty() {
        err_str.to_string()
    } else {
        format!("{}\n{}", out_str, err_str)
    };

    Ok((status, combined))
}

/// 判断命令 token 在锚点行中是否必须加引号。
///
/// 满足任一即需要引用：
/// - token 为空串（否则会在重新解析时整体消失）；
/// - 含任意空白字符（否则被切成多个 token）；
/// - 含 shell 元字符：`"` `'` `` ` `` `$` `&` `|` `;` `<` `>` `(` `)` `[` `]` `{` `}` `*` `!` `?` `#`。
///
/// 目的是保证锚点行「写出去再读回来」仍能还原成同一组 token。
pub(crate) fn should_quote_run_anchor_token(token: &str) -> bool {
    token.is_empty()
        || token.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '"' | '\''
                        | '`'
                        | '$'
                        | '&'
                        | '|'
                        | ';'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '*'
                        | '!'
                        | '?'
                        | '#'
                )
        })
}

/// 按需为单个命令 token 加双引号。
///
/// 不需要引用时（见 [`should_quote_run_anchor_token`]）原样返回；
/// 需要时先把 `\` 转义为 `\\`、`"` 转义为 `\"`，再整体包上双引号，
/// 与 [`tokenize_command_line`] 的双引号+转义解析规则严格对称。
pub(crate) fn quote_run_anchor_token(token: &str) -> String {
    if !should_quote_run_anchor_token(token) {
        return token.to_string();
    }
    let escaped = token.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// 按 shell 风格把一行命令切分为 token 列表。
///
/// 支持三种状态：无引号（空白分隔）、单引号（内部全部字面量）、双引号
/// （支持 `\` 转义下一个字符）。
///
/// 返回 `None` 表示**引号未闭合**（单/双引号状态未回到无引号态），
/// 调用方据此判定该行不是可信的命令锚点。若结尾残留一个悬空 `\`，
/// 则按字面量补回，不视为错误。
pub(crate) fn tokenize_command_line(line: &str) -> Option<Vec<String>> {
    #[derive(Clone, Copy)]
    enum QuoteMode {
        None,
        Single,
        Double,
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut mode = QuoteMode::None;
    let mut escaped = false;

    for ch in line.chars() {
        match mode {
            QuoteMode::None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else if ch == '"' {
                    mode = QuoteMode::Double;
                } else if ch == '\'' {
                    mode = QuoteMode::Single;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Single => {
                if ch == '\'' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Double => {
                if escaped {
                    current.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
        }
    }

    match mode {
        QuoteMode::None => {}
        QuoteMode::Single | QuoteMode::Double => return None,
    }

    if escaped {
        current.push('\\');
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Some(tokens)
}

/// 以 token 级比较判断 `line` 是否与 `prog + cmd_args` 表示同一条命令。
///
/// 判定步骤：
/// 1. [`tokenize_command_line`] 拆分 `line`（引号未闭合则判否）；
/// 2. token 数必须等于 `cmd_args.len() + 1`；
/// 3. 程序名用 [`command_keyword`] 归一化后比较——因此
///    `git`、`git.exe`、`C:\...\bin\git.exe` 视为同一程序；
/// 4. 其余参数逐一**严格相等**比较（不做大小写或路径归一化）。
pub(crate) fn is_equivalent_run_anchor_line(line: &str, prog: &str, cmd_args: &[String]) -> bool {
    let Some(actual_tokens) = tokenize_command_line(line.trim_start()) else {
        return false;
    };
    if actual_tokens.is_empty() {
        return false;
    }

    let expected_len = cmd_args.len() + 1;
    if actual_tokens.len() != expected_len {
        return false;
    }

    if command_keyword(&actual_tokens[0]) != command_keyword(prog) {
        return false;
    }

    actual_tokens
        .iter()
        .skip(1)
        .zip(cmd_args.iter())
        .all(|(actual, expected)| actual == expected)
}

/// 生成可安全回读的规范化命令锚点行。
///
/// 程序名与每个参数都先经 [`quote_run_anchor_token`] 处理（必要时加双引号并转义），
/// 再用单个空格连接。产出的行满足 [`tokenize_command_line`] 的可逆解析要求，
/// 因此二次解析后能与原始 `prog + cmd_args` 判定等价。
pub(crate) fn build_run_command_anchor(prog: &str, cmd_args: &[String]) -> String {
    let mut parts = Vec::with_capacity(cmd_args.len() + 1);
    parts.push(quote_run_anchor_token(prog));
    for arg in cmd_args {
        parts.push(quote_run_anchor_token(arg));
    }
    parts.join(" ")
}

/// 判断任意一行文本本身是否是一条 VCS 命令行。
///
/// 先 [`tokenize_command_line`] 切分，再把首 token 当程序名、其余当参数交给
/// [`detect_run_plugin_route`]，命中 `Vcs` 分组即为真。
///
/// 用于 [`place_path_dictionary_line`] 判断首行是否为命令锚点——
/// 若是，则路径字典行必须插到锚点之后，不能顶掉锚点（压缩协议法则 0）。
pub(crate) fn is_explicit_vcs_command_line(line: &str) -> bool {
    let Some(tokens) = tokenize_command_line(line.trim_start()) else {
        return false;
    };
    if tokens.is_empty() {
        return false;
    }
    let prog = &tokens[0];
    let args = tokens.iter().skip(1).cloned().collect::<Vec<_>>();
    matches!(detect_run_plugin_route(prog, &args), RunPluginRoute::Vcs)
}

/// 判断 `line` 是否已经是本次执行命令的显式锚点行。
///
/// 空行直接判否；否则委托 [`is_equivalent_run_anchor_line`] 做 token 级等价比较。
/// 当前仅有这一条判定规则，保留独立函数是为了后续扩展其他锚点形态
/// （如带 `$ ` 提示符前缀、带 `tokenslim run` 前缀等）时不影响调用方。
pub(crate) fn is_explicit_run_command_line(line: &str, prog: &str, cmd_args: &[String]) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    if is_equivalent_run_anchor_line(trimmed, prog, cmd_args) {
        return true;
    }

    false
}

/// 保证输入文本首行是命令锚点——**压缩协议法则 0 的落地点**。
///
/// 取 `combined` 的首个非空行（去掉行尾 `\r`）判断它是否已等价于本次执行的命令
/// （见 [`is_explicit_run_command_line`]）：
/// - 已是锚点 → 原样返回，避免锚点重复；
/// - 否则在最前面插入 [`build_run_command_anchor`] 生成的规范化命令行。
///
/// 缺失锚点会导致下游 Rust 解析器无法定位触发命令，进而 serde 反序列化失败，
/// 因此本函数是 run 模式压缩前的强制前置步骤。
pub(crate) fn prepend_run_command_anchor_if_needed(
    combined: &str,
    prog: &str,
    cmd_args: &[String],
) -> String {
    let first_non_empty = combined
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim_end_matches('\r');
    if is_explicit_run_command_line(first_non_empty, prog, cmd_args) {
        combined.to_string()
    } else {
        format!("{}\n{}", build_run_command_anchor(prog, cmd_args), combined)
    }
}

/// run 模式识别出的 VCS 命令意图，决定 VCS 插件启用哪一档语义紧凑规则。
///
/// `Other` 表示「确认是 VCS 命令但无细分档位」，与 `Option::None`（非 VCS 命令）不同。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VcsRunIntent {
    Status,
    Log,
    Diff,
    Other,
}

/// run 模式的插件路由分组，决定本次执行保留哪些插件参与压缩。
///
/// - `Vcs`：保留全部插件（VCS 插件需要与其他插件协同）；
/// - `Node` / `Build`：剔除 VCS 插件，避免把构建日志误判成 VCS 输出；
/// - `Generic`：仅保留通用清洗插件（见 [`keep_generic_run_plugins`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunPluginRoute {
    Vcs,
    Node,
    Build,
    Generic,
}

/// 把可执行路径归一化为「命令关键字」，用于路由与锚点比较。
///
/// 处理链：剥离首尾双引号 → 取文件名部分（丢弃目录）→ 转小写
/// → 去掉 Windows 可执行后缀（`.exe` / `.cmd` / `.bat` / `.com` / `.ps1`）。
///
/// 例：`"C:\Program Files\Git\bin\git.exe"` → `git`；`/usr/bin/npm` → `npm`。
pub(crate) fn command_keyword(prog: &str) -> String {
    let file = std::path::Path::new(prog.trim_matches('"'))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(prog)
        .to_ascii_lowercase();

    for suffix in [".exe", ".cmd", ".bat", ".com", ".ps1"] {
        if let Some(stripped) = file.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }

    file
}

/// 加载运行路由配置（配置文件优先，缺失时内置默认）
pub(crate) fn load_run_routes() -> Vec<RunRouteCapability> {
    let config_dir = std::path::Path::new("config").join("plugins");
    plugin_config_loader::load_run_route_capabilities(if config_dir.exists() {
        Some(&config_dir)
    } else {
        None
    })
}

/// 由 run 路由配置判定本次命令的插件分组。
///
/// 每次调用都会经 [`load_run_routes`] 重新加载配置，再用
/// `plugin_config_loader::resolve_run_route` 解析出 `route_group` 字符串，
/// 映射为 [`RunPluginRoute`]；未识别的分组统一落到 `Generic`（最保守路径）。
pub(crate) fn detect_run_plugin_route(prog: &str, cmd_args: &[String]) -> RunPluginRoute {
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, prog, cmd_args);
    match route.route_group.as_str() {
        "vcs" => RunPluginRoute::Vcs,
        "node" => RunPluginRoute::Node,
        "build" => RunPluginRoute::Build,
        _ => RunPluginRoute::Generic,
    }
}

/// 剔除 VCS 相关插件（`vcs` 与 `git_diff`）。
///
/// 用于 `Node` / `Build` 分组：这类输出常内嵌路径与 diff 片段，
/// 若让 VCS 插件参与会产生错误的 VCS 语义压缩。
pub(crate) fn remove_vcs_plugins(plugins: Vec<Box<dyn Plugin>>) -> Vec<Box<dyn Plugin>> {
    plugins
        .into_iter()
        .filter(|p| !matches!(p.name(), "vcs" | "git_diff"))
        .collect()
}

/// 仅保留通用兜底插件：`generic_text` / `ansi_cleaner` / `noise_filter` / `privacy`。
///
/// 用于 `Generic` 分组——命令未匹配任何专用族时，只做「去 ANSI + 去噪 + 脱敏 + 通用文本」
/// 这类无损或近无损处理，避免专用插件对未知格式做出错误结构化假设。
pub(crate) fn keep_generic_run_plugins(plugins: Vec<Box<dyn Plugin>>) -> Vec<Box<dyn Plugin>> {
    plugins
        .into_iter()
        .filter(|p| {
            matches!(
                p.name(),
                "generic_text" | "ansi_cleaner" | "noise_filter" | "privacy"
            )
        })
        .collect()
}

/// 为本次 run 命令挑选参与压缩的插件链。
///
/// 两条路径：
/// - `run_plugin` 指定了 `--run-plugin <name>` → 只保留同名插件，外加
///   `privacy` / `ansi_cleaner` / `noise_filter` 三个基础清洗插件；
/// - 未指定 → 按 [`detect_run_plugin_route`] 的分组裁剪：
///   `Vcs` 全量保留、`Node`/`Build` 走 [`remove_vcs_plugins`]、
///   `Generic` 走 [`keep_generic_run_plugins`]。
pub(crate) fn plugins_for_run_command(
    prog: &str,
    cmd_args: &[String],
    run_plugin: Option<&str>,
) -> Vec<Box<dyn Plugin>> {
    let plugins = get_plugins();
    if let Some(target_plugin_name) = run_plugin {
        // 如果强制指定了某个插件，仅使用它及基本清洗辅助插件
        plugins
            .into_iter()
            .filter(|p| {
                p.name() == target_plugin_name
                    || matches!(p.name(), "privacy" | "ansi_cleaner" | "noise_filter")
            })
            .collect()
    } else {
        let route = detect_run_plugin_route(prog, cmd_args);
        match route {
            RunPluginRoute::Vcs => plugins,
            RunPluginRoute::Node | RunPluginRoute::Build => remove_vcs_plugins(plugins),
            RunPluginRoute::Generic => keep_generic_run_plugins(plugins),
        }
    }
}

/// 渲染 `--explain-route` 的诊断报告（纯文本 `key=value` 行）。
///
/// 输出内容：命令锚点、归一化工具名、命中的插件与分组、意图、是否兜底、
/// 命中方式与命中模式、路由优先级，随后逐条列出全部候选路由
/// （`route_candidate_N=...`），最后附上输出格式、preset、
/// 是否启用 VCS AI 紧凑模式与最终插件链。
///
/// 只做解释不做压缩，用于排查「为什么这条命令走了这个插件」。
pub(crate) fn explain_run_route(prog: &str, cmd_args: &[String], args: &CliArgs) -> String {
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, prog, cmd_args);
    let route_candidates =
        plugin_config_loader::explain_run_route_candidates(&caps, prog, cmd_args);
    let plugins = plugins_for_run_command(prog, cmd_args, args.run_plugin.as_deref());
    let plugin_names = plugins
        .iter()
        .map(|plugin| plugin.name())
        .collect::<Vec<_>>()
        .join(", ");
    let vcs_intent = get_vcs_intent(prog, cmd_args);
    let vcs_ai_compact =
        should_enable_vcs_ai_compact(vcs_intent, args.output_format.clone(), args.preset);

    let mut out = String::new();
    out.push_str("run_route\n");
    out.push_str(&format!(
        "command={}\n",
        build_run_command_anchor(prog, cmd_args)
    ));
    out.push_str(&format!("normalized_tool={}\n", route.command_keyword));
    out.push_str(&format!("route_plugin={}\n", route.plugin_name));
    out.push_str(&format!("route_group={}\n", route.route_group));
    out.push_str(&format!(
        "intent={}\n",
        route.intent.as_deref().unwrap_or("none")
    ));
    out.push_str(&format!("fallback={}\n", route.is_fallback));
    out.push_str(&format!("matched_by={}\n", route.matched_by));
    out.push_str(&format!(
        "matched_pattern={}\n",
        route.matched_pattern.as_deref().unwrap_or("none")
    ));
    out.push_str(&format!(
        "route_priority={}\n",
        route
            .priority
            .map(|p| p.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    out.push_str(&format!("route_candidates={}\n", route_candidates.len()));
    for (idx, candidate) in route_candidates.iter().enumerate() {
        out.push_str(&format!(
            "route_candidate_{}={}|group={}|priority={}|matched_by={}|matched_pattern={}|intent={}|fallback={}\n",
            idx + 1,
            candidate.plugin_name,
            candidate.route_group,
            candidate
                .priority
                .map(|p| p.to_string())
                .unwrap_or_else(|| "none".to_string()),
            candidate.matched_by,
            candidate.matched_pattern.as_deref().unwrap_or("none"),
            candidate.intent.as_deref().unwrap_or("none"),
            candidate.is_fallback
        ));
    }
    out.push_str(&format!(
        "output_format={}\n",
        match args.output_format {
            OutputFormat::Json => "json",
            OutputFormat::Markdown => "markdown",
            OutputFormat::Text => "text",
        }
    ));
    out.push_str(&format!(
        "preset={}\n",
        match args.preset {
            Some(Preset::Fast) => "fast",
            Some(Preset::Balanced) => "balanced",
            Some(Preset::Ai) => "ai",
            None => "none",
        }
    ));
    out.push_str(&format!("vcs_ai_compact={}\n", vcs_ai_compact));
    out.push_str(&format!("plugin_chain={}\n", plugin_names));
    out
}

/// 由 run 路由配置解析出 VCS 意图。
///
/// 判定顺序：
/// 1. 命中 `route_group == "vcs"` 但配置未声明 `intent` → [`VcsRunIntent::Other`]
///    （仍按 VCS 处理，只是没有更细的档位）；
/// 2. 按 `intent` 字段忽略大小写映射 `status` / `log` / `diff`，其他非空值 → `Other`；
/// 3. 非 VCS 命令 → `None`。
///
/// 返回 `None` 与 `Some(Other)` 语义不同：前者不进 VCS 压缩路径，后者进但不启用细分档位。
pub(crate) fn get_vcs_intent(prog: &str, args: &[String]) -> Option<VcsRunIntent> {
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, prog, args);
    if route.route_group.eq_ignore_ascii_case("vcs") && route.intent.is_none() {
        return Some(VcsRunIntent::Other);
    }
    match route.intent {
        Some(intent) if intent.eq_ignore_ascii_case("status") => Some(VcsRunIntent::Status),
        Some(intent) if intent.eq_ignore_ascii_case("log") => Some(VcsRunIntent::Log),
        Some(intent) if intent.eq_ignore_ascii_case("diff") => Some(VcsRunIntent::Diff),
        Some(_) => Some(VcsRunIntent::Other),
        None => None,
    }
}

/// 检测 VCS 运行意图的对外稳定入口，直接委托 [`get_vcs_intent`]。
///
/// 本函数早期为硬编码子命令白名单，现已改为配置驱动（`config/plugins` 下的
/// run route 能力声明）；保留此薄封装是为了不改动 run 模式各处调用点。
pub(crate) fn detect_vcs_run_intent(prog: &str, cmd_args: &[String]) -> Option<VcsRunIntent> {
    get_vcs_intent(prog, cmd_args)
}

/// 统计文本中路径字典页脚行的数量。
///
/// 识别两种前缀：`paths: `（页脚形态）与 `[paths]`（块形态）。
/// 注意此处用 `starts_with` 判定**未 trim** 的整行，因此带缩进的字典行不会被计入。
pub(crate) fn count_paths_footer_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with("paths: ") || line.starts_with("[paths]"))
        .count()
}

/// 从文本中剥离所有路径字典块，返回 `(去重后的条目, 剩余正文)`。
///
/// 逐行识别 trim 后以 `[paths] ` 或 `paths: ` 开头的行，块体按 `;` 分段、
/// 每段按首个 `=` 拆成 `token=path`；token 与 path 均非空且 token 首次出现时才收录
/// （用 `HashSet` 去重，先出现者胜）。
///
/// 字典行不会进入剩余正文，其余行按原顺序以 `\n` 连接返回。
pub(crate) fn parse_path_dictionary_blocks(text: &str) -> (Vec<(String, String)>, String) {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut other_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("[paths] ") || t.starts_with("paths: ") {
            let body = if let Some(rest) = t.strip_prefix("[paths] ") {
                rest
            } else if let Some(rest) = t.strip_prefix("paths: ") {
                rest
            } else {
                ""
            };
            for part in body.split(';') {
                let part = part.trim();
                if let Some(eq) = part.find('=') {
                    let token = part[..eq].trim().to_string();
                    let path = part[eq + 1..].trim().to_string();
                    if !token.is_empty() && !path.is_empty() && seen.insert(token.clone()) {
                        entries.push((token, path));
                    }
                }
            }
            continue;
        }
        other_lines.push(line);
    }

    (entries, other_lines.join("\n"))
}

/// 按 `$P` 后的数值编号对字典条目原地升序排序。
///
/// 无法解析出编号的条目取 `usize::MAX`，被排到末尾。
/// 用数值序而非字符串序，避免 `$P10` 排在 `$P2` 之前。
pub(crate) fn sort_path_entries_by_token(entries: &mut [(String, String)]) {
    entries.sort_by(|a, b| {
        let na =
            a.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        let nb =
            b.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        na.cmp(&nb)
    });
}

/// 把字典条目渲染为 `[paths] $P1=a/b; $P2=c/d` 形式的块行（不含行尾换行）。
///
/// 与页脚形态 [`render_paths_footer_line`] 的区别：前缀是 `[paths] `、
/// 无条目时也会返回仅含前缀的字符串（不返回 `Option`），
/// 因此调用方需自行保证条目非空。
pub(crate) fn render_path_dictionary_line(entries: &[(String, String)]) -> String {
    let parts: Vec<String> = entries
        .iter()
        .map(|(t, p)| format!("{}={}", t, p))
        .collect();
    format!("[paths] {}", parts.join("; "))
}

/// 决定合并后的路径字典行插入位置。
///
/// 若首个非空行是一条 VCS 命令行（[`is_explicit_vcs_command_line`]），说明它是命令锚点，
/// 字典行必须插到**锚点之后**（首个 `\n` 之后），否则会顶掉首行锚点、
/// 违反压缩协议法则 0；首行无换行符时退化为追加到末尾。
///
/// 其余情况把字典行置顶，便于下游先读字典再读正文。
pub(crate) fn place_path_dictionary_line(body_text: &str, merged_dict: &str) -> String {
    if let Some(first) = body_text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_end_matches('\r'))
    {
        if is_explicit_vcs_command_line(first) {
            if let Some(pos) = body_text.find('\n') {
                let mut out = String::new();
                out.push_str(&body_text[..=pos]);
                out.push_str(merged_dict);
                out.push('\n');
                out.push_str(&body_text[(pos + 1)..]);
                return out;
            }
            return format!("{}\n{}", body_text, merged_dict);
        }
    }

    format!("{}\n{}", merged_dict, body_text)
}

/// 尝试用「扩充后的字典」整体重建文本。
///
/// 先复制一份条目交给 [`add_subdir_entries_from_text`] 扫描高频子目录模式；
/// 若无新增条目直接返回 `None`（调用方沿用原结果）。有新增则重新建立父前缀别名、
/// 用扩充字典重写正文，并返回「字典行 + `\n` + 正文」的完整文本。
///
/// 注意此分支产出的文本把字典行**无条件置顶**，不走
/// [`place_path_dictionary_line`] 的锚点避让逻辑。
pub(crate) fn rebuild_with_extended_subdir_entries(
    entries: &[(String, String)],
    body_text: &str,
) -> Option<String> {
    let mut extended = entries.to_vec();
    if !add_subdir_entries_from_text(&mut extended, body_text) {
        return None;
    }

    apply_parent_prefix_aliases_cli(&mut extended);
    let rewritten = replace_paths_with_dict(body_text, &extended);
    let merged_dict = render_path_dictionary_line(&extended);
    Some(format!("{}\n{}", merged_dict, rewritten))
}

/// 规范化路径字典条目：排序 → 建公共父级条目 → 建父前缀嵌套别名。
///
/// 三步顺序**不可调换**：
/// 1. [`sort_path_entries_by_token`] 按编号排序，保证后续新增编号可预测；
/// 2. [`add_common_parent_entries`] 为被 3+ 条目共享的父前缀新建独立条目；
/// 3. [`apply_parent_prefix_aliases_cli`] 把长路径改写成 `$P<父>/子` 形式——
///    必须在第 2 步之后，否则父级条目还不存在，无法形成嵌套引用。
pub(crate) fn normalize_path_dictionary_entries(entries: &mut Vec<(String, String)>) {
    // 排序：按 token 编号
    sort_path_entries_by_token(entries.as_mut_slice());
    // 为 3+ 子路径的公共前缀创建父级词典条目
    add_common_parent_entries(entries);
    // 父前缀别名（必须在 add_common_parent_entries 之后）
    apply_parent_prefix_aliases_cli(entries);
}

/// 用字典重写正文，并尝试再挖一轮跨块子目录条目。
///
/// 返回 `(重写后的正文, 可选的完整重建结果)`：
/// - 第一项是 [`replace_paths_with_dict`] 把原始路径替换为 `$P` 令牌后的正文；
/// - 第二项来自 [`rebuild_with_extended_subdir_entries`]，仅当扫描到值得新建条目的
///   高频 `$P<n>/子目录` 模式时为 `Some`（内含字典行 + 正文的完整文本），
///   否则为 `None`。
pub(crate) fn rewrite_body_with_path_dictionary(
    entries: &[(String, String)],
    body_text: &str,
) -> (String, Option<String>) {
    // 用合并后的字典替换所有原始路径为 $P 令牌
    let rewritten_body = replace_paths_with_dict(body_text, entries);
    let extended = rebuild_with_extended_subdir_entries(entries, &rewritten_body);
    (rewritten_body, extended)
}

/// 把分散的多个 `[paths]` / `paths:` 字典块合并为单一字典块，并全文替换路径为令牌。
///
/// 流程：[`parse_path_dictionary_blocks`] 抽出条目与正文 → 无条目则原样返回
/// → [`normalize_path_dictionary_entries`] 排序/建父级条目/建嵌套别名
/// → [`rewrite_body_with_path_dictionary`] 用字典重写正文。
/// 若重写过程中发现可新增的跨块子目录条目，直接返回其重建结果；
/// 否则由 [`place_path_dictionary_line`] 决定字典行的落位。
///
/// 注意：字典行**并非总是置顶**——当首行是命令锚点时会插到锚点之后，
/// 以免顶掉锚点违反压缩协议法则 0。
pub(crate) fn merge_path_dictionary_blocks(text: &str) -> String {
    let (mut entries, body_text) = parse_path_dictionary_blocks(text);

    if entries.is_empty() {
        return text.to_string();
    }

    normalize_path_dictionary_entries(&mut entries);

    // 构建合并的字典行
    let merged_dict = render_path_dictionary_line(&entries);

    let (body_text, extended_result) = rewrite_body_with_path_dictionary(&entries, &body_text);

    // 跨块扫描：找出正文中多次出现的 $P_base/subdir 模式，补建专有条目
    if let Some(rebuilt) = extended_result {
        return rebuilt;
    }

    place_path_dictionary_line(&body_text, &merged_dict)
}

/// 用字典条目替换文本中的路径（先解析嵌套引用；同时支持令牌→令牌降维）
pub(crate) fn replace_paths_with_dict(text: &str, entries: &[(String, String)]) -> String {
    // 先解析嵌套引用：$P6=$P15/vcs_bzr → $P6=src/plugins/vcs_bzr
    let mut resolved: Vec<(String, String)> = entries.to_vec();
    for i in 0..resolved.len() {
        let val = &resolved[i].1;
        if !val.starts_with('$') {
            continue;
        }
        if let Some(slash) = val.find('/') {
            let token = &val[..slash];
            if let Some(target) = resolved
                .iter()
                .find(|(t, _)| t == token)
                .map(|(_, p)| p.clone())
            {
                if !target.starts_with('$') {
                    resolved[i].1 = format!("{}/{}", target, &val[slash + 1..]);
                }
            }
        } else {
            if let Some(target) = resolved
                .iter()
                .find(|(t, _)| t == val)
                .map(|(_, p)| p.clone())
            {
                if !target.starts_with('$') {
                    resolved[i].1 = target;
                }
            }
        }
    }

    // 按路径长度降序排列
    let mut sorted: Vec<&(String, String)> = resolved.iter().collect();
    sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let mut result = text.to_string();
    for (token, path) in sorted {
        // 替换原始绝对路径
        result = result.replace(path.as_str(), token.as_str());
    }

    // 第二遍：令牌→令牌降维（$P15/subdir/ → $P16/）
    // 对路径值含 $P 的条目（如 $P16=$P15/vcs_fossil_plugin），替换正文中的 $P15/vcs_fossil_plugin/ 为 $P16/
    for (token, path) in entries.iter().filter(|(_, p)| p.contains("$P")) {
        // path 是 $P15/vcs_fossil_plugin 格式
        // 在正文中查找 $P15/vcs_fossil_plugin/ 并替换为 $P16/
        if path.contains('/') {
            result = result.replace(path.as_str(), token.as_str());
        }
    }
    result
}

/// 扫描正文中 2+ 次出现的 $Pbase/subdir 模式，创建跨块专有条目
pub(crate) fn add_subdir_entries_from_text(
    entries: &mut Vec<(String, String)>,
    text: &str,
) -> bool {
    let parent_map: std::collections::HashMap<String, String> = entries
        .iter()
        .filter(|(_, p)| !p.starts_with('$'))
        .map(|(t, p)| (t.clone(), p.clone()))
        .collect();

    let re = regex::Regex::new(r"\$P\d+/([^/\s]+)/").unwrap();
    let mut subdir_counts: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for cap in re.captures_iter(text) {
        let full = cap.get(0).unwrap().as_str();
        let subdir = cap.get(1).unwrap().as_str();
        if let Some(slash) = full.find('/') {
            let token = &full[..slash];
            *subdir_counts
                .entry((token.to_string(), subdir.to_string()))
                .or_insert(0) += 1;
        }
    }

    let existing: std::collections::HashSet<String> =
        entries.iter().map(|(_, p)| p.clone()).collect();
    let mut added = false;
    for ((parent, subdir), count) in subdir_counts {
        if count >= 2 {
            let full_path = format!("{}/{}", parent_map.get(&parent).unwrap_or(&parent), subdir);
            if !existing.contains(&full_path) {
                let n = entries
                    .iter()
                    .filter_map(|(t, _)| t.strip_prefix("$P").and_then(|s| s.parse::<usize>().ok()))
                    .max()
                    .unwrap_or(0)
                    + 1;
                entries.push((format!("$P{}", n), full_path));
                added = true;
            }
        }
    }
    if added {
        entries.sort_by(|a, b| {
            let na =
                a.0.strip_prefix("$P")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
            let nb =
                b.0.strip_prefix("$P")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
            na.cmp(&nb)
        });
    }
    added
}

/// 为被多个条目共享的公共父目录新建字典条目。
///
/// 三步：[`collect_existing_paths`] 收集已有路径 →
/// [`collect_parent_prefix_counts`] 统计各父前缀出现次数 →
/// [`append_common_parent_entries`] 对出现 ≥3 次且尚未收录的前缀追加新条目；
/// 最后按编号重新排序。
///
/// 阈值 3 是 ROI 权衡：新增条目自身要占 Token，被引用太少则不划算。
pub(crate) fn add_common_parent_entries(entries: &mut Vec<(String, String)>) {
    let existing_paths: std::collections::HashSet<String> = collect_existing_paths(entries);
    let prefix_counts = collect_parent_prefix_counts(entries);
    append_common_parent_entries(entries, &existing_paths, prefix_counts);

    entries.sort_by(|a, b| {
        let na =
            a.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        let nb =
            b.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        na.cmp(&nb)
    });
}

/// 收集字典中已登记的全部路径值，用于判重。
///
/// 注意收集的是 value 侧（路径），不是 token 侧；且包含 `$P<n>/sub` 这类
/// 已被改写为嵌套引用的值。
pub(crate) fn collect_existing_paths(
    entries: &[(String, String)],
) -> std::collections::HashSet<String> {
    entries.iter().map(|(_, p)| p.clone()).collect()
}

/// 统计每个「直接父目录前缀」在字典中被多少条目共享。
///
/// 跳过值以 `$` 开头的条目（已是嵌套引用，其父级已被抽取过）；
/// 其余按最后一个 `/` 切出父前缀累加计数。
/// 仅按 `/` 切分，Windows 风格 `\` 分隔的路径不会被统计。
pub(crate) fn collect_parent_prefix_counts(
    entries: &[(String, String)],
) -> std::collections::HashMap<String, usize> {
    let mut prefix_counts = std::collections::HashMap::new();
    for (_, path) in entries {
        if path.starts_with('$') {
            continue;
        }
        if let Some(last_slash) = path.rfind('/') {
            *prefix_counts
                .entry(path[..last_slash].to_string())
                .or_insert(0) += 1;
        }
    }
    prefix_counts
}

/// 计算下一个可用的 `$P` 编号：现有最大编号 + 1（无有效编号时从 1 起）。
///
/// 只做「取最大值 +1」，不复用中间空洞编号，保证新增令牌单调递增、不与历史令牌撞号。
pub(crate) fn next_path_token_id(entries: &[(String, String)]) -> usize {
    entries
        .iter()
        .filter_map(|(t, _)| t.strip_prefix("$P").and_then(|s| s.parse::<usize>().ok()))
        .max()
        .unwrap_or(0)
        + 1
}

/// 为出现 ≥3 次且未登记过的父前缀追加新字典条目。
///
/// 每次追加都重新调用 [`next_path_token_id`] 取号，保证同一批多个新条目编号不冲突。
///
/// 注意入参 `prefix_counts` 是 `HashMap`，遍历顺序不确定，
/// 因此多个新增条目之间的编号分配顺序不保证稳定（调用方随后会重新排序）。
pub(crate) fn append_common_parent_entries(
    entries: &mut Vec<(String, String)>,
    existing_paths: &std::collections::HashSet<String>,
    prefix_counts: std::collections::HashMap<String, usize>,
) {
    for (prefix, count) in prefix_counts {
        if count >= 3 && !existing_paths.contains(&prefix) {
            let next = next_path_token_id(entries);
            entries.push((format!("$P{}", next), prefix));
        }
    }
}

/// 把可归约的长路径改写为「父令牌 + 相对子路径」的嵌套别名形式。
///
/// 对任意两条均为字面量路径的条目 `i`、`j`，若 `path_i` 以 `path_j` 为前缀且更长，
/// 则把 `path_i` 改写为 `$P<j>/<剩余后缀>`（后缀去掉前导 `/` 与 `\`）。
///
/// 外层 `while changed` 循环反复迭代直至不再有可改写项，以支持多层嵌套归约
/// （如 `$P1` → `$P2=$P1/a` → `$P3=$P2/b`）。值已是 `$` 开头的条目被跳过，
/// 避免二次改写。
pub(crate) fn apply_parent_prefix_aliases_cli(entries: &mut Vec<(String, String)>) {
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..entries.len() {
            let path_i = entries[i].1.clone();
            if path_i.starts_with('$') {
                continue;
            }
            for j in 0..entries.len() {
                if i == j {
                    continue;
                }
                let (ref token_j, ref path_j) = entries[j];
                if path_j.starts_with('$') {
                    continue;
                }
                if path_i.starts_with(path_j.as_str()) && path_i.len() > path_j.len() {
                    let suffix = &path_i[path_j.len()..]
                        .trim_start_matches('/')
                        .trim_start_matches('\\');
                    if !suffix.is_empty() {
                        entries[i].1 = format!("{}/{}", token_j, suffix);
                        changed = true;
                        break;
                    }
                }
            }
        }
    }
}

/// 删除与压缩后语义标记重复的 VCS 原始表头行。
///
/// 压缩后已有结构化标记时，原始英文表头就是纯冗余，按四条规则收集待删行号：
/// - 出现 `BR:<name>` → 删除 `On branch <name>`；
/// - 出现 `[changes]` → 删除 `Changes not staged for commit:` /
///   `Changes to be committed:`；
/// - 出现 `[untracked]` → 删除 `Untracked files:`；
/// - 出现任意 `CH:` 行 → 删除所有以 `commit ` 开头且长度 > 7 的行。
///
/// 无待删行时原样返回；否则按行过滤后以 `\n` 重连并去掉尾部多余换行。
pub(crate) fn strip_duplicate_vcs_headers(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut to_remove: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // 检测 BR:X → 移除前置 "On branch X"
    let mut br_name = String::new();
    for line in &lines {
        if let Some(b) = line.trim().strip_prefix("BR:") {
            br_name = b.to_string();
            break;
        }
    }
    if !br_name.is_empty() {
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == format!("On branch {}", br_name) {
                to_remove.insert(i);
            }
        }
    }

    // 检测 [changes] → 移除原始 section header
    if lines.iter().any(|l| l.trim() == "[changes]") {
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t == "Changes not staged for commit:" || t == "Changes to be committed:" {
                to_remove.insert(i);
            }
        }
    }

    // 检测 [untracked] → 移除原始 section header
    if lines.iter().any(|l| l.trim() == "[untracked]") {
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == "Untracked files:" {
                to_remove.insert(i);
            }
        }
    }

    // 检测 CH: 行 → 移除前置 "commit <hash>" 原始行
    if lines.iter().any(|l| l.trim().starts_with("CH:")) {
        for (i, line) in lines.iter().enumerate() {
            if line.trim().starts_with("commit ") && line.trim().len() > 7 {
                to_remove.insert(i);
            }
        }
    }

    if to_remove.is_empty() {
        return text.to_string();
    }

    let kept: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !to_remove.contains(i))
        .map(|(_, s)| s.to_string())
        .collect();
    kept.join("\n").trim_end_matches('\n').to_string()
}

/// 把 `$P<n>` 形式的路径令牌解析为数值编号，用作排序键。
///
/// 解析失败（前缀不是 `$P`，或后缀非数字）时返回 `usize::MAX`，
/// 使这类非常规令牌被排到末尾而不是引发 panic。
pub(crate) fn token_key_as_num(token: &str) -> usize {
    token
        .strip_prefix("$P")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
}

/// 从压缩产物字典中取出全部路径条目，并按 `$P` 编号升序排列。
///
/// 排序键由 [`token_key_as_num`] 提供，保证页脚里 `$P1; $P2; $P10` 是自然数序
/// 而非字典序（`$P10` 不会排到 `$P2` 前面）。
pub(crate) fn collect_sorted_output_path_entries(
    output: &crate::core::compression::CompressionOutput,
) -> Vec<(String, String)> {
    let mut all_entries: Vec<(String, String)> = output
        .dictionary
        .paths
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    all_entries.sort_by(|a, b| token_key_as_num(&a.0).cmp(&token_key_as_num(&b.0)));
    all_entries
}

/// 按令牌在正文中的实际引用次数把路径条目分成「保留」与「丢弃」两组。
///
/// 引用次数由 [`count_path_token_uses`] 统计（带令牌边界校验）；
/// 达到 `min_footer_token_uses` 阈值的进保留组写入页脚，未达标的进丢弃组，
/// 由调用方回填成原始路径。
///
/// 返回 `(keep_entries, drop_entries)`。这是页脚 ROI 门控：字典条目本身占 Token，
/// 只有被多次引用才划得来。
pub(crate) fn partition_path_entries_by_min_uses(
    formatted: &str,
    all_entries: Vec<(String, String)>,
    min_footer_token_uses: usize,
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let mut keep_entries: Vec<(String, String)> = Vec::new();
    let mut drop_entries: Vec<(String, String)> = Vec::new();
    for (token, path) in all_entries {
        if count_path_token_uses(formatted, &token) >= min_footer_token_uses {
            keep_entries.push((token, path));
        } else {
            drop_entries.push((token, path));
        }
    }
    (keep_entries, drop_entries)
}

/// 把路径条目渲染为 `paths: $P1=a/b; $P2=c/d\n` 形式的页脚行（自带行尾换行）。
///
/// 条目为空时返回 `None`，让调用方跳过追加而不是写出一个空页脚。
pub(crate) fn render_paths_footer_line(entries: &[(String, String)]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    let parts: Vec<String> = entries
        .iter()
        .map(|(token, path)| format!("{}={}", token, path))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!("paths: {}\n", parts.join("; ")))
}

/// 判断是否跳过追加路径字典页脚（任一成立即跳过）。
///
/// - 文本中已存在页脚行（避免重复追加）；
/// - 正文不含 `$P` 令牌（没有引用，页脚纯属冗余）；
/// - 压缩产物的路径字典本身为空。
pub(crate) fn should_skip_paths_footer_append(
    formatted: &str,
    output: &crate::core::compression::CompressionOutput,
) -> bool {
    count_paths_footer_lines(formatted) > 0
        || !formatted.contains("$P")
        || output.dictionary.paths.is_empty()
}

/// 把未达页脚阈值的路径令牌**回填**为原始路径。
///
/// 逐条调用 `replace_path_token_boundary` 做带边界校验的替换，
/// 避免 `$P1` 误伤 `$P10`。回填后这些令牌不再出现在正文，
/// 因此也不需要写入页脚，净省 Token。
pub(crate) fn rewrite_dropped_path_tokens(
    formatted: &str,
    drop_entries: &[(String, String)],
) -> String {
    let mut rewritten = formatted.to_string();
    for (token, path) in drop_entries {
        rewritten = replace_path_token_boundary(&rewritten, token, path);
    }
    rewritten
}

/// 把页脚行追加到正文末尾，必要时先补一个换行。
///
/// 通过 `ends_with('\n')` 判断，保证页脚独占一行、不会与正文最后一行粘连。
pub(crate) fn append_paths_footer_line(mut rewritten: String, footer: &str) -> String {
    if !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    rewritten.push_str(footer);
    rewritten
}

/// 依据压缩产物的路径字典，为文本追加 `paths:` 页脚。
///
/// 流程：
/// 1. [`should_skip_paths_footer_append`] 命中则原样返回；
/// 2. [`collect_sorted_output_path_entries`] 取出按编号排序的全部条目；
/// 3. [`partition_path_entries_by_min_uses`] 按 `min_footer_token_uses` 阈值分为
///    保留组与丢弃组；
/// 4. 丢弃组经 [`rewrite_dropped_path_tokens`] **回填为原始路径**——低频令牌写进页脚
///    反而增加 Token，故就地还原；
/// 5. 保留组渲染成页脚行并追加（[`render_paths_footer_line`] +
///    [`append_paths_footer_line`]）；保留组为空则只返回回填后的正文。
pub(crate) fn append_paths_footer_from_output_dictionary(
    formatted: &str,
    output: &crate::core::compression::CompressionOutput,
    path_options: &PathDictionaryOptions,
) -> String {
    if should_skip_paths_footer_append(formatted, output) {
        return formatted.to_string();
    }

    let all_entries = collect_sorted_output_path_entries(output);
    let (keep_entries, drop_entries) = partition_path_entries_by_min_uses(
        formatted,
        all_entries,
        path_options.min_footer_token_uses,
    );

    let rewritten = rewrite_dropped_path_tokens(formatted, &drop_entries);

    if keep_entries.is_empty() {
        return rewritten;
    }

    let footer = match render_paths_footer_line(&keep_entries) {
        Some(line) => line,
        None => return rewritten,
    };

    append_paths_footer_line(rewritten, &footer)
}

/// 统计 `token` 在文本中作为**完整路径令牌**出现的次数。
///
/// 逐次 `find` 定位后，用 `is_path_token_boundary_next` 校验紧随其后的字节是否为
/// 合法边界，借此排除 `$P1` 命中 `$P10` 前缀这类误计。
/// 空 token 直接返回 0（否则 `find("")` 会无限命中）。
pub(crate) fn count_path_token_uses(text: &str, token: &str) -> usize {
    if token.is_empty() {
        return 0;
    }

    let mut count = 0usize;
    let mut start = 0usize;
    while let Some(pos) = text[start..].find(token) {
        let idx = start + pos;
        let end = idx + token.len();
        let next = text.as_bytes().get(end).copied();
        if is_path_token_boundary_next(next) {
            count += 1;
        }
        start = end;
    }
    count
}

/// 判断是否启用 VCS AI 紧凑模式（三条件同时成立）。
///
/// 1. 命令被识别为 VCS 命令（`vcs_intent.is_some()`）；
/// 2. 用户显式指定了 `--preset`（未指定则保守起见不启用）；
/// 3. 输出格式为 `Text` 或 `Markdown`——`Json` 面向机器消费，需保留结构化字段，
///    不做面向人读/LLM 的语义紧凑。
pub(crate) fn should_enable_vcs_ai_compact(
    vcs_intent: Option<VcsRunIntent>,
    output_format: OutputFormat,
    preset: Option<Preset>,
) -> bool {
    vcs_intent.is_some()
        && preset.is_some()
        && matches!(output_format, OutputFormat::Text | OutputFormat::Markdown)
}

/// 判断是否需要对已渲染文本再跑一轮路径字典优化。
///
/// 决策依据是文本中 `paths:` / `[paths]` 页脚的数量（见 [`count_paths_footer_lines`]）：
/// - 非 `Text` / `Markdown` 格式 → 一律不做；
/// - 页脚 ≥ 2 → 必须做（存在多个竞争字典块，需合并去重）；
/// - 页脚 == 1 → 仅当意图为 `Log` / `Diff`，或完全没有 VCS 意图时才做；
///   `Status` 等已高度紧凑的输出再优化收益低且有破坏结构的风险；
/// - 页脚 == 0 → 无字典可优化，不做。
pub(crate) fn should_apply_final_paths_optimizer(
    vcs_intent: Option<VcsRunIntent>,
    output_format: OutputFormat,
    formatted: &str,
) -> bool {
    if !matches!(output_format, OutputFormat::Text | OutputFormat::Markdown) {
        return false;
    }

    let footer_count = count_paths_footer_lines(formatted);
    if footer_count >= 2 {
        return true;
    }

    if footer_count == 1 {
        // Single-footer re-optimization is only safe/needed for explicit log/diff style outputs.
        return matches!(vcs_intent, Some(VcsRunIntent::Log | VcsRunIntent::Diff))
            || vcs_intent.is_none();
    }

    false
}

/// run 模式一次压缩所需的上下文快照（由 [`build_run_mode_compression_context`] 构造）。
///
/// 把「命令意图 → 路径字典预设 → VCS AI 档位 → 待压缩输入」打成一个不可变包，
/// 使 [`compress_run_mode_text`] 与流式分块路径 `flush_run_chunk` 复用同一套决策。
pub(crate) struct RunModeCompressionContext {
    vcs_intent: Option<VcsRunIntent>,
    path_options: crate::core::path_optimizer::methods::PathDictionaryOptions,
    enable_vcs_ai_compact: bool,
    profile: crate::plugins::vcs_plugin::methods::VcsAiProfile,
    run_input: String,
}

/// 把 CLI 的 `--preset` 映射为路径字典压缩预设。
///
/// 映射关系：`fast` → `Conservative`、`balanced` → `Balanced`、`ai` → `Aggressive`；
/// 未指定 `--preset` 时取 `Balanced` 作为安全默认（既不激进抽取父前缀，也不完全放弃字典）。
pub(crate) fn resolve_run_path_preset(
    preset: Option<Preset>,
) -> crate::core::path_optimizer::methods::PathDictionaryPreset {
    match preset {
        Some(Preset::Fast) => {
            crate::core::path_optimizer::methods::PathDictionaryPreset::Conservative
        }
        Some(Preset::Balanced) => {
            crate::core::path_optimizer::methods::PathDictionaryPreset::Balanced
        }
        Some(Preset::Ai) => crate::core::path_optimizer::methods::PathDictionaryPreset::Aggressive,
        None => crate::core::path_optimizer::methods::PathDictionaryPreset::Balanced,
    }
}

/// 把 run 模式识别出的 VCS 意图映射为 VCS 插件的 AI 压缩档位。
///
/// `Status`/`Log`/`Diff`/`Other` 一一对应同名档位；`None`（非 VCS 命令）映射为
/// `VcsAiProfile::None`，即不启用任何 VCS 专用的语义紧凑规则。
pub(crate) fn resolve_vcs_ai_profile(
    vcs_intent: Option<VcsRunIntent>,
) -> crate::plugins::vcs_plugin::methods::VcsAiProfile {
    match vcs_intent {
        Some(VcsRunIntent::Status) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Status,
        Some(VcsRunIntent::Log) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Log,
        Some(VcsRunIntent::Diff) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Diff,
        Some(VcsRunIntent::Other) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Other,
        None => crate::plugins::vcs_plugin::methods::VcsAiProfile::None,
    }
}

/// 汇总本次 run 压缩所需的全部决策，产出 [`RunModeCompressionContext`]。
///
/// 聚合四项决策 + 一份待压缩输入：
/// - `vcs_intent`：[`detect_vcs_run_intent`] 判定的 VCS 意图（status/log/diff/other）；
/// - `path_options`：由 `--preset` 映射的路径字典预设（见 [`resolve_run_path_preset`]），
///   再叠加 `--config` 指定的配置文件覆盖项；
/// - `enable_vcs_ai_compact`：[`should_enable_vcs_ai_compact`] 判定是否启用 VCS AI 紧凑模式；
/// - `profile`：[`resolve_vcs_ai_profile`] 把意图映射为 VCS 插件的 AI 档位；
/// - `run_input`：经 [`prepend_run_command_anchor_if_needed`] 保证首行为命令锚点的输入文本
///   （压缩协议法则 0 的硬性要求）。
pub(crate) fn build_run_mode_compression_context(
    args: &CliArgs,
    prog: &str,
    cmd_args: &[String],
    combined: &str,
) -> RunModeCompressionContext {
    let vcs_intent = detect_vcs_run_intent(prog, cmd_args);
    let path_preset = resolve_run_path_preset(args.preset);
    let path_options =
        crate::core::path_optimizer::methods::resolve_path_dictionary_options_from_files(
            path_preset,
            args.config.as_deref(),
        );
    let enable_vcs_ai_compact =
        should_enable_vcs_ai_compact(vcs_intent, args.output_format.clone(), args.preset);
    let profile = resolve_vcs_ai_profile(vcs_intent);
    let run_input = prepend_run_command_anchor_if_needed(combined, prog, cmd_args);

    RunModeCompressionContext {
        vcs_intent,
        path_options,
        enable_vcs_ai_compact,
        profile,
        run_input,
    }
}

/// 在「路径字典选项 + VCS AI 上下文」双层作用域内执行本次压缩。
///
/// 两层 `run_with_*` 包裹是线程局部上下文注入：内层压缩代码无需显式传参即可读到
/// 当前的路径字典选项与 VCS AI 档位，作用域结束自动还原。
///
/// 无论是 VCS 还是非 VCS 命令，统一走常规 [`CompressionPipeline::compress_str`]
/// （正常切片 + 插件分派）。VCS 命令不再走独立单文档旁路，从而复用完整流水线：
/// - 接入共享字典引擎、去重引擎、贝叶斯分发器与全插件链（含 `ansi_cleaner`/`noise_filter`）；
/// - 接入审计与指标采集，使 VCS 压缩在真实流量中的效果可观测、可透视；
/// - 由切片器的 `GitDiffBlock` 语义块识别与 VCS 插件的分片（fragment）探测共同保证
///   跨行语义块的完整性，不再丢失多插件协同的压缩空间。
///
/// 管线错误统一包装为 [`CliError::Pipeline`]。
pub(crate) fn compress_run_mode_text(
    pipeline: &mut CompressionPipeline,
    context: &RunModeCompressionContext,
) -> Result<crate::core::compression::CompressionOutput, CliError> {
    let output_res = crate::core::path_optimizer::methods::run_with_path_dictionary_options(
        context.path_options.clone(),
        || {
            crate::plugins::vcs_plugin::methods::run_with_vcs_ai_context(
                context.enable_vcs_ai_compact,
                context.profile,
                || pipeline.compress_str(&context.run_input),
            )
        },
    );
    output_res.map_err(CliError::Pipeline)
}

/// 拼出人类可读的命令字符串，供追踪事件记录使用。
///
/// 与 [`build_run_command_anchor`] 不同：本函数**不做引号转义**，
/// 仅用空格连接，因此只适合日志/统计展示，不可作为可回读的锚点行。
pub(crate) fn build_run_command_string(prog: &str, cmd_args: &[String]) -> String {
    if cmd_args.is_empty() {
        prog.to_string()
    } else {
        format!("{} {}", prog, cmd_args.join(" "))
    }
}

/// 推导本次运行归属的 filter 名称，用于追踪与统计归类。
///
/// 优先级自高到低：
/// 1. 依当前工作目录识别出的 npm test 变体（`resolve_npm_test_variant`），
///    可区分同一条 `npm test` 背后的不同测试框架；
/// 2~4. [`crate::core::filter_discover::filter_name::derive_tracking_filter_name`]
///    单一命名权威（P2-44）：VCS 意图 → 固定 `"vcs_plugin"`；首个子命令参数
///    （如 `cargo build` → `build`）；兜底用程序名本身。discover 读侧
///    （classifier）引用同一规则，保证两侧命名可互相命中。
pub(crate) fn resolve_run_filter_name(
    prog: &str,
    cmd_args: &[String],
    vcs_intent: Option<VcsRunIntent>,
) -> String {
    let variant_filter = std::env::current_dir().ok().and_then(|cwd| {
        crate::core::filter_variants::resolve_npm_test_variant(&cwd, prog, cmd_args)
    });
    if let Some(v) = variant_filter {
        return v.as_filter_name().to_string();
    }
    crate::core::filter_discover::filter_name::derive_tracking_filter_name(
        prog,
        cmd_args,
        vcs_intent.is_some(),
    )
}

/// 把压缩结果渲染为最终要打印的字符串。
///
/// 三步：
/// 1. [`format_run_mode_tokens`] 按输出格式序列化（JSON 序列化 / token 扁平化）；
/// 2. 仅 `Text` / `Markdown` 追加 `paths:` 字典页脚
///    （[`append_paths_footer_from_output_dictionary`]）——JSON 已含结构化字典，无需页脚；
/// 3. [`apply_run_mode_text_postprocessors`] 做 VCS 去重与路径字典二次优化。
pub(crate) fn render_run_mode_output(
    args: &CliArgs,
    output: &crate::core::compression::CompressionOutput,
    vcs_intent: Option<VcsRunIntent>,
    path_options: &crate::core::path_optimizer::methods::PathDictionaryOptions,
) -> Result<String, CliError> {
    let mut formatted = format_run_mode_tokens(args.output_format.clone(), output)?;

    if matches!(
        args.output_format,
        OutputFormat::Text | OutputFormat::Markdown
    ) {
        formatted = append_paths_footer_from_output_dictionary(&formatted, output, path_options);
    }

    Ok(apply_run_mode_text_postprocessors(
        formatted,
        vcs_intent,
        args.output_format.clone(),
        path_options,
    ))
}

/// 按输出格式把压缩产物序列化为文本。
///
/// - `Json` → 整个 [`CompressionOutput`] 的 pretty JSON（含 token、字典、元数据），
///   序列化失败包装为 [`CliError::Serialization`]；
/// - `Markdown` / `Text` → `flatten_tokens` 扁平化后的纯文本（两种格式当前产物相同）。
pub(crate) fn format_run_mode_tokens(
    output_format: OutputFormat,
    output: &crate::core::compression::CompressionOutput,
) -> Result<String, CliError> {
    match output_format {
        OutputFormat::Json => serde_json::to_string_pretty(output).map_err(CliError::Serialization),
        OutputFormat::Markdown | OutputFormat::Text => Ok(flatten_tokens(&output.tokens)),
    }
}

/// 对已渲染文本做 run 模式专属的后处理。
///
/// - 仅 VCS 命令：先 [`merge_path_dictionary_blocks`] 合并多处路径字典块，
///   再 [`strip_duplicate_vcs_headers`] 删除与压缩后语义标记重复的原始表头；
/// - 满足 [`should_apply_final_paths_optimizer`] 时，再交给
///   `optimize_path_dictionary_blocks_with_options` 做最终一轮路径字典优化。
///
/// 顺序不可交换：字典合并必须先于最终优化，否则优化器会看到多个竞争的字典块。
pub(crate) fn apply_run_mode_text_postprocessors(
    mut formatted: String,
    vcs_intent: Option<VcsRunIntent>,
    output_format: OutputFormat,
    path_options: &crate::core::path_optimizer::methods::PathDictionaryOptions,
) -> String {
    if vcs_intent.is_some() {
        formatted = merge_path_dictionary_blocks(&formatted);
        formatted = strip_duplicate_vcs_headers(&formatted);
    }
    if should_apply_final_paths_optimizer(vcs_intent, output_format, &formatted) {
        formatted = optimize_path_dictionary_blocks_with_options(&formatted, path_options);
    }
    formatted
}

/// `tokenslim run <command>` 的非流式主入口（run 模式的调用链起点）。
///
/// 执行顺序：
/// 1. `parse_run_target` 解析出真实可执行程序与其参数；
/// 2. `--explain-route` 时只打印路由解释并返回，不真正执行命令；
/// 3. `--stream` 时转交 [`run_run_stream_mode`]；
/// 4. 命中 [`detect_git_interactive`] 时放弃压缩，透传 stdio 并直接以子进程退出码退出；
/// 5. 用户显式提供 `--input <file>`（[`InputSource::File`]）时跳过真实执行：
///    把文件内容当作「已捕获的运行输出」，直接进入下面的统一压缩链路。该分支专为
///    静态样本批量回归 run 模式设计，避免 spawn 外部命令即可复现完整链路。
/// 6. 兜底（默认 [stdin])：[`run_external_command_capture`] 捕获 stdout+stderr，
///    再走 5 之后的统一压缩链路。
///
/// 退出码语义：子进程失败时本函数以子进程退出码调用 `std::process::exit`，
/// 保证 `tokenslim run` 对上游脚本与 CI 完全透明（Unix 下信号转换为 `128 + signal`）。
pub(crate) fn run_run_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
    program: &str,
) -> Result<(), CliError> {
    let (prog, cmd_args) = parse_run_target(program, &args.run_command)?;

    if args.explain_route {
        args.emit_text(&explain_run_route(prog, cmd_args, args), None)?;
        return Ok(());
    }

    if args.stream {
        return run_run_stream_mode(args, pipeline, prog, cmd_args);
    }

    // 统一运行输出来源：真实捕获（默认 stdin 输入源）或 `--input <file>` 喂入的静态文本。
    // 返回 `(子进程退出码, 待压缩的原始输出文本)`。喂入路径总视为成功（退出码 0），
    // 其余来源按子进程真实退出码处理，保证对上游脚本/CI 完全透明。
    let (captured_exit_code, combined) = match &args.input {
        // `--input <file>`：把文件内容当作「已捕获的运行输出」，跳过真实 spawn 外部命令，
        // 仍走同一压缩链路。专用于静态样本批量回归 run 模式。
        InputSource::File(path) => {
            let text = std::fs::read_to_string(path).map_err(CliError::Io)?;
            if text.trim().is_empty() {
                return Ok(());
            }
            (0i32, text)
        }
        // 默认：真实捕获外部命令 stdout+stderr。
        InputSource::Stdin => {
            // 启发式检测: git 交互式子命令 (commit 无 -m / rebase -i / tag -a 无 -m /
            // add -p / checkout -p / clean -i) → 放弃压缩, 透传 stdio 给 git 原生命令。
            // 不透传会让 vim/merge-tool 等编辑器读不到 tty 而卡死。
            if detect_git_interactive(prog, cmd_args) {
                eprintln!(
                    "[tokenslim] 检测到交互式 git 命令 (`{} {}`), fallback 到 git 原生命令 (该命令输出含用户决策输入, 压缩无意义)。",
                    prog,
                    cmd_args.join(" ")
                );
                eprintln!("[tokenslim] 提示: 如需查看压缩后的输出, 请改用 `git -c color.ui=always ... | tokenslim compress` 形式。");
                let status = run_external_command_passthrough(prog, cmd_args)?;
                std::process::exit(status.code().unwrap_or(1));
            }

            let (status, combined) = run_external_command_capture(
                prog,
                cmd_args,
                args.passthrough,
                args.tee.as_deref(),
            )?;

            let exit_code: i32 = {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    status
                        .code()
                        .or_else(|| status.signal().map(|s| 128 + s))
                        .unwrap_or(1)
                }
                #[cfg(not(unix))]
                {
                    status.code().unwrap_or(1)
                }
            };

            if combined.trim().is_empty() {
                if !status.success() {
                    eprintln!("{}", t1("run_command_failed_exit", status));
                    std::process::exit(exit_code);
                }
                return Ok(());
            }
            (exit_code, combined)
        }
    };

    // 至此输入来源无关：统一走 run 模式的「上一级压缩函数」（完整流水线 + 路径字典 + VCS AI 上下文）。
    let context = build_run_mode_compression_context(args, prog, cmd_args, &combined);
    // P2-47：压缩前起表，真实压缩耗时经 with_filter_time 写入 tracking。
    let start = std::time::Instant::now();
    let output = compress_run_mode_text(pipeline, &context)?;
    let filter_time_ms = start.elapsed().as_millis() as i64;
    // P2-89 负收益守门：产物不小于原文时回退原文透传，绝不产出比原文更大的压缩结果。
    let (output, guarded) = guard_negative_savings(output, &combined);
    if guarded {
        eprintln!("{}", t("cli_warn_no_compress_gain"));
    }

    let cmd_str = build_run_command_string(prog, cmd_args);
    let filter_name = resolve_run_filter_name(prog, cmd_args, context.vcs_intent);
    record_tracking_event(
        &cmd_str,
        Some(filter_name.as_str()),
        &output,
        captured_exit_code,
        filter_time_ms,
    );
    let formatted =
        render_run_mode_output(args, &output, context.vcs_intent, &context.path_options)?;

    let (original_size, compressed_size) = tracking_bytes(&output);
    let stats = json!({
        "original_size": original_size,
        "compressed_size": compressed_size,
    });
    args.emit_text(&formatted, Some(stats))?;

    if captured_exit_code != 0 {
        std::process::exit(captured_exit_code);
    }
    Ok(())
}

/// `tokenslim run --stream` 的流式主入口：边收边压边输出，不等命令跑完。
///
/// 结构：
/// - 流式 spawn 在 Windows 下经 `cmd /C <prog>` 启动（P2-82，与非流式一致，
///   保证 `.cmd`/`.bat` 包装器命令可用），其他平台直接 spawn `prog`；
///   stdout/stderr 各起一条读取线程，经同一 `mpsc` 通道汇总；两路都支持
///   `--passthrough` 回显与 `--tee` 落盘。
/// - 主循环用 `recv_timeout(--flush-interval)` 驱动：累积字节后按最后一个 `\n`
///   切出完整行（残缺行留在 `pending_bytes` 等下一批），
///   累计到 64 KiB 或收到超时信号即调用 `flush_run_chunk` 压缩并输出一块。
/// - 通道断开时把残留字节并入最后一块 flush 后退出循环。
/// - `is_first_chunk` 只对首块保留命令锚点，避免每块都重复插入锚点行。
/// - `--merge` 时各块产物先收集，最后 `merge_compression_outputs` 合并成单份输出统一渲染。
///
/// 退出码与非流式一致：子进程失败时以其退出码 `std::process::exit`
/// （Unix 下信号转 `128 + signal`）。
pub(crate) fn run_run_stream_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
    prog: &str,
    cmd_args: &[String],
) -> Result<(), CliError> {
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let mut child = if cfg!(target_os = "windows") {
        // P2-82：流式 spawn 与非流式一致走 `cmd /C`，否则 `.cmd`/`.bat` 包装器
        // 命令（如 `npm`）在 Windows 下无法解析定位。
        let mut c = std::process::Command::new("cmd");
        c.arg("/C");
        c.arg(prog);
        c.args(cmd_args);
        c
    } else {
        let mut c = std::process::Command::new(prog);
        c.args(cmd_args);
        c
    };
    let mut child = child
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(CliError::Io)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CliError::Compression("Failed to open child stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CliError::Compression("Failed to open child stderr".to_string()))?;

    use std::io::Write;
    use std::sync::{Arc, Mutex};
    let tee_writer = if let Some(ref path) = args.tee {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = std::fs::File::create(path).map_err(CliError::Io)?;
        Some(Arc::new(Mutex::new(file)))
    } else {
        None
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    let tx_out = tx.clone();
    let passthrough = args.passthrough;
    let tee_writer_clone = tee_writer.clone();
    thread::spawn(move || {
        let mut reader = stdout;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    if passthrough {
                        let mut stderr = std::io::stderr();
                        let _ = stderr.write_all(chunk);
                        let _ = stderr.flush();
                    }
                    if let Some(ref file_arc) = tee_writer_clone {
                        if let Ok(mut file) = file_arc.lock() {
                            let _ = file.write_all(chunk);
                            let _ = file.flush();
                        }
                    }
                    if tx_out.send(chunk.to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let tx_err = tx;
    let tee_writer_clone = tee_writer.clone();
    thread::spawn(move || {
        let mut reader = stderr;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    if passthrough {
                        let mut stderr = std::io::stderr();
                        let _ = stderr.write_all(chunk);
                        let _ = stderr.flush();
                    }
                    if let Some(ref file_arc) = tee_writer_clone {
                        if let Ok(mut file) = file_arc.lock() {
                            let _ = file.write_all(chunk);
                            let _ = file.flush();
                        }
                    }
                    if tx_err.send(chunk.to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let vcs_intent = detect_vcs_run_intent(prog, cmd_args);
    let path_preset = resolve_run_path_preset(args.preset);
    let path_options =
        crate::core::path_optimizer::methods::resolve_path_dictionary_options_from_files(
            path_preset,
            args.config.as_deref(),
        );

    let flush_interval = Duration::from_millis(args.flush_interval);
    let mut pending_bytes = Vec::new();
    let mut chunk_text = String::new();
    let mut chunk_outputs = Vec::new();
    let mut is_first_chunk = true;
    // P2-47：流式合并且未输出时累计各块压缩耗时，供收尾统一写入 tracking。
    let mut total_filter_time: i64 = 0;

    loop {
        let msg = rx.recv_timeout(flush_interval);
        match msg {
            Ok(bytes) => {
                pending_bytes.extend(bytes);
                if let Some(last_nl) = pending_bytes.iter().rposition(|&b| b == b'\n') {
                    let complete_part = &pending_bytes[..=last_nl];
                    let complete_str = String::from_utf8_lossy(complete_part);
                    chunk_text.push_str(&complete_str);
                    pending_bytes = pending_bytes[last_nl + 1..].to_vec();
                }

                if chunk_text.len() >= 64 * 1024 {
                    flush_run_chunk(
                        &chunk_text,
                        pipeline,
                        args,
                        prog,
                        cmd_args,
                        vcs_intent,
                        &path_options,
                        &mut chunk_outputs,
                        &mut total_filter_time,
                        is_first_chunk,
                    )?;
                    is_first_chunk = false;
                    chunk_text.clear();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !chunk_text.is_empty() {
                    flush_run_chunk(
                        &chunk_text,
                        pipeline,
                        args,
                        prog,
                        cmd_args,
                        vcs_intent,
                        &path_options,
                        &mut chunk_outputs,
                        &mut total_filter_time,
                        is_first_chunk,
                    )?;
                    is_first_chunk = false;
                    chunk_text.clear();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !pending_bytes.is_empty() {
                    let remaining_str = String::from_utf8_lossy(&pending_bytes);
                    chunk_text.push_str(&remaining_str);
                    pending_bytes.clear();
                }
                if !chunk_text.is_empty() {
                    flush_run_chunk(
                        &chunk_text,
                        pipeline,
                        args,
                        prog,
                        cmd_args,
                        vcs_intent,
                        &path_options,
                        &mut chunk_outputs,
                        &mut total_filter_time,
                        is_first_chunk,
                    )?;
                }
                break;
            }
        }
    }

    let status = child.wait().map_err(CliError::Io)?;

    if args.merge {
        if let Some(merged) = merge_compression_outputs(chunk_outputs) {
            let cmd_str = build_run_command_string(prog, cmd_args);
            let filter_name = resolve_run_filter_name(prog, cmd_args, vcs_intent);
            let exit_code = status.code().unwrap_or(1);
            // P2-47：merge 场景用累计的各块压缩耗时写入 tracking。
            record_tracking_event(
                &cmd_str,
                Some(filter_name.as_str()),
                &merged,
                exit_code,
                total_filter_time,
            );

            let formatted = render_run_mode_output(args, &merged, vcs_intent, &path_options)?;
            let (original_size, compressed_size) = tracking_bytes(&merged);
            let stats = json!({
                "original_size": original_size,
                "compressed_size": compressed_size,
            });
            args.emit_text(&formatted, Some(stats))?;
        }
    }

    if !status.success() {
        #[cfg(unix)]
        let exit_code = if let Some(code) = status.code() {
            code
        } else {
            use std::os::unix::process::ExitStatusExt;
            status.signal().map(|s| 128 + s).unwrap_or(1)
        };

        #[cfg(not(unix))]
        let exit_code = status.code().unwrap_or(1);

        std::process::exit(exit_code);
    }

    Ok(())
}

/// 压缩并落地流式模式的一个文本块。
///
/// 先复用 [`build_run_mode_compression_context`] 构造上下文；若不是首块，
/// 则把 `run_input` 覆盖为原始 `text`——**只有首块保留命令锚点**，
/// 后续块重复插锚点会污染输出。
///
/// 随后 [`compress_run_mode_text`] 压缩，并按 `--merge` 分流：
/// - `--merge` → 仅把产物推入 `chunk_outputs`，留给调用方合并后统一输出；
/// - 否则 → 立即记录追踪事件并打印本块结果；JSON 格式下按 `--json`
///   决定是否包裹 `status`/`data`/`stats` 外层信封，其他格式直接打印渲染文本。
fn flush_run_chunk(
    text: &str,
    pipeline: &mut CompressionPipeline,
    args: &CliArgs,
    prog: &str,
    cmd_args: &[String],
    vcs_intent: Option<VcsRunIntent>,
    path_options: &crate::core::path_optimizer::methods::PathDictionaryOptions,
    chunk_outputs: &mut Vec<CompressionOutput>,
    total_filter_time: &mut i64,
    is_first_chunk: bool,
) -> Result<(), CliError> {
    let mut context = build_run_mode_compression_context(args, prog, cmd_args, text);
    if !is_first_chunk {
        context.run_input = text.to_string();
    }

    // P2-47：压缩前起表，记录本块真实压缩耗时；merge 场景下累加进
    // total_filter_time 供收尾统一写入 tracking。
    let start = std::time::Instant::now();
    let output = compress_run_mode_text(pipeline, &context)?;
    let filter_time_ms = start.elapsed().as_millis() as i64;
    *total_filter_time += filter_time_ms;
    // P2-89 负收益守门：按块保证产物不大于块原文（逐块成立 ⇒ 合并后亦成立）。
    let (output, guarded) = guard_negative_savings(output, text);
    if guarded {
        eprintln!("{}", t("cli_warn_no_compress_gain"));
    }

    if args.merge {
        chunk_outputs.push(output);
    } else {
        let cmd_str = build_run_command_string(prog, cmd_args);
        let filter_name = resolve_run_filter_name(prog, cmd_args, vcs_intent);
        record_tracking_event(
            &cmd_str,
            Some(filter_name.as_str()),
            &output,
            0,
            filter_time_ms,
        );

        let formatted = render_run_mode_output(args, &output, vcs_intent, path_options)?;
        let (original_size, compressed_size) = tracking_bytes(&output);
        let stats = json!({
            "original_size": original_size,
            "compressed_size": compressed_size,
        });

        match args.output_format {
            OutputFormat::Json => {
                if args.json {
                    let mut obj = serde_json::Map::new();
                    obj.insert("status".to_string(), "success".into());
                    obj.insert("data".to_string(), serde_json::to_value(&output)?);
                    obj.insert("stats".to_string(), stats);
                    println!("{}", serde_json::to_string(&obj)?);
                } else {
                    println!("{}", serde_json::to_string(&output)?);
                }
            }
            _ => {
                println!("{}", formatted);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;

    // v0.3.7 的 is_git_program / detect_git_interactive heuristic 已在 v0.4.0
    // 删除 (改用 crate::cli::whitelist 双清单 + ConPTY 转发). 相关 12 个
    // unit test 一并删除, 新的双清单 / ConPTY / 3 路分发 unit test 放在
    // crate::cli::whitelist / crate::cli::conpty_probe / crate::cli::pty_runner
    // 各自模块的 #[cfg(test)] mod tests 段.

    /// 回归：编译器/交叉编译/binutils 变体名（版本后缀、交叉前缀、ld.gold/lld 家族）
    /// 必须路由到 `build` 组。覆盖 `command_regex` 兜底（`command_keywords` 只精确命中
    /// 基础名，变体名靠 regex）。
    #[test]
    fn run_route_regex_covers_compiler_script_variants() {
        // 交叉编译前缀 + 版本后缀 + binutils 点分家族
        for prog in [
            "gcc-12",
            "g++-13",
            "clang-14",
            "clang++-15",
            "cc",
            "c++",
            "x86_64-linux-gnu-gcc",
            "arm-none-eabi-g++",
            "aarch64-linux-gnu-ld.gold",
            "riscv64-unknown-elf-gcc",
            "ld.lld",
            "ld.gold",
        ] {
            assert_eq!(
                detect_run_plugin_route(prog, &[]),
                RunPluginRoute::Build,
                "变体名 `{prog}` 应路由到 build 组（command_regex 兜底）"
            );
        }
    }

    /// 反例保护：非编译工具、编译器无关单词不得被 build 组误判。
    #[test]
    fn run_route_regex_rejects_non_build_programs() {
        // 易误配的短词/相似名
        for prog in [
            "git",
            "npm",
            "python",
            "sed",
            "grep",
            "mygccx",
            "cargo-aurora",
        ] {
            assert_ne!(
                detect_run_plugin_route(prog, &[]),
                RunPluginRoute::Build,
                "`{prog}` 不应被误路由到 build 组（保持该命令原语义分组）"
            );
        }
    }

    /// 回归：binutils 分析工具（nm/size/objdump/readelf 等）产出符号表/节大小清单，
    /// gcc_log 已具备感知压缩能力（`$NM/$SIZE/$SECTION`），因此必须路由到 `build` 组
    /// 使 gcc_log 参与，而非被收窄到 generic。交叉前缀变体由 `command_regex` 兜底。
    #[test]
    fn run_route_regex_routes_binutils_tools_to_build_group() {
        // 纯分析/查看工具：应路由到 build 组（gcc_log 感知压缩）
        for prog in [
            "nm",
            "size",
            "objdump",
            "readelf",
            "strip",
            "objcopy",
            "addr2line",
            "gprof",
            // 带交叉前缀的分析工具同样路由到 build 组
            "x86_64-linux-gnu-nm",
            "aarch64-linux-gnu-objdump",
        ] {
            assert_eq!(
                detect_run_plugin_route(prog, &[]),
                RunPluginRoute::Build,
                "binutils 分析工具 `{prog}` 应路由到 build 组（gcc_log 感知压缩）"
            );
        }
    }

    /// 回归：gcc/g++/clang 等 C/C++ 编译命令必须路由到 `build` 组，
    /// 从而保留 gcc_log 等专用插件参与压缩。
    ///
    /// 背景：`config/plugins/build_plugin.route.json` 的 `command_keywords` 一度缺失
    /// `gcc/g++/clang/cc/c++`，导致真实执行 `gcc -c` 时落入 `Generic` 组，
    /// 被 [`keep_generic_run_plugins`] 裁剪掉 gcc_log，gcc_log 对任何样本都不参与压缩
    /// （分类器链路巡检暴露 15/15 gcc 样本全落 generic_text）。本测试固化修复：
    /// 这些命令必须以 `build` 组返回，且 `plugins_for_run_command` 保留 gcc_log。
    #[test]
    fn run_route_routes_compiler_commands_to_build_group() {
        for prog in ["gcc", "g++", "clang", "clang++", "cc", "c++"] {
            let route = detect_run_plugin_route(prog, &[]);
            assert_eq!(
                route,
                RunPluginRoute::Build,
                "编译器命令 `{prog}` 应路由到 build 组（否则 gcc_log 被 Generic 裁剪）"
            );
        }
    }

    /// 回归：build 组路由必须保留 gcc_log 专用插件，供 C/C++ 编译输出压缩。
    #[test]
    fn run_route_build_group_keeps_gcc_log_plugin() {
        let plugins =
            plugins_for_run_command("gcc", &["-c".to_string(), "main.c".to_string()], None);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(
            names.contains(&"gcc_log"),
            "build 组应保留 gcc_log 插件，实际插件链: {names:?}"
        );
        assert!(
            !names.contains(&"git_diff") && !names.contains(&"vcs"),
            "build 组应剔除 VCS 插件，实际: {names:?}"
        );
    }

    /// 反例保护：非编译命令（如 git）不应因本次改动误入 build 组。
    #[test]
    fn run_route_vcs_command_not_reclassified_as_build() {
        let route = detect_run_plugin_route("git", &["status".to_string()]);
        assert_ne!(route, RunPluginRoute::Build, "git 不应被误判为 build 组");
    }
}
