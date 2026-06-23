use axum::{
    extract::{State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{sse::Event as SseEvent, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use notify::{Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;
use tokio_stream::wrappers::ReceiverStream;
use tokenslim::cli::get_plugins;
use tokenslim::core::compression_pipeline::{
    CompressionOutput, CompressionPipeline, PipelineConfig,
};
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim::core::rehydration_pipeline::{RehydrationConfig, RehydrationPipeline};
use tokenslim::core::tracking::{Tracker, TrackingEvent};
use tokenslim::utils::i18n::{t, t1, t_en, t_zh};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use axum::http::Uri;
use axum::http::header;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "webui/"]
struct WebUiAsset;
// 健康检查响应
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_seconds: u64,
}

#[derive(Serialize)]
struct MetricsDetailResponse {
    total_input_size: usize,
    total_output_size: usize,
    compression_ratio: f32,
    slice_count: usize,
    processing_time_ms: u64,
    module_timings_ms: HashMap<String, u64>,
    plugin_stats: Vec<PluginMetricItem>,
    error_count: usize,
    errors: Vec<MetricErrorItem>,
}

#[derive(Serialize)]
struct PluginMetricItem {
    plugin: String,
    detect_calls: usize,
    compress_calls: usize,
    decompress_calls: usize,
    total_detect_ms: u64,
    total_compress_ms: u64,
    total_decompress_ms: u64,
    panic_count: usize,
    timeout_count: usize,
    fallback_count: usize,
}

#[derive(Serialize)]
struct MetricErrorItem {
    timestamp: String,
    module: String,
    plugin: Option<String>,
    error_type: String,
    message: String,
    slice_id: Option<u64>,
}

// 统计聚合响应
#[derive(Serialize)]
struct StatsAggregateResponse {
    total_commands: i64,
    total_input_tokens: i64,
    total_output_tokens: i64,
    tokens_saved: i64,
    savings_pct: f64,
    period_days: i64,
}

// 按日统计请求
#[derive(Deserialize)]
struct StatsDailyRequest {
    #[serde(default = "default_days")]
    days: i64,
}

fn default_days() -> i64 {
    7
}

// 按日统计响应
#[derive(Serialize)]
struct StatsDailyResponse {
    daily_stats: Vec<DailyStat>,
    period_days: i64,
}

#[derive(Serialize)]
struct DailyStat {
    date: String,
    commands: i64,
    input_tokens: i64,
    output_tokens: i64,
    tokens_saved: i64,
    savings_pct: f64,
}

// 按过滤器统计响应
#[derive(Serialize)]
struct StatsByFilterResponse {
    filter_stats: Vec<FilterStat>,
}

#[derive(Serialize)]
struct FilterStat {
    filter_name: String,
    commands: i64,
    input_tokens: i64,
    output_tokens: i64,
    tokens_saved: i64,
    savings_pct: f64,
}

#[derive(Deserialize)]
struct CompressRequest {
    text: String,
    #[serde(default)]
    ai_export: bool,
    #[serde(default)]
    reorder: bool,
}

// We just accept the same structure that /compress returns
#[derive(Deserialize)]
struct DecompressRequest {
    tokens: Vec<tokenslim::core::compression::Token<'static>>,
    dictionary: tokenslim::core::dictionary_engine::Dictionary,
}

// 服务器统计
#[derive(Default)]
struct ServerStats {
    total_requests: u64,
    total_compressions: u64,
    total_decompressions: u64,
    total_bytes_in: u64,
    total_bytes_out: u64,
}

struct AppState {
    pipeline_mutex: Mutex<CompressionPipeline>,
    api_key: Option<String>,
    start_time: SystemTime,
    stats: RwLock<ServerStats>,
    tracker: Option<Arc<Mutex<Tracker>>>,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    code: &'static str,
    message_zh: String,
    message_en: String,
    hint_zh: Option<String>,
    hint_en: Option<String>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    body: ApiErrorBody,
}

impl ApiError {
    fn new(
        status: StatusCode,
        code: &'static str,
        message_zh: impl Into<String>,
        message_en: impl Into<String>,
        hint_zh: Option<String>,
        hint_en: Option<String>,
    ) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code,
                message_zh: message_zh.into(),
                message_en: message_en.into(),
                hint_zh,
                hint_en,
            },
        }
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "E_API_UNAUTHORIZED",
            t_zh("api_err_unauthorized_msg"),
            t_en("api_err_unauthorized_msg"),
            Some(t_zh("api_err_unauthorized_hint").to_string()),
            Some(t_en("api_err_unauthorized_hint").to_string()),
        )
    }

    fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "E_API_INTERNAL",
            t_zh("api_err_internal_msg"),
            t_en("api_err_internal_msg"),
            Some(t_zh("api_err_internal_hint").to_string()),
            Some(t_en("api_err_internal_hint").to_string()),
        )
    }

    fn service_unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "E_API_UNAVAILABLE",
            t_zh("api_err_unavailable_msg"),
            t_en("api_err_unavailable_msg"),
            Some(t_zh("api_err_unavailable_hint").to_string()),
            Some(t_en("api_err_unavailable_hint").to_string()),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

