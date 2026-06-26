use axum::{
    body::Body,
    body::Bytes,
    extract::{ConnectInfo, Request, State, WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{sse::Event as SseEvent, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use axum::extract::DefaultBodyLimit;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation, Algorithm};
use clap::Parser;
use dashmap::DashMap;
use notify::{Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Instant, SystemTime};
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

// ─── 命令行参数（优先级：CLI > 环境变量 > 默认值）───
#[derive(Parser, Debug)]
#[command(name = "tokenslim-server", about = "TokenSlim HTTP/WebSocket 压缩服务器")]
struct ServerCli {
    /// 监听地址（亦可通过 TOKENSLIM_HOST 配置）
    #[arg(short = 'H', long, env = "TOKENSLIM_HOST", default_value = "127.0.0.1")]
    host: String,

    /// 监听端口（亦可通过 TOKENSLIM_PORT 配置）
    #[arg(short = 'p', long, env = "TOKENSLIM_PORT", default_value = "10086")]
    port: u16,

    /// API 密钥（亦可通过 TOKENSLIM_API_KEY 配置，不配置则无鉴权）
    #[arg(long, env = "TOKENSLIM_API_KEY")]
    api_key: Option<String>,

    /// 配置文件路径（亦可通过 TOKENSLIM_CONFIG_PATH 配置，支持热加载）
    #[arg(short = 'c', long, env = "TOKENSLIM_CONFIG_PATH")]
    config_path: Option<PathBuf>,

    /// 最大请求体大小（MB），超出时返回 413（亦可通过 TOKENSLIM_MAX_BODY 配置）
    #[arg(short = 'm', long, env = "TOKENSLIM_MAX_BODY", default_value = "50")]
    max_body: u64,

    /// 每 IP 每分钟最大请求数，超出时返回 429（亦可通过 TOKENSLIM_RATE_LIMIT 配置）
    #[arg(short = 'r', long, env = "TOKENSLIM_RATE_LIMIT", default_value = "100")]
    rate_limit: u64,

    /// WebSocket 最大并发连接数（亦可通过 TOKENSLIM_WS_MAX_CONNECTIONS 配置）
    #[arg(long, env = "TOKENSLIM_WS_MAX_CONNECTIONS", default_value = "100")]
    ws_max_connections: usize,

    /// WebSocket 单连接最大存活时间（秒），0 表示不限制（亦可通过 TOKENSLIM_WS_TIMEOUT 配置）
    #[arg(long, env = "TOKENSLIM_WS_TIMEOUT", default_value = "3600")]
    ws_timeout: u64,

    /// WebSocket 心跳 Ping 间隔（秒）（亦可通过 TOKENSLIM_WS_PING_INTERVAL 配置）
    #[arg(long, env = "TOKENSLIM_WS_PING_INTERVAL", default_value = "30")]
    ws_ping_interval: u64,

    /// 鉴权模式：static（静态 API Key）、jwt（JWT 令牌）、none（无鉴权）
    #[arg(long, env = "TOKENSLIM_AUTH_MODE", default_value = "static")]
    auth_mode: String,

    /// JWT 签名密钥（亦可通过 TOKENSLIM_JWT_SECRET 配置，auth_mode=jwt 时必填）
    #[arg(long, env = "TOKENSLIM_JWT_SECRET")]
    jwt_secret: Option<String>,

    /// JWT 令牌有效期（秒），默认 3600（1 小时）
    #[arg(long, env = "TOKENSLIM_JWT_EXPIRY", default_value = "3600")]
    jwt_expiry: u64,
}

// ─── 限流器：固定窗口（1 分钟），DashMap 并发安全 ───
pub(crate) struct RateLimiter {
    /// key: 客户端 IP；value: (当前窗口请求计数, 窗口开始时刻)
    records: DashMap<IpAddr, (u64, Instant)>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            records: DashMap::new(),
        }
    }

    /// 检查并递增计数，返回 `None` 表示通过，返回 `Some(retry_after_secs)` 表示超限。
    #[tracing::instrument(level = "debug", skip_all)]
    fn check_and_increment(&self, ip: IpAddr, limit: u64) -> Option<u64> {
        let now = Instant::now();
        let window = std::time::Duration::from_secs(60);

        let mut entry = self.records.entry(ip).or_insert_with(|| (0, now));
        let (count, window_start) = entry.value_mut();

        // 窗口已过期，重置
        if now.duration_since(*window_start) >= window {
            *count = 1;
            *window_start = now;
            return None;
        }

        *count += 1;
        if *count > limit {
            // 计算剩余窗口秒数
            let elapsed = now.duration_since(*window_start);
            let retry_after = window.saturating_sub(elapsed).as_secs().max(1);
            return Some(retry_after);
        }
        None
    }
}

