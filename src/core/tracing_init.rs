use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// 解析日志过滤指令。
/// 优先读取 TOKENSLIM_LOG，为空时回退到 RUST_LOG；两者皆空则返回 None。
fn resolve_log_filter() -> Option<String> {
    std::env::var("TOKENSLIM_LOG")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::env::var("RUST_LOG")
                .ok()
                .filter(|v| !v.trim().is_empty())
        })
}

/// 根据环境变量初始化全局 tracing 订阅器。
/// 未配置过滤指令或指令解析失败时静默返回，不抛错。
pub fn init_tracing() {
    let Some(filter_directive) = resolve_log_filter() else {
        return;
    };

    let Ok(env_filter) = EnvFilter::try_new(filter_directive) else {
        return;
    };

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_target(true)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}