fn check_auth(headers: &HeaderMap, expected_key: &Option<String>) -> Result<(), ApiError> {
    if let Some(expected) = expected_key {
        if let Some(auth_header) = headers.get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                let token = auth_str.trim_start_matches("Bearer ").trim();
                if token == expected {
                    return Ok(());
                }
            }
        }
        log::warn!("{}", t("server_unauthorized_access_attempt"));
        return Err(ApiError::unauthorized());
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    // 初始化日志
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    log::info!("{}", t("server_starting"));

    // 设置压缩管道
    let config = PipelineConfig::default();
    let metrics = MetricsCollector::new(MetricsConfig {
        enabled: true,
        enable_module_timing: true,
        enable_plugin_stats: true,
        enable_error_logging: true,
        max_error_logs: 100,
    });
    let plugins = get_plugins();
    let pipeline = CompressionPipeline::new(config, plugins, metrics);

    // API Key 认证
    let api_key = std::env::var("TOKENSLIM_API_KEY").ok();
    if api_key.is_some() {
        log::info!("{}", t("server_api_key_enabled"));
    } else {
        log::info!("{}", t("server_api_key_disabled"));
    }

    // 初始化 Tracker（用于统计聚合）
    let tracker = match Tracker::open_default() {
        Ok(tracker_instance) => {
            log::info!("{}", t("server_tracker_init_ok"));
            Some(Arc::new(Mutex::new(tracker_instance)))
        }
        Err(e) => {
            log::warn!("{}", t1("server_tracker_init_failed", format!("{e:?}")));
            log::warn!("{}", t("server_stats_disabled"));
            None
        }
    };

    // 配置文件路径（用于热加载）
    let config_path = std::env::var("TOKENSLIM_CONFIG_PATH")
        .ok()
        .map(PathBuf::from);

    if let Some(ref path) = config_path {
        log::info!("{}", t1("server_hot_reload_enabled", format!("{path:?}")));
    } else {
        log::info!("{}", t("server_hot_reload_disabled"));
    }

    let start_time = SystemTime::now();

    let shared_state = Arc::new(AppState {
        pipeline_mutex: Mutex::new(pipeline),
        api_key,
        start_time,
        stats: RwLock::new(ServerStats::default()),
        tracker,
    });

    // 启动配置文件监听器（如果启用）
    if let Some(config_path) = config_path {
        let state_clone = Arc::clone(&shared_state);
        tokio::spawn(async move {
            if let Err(e) = watch_config_file(config_path, state_clone).await {
                log::error!("{}", t1("server_config_watcher_error", format!("{e:?}")));
            }
        });
    }

    // 设置 CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 构建路由
    let mut app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/metrics/detail", get(metrics_detail_handler))
        .route("/stats/aggregate", get(stats_aggregate_handler))
        .route("/stats/daily", get(stats_daily_handler))
        .route("/stats/by-filter", get(stats_by_filter_handler))
        .route("/compress", post(compress_handler))
        .route("/compress/stream", post(compress_stream_handler))
        .route("/decompress", post(decompress_handler))
        .route("/ws/tail", get(tail_ws_handler))
        .route("/reload", post(reload_config_handler))
        .route("/plugins", get(plugins_handler))
        .layer(cors)
        .with_state(shared_state);

    // 解析 Web UI 静态目录（如果用户显式指定了 TOKENSLIM_WEBUI_DIR 且存在，则从该目录提供，方便开发）
    // 否则直接使用内嵌的静态资源
    if let Ok(webui_dir) = std::env::var("TOKENSLIM_WEBUI_DIR") {
        let webui_path = PathBuf::from(&webui_dir);
        if webui_path.is_dir() {
            let serve = ServeDir::new(&webui_path).append_index_html_on_directories(true);
            app = app.fallback_service(serve);
            log::info!("{}", t1("server_webui_enabled", webui_path.display().to_string()));
        } else {
            app = app.fallback(get(static_handler));
            log::info!("{}", t1("server_webui_enabled", "embedded".to_string()));
        }
    } else {
        app = app.fallback(get(static_handler));
        log::info!("{}", t1("server_webui_enabled", "embedded".to_string()));
    }

    // 启动服务器
    let host = std::env::var("TOKENSLIM_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("TOKENSLIM_PORT").unwrap_or_else(|_| "10086".to_string());
    let addr_str = format!("{}:{}", host, port);

    let addr = SocketAddr::from_str(&addr_str).unwrap_or_else(|_| {
        log::warn!("{}", t("server_invalid_host_port_fallback"));
        SocketAddr::from(([127, 0, 0, 1], 10086))
    });

    log::info!("{}", t1("server_listening_on", addr));
    log::info!("{}", t("server_available_endpoints"));
    log::info!("{}", t("server_endpoint_health"));
    log::info!("{}", t("server_endpoint_metrics"));
    log::info!("{}", t("server_endpoint_metrics_detail"));
    log::info!("{}", t("server_endpoint_stats_aggregate"));
    log::info!("{}", t("server_endpoint_stats_daily"));
    log::info!("{}", t("server_endpoint_stats_by_filter"));
    log::info!("{}", t("server_endpoint_compress"));
    log::info!("{}", t("server_endpoint_decompress"));
    log::info!("{}", t("server_endpoint_reload"));

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed().unwrap_or_default().as_secs();
    Json(HealthResponse {
        status: "UP".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    })
}