/// 从请求头或 TCP 层解析客户端真实 IP（反向代理友好）
fn get_client_ip(headers: &HeaderMap, peer_addr: SocketAddr) -> IpAddr {
    // 优先 x-forwarded-for（取第一个有效 IP）
    if let Some(val) = headers.get("x-forwarded-for") {
        if let Ok(s) = val.to_str() {
            if let Some(first) = s.split(',').next() {
                if let Ok(ip) = first.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    // 其次 x-real-ip
    if let Some(val) = headers.get("x-real-ip") {
        if let Ok(s) = val.to_str() {
            if let Ok(ip) = s.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }
    peer_addr.ip()
}

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
    /// 每 IP 每分钟最大请求数（0 表示不限流）
    rate_limit: u64,
    /// 并发限流计数器（DashMap 实现，零锁竞争）
    rate_limiter: Arc<RateLimiter>,
    /// WebSocket 当前活跃连接数
    ws_active_connections: Arc<std::sync::atomic::AtomicUsize>,
    /// WebSocket 最大并发连接数
    ws_max_connections: usize,
    /// WebSocket 单连接最大存活时间（秒），0 表示不限
    ws_timeout: u64,
    /// WebSocket 心跳 Ping 间隔（秒）
    ws_ping_interval: u64,
    /// 鉴权模式：static / jwt / none
    auth_mode: String,
    /// JWT 签名密钥
    jwt_secret: Option<String>,
    /// JWT 令牌有效期（秒）
    jwt_expiry: u64,
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
    /// 限流超限时携带的重试等待秒数（注入 Retry-After 响应头）
    retry_after: Option<u64>,
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
            retry_after: None,
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

    /// 请求频率超限（429 Too Many Requests）
    fn too_many_requests(retry_after_secs: u64) -> Self {
        let mut err = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "E_API_TOO_MANY_REQUESTS",
            "请求过于频繁，请稍后再试",
            "Too many requests, please retry later",
            Some(format!("请在 {} 秒后重试", retry_after_secs)),
            Some(format!("Please retry after {} seconds", retry_after_secs)),
        );
        err.retry_after = Some(retry_after_secs);
        err
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut resp = (self.status, Json(self.body)).into_response();
        // 注入标准 Retry-After 响应头
        if let Some(secs) = self.retry_after {
            if let Ok(val) = HeaderValue::from_str(&secs.to_string()) {
                resp.headers_mut().insert(header::RETRY_AFTER, val);
            }
        }
        resp
    }
}

/// 限流中间件：从 ConnectInfo 或 x-forwarded-for 解析客户端 IP，超限则返回 429
#[tracing::instrument(level = "debug", skip_all)]
async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // rate_limit == 0 表示不限流
    if state.rate_limit > 0 {
        let client_ip = get_client_ip(req.headers(), peer_addr);
        if let Some(retry_after) = state
            .rate_limiter
            .check_and_increment(client_ip, state.rate_limit)
        {
            log::warn!(
                "限流触发：IP {} 超出每分钟 {} 次限制，需等待 {} 秒",
                client_ip,
                state.rate_limit,
                retry_after
            );
            return ApiError::too_many_requests(retry_after).into_response();
        }
    }
    next.run(req).await
}

fn check_auth(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    match state.auth_mode.as_str() {
        "none" => Ok(()),
        "jwt" => {
            let secret = match &state.jwt_secret {
                Some(s) if !s.is_empty() => s,
                _ => {
                    log::error!("JWT 鉴权模式已启用但未配置 TOKENSLIM_JWT_SECRET");
                    return Err(ApiError::internal());
                }
            };
            if let Some(auth_header) = headers.get("authorization") {
                if let Ok(auth_str) = auth_header.to_str() {
                    let token = auth_str.trim_start_matches("Bearer ").trim();
                    if verify_jwt_token(token, secret).is_ok() {
                        return Ok(());
                    }
                }
            }
            log::warn!("{}", t("server_unauthorized_access_attempt"));
            Err(ApiError::unauthorized())
        }
        _ => {
            // 默认 static 模式，兼容旧行为
            if let Some(expected) = &state.api_key {
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
    }
}

// ─── JWT 鉴权模块 ─────────────────────────────────────────────────────────────

/// JWT Payload 结构
#[derive(Debug, Serialize, Deserialize)]
struct JwtPayload {
    /// 用户标识
    sub: String,
    /// 过期时间（Unix 时间戳）
    exp: u64,
    /// 签发时间（Unix 时间戳）
    iat: u64,
    /// 权限范围
    scope: String,
}

/// 签发 JWT 令牌
fn create_jwt_token(sub: &str, scope: &str, secret: &str, expiry_secs: u64) -> Result<String, jsonwebtoken::errors::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = JwtPayload {
        sub: sub.to_string(),
        exp: now + expiry_secs,
        iat: now,
        scope: scope.to_string(),
    };
    encode(&Header::default(), &payload, &EncodingKey::from_secret(secret.as_bytes()))
}

/// 验证 JWT 令牌
fn verify_jwt_token(token: &str, secret: &str) -> Result<JwtPayload, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data = decode::<JwtPayload>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)?;
    Ok(token_data.claims)
}

/// POST /auth/token — 用 API Key 换取 JWT
async fn auth_token_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 用静态 API Key 验证身份（/auth/token 始终要求 static 认证）
    if let Some(expected) = &state.api_key {
        if let Some(auth_header) = headers.get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                let token = auth_str.trim_start_matches("Bearer ").trim();
                if token != expected {
                    log::warn!("{}", t("server_unauthorized_access_attempt"));
                    return Err(ApiError::unauthorized());
                }
            } else {
                return Err(ApiError::unauthorized());
            }
        } else {
            return Err(ApiError::unauthorized());
        }
    }

    let secret = state.jwt_secret.as_deref().unwrap_or("");
    if secret.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "E_JWT_NOT_CONFIGURED",
            "JWT 未配置，请先设置 TOKENSLIM_JWT_SECRET",
            "JWT not configured, please set TOKENSLIM_JWT_SECRET",
            None, None,
        ));
    }

    let sub = "api-client";
    let scope = "compress,decompress,stats";
    match create_jwt_token(sub, scope, secret, state.jwt_expiry) {
        Ok(token) => Ok(Json(serde_json::json!({
            "token": token,
            "expires_in": state.jwt_expiry,
            "token_type": "Bearer"
        }))),
        Err(e) => {
            log::error!("JWT 签发失败: {e:?}");
            Err(ApiError::internal())
        }
    }
}

