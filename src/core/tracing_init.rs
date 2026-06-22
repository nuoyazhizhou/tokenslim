use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

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