// 列出当前 server 注册的所有插件名（供 Web UI 侧边栏展示）
async fn plugins_handler() -> Json<serde_json::Value> {
    let plugins = get_plugins();
    let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
    Json(serde_json::json!({ "plugins": names, "count": names.len() }))
}

// Metrics 端点（Prometheus 风格）
async fn metrics_handler(State(state): State<Arc<AppState>>) -> Result<String, ApiError> {
    let stats = state.stats.read().map_err(|_| ApiError::internal())?;
    let uptime = state.start_time.elapsed().unwrap_or_default().as_secs();

    let compression_ratio = if stats.total_bytes_in > 0 {
        (stats.total_bytes_out as f64 / stats.total_bytes_in as f64) * 100.0
    } else {
        0.0
    };

    let mut output = String::new();
    output.push_str("# HELP tokenslim_requests_total Total number of requests\n");
    output.push_str("# TYPE tokenslim_requests_total counter\n");
    output.push_str(&format!(
        "tokenslim_requests_total {}\n",
        stats.total_requests
    ));

    output.push_str("# HELP tokenslim_compressions_total Total number of compressions\n");
    output.push_str("# TYPE tokenslim_compressions_total counter\n");
    output.push_str(&format!(
        "tokenslim_compressions_total {}\n",
        stats.total_compressions
    ));

    output.push_str("# HELP tokenslim_decompressions_total Total number of decompressions\n");
    output.push_str("# TYPE tokenslim_decompressions_total counter\n");
    output.push_str(&format!(
        "tokenslim_decompressions_total {}\n",
        stats.total_decompressions
    ));

    output.push_str("# HELP tokenslim_bytes_in_total Total input bytes\n");
    output.push_str("# TYPE tokenslim_bytes_in_total counter\n");
    output.push_str(&format!(
        "tokenslim_bytes_in_total {}\n",
        stats.total_bytes_in
    ));

    output.push_str("# HELP tokenslim_bytes_out_total Total output bytes\n");
    output.push_str("# TYPE tokenslim_bytes_out_total counter\n");
    output.push_str(&format!(
        "tokenslim_bytes_out_total {}\n",
        stats.total_bytes_out
    ));

    output.push_str("# HELP tokenslim_compression_ratio Current compression ratio percentage\n");
    output.push_str("# TYPE tokenslim_compression_ratio gauge\n");
    output.push_str(&format!(
        "tokenslim_compression_ratio {:.2}\n",
        compression_ratio
    ));

    output.push_str("# HELP tokenslim_uptime_seconds Server uptime in seconds\n");
    output.push_str("# TYPE tokenslim_uptime_seconds counter\n");
    output.push_str(&format!("tokenslim_uptime_seconds {}\n", uptime));

    Ok(output)
}