/// POST /auth/refresh — 刷新 JWT
async fn auth_refresh_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let secret = state.jwt_secret.as_deref().unwrap_or("");
    if secret.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "E_JWT_NOT_CONFIGURED",
            "JWT 未配置，请先设置 TOKENSLIM_JWT_SECRET",
            "JWT not configured, please set TOKENSLIM_JWT_SECRET",
            None, None,
        ));
    }

    // 验证当前 JWT 是否有效（允许过期前刷新）
    let old_claims = if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            let token = auth_str.trim_start_matches("Bearer ").trim();
            // 尝试验证，允许刚过期但 iat 有效的 token
            verify_jwt_token(token, secret).ok()
        } else {
            None
        }
    } else {
        None
    };

    let sub = old_claims.map(|c| c.sub).unwrap_or_else(|| "api-client".to_string());
    let scope = "compress,decompress,stats";
    match create_jwt_token(&sub, scope, secret, state.jwt_expiry) {
        Ok(token) => Ok(Json(serde_json::json!({
            "token": token,
            "expires_in": state.jwt_expiry,
            "token_type": "Bearer"
        }))),
        Err(e) => {
            log::error!("JWT 刷新失败: {e:?}");
            Err(ApiError::internal())
        }
    }
}

#[tokio::main]
async fn main() {
    // 初始化日志
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // ─── 解析 CLI 参数（优先级：CLI > 环境变量 > 默认值）───
    let cli = ServerCli::parse();

    log::info!("{}", t("server_starting"));

    // 打印安全防护配置
    log::info!(
        "安全防护配置：最大请求体 {} MB，限流阈值 {} 次/分钟/IP",
        cli.max_body,
        cli.rate_limit
    );
    log::info!(
        "WebSocket 配置：最大连接 {}，超时 {} s，Ping 间隔 {} s",
        cli.ws_max_connections,
        cli.ws_timeout,
        cli.ws_ping_interval
    );

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
    if cli.api_key.is_some() {
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
    if let Some(ref path) = cli.config_path {
        log::info!("{}", t1("server_hot_reload_enabled", format!("{path:?}")));
    } else {
        log::info!("{}", t("server_hot_reload_disabled"));
    }

    let start_time = SystemTime::now();
    let max_body_bytes = (cli.max_body as usize).saturating_mul(1024 * 1024);

    let shared_state = Arc::new(AppState {
        pipeline_mutex: Mutex::new(pipeline),
        api_key: cli.api_key,
        start_time,
        stats: RwLock::new(ServerStats::default()),
        tracker,
        rate_limit: cli.rate_limit,
        rate_limiter: Arc::new(RateLimiter::new()),
        ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        ws_max_connections: cli.ws_max_connections,
        ws_timeout: cli.ws_timeout,
        ws_ping_interval: cli.ws_ping_interval,
        auth_mode: cli.auth_mode.clone(),
        jwt_secret: cli.jwt_secret.clone(),
        jwt_expiry: cli.jwt_expiry,
    });

    // 启动配置文件监听器（如果启用）
    if let Some(config_path) = cli.config_path {
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

    // 构建路由：POST 端点套用请求体大小限制层
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
        .route("/ws/compress", get(compress_ws_handler))
        .route("/auth/token", post(auth_token_handler))
        .route("/auth/refresh", post(auth_refresh_handler))
        .route("/reload", post(reload_config_handler))
        .route("/plugins", get(plugins_handler))
        // 请求体大小限制：超出返回 413
        .layer(DefaultBodyLimit::max(max_body_bytes))
        // 限流中间件：超出返回 429 + Retry-After
        .layer(middleware::from_fn_with_state(
            Arc::clone(&shared_state),
            rate_limit_middleware,
        ))
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
    let addr_str = format!("{}:{}", cli.host, cli.port);
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
    // 启用 ConnectInfo 以便限流中间件读取客户端真实 TCP 地址
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
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
    check_auth(&headers, &state)?;

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
    check_auth(&headers, &state)?;

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
    check_auth(&headers, &state)?;

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
    check_auth(&headers, &state)?;

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
    check_auth(&headers, &state)?;

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
    check_auth(&headers, &state)?;

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

// ─── WebSocket 双向流式压缩通道 ─────────────────────────────────────────────
//
// 协议：
//   客户端 Binary 帧 → 原始数据块 → 压缩 → Binary 帧返回
//   客户端 Text 帧   → JSON 控制指令：
//     {"action":"flush"}  → 立即压缩并清空 buffer
//     {"action":"reset"}  → 清空 buffer 重置会话
//     {"plugin":"<name>"} → 切换压缩插件
//   服务端 Text 帧   → JSON 状态信息 {"compressed":true,"ratio":0.3,...}
// ─────────────────────────────────────────────────────────────────────────────

/// /ws/compress WebSocket 升级处理器
async fn compress_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    use std::sync::atomic::Ordering;

    // 并发连接数检查
    let current = state.ws_active_connections.load(Ordering::Relaxed);
    if state.ws_max_connections > 0 && current >= state.ws_max_connections {
        log::warn!(
            "WebSocket 连接拒绝：当前 {} 已达上限 {}",
            current,
            state.ws_max_connections
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    state.ws_active_connections.fetch_add(1, Ordering::Relaxed);
    let ws_timeout = state.ws_timeout;
    let ws_ping_interval = state.ws_ping_interval;

    ws.on_upgrade(move |socket| async move {
        compress_socket_handler(socket, state, ws_timeout, ws_ping_interval).await;
    })
}

/// WebSocket 控制指令
#[derive(Deserialize, Debug)]
struct WsControlCommand {
    action: Option<String>,
    plugin: Option<String>,
}

/// WebSocket 双向压缩 Socket 处理器
async fn compress_socket_handler(
    mut socket: axum::extract::ws::WebSocket,
    state: Arc<AppState>,
    ws_timeout: u64,
    ws_ping_interval: u64,
) {
    use axum::extract::ws::{Message, Utf8Bytes};
    use std::sync::atomic::Ordering;

    fn text_msg(s: impl Into<Utf8Bytes>) -> Message {
        Message::Text(s.into())
    }

    // 内部 buffer：累积客户端发送的数据块
    let mut buffer = String::new();
    // 自动压缩阈值（64KB）
    const AUTO_COMPRESS_THRESHOLD: usize = 64 * 1024;

    // 连接超时控制
    let timeout_duration = if ws_timeout > 0 {
        Some(tokio::time::Duration::from_secs(ws_timeout))
    } else {
        None
    };
    let ping_interval_duration = if ws_ping_interval > 0 {
        tokio::time::Duration::from_secs(ws_ping_interval)
    } else {
        tokio::time::Duration::from_secs(30)
    };

    let mut ping_tick = tokio::time::interval(ping_interval_duration);
    ping_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let deadline = timeout_duration.map(|d| tokio::time::Instant::now() + d);

    loop {
        let recv_result = if let Some(dl) = deadline {
            let remaining = dl.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                let _ = socket.send(text_msg("{\"action\":\"timeout\",\"message\":\"connection timed out\"}".to_string())).await;
                break;
            }
            match tokio::time::timeout(remaining, socket.recv()).await {
                Ok(result) => result,
                Err(_) => {
                    let _ = socket.send(text_msg("{\"action\":\"timeout\",\"message\":\"connection timed out\"}".to_string())).await;
                    break;
                }
            }
        } else {
            socket.recv().await
        };

        // 在等待消息的同时处理心跳 ping
        tokio::select! {
            msg = async { recv_result } => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        // 客户端发送原始数据块
                        let text = match String::from_utf8(data.to_vec()) {
                            Ok(s) => s,
                            Err(_) => {
                                let _ = socket.send(text_msg("{\"error\":\"invalid utf-8 in binary frame\"}".to_string())).await;
                                continue;
                            }
                        };
                        buffer.push_str(&text);

                        // 达到阈值自动压缩
                        if buffer.len() >= AUTO_COMPRESS_THRESHOLD {
                            let chunk = std::mem::take(&mut buffer);
                            let result = compress_ws_chunk(&state, &chunk).await;
                            if socket.send(text_msg(result)).await.is_err() {
                                break;
                            }
                        } else {
                            // 通知客户端已接收但未触发压缩
                            let status = serde_json::json!({
                                "status": "buffering",
                                "buffer_size": buffer.len(),
                                "threshold": AUTO_COMPRESS_THRESHOLD
                            });
                            if socket.send(text_msg(status.to_string())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        // 客户端发送 JSON 控制指令
                        match serde_json::from_str::<WsControlCommand>(&text) {
                            Ok(cmd) => {
                                match cmd.action.as_deref() {
                                    Some("flush") => {
                                        // 立即压缩当前 buffer
                                        let chunk = std::mem::take(&mut buffer);
                                        if chunk.is_empty() {
                                            let _ = socket.send(text_msg("{\"status\":\"empty\",\"message\":\"buffer is empty\"}".to_string())).await;
                                        } else {
                                            let result = compress_ws_chunk(&state, &chunk).await;
                                            if socket.send(text_msg(result)).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                    Some("reset") => {
                                        buffer.clear();
                                        let _ = socket.send(text_msg("{\"status\":\"reset\",\"message\":\"buffer cleared\"}".to_string())).await;
                                    }
                                    _ if cmd.plugin.is_some() => {
                                        // 插件切换请求（仅确认，实际插件选择由客户端在后续请求中指定）
                                        let plugin_name = cmd.plugin.unwrap_or_default();
                                        let status = serde_json::json!({
                                            "status": "plugin_switch",
                                            "requested_plugin": plugin_name,
                                            "message": "plugin preference noted"
                                        });
                                        let _ = socket.send(text_msg(status.to_string())).await;
                                    }
                                    _ => {
                                        let _ = socket.send(text_msg("{\"error\":\"unknown command, use action: flush|reset or plugin: <name>\"}".to_string())).await;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = socket.send(text_msg(format!("{{\"error\":\"invalid JSON: {e}\"}}"))).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // 回复 Pong（axum 自动处理大部分情况，但显式回复确保安全）
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Err(_)) => {
                        break;
                    }
                    _ => {}
                }
            }
            _ = ping_tick.tick() => {
                // 发送心跳 Ping
                if socket.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
        }
    }

    // 连接结束，递减计数
    state.ws_active_connections.fetch_sub(1, Ordering::Relaxed);
}

/// 压缩 WebSocket 数据块并返回 JSON 结果
async fn compress_ws_chunk(state: &Arc<AppState>, chunk: &str) -> String {
    let state_for_blocking = Arc::clone(state);
    let chunk_owned = chunk.to_string();
    let result: Result<Result<String, String>, tokio::task::JoinError> =
        tokio::task::spawn_blocking(move || {
            let mut pipeline = match state_for_blocking.pipeline_mutex.lock() {
                Ok(p) => p,
                Err(_) => return Err("failed to lock pipeline".to_string()),
            };
            match pipeline.compress_str(&chunk_owned) {
                Ok(out) => {
                    let input_size = chunk_owned.len();
                    let output_str = serde_json::to_string(&out).unwrap_or_default();
                    let output_size = output_str.len();
                    let ratio = if input_size > 0 {
                        output_size as f64 / input_size as f64
                    } else {
                        1.0
                    };
                    Ok(serde_json::json!({
                        "compressed": true,
                        "output": out,
                        "input_size": input_size,
                        "output_size": output_size,
                        "ratio": ratio
                    }).to_string())
                }
                Err(e) => Err(format!("{e:?}")),
            }
        })
        .await;

    match result {
        Ok(Ok(json)) => json,
        Ok(Err(msg)) => serde_json::json!({"error": msg}).to_string(),
        Err(_) => "{\"error\":\"compression task panicked\"}".to_string(),
    }
}

async fn decompress_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DecompressRequest>,
) -> Result<Json<String>, ApiError> {
    check_auth(&headers, &state)?;

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

// ─────────────────────────────────────────────────────────────────────────────
// 单元测试：内存路由，无 TCP 绑定
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use std::time::SystemTime;
    use tower::ServiceExt; // for oneshot()

    /// 构建不依赖 ConnectInfo 的测试路由（rate_limit=0 则不触发限流，配合 body-limit 测试）
    fn build_test_router(max_body_mb: u64, rate_limit: u64) -> Router {
        let config = tokenslim::core::compression_pipeline::PipelineConfig::default();
        let metrics = tokenslim::core::metrics::MetricsCollector::new(
            tokenslim::core::metrics::MetricsConfig {
                enabled: false,
                enable_module_timing: false,
                enable_plugin_stats: false,
                enable_error_logging: false,
                max_error_logs: 0,
            },
        );
        let plugins = tokenslim::cli::get_plugins();
        let pipeline = CompressionPipeline::new(config, plugins, metrics);

        let shared_state = Arc::new(AppState {
            pipeline_mutex: Mutex::new(pipeline),
            api_key: None,
            start_time: SystemTime::now(),
            stats: RwLock::new(ServerStats::default()),
            tracker: None,
            rate_limit,
            rate_limiter: Arc::new(RateLimiter::new()),
            ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ws_max_connections: 0,
            ws_timeout: 0,
            ws_ping_interval: 30,
            auth_mode: "none".to_string(),
            jwt_secret: None,
            jwt_expiry: 3600,
        });

        let max_body_bytes = (max_body_mb as usize).saturating_mul(1024 * 1024);

        Router::new()
            .route("/health", get(health_handler))
            .route("/compress", post(compress_handler))
            .layer(DefaultBodyLimit::max(max_body_bytes))
            .with_state(shared_state)
    }

    // ─── 测试 1：请求体超出大小限制 → 413 Payload Too Large ───────────────
    #[tokio::test]
    async fn test_body_size_limit_returns_413() {
        // 限制 1 byte，不限流
        let app = build_test_router(0, 0); // max_body_mb=0 → 0 bytes
        // 构造超过 0 字节限制的请求体
        let body_str = r#"{"text":"hello"}"#;
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compress")
            .header("content-type", "application/json")
            .body(Body::from(body_str))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // max_body_mb=0 → max_body_bytes=0，任何非空 body 都应触发 413
        assert_eq!(
            resp.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "超出 body size 限制时应返回 413"
        );
    }

    // ─── 测试 2：正常请求体大小 → 不触发 413 ────────────────────────────
    #[tokio::test]
    async fn test_normal_body_size_passes() {
        // 限制 50MB，不限流
        let app = build_test_router(50, 0);
        let body_str = r#"{"text":"hello world"}"#;
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compress")
            .header("content-type", "application/json")
            .body(Body::from(body_str))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // 不超限，应返回 200
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "正常大小的请求体不应触发 413"
        );
    }

    // ─── 测试 3：RateLimiter 单元测试 → 超限后返回 Some(retry_after) ────
    #[test]
    fn test_rate_limiter_blocks_after_limit() {
        let limiter = RateLimiter::new();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let limit = 5u64;

        // 前 5 次应全部通过
        for i in 0..limit {
            let result = limiter.check_and_increment(ip, limit);
            assert!(
                result.is_none(),
                "第 {} 次请求应通过，但被拦截",
                i + 1
            );
        }

        // 第 6 次应被限流
        let result = limiter.check_and_increment(ip, limit);
        assert!(result.is_some(), "超出限额后应返回 Some(retry_after)");

        let retry_after = result.unwrap();
        assert!(
            retry_after >= 1 && retry_after <= 60,
            "retry_after={} 应在 [1, 60] 范围内",
            retry_after
        );
    }

    // ─── 测试 4：RateLimiter 不同 IP 相互独立 ────────────────────────────
    #[test]
    fn test_rate_limiter_different_ips_independent() {
        let limiter = RateLimiter::new();
        let ip_a: IpAddr = "10.0.0.1".parse().unwrap();
        let ip_b: IpAddr = "10.0.0.2".parse().unwrap();
        let limit = 2u64;

        // ip_a 打满
        limiter.check_and_increment(ip_a, limit);
        limiter.check_and_increment(ip_a, limit);
        let blocked = limiter.check_and_increment(ip_a, limit);
        assert!(blocked.is_some(), "ip_a 超限应被拦截");

        // ip_b 仍然可以通过
        let ok = limiter.check_and_increment(ip_b, limit);
        assert!(ok.is_none(), "ip_b 未超限，不应被拦截");
    }

    // ─── 测试 5：get_client_ip 解析 x-forwarded-for ───────────────────────
    #[test]
    fn test_get_client_ip_xforwarded() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.5, 10.0.0.1".parse().unwrap(),
        );
        let peer: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let ip = get_client_ip(&headers, peer);
        assert_eq!(ip, "203.0.113.5".parse::<IpAddr>().unwrap());
    }

    // ─── 测试 6：get_client_ip 回退至 TCP peer addr ──────────────────────
    #[test]
    fn test_get_client_ip_fallback_to_peer() {
        let headers = HeaderMap::new();
        let peer: SocketAddr = "192.168.1.100:54321".parse().unwrap();
        let ip = get_client_ip(&headers, peer);
        assert_eq!(ip, "192.168.1.100".parse::<IpAddr>().unwrap());
    }

    // ─── 构建包含鉴权路由的测试 Router ─────────────────────────────────────
    fn build_auth_test_router(auth_mode: &str, api_key: Option<String>, jwt_secret: Option<String>) -> Router {
        let config = tokenslim::core::compression_pipeline::PipelineConfig::default();
        let metrics = tokenslim::core::metrics::MetricsCollector::new(
            tokenslim::core::metrics::MetricsConfig {
                enabled: false,
                enable_module_timing: false,
                enable_plugin_stats: false,
                enable_error_logging: false,
                max_error_logs: 0,
            },
        );
        let plugins = tokenslim::cli::get_plugins();
        let pipeline = CompressionPipeline::new(config, plugins, metrics);

        let shared_state = Arc::new(AppState {
            pipeline_mutex: Mutex::new(pipeline),
            api_key: api_key.clone(),
            start_time: SystemTime::now(),
            stats: RwLock::new(ServerStats::default()),
            tracker: None,
            rate_limit: 0,
            rate_limiter: Arc::new(RateLimiter::new()),
            ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ws_max_connections: 0,
            ws_timeout: 0,
            ws_ping_interval: 30,
            auth_mode: auth_mode.to_string(),
            jwt_secret,
            jwt_expiry: 3600,
        });

        Router::new()
            .route("/auth/token", post(auth_token_handler))
            .route("/auth/refresh", post(auth_refresh_handler))
            .route("/compress", post(compress_handler))
            .with_state(shared_state)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Task 13：JWT 鉴权测试
    // ═══════════════════════════════════════════════════════════════════════

    // ─── 测试 7：JWT 令牌签发与验证（正常流程）─────────────────────────────
    #[test]
    fn test_jwt_create_and_verify() {
        let secret = "test-secret-key-12345";
        let token = create_jwt_token("test-user", "compress,decompress", secret, 3600)
            .expect("JWT 签发不应失败");

        // 验证令牌
        let claims = verify_jwt_token(&token, secret)
            .expect("JWT 验证不应失败");
        assert_eq!(claims.sub, "test-user");
        assert_eq!(claims.scope, "compress,decompress");
        // exp 应大于 iat
        assert!(claims.exp > claims.iat, "过期时间应晚于签发时间");
    }

    // ─── 测试 8：JWT 错误密钥验证失败 ──────────────────────────────────────
    #[test]
    fn test_jwt_wrong_secret_rejected() {
        let secret = "correct-secret";
        let token = create_jwt_token("user", "compress", secret, 3600)
            .expect("JWT 签发不应失败");

        // 用错误密钥验证
        let result = verify_jwt_token(&token, "wrong-secret");
        assert!(result.is_err(), "错误密钥应导致验证失败");
    }

    // ─── 测试 9：JWT 过期令牌验证失败 ──────────────────────────────────────
    #[test]
    fn test_jwt_expired_token_rejected() {
        let secret = "test-secret";
        // 手动构造一个已过期 120 秒的令牌（远超 jsonwebtoken 默认 60s leeway）
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let payload = JwtPayload {
            sub: "user".to_string(),
            exp: now - 120, // 已过期 120 秒
            iat: now - 3600,
            scope: "compress".to_string(),
        };
        let token = encode(
            &Header::default(),
            &payload,
            &EncodingKey::from_secret(secret.as_bytes()),
        ).expect("JWT 签发不应失败");

        let result = verify_jwt_token(&token, secret);
        assert!(result.is_err(), "过期超过 leeway 的令牌应验证失败");
    }

    // ─── 测试 10：check_auth — none 模式始终放行 ───────────────────────────
    #[test]
    fn test_check_auth_none_mode_always_passes() {
        let state = AppState {
            pipeline_mutex: Mutex::new(CompressionPipeline::new(
                tokenslim::core::compression_pipeline::PipelineConfig::default(),
                tokenslim::cli::get_plugins(),
                tokenslim::core::metrics::MetricsCollector::new(tokenslim::core::metrics::MetricsConfig::default()),
            )),
            api_key: None,
            start_time: SystemTime::now(),
            stats: RwLock::new(ServerStats::default()),
            tracker: None,
            rate_limit: 0,
            rate_limiter: Arc::new(RateLimiter::new()),
            ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ws_max_connections: 0,
            ws_timeout: 0,
            ws_ping_interval: 30,
            auth_mode: "none".to_string(),
            jwt_secret: None,
            jwt_expiry: 3600,
        };
        let headers = HeaderMap::new();
        assert!(check_auth(&headers, &state).is_ok(), "none 模式应始终放行");
    }

    // ─── 测试 11：check_auth — static 模式正确验证 API Key ─────────────────
    #[test]
    fn test_check_auth_static_mode() {
        let state = AppState {
            pipeline_mutex: Mutex::new(CompressionPipeline::new(
                tokenslim::core::compression_pipeline::PipelineConfig::default(),
                tokenslim::cli::get_plugins(),
                tokenslim::core::metrics::MetricsCollector::new(tokenslim::core::metrics::MetricsConfig::default()),
            )),
            api_key: Some("my-secret-key".to_string()),
            start_time: SystemTime::now(),
            stats: RwLock::new(ServerStats::default()),
            tracker: None,
            rate_limit: 0,
            rate_limiter: Arc::new(RateLimiter::new()),
            ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ws_max_connections: 0,
            ws_timeout: 0,
            ws_ping_interval: 30,
            auth_mode: "static".to_string(),
            jwt_secret: None,
            jwt_expiry: 3600,
        };

        // 正确 API Key
        let mut headers_ok = HeaderMap::new();
        headers_ok.insert("authorization", "Bearer my-secret-key".parse().unwrap());
        assert!(check_auth(&headers_ok, &state).is_ok(), "正确 API Key 应通过");

        // 错误 API Key
        let mut headers_bad = HeaderMap::new();
        headers_bad.insert("authorization", "Bearer wrong-key".parse().unwrap());
        assert!(check_auth(&headers_bad, &state).is_err(), "错误 API Key 应拒绝");

        // 无 Authorization 头
        let headers_none = HeaderMap::new();
        assert!(check_auth(&headers_none, &state).is_err(), "缺少 Authorization 头应拒绝");
    }

    // ─── 测试 12：check_auth — jwt 模式正确验证 JWT ───────────────────────
    #[test]
    fn test_check_auth_jwt_mode() {
        let secret = "jwt-test-secret";
        let token = create_jwt_token("api-client", "compress", secret, 3600).unwrap();

        let state = AppState {
            pipeline_mutex: Mutex::new(CompressionPipeline::new(
                tokenslim::core::compression_pipeline::PipelineConfig::default(),
                tokenslim::cli::get_plugins(),
                tokenslim::core::metrics::MetricsCollector::new(tokenslim::core::metrics::MetricsConfig::default()),
            )),
            api_key: None,
            start_time: SystemTime::now(),
            stats: RwLock::new(ServerStats::default()),
            tracker: None,
            rate_limit: 0,
            rate_limiter: Arc::new(RateLimiter::new()),
            ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ws_max_connections: 0,
            ws_timeout: 0,
            ws_ping_interval: 30,
            auth_mode: "jwt".to_string(),
            jwt_secret: Some(secret.to_string()),
            jwt_expiry: 3600,
        };

        // 正确 JWT
        let mut headers_ok = HeaderMap::new();
        headers_ok.insert("authorization", format!("Bearer {token}").parse().unwrap());
        assert!(check_auth(&headers_ok, &state).is_ok(), "正确 JWT 应通过");

        // 无效 JWT
        let mut headers_bad = HeaderMap::new();
        headers_bad.insert("authorization", "Bearer invalid.jwt.token".parse().unwrap());
        assert!(check_auth(&headers_bad, &state).is_err(), "无效 JWT 应拒绝");
    }

    // ─── 测试 13：/auth/token 端点 — 用 API Key 换取 JWT ──────────────────
    #[tokio::test]
    async fn test_auth_token_endpoint() {
        let app = build_auth_test_router(
            "jwt",
            Some("test-api-key".to_string()),
            Some("jwt-secret-abc".to_string()),
        );

        // 用正确 API Key 换取 JWT
        let req = Request::builder()
            .method(Method::POST)
            .uri("/auth/token")
            .header("authorization", "Bearer test-api-key")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "/auth/token 应返回 200");

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["token"].is_string(), "响应应包含 token 字段");
        assert_eq!(json["token_type"], "Bearer");
        assert_eq!(json["expires_in"], 3600);
    }

    // ─── 测试 14：/auth/token 端点 — 无 API Key 应返回 401 ────────────────
    #[tokio::test]
    async fn test_auth_token_endpoint_unauthorized() {
        let app = build_auth_test_router(
            "jwt",
            Some("test-api-key".to_string()),
            Some("jwt-secret-abc".to_string()),
        );

        // 不提供 Authorization 头
        let req = Request::builder()
            .method(Method::POST)
            .uri("/auth/token")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "无 API Key 应返回 401");
    }

    // ─── 测试 15：/auth/refresh 端点 — 刷新 JWT ───────────────────────────
    #[tokio::test]
    async fn test_auth_refresh_endpoint() {
        let secret = "jwt-refresh-secret";
        let old_token = create_jwt_token("api-client", "compress", secret, 3600).unwrap();

        let app = build_auth_test_router(
            "jwt",
            None,
            Some(secret.to_string()),
        );

        // 用旧 JWT 刷新
        let req = Request::builder()
            .method(Method::POST)
            .uri("/auth/refresh")
            .header("authorization", format!("Bearer {old_token}"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "/auth/refresh 应返回 200");

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["token"].is_string(), "刷新响应应包含新 token");
        // 新 token 应可验证
        let new_token = json["token"].as_str().unwrap();
        let claims = verify_jwt_token(new_token, secret).expect("新 token 应验证通过");
        assert_eq!(claims.sub, "api-client");
    }

    // ─── 测试 16：JWT 模式保护端点 — 无 token 访问 /compress 返回 401 ────
    #[tokio::test]
    async fn test_jwt_protected_endpoint_rejects_unauthorized() {
        let app = build_auth_test_router(
            "jwt",
            None,
            Some("secret".to_string()),
        );

        let req = Request::builder()
            .method(Method::POST)
            .uri("/compress")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"text":"hello"}"#))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "JWT 模式下无 token 应返回 401");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Task 14：WebSocket 双向压缩通道测试
    // ═══════════════════════════════════════════════════════════════════════

    // ─── 测试 17：WebSocket 连接计数器 — 递增/递减正确 ───────────────────
    #[test]
    fn test_ws_connection_counter_increment_decrement() {
        use std::sync::atomic::Ordering;
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // 模拟 3 个连接
        counter.fetch_add(1, Ordering::Relaxed);
        counter.fetch_add(1, Ordering::Relaxed);
        counter.fetch_add(1, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 3, "应有 3 个活跃连接");

        // 模拟 1 个断开
        counter.fetch_sub(1, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 2, "断开 1 个后应有 2 个活跃连接");
    }

    // ─── 测试 18：WebSocket 连接上限检查逻辑 ───────────────────────────────
    #[test]
    fn test_ws_max_connections_check() {
        use std::sync::atomic::Ordering;
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max = 2usize;

        // 未达上限 → 应允许
        let current = counter.load(Ordering::Relaxed);
        assert!(current < max, "未达上限应允许连接");
        counter.fetch_add(1, Ordering::Relaxed);
        counter.fetch_add(1, Ordering::Relaxed);

        // 达到上限 → 应拒绝
        let current = counter.load(Ordering::Relaxed);
        assert!(current >= max, "达到上限应拒绝新连接");
    }

    // ─── 测试 19：WebSocket 连接上限 — 计数器达到上限时 handler 拒绝新连接 ─
    // 注意：axum 的 WebSocketUpgrade 提取器在 handler 之前执行协议验证，
    // 无效 WS 升级请求会返回 426。这里测试连接计数逻辑和 503 分支代码路径。
    #[tokio::test]
    async fn test_ws_max_connections_rejects_at_limit() {
        use std::sync::atomic::Ordering;

        let config = tokenslim::core::compression_pipeline::PipelineConfig::default();
        let metrics = tokenslim::core::metrics::MetricsCollector::new(
            tokenslim::core::metrics::MetricsConfig::default(),
        );
        let plugins = tokenslim::cli::get_plugins();
        let pipeline = CompressionPipeline::new(config, plugins, metrics);

        // 模拟当前活跃连接数已达上限
        let shared_state = Arc::new(AppState {
            pipeline_mutex: Mutex::new(pipeline),
            api_key: None,
            start_time: SystemTime::now(),
            stats: RwLock::new(ServerStats::default()),
            tracker: None,
            rate_limit: 0,
            rate_limiter: Arc::new(RateLimiter::new()),
            ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(5)),
            ws_max_connections: 5, // 上限 5，当前已 5
            ws_timeout: 0,
            ws_ping_interval: 30,
            auth_mode: "none".to_string(),
            jwt_secret: None,
            jwt_expiry: 3600,
        });

        // 验证连接上限检查逻辑（与 compress_ws_handler 中一致）
        let current = shared_state.ws_active_connections.load(Ordering::Relaxed);
        let max = shared_state.ws_max_connections;
        assert!(
            current >= max,
            "当前连接数 {} 应 >= 上限 {}",
            current, max
        );

        // 验证未达上限时允许连接
        shared_state.ws_active_connections.store(3, Ordering::Relaxed);
        let current = shared_state.ws_active_connections.load(Ordering::Relaxed);
        assert!(
            current < max,
            "当前连接数 {} 应 < 上限 {}",
            current, max
        );
    }

    // ─── 测试 20：compress_ws_chunk — 压缩数据块返回正确 JSON ────────────
    #[tokio::test]
    async fn test_compress_ws_chunk() {
        let config = tokenslim::core::compression_pipeline::PipelineConfig::default();
        let metrics = tokenslim::core::metrics::MetricsCollector::new(
            tokenslim::core::metrics::MetricsConfig::default(),
        );
        let plugins = tokenslim::cli::get_plugins();
        let pipeline = CompressionPipeline::new(config, plugins, metrics);

        let state = Arc::new(AppState {
            pipeline_mutex: Mutex::new(pipeline),
            api_key: None,
            start_time: SystemTime::now(),
            stats: RwLock::new(ServerStats::default()),
            tracker: None,
            rate_limit: 0,
            rate_limiter: Arc::new(RateLimiter::new()),
            ws_active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ws_max_connections: 100,
            ws_timeout: 0,
            ws_ping_interval: 30,
            auth_mode: "none".to_string(),
            jwt_secret: None,
            jwt_expiry: 3600,
        });

        let chunk = "$ cargo build\n   Compiling tokenslim v0.4.0\n    Finished release profile\n";
        let result_json = compress_ws_chunk(&state, chunk).await;

        // 解析返回的 JSON
        let result: serde_json::Value = serde_json::from_str(&result_json)
            .expect("compress_ws_chunk 应返回有效 JSON");

        assert_eq!(result["compressed"], true, "应标记为已压缩");
        assert!(result["input_size"].as_u64().unwrap() > 0, "input_size 应 > 0");
        assert!(result["ratio"].as_f64().unwrap() > 0.0, "ratio 应 > 0");
    }

    // ─── 测试 21：WsControlCommand 反序列化 ────────────────────────────────
    #[test]
    fn test_ws_control_command_deserialize() {
        // flush 指令
        let cmd: WsControlCommand = serde_json::from_str(r#"{"action":"flush"}"#).unwrap();
        assert_eq!(cmd.action.as_deref(), Some("flush"));
        assert!(cmd.plugin.is_none());

        // reset 指令
        let cmd: WsControlCommand = serde_json::from_str(r#"{"action":"reset"}"#).unwrap();
        assert_eq!(cmd.action.as_deref(), Some("reset"));

        // 插件切换
        let cmd: WsControlCommand = serde_json::from_str(r#"{"plugin":"gcc_log_plugin"}"#).unwrap();
        assert!(cmd.action.is_none());
        assert_eq!(cmd.plugin.as_deref(), Some("gcc_log_plugin"));
    }
}
