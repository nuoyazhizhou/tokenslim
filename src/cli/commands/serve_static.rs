//! 静态文件服务命令实现
//! 所有 Rust 注释必须使用中文

use crate::cli::types::{CliArgs, CliError};
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::services::ServeDir;
use axum::Router;

/// 跨平台打开默认浏览器，确保零依赖
fn open_browser(url: &str) {
    let status = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .status()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg(url)
            .status()
    } else {
        std::process::Command::new("xdg-open")
            .arg(url)
            .status()
    };
    
    if let Err(e) = status {
        log::warn!("自动打开浏览器失败: {}", e);
    }
}

/// 启动静态文件托管服务
#[tracing::instrument(level = "debug", skip_all)]
pub fn handle_serve_static_command(args: &CliArgs) -> Result<(), CliError> {
    // 1. 确定服务的静态目录，默认为当前目录 "."
    let serve_dir = args
        .serve_static
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));

    // 尝试获取绝对路径
    let abs_dir = std::fs::canonicalize(&serve_dir)
        .unwrap_or_else(|_| serve_dir.clone());

    if !abs_dir.exists() {
        return Err(CliError::InvalidArgs(format!(
            "指定的托管目录不存在: {}",
            serve_dir.display()
        )));
    }

    if !abs_dir.is_dir() {
        return Err(CliError::InvalidArgs(format!(
            "指定的托管路径不是一个有效的目录: {}",
            serve_dir.display()
        )));
    }

    // 2. 解析绑定的 IP 和端口
    let bind_ip = args
        .serve_bind
        .clone()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = args.serve_port.unwrap_or(8080);

    let socket_str = format!("{}:{}", bind_ip, port);
    let addr: SocketAddr = socket_str.parse().map_err(|e| {
        CliError::InvalidArgs(format!(
            "无效的绑定地址 '{socket_str}': {e}"
        ))
    })?;

    // 3. 构建静态文件服务路由，将 fallback 指向 ServeDir 以托管全部请求
    let app = Router::new().fallback_service(ServeDir::new(&abs_dir));

    // 4. 创建 Tokio 异步运行时来启动 axum 服务
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(CliError::Io)?;

    rt.block_on(async {
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                return Err(CliError::Io(e));
            }
        };

        // 构造提示的 URL
        let actual_addr = listener.local_addr().unwrap_or(addr);
        let port = actual_addr.port();
        
        let local_url = if actual_addr.ip().is_unspecified() {
            format!("http://127.0.0.1:{}/", port)
        } else {
            format!("http://{}:{}/", actual_addr.ip(), port)
        };

        // 输出终端引导彩色日志 (ANSI 颜色支持)
        println!("\x1b[1;36m⚡ TokenSlim 静态文件服务器已启动\x1b[0m");
        println!("\x1b[33m------------------------------------------------\x1b[0m");
        println!("📂 \x1b[1m托管目录:\x1b[0m {}", abs_dir.display());
        println!("🌐 \x1b[1m本地访问:\x1b[0m \x1b[4;32m{}\x1b[0m", local_url);
        
        if actual_addr.ip().is_unspecified() {
            // 在 Linux 上尝试获取局域网 IP 以方便外部设备调试
            if let Ok(hostname_out) = std::process::Command::new("hostname").arg("-I").output() {
                let ip_str = String::from_utf8_lossy(&hostname_out.stdout);
                let ips: Vec<&str> = ip_str.split_whitespace().collect();
                for ip in ips {
                    println!("🌐 \x1b[1m局域网访问:\x1b[0m \x1b[4;32mhttp://{}:{}/\x1b[0m", ip, port);
                }
            }
        }
        
        println!("\x1b[33m------------------------------------------------\x1b[0m");
        println!("💡 按 \x1b[1;31mCtrl+C\x1b[0m 平滑关闭服务...");

        // 如果设置了 --open，则自动在浏览器打开
        if args.serve_open {
            println!("🚀 正在自动在默认浏览器中打开: {}", local_url);
            open_browser(&local_url);
        }

        // 定义平滑退出的信号监听
        let shutdown = async {
            tokio::signal::ctrl_c()
                .await
                .expect("无法安装 Ctrl+C 信号监听器");
            println!("\n\x1b[1;33m🛑 正在关闭 TokenSlim 静态文件服务器...\x1b[0m");
        };

        // 启动 axum 服务并挂载优雅退出回调
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| CliError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        println!("\x1b[1;32m✨ 服务已安全关闭。\x1b[0m");
        Ok(())
    })
}