async fn metrics_detail_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MetricsDetailResponse>, ApiError> {
    let pipeline = state
        .pipeline_mutex
        .lock()
        .map_err(|_| ApiError::internal())?;
    let snapshot = pipeline.get_metrics().snapshot();

    let mut module_timings_ms = HashMap::new();
    module_timings_ms.insert(
        "stream_reader".to_string(),
        snapshot.module_timings.stream_reader.as_millis() as u64,
    );
    module_timings_ms.insert(
        "text_slicer".to_string(),
        snapshot.module_timings.text_slicer.as_millis() as u64,
    );
    module_timings_ms.insert(
        "content_analyzer".to_string(),
        snapshot.module_timings.content_analyzer.as_millis() as u64,
    );
    module_timings_ms.insert(
        "plugin_dispatcher".to_string(),
        snapshot.module_timings.plugin_dispatcher.as_millis() as u64,
    );
    module_timings_ms.insert(
        "dictionary_engine".to_string(),
        snapshot.module_timings.dictionary_engine.as_millis() as u64,
    );
    module_timings_ms.insert(
        "dedup_engine".to_string(),
        snapshot.module_timings.dedup_engine.as_millis() as u64,
    );
    module_timings_ms.insert(
        "compression_pipeline".to_string(),
        snapshot.module_timings.compression_pipeline.as_millis() as u64,
    );
    module_timings_ms.insert(
        "rehydration_pipeline".to_string(),
        snapshot.module_timings.rehydration_pipeline.as_millis() as u64,
    );

    let mut plugin_stats: Vec<PluginMetricItem> = snapshot
        .plugin_stats
        .iter()
        .map(|(name, s)| PluginMetricItem {
            plugin: name.clone(),
            detect_calls: s.detect_calls,
            compress_calls: s.compress_calls,
            decompress_calls: s.decompress_calls,
            total_detect_ms: s.total_detect_time.as_millis() as u64,
            total_compress_ms: s.total_compress_time.as_millis() as u64,
            total_decompress_ms: s.total_decompress_time.as_millis() as u64,
            panic_count: s.panic_count,
            timeout_count: s.timeout_count,
            fallback_count: s.fallback_count,
        })
        .collect();
    plugin_stats.sort_by(|a, b| {
        b.compress_calls
            .cmp(&a.compress_calls)
            .then_with(|| a.plugin.cmp(&b.plugin))
    });

    let errors: Vec<MetricErrorItem> = snapshot
        .errors
        .iter()
        .map(|e| MetricErrorItem {
            timestamp: e.timestamp.to_rfc3339(),
            module: e.module.clone(),
            plugin: e.plugin.clone(),
            error_type: e.error_type.clone(),
            message: e.message.clone(),
            slice_id: e.slice_id,
        })
        .collect();

    Ok(Json(MetricsDetailResponse {
        total_input_size: snapshot.total_input_size,
        total_output_size: snapshot.total_output_size,
        compression_ratio: snapshot.compression_ratio,
        slice_count: snapshot.slice_count,
        processing_time_ms: snapshot.processing_time.as_millis() as u64,
        module_timings_ms,
        plugin_stats,
        error_count: errors.len(),
        errors,
    }))
}

