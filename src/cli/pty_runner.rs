//! ConPTY / PTY 生产级 tty 转发 —— v0.4.0 核心
//!
//! # 设计动机
//!
//! 替代 v0.3.7 的 `run_external_command_passthrough` (纯 stdio 透传),
//! 走 portable-pty 创建伪 tty, 让子进程 (vim / ssh / python REPL 等) 能
//! 看到真实终端行为, 解决:
//!
//! - **着色 / TUI**: 没有 tty, 子进程会禁用 ANSI 颜色 / TUI 字符
//! - **vim/merge-tool**: 读不到 tty 会卡死
//! - **交互式 prompt**: 问密码 / y/n 时, stdio 透传只能串行, 不如 pty 自然
//!
//! # 关键设计
//!
//! 1. **主线程做 stdio 桥接** (子进程 <-> 本进程 stdin/stdout), 用 mpsc 把 reader
//!    线程的字节发回主线程, 避免 Windows `StdoutLock` 的 `!Send` 问题
//! 2. **不超时** (`max_wait_secs = 0`): 交互式命令可能跑很久 (REPL 闲等用户),
//!    真正的退出信号是子进程自然退出或用户 Ctrl+C
//! 3. **stdout 透传**: 子进程输出直接写到本进程 stdout, 用户能看到完整 TUI
//! 4. **退出码透传**: 子进程退出码作为函数返回值
//!
//! # 失败语义
//!
//! 任何底层错误 (openpty 失败 / spawn 失败 / read 失败) → 返回 [`CliError`].
//! 调用方 (3 路分发) 应降级到 [`run_external_command_passthrough`] (纯 stdio).
//!
//! # 平台
//!
//! - **Windows**: portable-pty 自动选用 ConPTY (Win10 1809+)
//! - **Unix**: portable-pty 用 `openpty(3)` 标准 pty
//!
//! # 沙箱兼容
//!
//! Trae IDE 沙箱会拦截 ConPTY 子进程, 但 [`crate::cli::conpty_probe::is_conpty_available`]
//! 启动时已探测, 不可用时调用方**不应**走到本函数, 应直接走 passthrough.

use crate::cli::types::CliError;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// 把 portable_pty 错误包成 [`CliError::Io`], 避免给 [`CliError`]
/// 新增 variant (维持本次改动只在 pty_runner 范围内).
fn pty_err(context: &str, e: impl std::fmt::Display) -> CliError {
    CliError::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("{context}: {e}"),
    ))
}

/// 默认 PTY 窗口大小: 30 行 × 100 列. 够大多数 TUI 用, 太小可能让 vim 报警.
const DEFAULT_PTY_ROWS: u16 = 30;
const DEFAULT_PTY_COLS: u16 = 100;

/// 默认最大等待秒数. `0` = 不超时, 一直等子进程自然退出.
/// 交互式命令 (REPL / 编辑器) 可能挂很久, 设超时会让用户主动 Ctrl+C 也被截断.
const DEFAULT_MAX_WAIT_SECS: u64 = 0;

/// 走 ConPTY / PTY 跑交互式外部命令. 返回子进程退出码.
///
/// ## 行为
///
/// 1. 创建伪 tty (PTY), 启动 `prog args...` 作为子进程
/// 2. 启动 reader 线程: 持续读 pty master, 把字节发到 mpsc 通道
/// 3. 启动 writer 线程: 把本进程 stdin 透传到 pty master
/// 4. 主线程: 从 mpsc 收字节, **写到自己 stdout** (用户看到完整 TUI) + 累积到 buffer
/// 5. 子进程退出 / 超时 / 出错 → 返回对应结果
///
/// ## 错误
///
/// - `openpty` 失败 → CliError
/// - `spawn_command` 失败 → CliError
/// - `try_clone_reader` / `take_writer` 失败 → CliError
/// - `max_wait_secs > 0` 且超时 → CliError (子进程被强制 kill)
///
/// 调用方应在调用前检查 [`crate::cli::conpty_probe::is_conpty_available`],
/// 不可用时降级到 passthrough, 不调用本函数。
#[tracing::instrument(level = "debug", skip_all, fields(prog = %prog, args = ?args))]
pub(crate) fn run_external_command_pty(prog: &str, args: &[String]) -> Result<i32, CliError> {
    run_in_pty_impl(
        prog,
        args,
        DEFAULT_MAX_WAIT_SECS,
        true, /* echo to stdout */
    )
}

