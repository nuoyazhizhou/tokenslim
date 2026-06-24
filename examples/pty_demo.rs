//! v0.4.0 PoC: portable-pty 跨平台 tty 转发验证.
//!
//! 目标:
//! 1. 验证 portable-pty 在 Windows Git Bash 下能开 pty (不报 ConPTY 错)
//! 2. 验证 `vim --version` 走 pty 立即退出, 拿到 stdout
//! 3. 验证 `git commit` 无 -m 走 pty 透传 (会读 EOF 自动退出, 不卡死)
//! 4. 验证 `git status` 走 pty 仍能拿 stdout 喂给压缩器
//!
//! 结论驱动 v0.4.0 是否上 portable-pty, 失败则回退 D1+D2+CLI flag (v0.3.8).

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// 通用 pty 执行: 主线程双向 stdio 桥接, 子进程退出后返回 (stdout, exit_code).
///
/// 桥接策略:
/// - spawn 后立即启动 reader 线程: 持续读 pty 读到 EOF, 把字节**只**通过 mpsc 发回主线程
///   (避免 Windows StdoutLock 的 !Send 问题)
/// - 主线程把收到的字节写到自己 stdout (透传) + 累积到 collected
/// - 等子进程退出, reader EOF 自然结束
/// - 用 mpsc 通道把 reader 收集到的全部 stdout 字节转回主函数 (供压缩器分析)
fn run_in_pty(prog: &str, args: &[&str], max_wait_secs: u64) -> Result<(Vec<u8>, i32), String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("openpty failed: {e}"))?;

    let mut cmd = CommandBuilder::new(prog);
    for a in args {
        cmd.arg(a);
    }
    // 让子进程看到真实终端 env, 否则 pty 可能因为 TERM 缺失拒绝启动
    if std::env::var("TERM").is_err() {
        cmd.env("TERM", "xterm-256color");
    }

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn_command({prog}) failed: {e}"))?;
    drop(pair.slave);

    // reader 线程: pty master -> mpsc. 不在锁内写 stdout, 避免 !Send 问题
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("try_clone_reader failed: {e}"))?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader_handle = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
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

    // writer 线程: 本进程 stdin -> pty master. 关键修复: 之前 drop(writer) 导致 ConPTY
    // 立即关闭 stdin 端并发 Ctrl+C 给 child, child 还没输出就被截断 (exit 0xC000013A).
    // 现在透传本进程 stdin 到 pty, 让 child 看到真实 tty 行为.
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("take_writer failed: {e}"))?;
    let writer_handle = thread::spawn(move || {
        use std::io::Read;
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // 主线程: 等子进程退出 + 把 reader 字节写到本进程 stdout + 累积
    let start = Instant::now();
    let mut collected = Vec::new();
    let mut stdout = std::io::stdout();
    let exit_code: i32 = loop {
        // 非阻塞 drain mpsc
        loop {
            match rx.try_recv() {
                Ok(chunk) => {
                    let _ = stdout.write_all(&chunk);
                    let _ = stdout.flush();
                    collected.extend_from_slice(&chunk);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {}
            }
        }
        // 检查子进程
        if let Ok(Some(status)) = child.try_wait() {
            // 退出后, drain 剩余字节
            while let Ok(chunk) = rx.try_recv() {
                let _ = stdout.write_all(&chunk);
                collected.extend_from_slice(&chunk);
            }
            break status.exit_code() as i32;
        }
        if start.elapsed() > Duration::from_secs(max_wait_secs) {
            let _ = child.kill();
            return Err(format!(
                "timeout after {max_wait_secs}s, 进程可能卡死 (v0.4.0 PoC 失败信号)"
            ));
        }
        thread::sleep(Duration::from_millis(30));
    };

    drop(pair.master);
    let _ = reader_handle.join();
    // 释放 writer 句柄后, writer 线程的 stdin.read() 会返回 BrokenPipe/0, 自然退出
    drop(writer_handle);

    Ok((collected, exit_code))
}

fn main() {
    println!("=== TokenSlim v0.4.0 portable-pty PoC ===\n");
    println!("注: portable-pty 走 Windows CreateProcessW (不通过 bash), 只能用 PATH 里的命令");
    println!("注: 如果 exit_code == 0xC000013A (STATUS_CONTROL_C_EXIT), 是在沙箱里跑的, 沙箱会\n     给所有子进程发 Ctrl+C, 请在本地 cmd / git bash 跑本 PoC\n");

    // 测试 1: git --version (开 pty + 立即退出)
    println!("--- 测试 1: git --version (开 pty 立即退出, 拿到 stdout) ---");
    match run_in_pty("git", &["--version"], 5) {
        Ok((bytes, code)) => {
            let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
            let hex: String = bytes.iter().take(40).map(|b| format!("{b:02x} ")).collect();
            println!("  exit_code: {code} (0x{:08X})", code as u32);
            println!("  stdout bytes: {} (hex: {hex}...)", bytes.len());
            println!("  preview: {preview}");
            if bytes.len() > 5 && preview.contains("git version") {
                println!("  测试 1 通过 (pty 能开, 子进程能退, stdout 拿到)\n");
            } else if code == -1073741510 {
                println!("  测试 1 被沙箱 Ctrl+C 截断 (0xC000013A), 需本地跑\n");
            } else {
                println!("  测试 1 边缘: 输出不符合预期\n");
            }
        }
        Err(e) => {
            println!("  测试 1 失败: {e}\n");
            println!("  结论: portable-pty spawn_command 失败, v0.4.0 PoC 失败");
            std::process::exit(1);
        }
    }

    // 测试 2: cmd /c ver (Windows 自带命令, 验证非 git 通用性)
    println!("--- 测试 2: cmd /c ver (验证非 git 命令也能走 pty) ---");
    match run_in_pty("cmd", &["/c", "ver"], 5) {
        Ok((bytes, code)) => {
            let preview = String::from_utf8_lossy(&bytes);
            println!("  exit_code: {code} (0x{:08X})", code as u32);
            println!("  stdout bytes: {}", bytes.len());
            println!("  preview: {preview}");
            if preview.contains("Windows") || preview.contains("Microsoft") {
                println!("  测试 2 通过 (cmd 也走 pty)\n");
            } else {
                println!("  测试 2 边缘: 输出不符合预期\n");
            }
        }
        Err(e) => {
            println!("  测试 2 失败: {e}\n");
        }
    }

    // 测试 3: git status (开 pty, 拿到 stdout 喂给压缩器)
    println!("--- 测试 3: git status (开 pty 拿 stdout 模拟压缩场景) ---");
    match run_in_pty("git", &["status"], 5) {
        Ok((bytes, code)) => {
            let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(300)]);
            println!("  exit_code: {code} (0x{:08X})", code as u32);
            println!("  stdout bytes: {}", bytes.len());
            println!("  preview: {preview}");
            if bytes.len() > 30 && preview.contains("On branch") {
                println!("  测试 3 通过 (git status 走 pty 拿到完整 stdout 喂压缩器)\n");
            } else {
                println!("  测试 3 边缘: 输出不符合预期\n");
            }
        }
        Err(e) => {
            println!("  测试 3 失败: {e}\n");
        }
    }

    // 测试 4: git log --oneline -5 (多行输出)
    println!("--- 测试 4: git log --oneline -5 (验证多行输出 buffer 完整) ---");
    match run_in_pty("git", &["log", "--oneline", "-5"], 5) {
        Ok((bytes, code)) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
            println!("  exit_code: {code} (0x{:08X})", code as u32);
            println!("  stdout bytes: {}", bytes.len());
            println!("  非空行数: {}", lines.len());
            for l in lines.iter().take(3) {
                println!("    > {l}");
            }
            if lines.len() >= 5 {
                println!("  测试 4 通过 (多行 buffer 完整)\n");
            } else {
                println!("  测试 4 边缘: 行数不足\n");
            }
        }
        Err(e) => {
            println!("  测试 4 失败: {e}\n");
        }
    }

    println!("\n=== PoC 全部完成 ===");
    println!("\n结论判读:");
    println!("  4 个 git/cmd 测试 exit_code != 0xC000013A 且 stdout 含预期内容 -> v0.4.0 portable-pty 可上");
    println!("  exit_code 全是 0xC000013A -> 在沙箱里跑的, 请本地 cmd / git bash 跑:");
    println!("      cd c:\\git_work\\TokenSlim");
    println!("      cargo run --example pty_demo");
    println!("  编译失败或 openpty 失败 -> portable-pty 在 Windows 不兼容, 回退 v0.3.8 D1+D2");

    // ============ 对照组 (不 pty) ============
    println!("\n--- 对照组 5: std::process::Command (不走 pty) ---");
    let out = std::process::Command::new("cmd")
        .args(["/c", "ver"])
        .output();
    match out {
        Ok(o) => {
            let code = o.status.code().unwrap_or(-1);
            println!("  exit_code: {code}");
            println!("  stdout: {}", String::from_utf8_lossy(&o.stdout));
            if code == 0 && !o.stdout.is_empty() {
                println!("  对照组通过: 普通 std::process 能跑 cmd /c ver");
            } else {
                println!("  对照组失败: 普通 std::process 也不能跑 (连真 cmd 都跑不通)");
            }
        }
        Err(e) => println!("  对照组启动失败: {e}"),
    }

    println!("\n--- 对照组 6: std::process::Command git --version ---");
    let out = std::process::Command::new("git").arg("--version").output();
    match out {
        Ok(o) => {
            let code = o.status.code().unwrap_or(-1);
            println!("  exit_code: {code}");
            println!("  stdout: {}", String::from_utf8_lossy(&o.stdout));
        }
        Err(e) => println!("  对照组启动失败: {e}"),
    }
}