// 统计聚合端点
async fn stats_aggregate_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<StatsAggregateResponse>, ApiError> {
    check_auth(&headers, &state.api_key)?;

    let tracker = state
        .tracker
        .as_ref()
        .ok_or_else(ApiError::service_unavailable)?;
    let tracker = tracker.lock().map_err(|_| ApiError::internal())?;

    match tracker.get_summary() {
        Ok(summary) => {
            Ok(Json(StatsAggregateResponse {
                total_commands: summary.total_commands,
                total_input_tokens: summary.total_input_tokens,
                total_output_tokens: summary.total_output_tokens,
                tokens_saved: summary.tokens_saved,
                savings_pct: summary.savings_pct,
                period_days: 90, // 默认 90 天
            }))
        }
        Err(e) => {
            log::error!("{}", t1("server_summary_failed", format!("{e:?}")));
            Err(ApiError::internal())
        }
    }
}

// 按日统计端点
async fn stats_daily_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<StatsDailyRequest>,
) -> Result<Json<StatsDailyResponse>, ApiError> {
    check_auth(&headers, &state.api_key)?;

    let tracker = state
        .tracker
        .as_ref()
        .ok_or_else(ApiError::service_unavailable)?;
    let tracker = tracker.lock().map_err(|_| ApiError::internal())?;

    match tracker.get_daily(params.days) {
        Ok(daily_gains) => {
            let daily_stats = daily_gains
                .into_iter()
                .map(|gain| DailyStat {
                    date: gain.date,
                    commands: gain.commands,
                    input_tokens: gain.input_tokens,
                    output_tokens: gain.output_tokens,
                    tokens_saved: gain.tokens_saved,
                    savings_pct: gain.savings_pct,
                })
                .collect();

            Ok(Json(StatsDailyResponse {
                daily_stats,
                period_days: params.days,
            }))
        }
        Err(e) => {
            log::error!("{}", t1("server_daily_stats_failed", format!("{e:?}")));
            Err(ApiError::internal())
        }
    }
}

// 按过滤器统计端点
async fn stats_by_filter_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<StatsByFilterResponse>, ApiError> {
    check_auth(&headers, &state.api_key)?;

    let tracker = state
        .tracker
        .as_ref()
        .ok_or_else(ApiError::service_unavailable)?;
    let tracker = tracker.lock().map_err(|_| ApiError::internal())?;

    match tracker.get_by_filter() {
        Ok(filter_gains) => {
            let filter_stats = filter_gains
                .into_iter()
                .map(|gain| FilterStat {
                    filter_name: gain.filter_name,
                    commands: gain.commands,
                    input_tokens: gain.input_tokens,
                    output_tokens: gain.output_tokens,
                    tokens_saved: gain.tokens_saved,
                    savings_pct: gain.savings_pct,
                })
                .collect();

            Ok(Json(StatsByFilterResponse { filter_stats }))
        }
        Err(e) => {
            log::error!("{}", t1("server_filter_stats_failed", format!("{e:?}")));
            Err(ApiError::internal())
        }
    }
}

// 重新加载配置端点
async fn reload_config_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    check_auth(&headers, &state.api_key)?;

    log::info!("{}", t("server_reload_requested"));

    // 重新创建管道
    let config = PipelineConfig::default();
    let metrics = MetricsCollector::new(MetricsConfig {
        enabled: true,
        enable_module_timing: true,
        enable_plugin_stats: true,
        enable_error_logging: true,
        max_error_logs: 100,
    });
    let plugins = get_plugins();
    let new_pipeline = CompressionPipeline::new(config, plugins, metrics);

    // 替换管道
    let mut pipeline = state
        .pipeline_mutex
        .lock()
        .map_err(|_| ApiError::internal())?;
    *pipeline = new_pipeline;

    log::info!("{}", t("server_reload_ok"));

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Configuration reloaded"
    })))
}

// 配置文件监听器
async fn watch_config_file(
    config_path: PathBuf,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    use notify::EventKind;

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                let _ = tx.blocking_send(());
            }
        }
    })?;

    watcher.watch(&config_path, RecursiveMode::NonRecursive)?;

    log::info!(
        "{}",
        t1("server_watch_config_file", format!("{config_path:?}"))
    );

    while let Some(_) = rx.recv().await {
        log::info!("{}", t("server_config_changed_reloading"));

        // 等待一小段时间，确保文件写入完成
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 重新创建管道
        let config = PipelineConfig::default();
        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: true,
            enable_module_timing: true,
            enable_plugin_stats: true,
            enable_error_logging: true,
            max_error_logs: 100,
        });
        let plugins = get_plugins();
        let new_pipeline = CompressionPipeline::new(config, plugins, metrics);

        // 替换管道
        if let Ok(mut pipeline) = state.pipeline_mutex.lock() {
            *pipeline = new_pipeline;
            log::info!("{}", t("server_hot_reload_ok"));
        } else {
            log::error!("{}", t("server_hot_reload_lock_failed"));
        }
    }

    Ok(())
}