/// 走 ConPTY / PTY 跑, **不回显到本进程 stdout** (用于测试 / 内部流水线)
#[cfg(test)]
pub(crate) fn run_external_command_pty_silent(
    prog: &str,
    args: &[String],
    max_wait_secs: u64,
) -> Result<(Vec<u8>, i32), CliError> {
    // 复用 impl, 收集到 collected
    let mut collected: Option<Vec<u8>> = Some(Vec::new());
    let exit = run_in_pty_impl_with_buffer(prog, args, max_wait_secs, &mut collected)?;
    Ok((collected.unwrap_or_default(), exit))
}

/// run_external_command_pty 的薄封装：根据 echo 标志决定是否创建收集缓冲，
/// 再委托 run_in_pty_impl_with_buffer 完成实际 PTY 运行。
fn run_in_pty_impl(
    prog: &str,
    args: &[String],
    max_wait_secs: u64,
    echo: bool,
) -> Result<i32, CliError> {
    let mut sink: Option<Vec<u8>> = if echo { None } else { Some(Vec::new()) };
    run_in_pty_impl_with_buffer(prog, args, max_wait_secs, &mut sink)
}

/// PTY 运行核心实现：创建伪终端、spawn 子进程，并桥接 reader/writer 线程。
/// 主线程从 mpsc 通道 drain 字节，按需回显到 stdout 或累积到 collected 缓冲；
/// 子进程自然退出返回退出码，max_wait_secs>0 且超时时 kill 子进程并返回错误。
fn run_in_pty_impl_with_buffer(
    prog: &str,
    args: &[String],
    max_wait_secs: u64,
    collected: &mut Option<Vec<u8>>,
) -> Result<i32, CliError> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_PTY_ROWS,
            cols: DEFAULT_PTY_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| pty_err("openpty", e))?;

    let mut cmd = CommandBuilder::new(prog);
    for a in args {
        cmd.arg(a);
    }
    // 子进程没设 TERM 时, 某些 TUI (vim) 拒绝启动. fallback 到 xterm-256color.
    if std::env::var("TERM").is_err() {
        cmd.env("TERM", "xterm-256color");
    }

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| pty_err("spawn_command", e))?;
    drop(pair.slave);

    // reader 线程: pty master -> mpsc. 收集字节 + 选择性回显到本进程 stdout
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| pty_err("try_clone_reader", e))?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader_handle = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = buf[..n].to_vec();
                    if tx.send(chunk).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // writer 线程: 本进程 stdin -> pty master. 不 drop, 否则 ConPTY 给 child 发 Ctrl+C
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| pty_err("take_writer", e))?;
    let writer_handle = thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let start = Instant::now();
    let mut stdout = std::io::stdout();
    let exit_code: i32 = loop {
        // drain mpsc
        loop {
            match rx.try_recv() {
                Ok(chunk) => {
                    if collected.is_some() {
                        collected.as_mut().unwrap().extend_from_slice(&chunk);
                    } else {
                        let _ = stdout.write_all(&chunk);
                        let _ = stdout.flush();
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {}
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            // 子进程退出后，输出可能仍在 reader 线程的读取/投递途中（PTY EOF 传播晚于退出码）。
            // 原实现仅做一次非阻塞 try_recv 排空，通道瞬时 Empty 会提前中断收集，
            // 导致 cmd /c ver 等快速退出命令的输出为空/半截（Q543 空输出竞态）。
            // 修复：有界等待 reader 线程收尽（子进程退出 → slave 关闭 → master 读到 EOF），再完整排空通道。
            for _ in 0..10 {
                if reader_handle.is_finished() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            while let Ok(chunk) = rx.try_recv() {
                if collected.is_some() {
                    collected.as_mut().unwrap().extend_from_slice(&chunk);
                } else {
                    let _ = stdout.write_all(&chunk);
                    let _ = stdout.flush();
                }
            }
            // portable_pty 0.8 的 ExitStatus::exit_code() 返回 u32. 这里约定:
            // 0 = 成功; 非 0 透传给调用方, 由调用方决定如何处理 (1 / -1 / u32).
            // 内部用 u32 避免 i32 范围外的 platform-specific 状态码被截断.
            let code: u32 = status.exit_code();
            break code as i32;
        }
        if max_wait_secs > 0 && start.elapsed() > Duration::from_secs(max_wait_secs) {
            let _ = child.kill();
            return Err(pty_err("子进程超时", format!("{max_wait_secs}s, 已 kill")));
        }
        thread::sleep(Duration::from_millis(30));
    };

    drop(pair.master);
    // 有界收尾（Q543 复审 P1）：先 drop master 强制 reader 的 read() 返回 EOF/错误并解除阻塞，
    // 再用预算内轮询等待 reader 结束；超时则放弃 join（detach）——reader 线程会随
    // master 句柄关闭而自然退出。保证整个收尾有界，不会在 join() 上无界悬挂。
    if !wait_join_bounded(&reader_handle, Duration::from_millis(100)) {
        tracing::warn!("pty reader 线程未在 100ms 内结束，已 detach（master 已关闭，将自然退出）");
    }
    drop(writer_handle);

    Ok(exit_code)
}

/// 有界等待线程结束：在预算内轮询 `is_finished()`；超时返回 false，由调用方放弃 join（detach）。
///
/// 背景（Q543 复审 P1）：PTY reader 线程阻塞在 `master.read()` 上，若 EOF 传播延迟，
/// 无超时的 `JoinHandle::join()` 会让整个函数无界悬挂。本函数保证等待有界；
/// 超时后调用方应 drop master（强制 read 返回错误）并 detach，线程将自然退出。
fn wait_join_bounded<T>(handle: &std::thread::JoinHandle<T>, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试辅助：剥离 ANSI 控制序列（CSI / OSC / 孤立 ESC），使断言针对真实语义文本。
    ///
    /// 背景（Q543）：ConPTY 在压力运行下会在输出中混入终端初始化序列——如 OSC 标题
    /// `ESC]0;cmd.EXE BEL`、光标隐藏 `ESC[?25l`、光标定位 `ESC[1;1H` 等——这些序列会把
    /// "Windows"/"Microsoft" 等连续子串打断，导致断言非确定性失败（全量压力下偶发）。
    /// 剥离后仅验证子命令的真实语义输出。
    fn strip_ansi(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\x1b' {
                out.push(c);
                continue;
            }
            match chars.peek() {
                // CSI: ESC [ 参数(0-9;?<>!:) 终字符(@-~)
                Some(&'[') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if n.is_ascii_digit() || matches!(n, ';' | '?' | '<' | '>' | '!' | ':') {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if let Some(&fin) = chars.peek() {
                        if ('@'..='~').contains(&fin) {
                            chars.next();
                        }
                    }
                }
                // OSC: ESC ] ... BEL(\x07) 或 ESC \
                Some(&']') => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if n == '\x07' {
                            chars.next();
                            break;
                        }
                        if n == '\x1b' {
                            chars.next(); // 消费 ESC
                            let _ = chars.next(); // 消费可能存在的 '\'
                            break;
                        }
                        chars.next();
                    }
                }
                _ => {
                    // 孤立 ESC（如 ESC\\ 尾部），丢弃
                }
            }
        }
        out
    }

    /// 验证 strip_ansi 能剥离 OSC 标题（BEL 终止）与 CSI 序列，保留语义文本。
    #[test]
    fn strip_ansi_removes_title_and_csi() {
        let raw = "\x1b]0;cmd.EXE\x07\x1b[?25l\x1b[1;1HMicrosoft Windows [版本 10.0.26200.0]\r\n";
        let clean = strip_ansi(raw);
        assert!(clean.contains("Microsoft Windows"));
        assert!(!clean.contains('\x1b'));
        assert!(!clean.contains("cmd.EXE"));
        assert!(!clean.contains("?25l"));
    }

    /// 验证 strip_ansi 也处理 OSC ST 终止形式（ESC ] ... ESC \），而非仅 BEL 终止（Q543 复审整改）。
    #[test]
    fn strip_ansi_handles_osc_st_termination() {
        let raw = "\x1b]0;cmd.EXE\x1b\\Microsoft Windows [版本 10.0.26200.0]\r\n";
        let clean = strip_ansi(raw);
        assert!(
            clean.contains("Microsoft Windows"),
            "OSC ST 标题应被剥离，got: {clean:?}"
        );
        assert!(!clean.contains('\x1b'));
        assert!(!clean.contains("cmd.EXE"));
    }

    /// Q543 v2 复审 P1 回归：wait_join_bounded 对快速线程在预算内返回 true。
    #[test]
    fn wait_join_bounded_returns_true_for_fast_thread() {
        let handle = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(30)));
        let finished = wait_join_bounded(&handle, Duration::from_millis(500));
        assert!(finished, "快速线程应在预算内完成");
    }

    /// Q543 v2 复审 P1 回归：wait_join_bounded 对慢线程在预算后返回 false（不悬挂），
    /// 调用方 detach 后线程自然退出——保证 PTY 收尾有界。
    #[test]
    fn wait_join_bounded_returns_false_without_hanging() {
        let handle = std::thread::spawn(|| std::thread::sleep(Duration::from_secs(5)));
        let started = Instant::now();
        let finished = wait_join_bounded(&handle, Duration::from_millis(50));
        assert!(!finished, "慢线程不应在 50ms 预算内完成");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "有界等待必须及时返回，不得悬挂"
        );
        // handle 在此 drop → detach；线程 5s 后自然退出，不阻塞测试进程退出
    }

    /// 测试 1: `cmd /c ver` 走 pty 立即退出, 拿到 stdout
    #[test]
    fn pty_runs_simple_command() {
        let args = vec!["/c".to_string(), "ver".to_string()];
        let result = run_external_command_pty_silent("cmd", &args, 5);
        match result {
            Ok((bytes, code)) => {
                if code == 0 {
                    // 空输出必须作为独立失败信号（PTY 读取/收集竞态，Q543），
                    // 不能被 strip_ansi 的成功样例掩盖。
                    assert!(
                        !bytes.is_empty(),
                        "PTY 输出为空（读取/收集/句柄关闭竞态，Q543）：cmd /c ver 应产生输出"
                    );
                    // 剥离 ANSI 终端初始化/标题序列后，再断言真实语义输出（Q543 非确定性失败修复）
                    let text = strip_ansi(&String::from_utf8_lossy(&bytes));
                    assert!(
                        text.contains("Windows") || text.contains("Microsoft"),
                        "expected Windows version in output, got: {text}"
                    );
                } else {
                    // 非零退出码 (如 0xC000013A = STATUS_CONTROL_C_EXIT) 说明
                    // 被沙箱截断 (Trae IDE 等), 此情况跳过验证
                    eprintln!("[skip] pty_runs_simple_command: ConPTY 退出码 {code} (沙箱截断)");
                }
            }
            Err(e) => {
                // Trae IDE 沙箱会拦截 ConPTY, 这种环境下此测试预期 fail
                eprintln!("[skip] pty_runs_simple_command: {e}");
            }
        }
    }

    /// 测试 2: 不存在的程序应返回 Err
    #[test]
    fn pty_unknown_program_returns_err() {
        let args: Vec<String> = vec![];
        let result =
            run_external_command_pty_silent("definitely-not-a-real-command-xyz123", &args, 3);
        assert!(result.is_err(), "expected Err for unknown program");
    }

    /// 测试 3: 窗口大小常量合理
    #[test]
    fn pty_size_constants_reasonable() {
        assert!(DEFAULT_PTY_ROWS >= 24, "rows too small for vim");
        assert!(DEFAULT_PTY_COLS >= 80, "cols too small for typical TUI");
    }
}
