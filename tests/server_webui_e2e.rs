//! TokenSlim server 的 e2e 测试：Web UI 静态文件 + 压缩接口联通
//!
//! 流程：
//! 1. 找一个空闲端口
//! 2. 启动 tokenslim-server 进程
//! 3. GET / 期望返回 200 + text/html
//! 4. GET /assets/style.css 期望 200 + CSS
//! 5. GET /assets/app.js 期望 200 + JS
//! 6. POST /compress 期望 200 + JSON

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

fn wait_for_server(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn webui_index_loads() {
    let port = free_port();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokenslim-server"));
    cmd.env("TOKENSLIM_HOST", "127.0.0.1")
        .env("TOKENSLIM_PORT", port.to_string())
        .env("TOKENSLIM_WEBUI_DIR", "webui")
        // 显式移除任何继承的 API key，确保无鉴权
        .env_remove("TOKENSLIM_API_KEY")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().expect("spawn tokenslim-server");

    let ok = wait_for_server(port, Duration::from_secs(5));
    if !ok {
        let _ = child.kill();
        panic!("server did not start on port {}", port);
    }

    // 1) GET / 应该返回 index.html
    let body = reqwest::blocking::get(format!("http://127.0.0.1:{}/", port))
        .expect("GET /")
        .text()
        .expect("body");
    assert!(
        body.contains("TokenSlim Web UI") || body.contains("ui.subtitle"),
        "expected UI shell, got first 200 chars: {}",
        &body[..body.len().min(200)]
    );

    // 2) GET /assets/style.css 应该 200
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/assets/style.css", port))
        .expect("GET /assets/style.css");
    assert!(resp.status().is_success(), "style.css status: {}", resp.status());
    let css = resp.text().expect("css body");
    assert!(css.contains("--bg"), "style.css missing CSS vars");

    // 3) GET /assets/app.js 应该 200
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/assets/app.js", port))
        .expect("GET /assets/app.js");
    assert!(resp.status().is_success(), "app.js status: {}", resp.status());
    let js = resp.text().expect("js body");
    assert!(js.contains("TokenSlim Web UI") || !js.contains("compress_str"));
    // 至少包含关键函数名
    assert!(js.contains("fetch") || js.contains("FormData"));

    // 4) GET /plugins 应该返回 JSON
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/plugins", port))
        .expect("GET /plugins");
    assert!(resp.status().is_success());
    let json: serde_json::Value = resp.json().expect("parse json");
    assert!(json["plugins"].is_array());
    assert!(json["count"].as_u64().unwrap() > 0);

    // 5) POST /compress 应该工作
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/compress", port))
        .json(&serde_json::json!({
            "text": "2026-05-13T08:13:39.939000+00:00 ecs/backend/x INFO: hello world\n2026-05-13T08:13:40.150000+00:00 ecs/backend/x INFO: hello again",
            "reorder": false,
            "ai_export": false
        }))
        .send()
        .expect("POST /compress");
    assert!(resp.status().is_success(), "compress status: {}", resp.status());

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn webui_disabled_when_dir_missing() {
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tokenslim-server"))
        .env("TOKENSLIM_HOST", "127.0.0.1")
        .env("TOKENSLIM_PORT", port.to_string())
        .env("TOKENSLIM_WEBUI_DIR", "this_dir_definitely_does_not_exist_12345")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tokenslim-server");

    let ok = wait_for_server(port, Duration::from_secs(5));
    if !ok {
        let _ = child.kill();
        panic!("server did not start");
    }

    // GET / 应该回 404（fallback service 没启用）
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/", port)).expect("GET /");
    assert_eq!(resp.status().as_u16(), 404, "expected 404 when webui dir missing");

    // 但 /health 仍然 200
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/health", port)).expect("GET /health");
    assert!(resp.status().is_success());

    let _ = child.kill();
    let _ = child.wait();
}