async fn compress_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CompressRequest>,
) -> Result<Response, ApiError> {
    check_auth(&headers, &state.api_key)?;

    // 更新统计
    {
        let mut stats = state.stats.write().map_err(|_| ApiError::internal())?;
        stats.total_requests += 1;
        stats.total_compressions += 1;
        stats.total_bytes_in += payload.text.len() as u64;
    }

    let mut pipeline = state
        .pipeline_mutex
        .lock()
        .map_err(|_| ApiError::internal())?;

    // 应用重排偏好
    if payload.reorder != pipeline.config.reorder_config.enabled {
        pipeline.config.reorder_config.enabled = payload.reorder;
    }

    match pipeline.compress_str(&payload.text) {
        Ok(output) => {
            // 更新输出字节统计
            let output_size = serde_json::to_string(&output).unwrap_or_default().len() as u64;
            {
                let mut stats = state.stats.write().map_err(|_| ApiError::internal())?;
                stats.total_bytes_out += output_size;
            }

            // 记录到 Tracker（如果可用）
            if let Some(ref tracker) = state.tracker {
                if let Ok(t) = tracker.lock() {
                    let event = TrackingEvent::new(
                        "server_compress",
                        None,
                        payload.text.len(),
                        output_size as usize,
                        0, // exit_code
                    );
                    let _ = t.record(&event);
                }
            }

            if payload.ai_export {
                let rehydrator = RehydrationPipeline::new(
                    output.dictionary.clone(),
                    get_plugins(),
                    RehydrationConfig::default(),
                );

                match rehydrator.rehydrate_for_ai(&output) {
                    Ok(text) => {
                        let mut ai_formatted = String::new();
                        ai_formatted
                            .push_str("========== TokenSlim AI Export Context ==========\n");
                        ai_formatted.push_str("[Directories]\n");
                        let mut dirs: Vec<_> = output.dictionary.directories.iter().collect();
                        dirs.sort_by_key(|(k, _)| k[2..].parse::<usize>().unwrap_or(0));
                        for (k, v) in dirs {
                            ai_formatted.push_str(&format!("{}: {}\n", k, v));
                        }
                        ai_formatted.push_str("\n[Semantic Logs]\n");
                        ai_formatted.push_str(&text);

                        Ok(Json(serde_json::json!({
                            "ai_text": ai_formatted
                        }))
                        .into_response())
                    }
                    Err(e) => {
                        log::error!("{}", t1("server_ai_rehydrate_failed", format!("{e:?}")));
                        Err(ApiError::internal())
                    }
                }
            } else {
                Ok(Json(output).into_response())
            }
        }
        Err(e) => {
            log::error!("{}", t1("server_compression_failed", format!("{e:?}")));
            Err(ApiError::internal())
        }
    }
}

// SSE 流式压缩：先推送 start 状态，然后在后台线程完成压缩后推送 done/error
async fn compress_stream_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CompressRequest>,
) -> Result<Sse<ReceiverStream<Result<SseEvent, Infallible>>>, ApiError> {
    check_auth(&headers, &state.api_key)?;

    {
        let mut stats = state.stats.write().map_err(|_| ApiError::internal())?;
        stats.total_requests += 1;
        stats.total_compressions += 1;
        stats.total_bytes_in += payload.text.len() as u64;
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<SseEvent, Infallible>>(8);
    let state = Arc::clone(&state);
    let reorder = payload.reorder;
    let ai_export = payload.ai_export;
    let text = payload.text;
    let input_len = text.len();

    tokio::spawn(async move {
        let start_data = serde_json::json!({ "stage": "start" }).to_string();
        let _ = tx
            .send(Ok(SseEvent::default().event("status").data(start_data)))
            .await;

        let state_for_blocking = Arc::clone(&state);
        let result: Result<Result<CompressionOutput, String>, tokio::task::JoinError> =
            tokio::task::spawn_blocking(move || {
                let mut pipeline = match state_for_blocking.pipeline_mutex.lock() {
                    Ok(p) => p,
                    Err(_) => return Err("failed to lock pipeline".to_string()),
                };
                pipeline.config.reorder_config.enabled = reorder;
                match pipeline.compress_str(&text) {
                    Ok(out) => Ok(out),
                    Err(e) => Err(format!("{e:?}")),
                }
            })
            .await;

        match result {
            Ok(Ok(output)) => {
                let output_size = serde_json::to_string(&output).unwrap_or_default().len() as u64;
                if let Ok(mut stats) = state.stats.write() {
                    stats.total_bytes_out += output_size;
                }
                if let Some(ref tracker) = state.tracker {
                    if let Ok(t) = tracker.lock() {
                        let event = TrackingEvent::new(
                            "server_compress_stream",
                            None,
                            input_len,
                            output_size as usize,
                            0,
                        );
                        let _ = t.record(&event);
                    }
                }

                let done_payload = if ai_export {
                    let rehydrator = RehydrationPipeline::new(
                        output.dictionary.clone(),
                        get_plugins(),
                        RehydrationConfig::default(),
                    );
                    match rehydrator.rehydrate_for_ai(&output) {
                        Ok(ai_text) => serde_json::json!({
                            "output": output,
                            "ai_text": ai_text,
                        }),
                        Err(e) => {
                            let err_data = serde_json::json!({ "stage": "error", "message": format!("{e:?}") }).to_string();
                            let _ = tx.send(Ok(SseEvent::default().event("error").data(err_data))).await;
                            return;
                        }
                    }
                } else {
                    serde_json::json!({ "output": output })
                };
                let done_data = serde_json::json!({ "stage": "done", "payload": done_payload }).to_string();
                let _ = tx.send(Ok(SseEvent::default().event("done").data(done_data))).await;
            }
            Ok(Err(msg)) => {
                log::error!("{}", t1("server_compression_failed", msg.clone()));
                let err_data = serde_json::json!({ "stage": "error", "message": msg }).to_string();
                let _ = tx.send(Ok(SseEvent::default().event("error").data(err_data))).await;
            }
            Err(_) => {
                let err_data = serde_json::json!({ "stage": "error", "message": "compression task panicked" }).to_string();
                let _ = tx.send(Ok(SseEvent::default().event("error").data(err_data))).await;
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)))
}

// WebSocket 实时日志 tail：客户端先发 {"path":"...","interval_ms":1000,"compress":true}
async fn tail_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| tail_socket_handler(socket, state))
}

#[derive(Deserialize, Debug)]
struct TailRequest {
    path: String,
    #[serde(default = "default_interval_ms")]
    interval_ms: u64,
    #[serde(default = "default_compress")]
    compress: bool,
}

fn default_interval_ms() -> u64 { 1000 }
fn default_compress() -> bool { true }

async fn tail_socket_handler(mut socket: axum::extract::ws::WebSocket, state: Arc<AppState>) {
    use axum::extract::ws::{Message, Utf8Bytes};

    fn text_msg(s: impl Into<Utf8Bytes>) -> Message {
        Message::Text(s.into())
    }

    // 等待客户端第一条配置消息
    let req = match socket.recv().await {
        Some(Ok(Message::Text(text))) => {
            match serde_json::from_str::<TailRequest>(&text) {
                Ok(r) => r,
                Err(e) => {
                    let _ = socket.send(text_msg(format!("{{\"error\":\"invalid request: {e}\"}}"))).await;
                    return;
                }
            }
        }
        _ => {
            let _ = socket.send(text_msg("{\"error\":\"expected JSON text message\"}".to_string())).await;
            return;
        }
    };

    // 路径安全校验：限制在当前工作目录内
    let base = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => {
            let _ = socket.send(text_msg("{\"error\":\"cannot get cwd\"}".to_string())).await;
            return;
        }
    };
    let target = base.join(&req.path);
    let Ok(canonical_base) = base.canonicalize() else {
        let _ = socket.send(text_msg("{\"error\":\"cannot canonicalize cwd\"}".to_string())).await;
        return;
    };
    let Ok(canonical_target) = target.canonicalize() else {
        let _ = socket.send(text_msg(format!("{{\"error\":\"path not found: {}\"}}", req.path))).await;
        return;
    };
    if !canonical_target.starts_with(&canonical_base) {
        let _ = socket.send(text_msg("{\"error\":\"path outside cwd\"}".to_string())).await;
        return;
    }
    if !canonical_target.is_file() {
        let _ = socket.send(text_msg("{\"error\":\"not a regular file\"}".to_string())).await;
        return;
    }

    // 打开文件并定位到末尾
    let file = match tokio::fs::File::open(&canonical_target).await {
        Ok(f) => f,
        Err(e) => {
            let _ = socket.send(text_msg(format!("{{\"error\":\"open failed: {e}\"}}"))).await;
            return;
        }
    };
    let mut reader = tokio::io::BufReader::new(file);
    if let Err(e) = tokio::io::AsyncSeekExt::seek(&mut reader, std::io::SeekFrom::End(0)).await {
        let _ = socket.send(text_msg(format!("{{\"error\":\"seek failed: {e}\"}}"))).await;
        return;
    }

    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(req.interval_ms.max(100)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        // 读取新增内容
        let mut chunk = String::new();
        let mut limited = false;
        loop {
            let mut line = String::new();
            match tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    if chunk.len() + line.len() > 64 * 1024 {
                        limited = true;
                        break;
                    }
                    chunk.push_str(&line);
                }
                Err(e) => {
                    let _ = socket.send(text_msg(format!("{{\"error\":\"read failed: {e}\"}}"))).await;
                    return;
                }
            }
        }
        if chunk.is_empty() && !limited {
            continue;
        }

        let payload = if req.compress {
            let state_for_blocking = Arc::clone(&state);
            let result: Result<Result<String, String>, tokio::task::JoinError> =
                tokio::task::spawn_blocking(move || {
                    let mut pipeline = match state_for_blocking.pipeline_mutex.lock() {
                        Ok(p) => p,
                        Err(_) => return Err("failed to lock pipeline".to_string()),
                    };
                    match pipeline.compress_str(&chunk) {
                        Ok(out) => Ok(serde_json::to_string(&out).unwrap_or_default()),
                        Err(e) => Err(format!("{e:?}")),
                    }
                })
                .await;
            match result {
                Ok(Ok(json)) => format!("{{\"compressed\":true,\"output\":{json},\"truncated\":{limited}}}"),
                Ok(Err(msg)) => format!("{{\"error\":\"{msg}\"}}"),
                Err(_) => "{\"error\":\"compression task panicked\"}".to_string(),
            }
        } else {
            serde_json::json!({ "compressed": false, "text": chunk, "truncated": limited }).to_string()
        };

        if socket.send(text_msg(payload)).await.is_err() {
            break;
        }
    }
}

async fn decompress_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DecompressRequest>,
) -> Result<Json<String>, ApiError> {
    check_auth(&headers, &state.api_key)?;

    // 更新统计
    {
        let mut stats = state.stats.write().map_err(|_| ApiError::internal())?;
        stats.total_requests += 1;
        stats.total_decompressions += 1;
    }

    log::info!("{}", t("server_decompress_request_received"));

    // 初始化还原管道
    let plugins = get_plugins();
    let rehydrator =
        RehydrationPipeline::new(payload.dictionary, plugins, RehydrationConfig::default());

    // 模拟 CompressionOutput
    let output = CompressionOutput {
        tokens: payload.tokens,
        dictionary: tokenslim::core::dictionary_engine::Dictionary::default(),
        metadata: tokenslim::core::compression_pipeline::CompressionMetadata::default(),
    };

    match rehydrator.rehydrate(&output) {
        Ok(text) => Ok(Json(text)),
        Err(e) => {
            log::error!("{}", t1("server_decompression_failed", format!("{e:?}")));
            Err(ApiError::internal())
        }
    }
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }
    match WebUiAsset::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            if path != "index.html" {
                // SPA fallback for client-side routing
                if let Some(content) = WebUiAsset::get("index.html") {
                    let mime = mime_guess::from_path("index.html").first_or_octet_stream();
                    return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
                }
            }
            (StatusCode::NOT_FOUND, "404 Not Found").into_response()
        }
    }
}
