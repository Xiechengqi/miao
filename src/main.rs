use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Html, Json, Response},
    routing::{delete, get, post, put},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use lazy_static::lazy_static;
use nix::sys::signal::{kill, Signal};
use nix::unistd::{Pid, Uid};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio_tungstenite::connect_async;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use base64::Engine;

mod tcp_tunnel;
mod full_tunnel;

// Version embedded at compile time
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Embed sing-box binary based on target architecture
#[cfg(target_arch = "x86_64")]
const SING_BOX_BINARY: &[u8] = include_bytes!("../embedded/sing-box-amd64");

#[cfg(target_arch = "aarch64")]
const SING_BOX_BINARY: &[u8] = include_bytes!("../embedded/sing-box-arm64");

// ============================================================================
// Data Structures
// ============================================================================

fn default_true() -> bool {
    true
}

fn default_local_addr() -> String {
    "127.0.0.1".to_string()
}

fn default_remote_bind_addr() -> String {
    "127.0.0.1".to_string()
}

fn default_ssh_port() -> u16 {
    22
}

fn default_connect_timeout_ms() -> u64 {
    5_000
}

fn default_keepalive_interval_ms() -> u64 {
    10_000
}

fn default_tcp_tunnel_backoff() -> TcpTunnelBackoff {
    TcpTunnelBackoff {
        base_ms: 1_000,
        max_ms: 30_000,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TcpTunnelAuth {
    Password { password: String },
    PrivateKeyPath {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TcpTunnelBackoff {
    base_ms: u64,
    max_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TcpTunnelConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    enabled: bool,

    #[serde(default = "default_local_addr")]
    local_addr: String,
    local_port: u16,

    #[serde(default = "default_remote_bind_addr")]
    remote_bind_addr: String,
    remote_port: u16,

    ssh_host: String,
    #[serde(default = "default_ssh_port")]
    ssh_port: u16,
    username: String,

    auth: TcpTunnelAuth,

    #[serde(default = "default_true")]
    strict_host_key_checking: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    host_key_fingerprint: String,
    #[serde(default)]
    allow_public_bind: bool,

    #[serde(default = "default_connect_timeout_ms")]
    connect_timeout_ms: u64,
    #[serde(default = "default_keepalive_interval_ms")]
    keepalive_interval_ms: u64,
    #[serde(default = "default_tcp_tunnel_backoff")]
    reconnect_backoff_ms: TcpTunnelBackoff,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_by: Option<TcpTunnelManagedBy>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TcpTunnelManagedBy {
    FullTunnel { set_id: String, managed_port: u16 },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TcpTunnelSetConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    enabled: bool,

    #[serde(default = "default_remote_bind_addr")]
    remote_bind_addr: String,

    ssh_host: String,
    #[serde(default = "default_ssh_port")]
    ssh_port: u16,
    username: String,

    auth: TcpTunnelAuth,

    #[serde(default = "default_true")]
    strict_host_key_checking: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    host_key_fingerprint: String,

    #[serde(default)]
    exclude_ports: Vec<u16>,
    #[serde(default)]
    scan_interval_ms: u64,
    #[serde(default)]
    debounce_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sing_box_home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,  // 登录密码
    #[serde(default)]
    selections: HashMap<String, String>, // selector group -> node name
    #[serde(default)]
    nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dns_active: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dns_candidates: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dns_check_interval_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dns_fail_threshold: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dns_cooldown_ms: Option<u64>,
    // Proxy multi-select & auto failover (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_pool: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_monitor_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_check_interval_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_check_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_fail_threshold: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_window_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_window_fail_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_pause_ms: Option<u64>,

    // SSH reverse TCP tunnels (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tcp_tunnels: Vec<TcpTunnelConfig>,

    // Full tunnels (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tcp_tunnel_sets: Vec<TcpTunnelSetConfig>,
}

const DEFAULT_PORT: u16 = 6161;
const DEFAULT_DNS_ACTIVE: &str = "doh-cf";

// JWT 密钥（生产环境应使用环境变量）
const JWT_SECRET: &str = "miao_jwt_secret_key_change_in_production";

// JWT Claims 结构
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,   // subject (用户标识)
    exp: usize,    // expiration time
}

// 登录请求结构
#[derive(Deserialize)]
struct LoginRequest {
    password: String,
}

// 登录响应结构
#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

#[derive(Serialize, Deserialize)]
struct ClashSwitchRequest {
    name: String,
}

#[derive(Serialize)]
struct SelectionsResponse {
    selections: HashMap<String, String>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TcpTunnelAuthPublic {
    Password,
    PrivateKeyPath { path: String },
}

#[derive(Serialize)]
struct TcpTunnelItem {
    id: String,
    name: Option<String>,
    enabled: bool,
    local_addr: String,
    local_port: u16,
    remote_bind_addr: String,
    remote_port: u16,
    ssh_host: String,
    ssh_port: u16,
    username: String,
    auth: TcpTunnelAuthPublic,
    strict_host_key_checking: bool,
    host_key_fingerprint: String,
    allow_public_bind: bool,
    connect_timeout_ms: u64,
    keepalive_interval_ms: u64,
    reconnect_backoff_ms: TcpTunnelBackoff,
    status: tcp_tunnel::TunnelRuntimeStatus,
}

#[derive(Serialize)]
struct TcpTunnelListResponse {
    supported: bool,
    items: Vec<TcpTunnelItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum TcpTunnelOverviewMode {
    Single,
    Full,
}

#[derive(Serialize)]
struct TcpTunnelOverviewItem {
    mode: TcpTunnelOverviewMode,
    id: String,
    name: Option<String>,
    enabled: bool,
    ssh_host: String,
    ssh_port: u16,
    username: String,
    remote_bind_addr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_addr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<TcpTunnelAuthPublic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    strict_host_key_checking: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host_key_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allow_public_bind: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    connect_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keepalive_interval_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reconnect_backoff_ms: Option<TcpTunnelBackoff>,
    // For UI compatibility, keep a status object with a state string.
    status: tcp_tunnel::TunnelRuntimeStatus,
}

#[derive(Serialize)]
struct TcpTunnelOverviewResponse {
    supported: bool,
    items: Vec<TcpTunnelOverviewItem>,
}

#[derive(Deserialize)]
struct TcpTunnelUpsertRequest {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    local_addr: Option<String>,
    local_port: u16,
    #[serde(default)]
    remote_bind_addr: Option<String>,
    remote_port: u16,
    ssh_host: String,
    #[serde(default)]
    ssh_port: Option<u16>,
    username: String,
    auth: TcpTunnelAuth,
    #[serde(default)]
    strict_host_key_checking: Option<bool>,
    #[serde(default)]
    host_key_fingerprint: Option<String>,
    #[serde(default)]
    allow_public_bind: Option<bool>,
    #[serde(default)]
    connect_timeout_ms: Option<u64>,
    #[serde(default)]
    keepalive_interval_ms: Option<u64>,
    #[serde(default)]
    reconnect_backoff_ms: Option<TcpTunnelBackoff>,
}

#[derive(Serialize)]
struct TcpTunnelTestResponse {
    ok: bool,
}

#[derive(Deserialize)]
struct BulkIdsRequest {
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct TcpTunnelSetCreateRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    remote_bind_addr: Option<String>,
    ssh_host: String,
    #[serde(default)]
    ssh_port: Option<u16>,
    username: String,
    auth: TcpTunnelAuth,
    #[serde(default)]
    strict_host_key_checking: Option<bool>,
    #[serde(default)]
    host_key_fingerprint: Option<String>,
    #[serde(default)]
    exclude_ports: Option<Vec<u16>>,
    #[serde(default)]
    scan_interval_ms: Option<u64>,
    #[serde(default)]
    debounce_ms: Option<u64>,
}

#[derive(Serialize)]
struct TcpTunnelSetDetailResponse {
    id: String,
    name: Option<String>,
    enabled: bool,
    remote_bind_addr: String,
    ssh_host: String,
    ssh_port: u16,
    username: String,
    auth: TcpTunnelAuthPublic,
    strict_host_key_checking: bool,
    host_key_fingerprint: String,
    exclude_ports: Vec<u16>,
    scan_interval_ms: u64,
    debounce_ms: u64,
}

fn generate_tunnel_set_id() -> String {
    format!("s-{}", uuid::Uuid::new_v4())
}

#[derive(Serialize)]
struct SetupStatusResponse {
    initialized: bool,
}

#[derive(Deserialize)]
struct SetupInitRequest {
    password: String,
}

#[derive(Deserialize)]
struct WsAuthQuery {
    token: String,
    #[serde(default)]
    level: Option<String>,
}

struct AppState {
    config: Mutex<Config>,
    sing_box_home: String,
    sub_dir: PathBuf,
    sub_source: SubSource,
    sub_files: Mutex<Vec<SubFileStatus>>,
    sub_dir_error: Mutex<Option<String>>,
    node_type_by_tag: Mutex<HashMap<String, String>>,
    dns_monitor: Mutex<DnsMonitorState>,
    proxy_monitor: Mutex<ProxyMonitorState>,
    setup_required: AtomicBool,
    tcp_tunnel: tcp_tunnel::TunnelManager,
    full_tunnel: full_tunnel::FullTunnelManager,
}

#[derive(Clone)]
enum SubSource {
    Path { value: String },
    Git { url: String },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum SubSourceResponse {
    Path { value: String },
    Git { url: String, workdir: String },
}

#[derive(Deserialize, Debug)]
struct IpCheckResponse {
    ip: String,
    location: String,
    #[serde(default)]
    xad: String,
}

#[derive(Serialize, Deserialize)]
struct Hysteria2 {
    #[serde(rename = "type")]
    outbound_type: String,
    tag: String,
    server: String,
    server_port: u16,
    password: String,
    up_mbps: u32,
    down_mbps: u32,
    tls: Tls,
}

#[derive(Serialize, Deserialize)]
struct Tls {
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_name: Option<String>,
    insecure: bool,
}

#[derive(Serialize, Deserialize)]
struct AnyTls {
    #[serde(rename = "type")]
    outbound_type: String,
    tag: String,
    server: String,
    server_port: u16,
    password: String,
    tls: Tls,
}

#[derive(Serialize, Deserialize)]
struct Shadowsocks {
    #[serde(rename = "type")]
    outbound_type: String,
    tag: String,
    server: String,
    server_port: u16,
    method: String,
    password: String,
}

// ============================================================================
// API Response Types
// ============================================================================

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    fn success(message: impl Into<String>, data: T) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data),
        }
    }

    fn success_no_data(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

// ============================================================================
// JWT Helper Functions
// ============================================================================

// 生成 JWT token
fn generate_token() -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: "admin".to_string(),
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_ref()),
    )
}

// 验证 JWT token
fn verify_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET.as_ref()),
        &Validation::default(),
    )
    .map(|data| data.claims)
}

#[derive(Serialize)]
struct StatusData {
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uptime_secs: Option<u64>,
}

#[derive(Serialize, Clone)]
struct ConnectivityResult {
    name: String,
    url: String,
    latency_ms: Option<u64>,
    success: bool,
}

#[derive(Serialize, Clone)]
struct SubFileStatus {
    file_name: String,
    file_path: String,
    loaded: bool,
    node_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct SubFilesResponse {
    sub_dir: String,
    sub_source: SubSourceResponse,
    files: Vec<SubFileStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct LoadedSubscriptions {
    files: Vec<SubFileStatus>,
    outbounds: Vec<serde_json::Value>,
    node_names: Vec<String>,
    dir_error: Option<String>,
}

#[derive(Deserialize)]
struct NodeRequest {
    node_type: Option<String>,
    tag: String,
    server: String,
    server_port: u16,
    #[serde(default)]
    user: Option<String>,
    password: String,
    #[serde(default)]
    sni: Option<String>,
    #[serde(default)]
    cipher: Option<String>,
}

#[derive(Deserialize)]
struct NodeUpdateRequest {
    #[serde(default)]
    node_type: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    server_port: Option<u16>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    sni: Option<String>,
    #[serde(default)]
    cipher: Option<String>,
}

#[derive(Deserialize)]
struct DeleteNodeRequest {
    tag: String,
}

#[derive(Serialize)]
struct NodeInfo {
    node_type: String,
    tag: String,
    server: String,
    server_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    sni: Option<String>,
}

#[derive(Serialize)]
struct NodeDetailResponse {
    node_type: String,
    tag: String,
    server: String,
    server_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    sni: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cipher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

#[derive(Deserialize)]
struct NodeTestRequest {
    server: String,
    server_port: u16,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Serialize)]
struct NodeTestResponse {
    latency_ms: u128,
}

// ============================================================================
// Global State
// ============================================================================

struct SingBoxProcess {
    child: tokio::process::Child,
    started_at: Instant,
}

lazy_static! {
    static ref SING_PROCESS: Mutex<Option<SingBoxProcess>> = Mutex::new(None);
}

// ============================================================================
// API Handlers
// ============================================================================

async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../public/index.html"))
}

async fn serve_icon() -> impl axum::response::IntoResponse {
    (
        [("content-type", "image/svg+xml")],
        include_str!("../public/icon.svg"),
    )
}

/// POST /api/login - User login
async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Json<ApiResponse<LoginResponse>> {
    if state.setup_required.load(Ordering::Relaxed) {
        return Json(ApiResponse {
            success: false,
            message: "未初始化，请先完成初始化设置".to_string(),
            data: None,
        });
    }

    let config = state.config.lock().await;

    // 获取配置中的密码，如果未设置则使用默认密码 "admin"
    let expected_password = config.password.as_deref().unwrap_or("admin");

    // 验证密码
    if req.password != expected_password {
        return Json(ApiResponse {
            success: false,
            message: "密码错误".to_string(),
            data: None,
        });
    }

    // 生成 token
    match generate_token() {
        Ok(token) => Json(ApiResponse {
            success: true,
            message: "登录成功".to_string(),
            data: Some(LoginResponse { token }),
        }),
        Err(_) => Json(ApiResponse {
            success: false,
            message: "生成 token 失败".to_string(),
            data: None,
        }),
    }
}

/// GET /api/status - Get sing-box running status
async fn get_status() -> Json<ApiResponse<StatusData>> {
    let mut lock = SING_PROCESS.lock().await;

    let (running, pid, uptime_secs) = if let Some(ref mut proc) = *lock {
        match proc.child.try_wait() {
            Ok(Some(_)) => {
                *lock = None;
                (false, None, None)
            }
            Ok(None) => {
                let uptime = proc.started_at.elapsed().as_secs();
                (true, proc.child.id(), Some(uptime))
            }
            Err(_) => (false, None, None),
        }
    } else {
        (false, None, None)
    };

    Json(ApiResponse::success(
        if running { "running" } else { "stopped" },
        StatusData {
            running,
            pid,
            uptime_secs,
        },
    ))
}

/// POST /api/service/start - Start sing-box
async fn start_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let mut lock = SING_PROCESS.lock().await;

    if let Some(ref mut proc) = *lock {
        if proc.child.try_wait().ok().flatten().is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("sing-box is already running")),
            ));
        }
    }

    drop(lock);

    match start_sing_internal(&state.sing_box_home).await {
        Ok(_) => {
            let config = state.config.lock().await;
            let _ = apply_saved_selections(&config).await;
            Ok(Json(ApiResponse::success_no_data(
                "sing-box started successfully",
            )))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Failed to start: {}", e))),
        )),
    }
}

/// POST /api/service/stop - Stop sing-box
async fn stop_service() -> Json<ApiResponse<()>> {
    stop_sing_internal().await;
    Json(ApiResponse::success_no_data("sing-box stopped"))
}

/// POST /api/connectivity - Test connectivity to a single site
#[derive(Deserialize)]
struct ConnectivityRequest {
    url: String,
}

async fn test_connectivity(
    Json(req): Json<ConnectivityRequest>,
) -> Json<ApiResponse<ConnectivityResult>> {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Json(ApiResponse::error(format!("Failed to create client: {}", e)));
        }
    };

    let start = Instant::now();
    let result = match client.head(&req.url).send().await {
        Ok(_) => ConnectivityResult {
            name: String::new(),
            url: req.url,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            success: true,
        },
        Err(_) => ConnectivityResult {
            name: String::new(),
            url: req.url,
            latency_ms: None,
            success: false,
        },
    };

    Json(ApiResponse::success("Test completed", result))
}

// ============================================================================
// Setup APIs (first run)
// ============================================================================

async fn setup_status(State(state): State<Arc<AppState>>) -> Json<ApiResponse<SetupStatusResponse>> {
    Json(ApiResponse::success(
        "Setup status",
        SetupStatusResponse {
            initialized: !state.setup_required.load(Ordering::Relaxed),
        },
    ))
}

async fn setup_init(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetupInitRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    if !state.setup_required.load(Ordering::Relaxed) {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiResponse::error("Already initialized")),
        ));
    }

    let password = req.password.trim();
    if password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Password is required")),
        ));
    }

    let mut new_config = {
        let config = state.config.lock().await;
        let mut c = config.clone();
        c.password = Some(password.to_string());
        c.nodes = vec![];
        c.selections = HashMap::new();
        c
    };
    new_config.port = Some(DEFAULT_PORT);

    if let Err(e) = save_config(&new_config).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Failed to save config: {}", e))),
        ));
    }

    {
        let mut config = state.config.lock().await;
        *config = new_config;
    }
    state.setup_required.store(false, Ordering::Relaxed);

    // Best-effort generate config and start sing-box in background (may fail if no nodes exist yet)
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = regenerate_and_restart(state_clone).await {
            eprintln!("Background regenerate failed after setup: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data("Initialized")))
}

// ============================================================================
// Clash API Proxy (HTTP + WebSocket)
// ============================================================================

const CLASH_HTTP_BASE: &str = "http://127.0.0.1:6262";
const CLASH_WS_BASE: &str = "ws://127.0.0.1:6262";

async fn clash_get_proxies() -> Result<Json<ApiResponse<serde_json::Value>>, (StatusCode, Json<ApiResponse<()>>)> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/proxies", CLASH_HTTP_BASE))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(ApiResponse::error(format!("Clash API request failed: {}", e)))))?;

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(ApiResponse::error(format!("Clash API parse failed: {}", e)))))?;

    Ok(Json(ApiResponse::success("Clash proxies", json)))
}

async fn clash_switch_selector(
    client: &reqwest::Client,
    group: &str,
    name: &str,
) -> Result<(), String> {
    let resp = client
        .put(format!(
            "{}/proxies/{}",
            CLASH_HTTP_BASE,
            percent_encoding::utf8_percent_encode(group, percent_encoding::NON_ALPHANUMERIC)
        ))
        .json(&ClashSwitchRequest {
            name: name.to_string(),
        })
        .send()
        .await
        .map_err(|e| format!("Clash API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Clash API returned {}", resp.status()));
    }
    Ok(())
}

async fn clash_get_selector_choices(
    client: &reqwest::Client,
    group: &str,
) -> Result<Vec<String>, String> {
    let resp = client
        .get(format!(
            "{}/proxies/{}",
            CLASH_HTTP_BASE,
            percent_encoding::utf8_percent_encode(group, percent_encoding::NON_ALPHANUMERIC)
        ))
        .send()
        .await
        .map_err(|e| format!("Clash API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Clash API returned {}", resp.status()));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Clash API parse failed: {}", e))?;

    let mut choices = Vec::new();
    if let Some(all) = json.get("all").and_then(|v| v.as_array()) {
        for item in all {
            if let Some(s) = item.as_str() {
                choices.push(s.to_string());
            }
        }
    }
    Ok(choices)
}

async fn clash_switch_selector_resilient(
    client: &reqwest::Client,
    group: &str,
    desired: &str,
) -> Result<(), String> {
    if clash_switch_selector(client, group, desired).await.is_ok() {
        return Ok(());
    }
    let choices = clash_get_selector_choices(client, group).await?;
    if let Some(actual) = choices
        .into_iter()
        .find(|c| c.eq_ignore_ascii_case(desired))
    {
        return clash_switch_selector(client, group, &actual).await;
    }
    Err(format!("No matching choice for {}", desired))
}

async fn clash_switch_proxy(
    State(state): State<Arc<AppState>>,
    Path(group): Path<String>,
    Json(req): Json<ClashSwitchRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    switch_selector_and_save(&state, &group, &req.name)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(ApiResponse::error(e))))?;
    Ok(Json(ApiResponse::success_no_data("Switched")))
}

#[derive(Deserialize)]
struct DelayQuery {
    timeout: Option<u32>,
    url: Option<String>,
}

async fn clash_test_delay(
    Path(node): Path<String>,
    Query(q): Query<DelayQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, (StatusCode, Json<ApiResponse<()>>)> {
    let client = reqwest::Client::new();
    let mut req = client.get(format!(
        "{}/proxies/{}/delay",
        CLASH_HTTP_BASE,
        percent_encoding::utf8_percent_encode(&node, percent_encoding::NON_ALPHANUMERIC)
    ));
    if let Some(timeout) = q.timeout {
        req = req.query(&[("timeout", timeout.to_string())]);
    }
    if let Some(url) = q.url {
        req = req.query(&[("url", url)]);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(ApiResponse::error(format!("Clash API request failed: {}", e)))))?;

    if !resp.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiResponse::error(format!("Clash API returned {}", resp.status()))),
        ));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(ApiResponse::error(format!("Clash API parse failed: {}", e)))))?;

    Ok(Json(ApiResponse::success("Delay", json)))
}

async fn get_selections(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<SelectionsResponse>> {
    let config = state.config.lock().await;
    Json(ApiResponse::success(
        "Selections",
        SelectionsResponse {
            selections: config.selections.clone(),
        },
    ))
}

async fn clash_ws_traffic(
    Query(q): Query<WsAuthQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    if verify_token(&q.token).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(ws.on_upgrade(|socket| proxy_websocket(socket, format!("{}/traffic", CLASH_WS_BASE))))
}

async fn clash_ws_logs(
    Query(q): Query<WsAuthQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    if verify_token(&q.token).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let level = q.level.unwrap_or_else(|| "info".to_string());
    Ok(ws.on_upgrade(move |socket| {
        proxy_websocket(
            socket,
            format!(
                "{}/logs?level={}",
                CLASH_WS_BASE,
                percent_encoding::utf8_percent_encode(&level, percent_encoding::NON_ALPHANUMERIC)
            ),
        )
    }))
}

async fn proxy_websocket(mut client_socket: WebSocket, upstream_url: String) {
    let upstream = connect_async(&upstream_url).await;
    let (upstream_ws, _) = match upstream {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to connect upstream websocket {}: {}", upstream_url, e);
            let _ = client_socket.close().await;
            return;
        }
    };

    let (mut client_tx, mut client_rx) = client_socket.split();
    let (mut upstream_tx, mut upstream_rx) = upstream_ws.split();

    let client_to_upstream = async {
        while let Some(Ok(msg)) = client_rx.next().await {
            match msg {
                Message::Text(t) => {
                    if upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            t.to_string(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Message::Binary(b) => {
                    if upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            b.to_vec(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Message::Ping(b) => {
                    let _ = upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Ping(
                            b.to_vec(),
                        ))
                        .await;
                }
                Message::Pong(b) => {
                    let _ = upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Pong(
                            b.to_vec(),
                        ))
                        .await;
                }
                Message::Close(_) => {
                    let _ = upstream_tx
                        .send(tokio_tungstenite::tungstenite::Message::Close(None))
                        .await;
                    break;
                }
            }
        }
    };

    let upstream_to_client = async {
        while let Some(msg) = upstream_rx.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => {
                    if client_tx.send(Message::Text(t.into())).await.is_err() {
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Binary(b)) => {
                    if client_tx
                        .send(Message::Binary(axum::body::Bytes::from(b)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Ping(b)) => {
                    let _ = client_tx
                        .send(Message::Ping(axum::body::Bytes::from(b)))
                        .await;
                }
                Ok(tokio_tungstenite::tungstenite::Message::Pong(b)) => {
                    let _ = client_tx
                        .send(Message::Pong(axum::body::Bytes::from(b)))
                        .await;
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    let _ = client_tx.send(Message::Close(None)).await;
                    break;
                }
                Err(e) => {
                    eprintln!("Upstream websocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    };

    tokio::select! {
        _ = client_to_upstream => {},
        _ = upstream_to_client => {},
    }
}

// ============================================================================
// Version and Upgrade APIs
// ============================================================================

#[derive(Serialize)]
struct VersionInfo {
    current: String,
    latest: Option<String>,
    has_update: bool,
    download_url: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Parse version string like "v0.6.10" into comparable tuple
fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// Compare two version strings, returns true if `latest` is newer than `current`
fn is_newer_version(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// GET /api/version - Get current version and check for updates
async fn get_version() -> Json<ApiResponse<VersionInfo>> {
    let current = format!("v{}", VERSION);

    // Try to fetch latest version from GitHub
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(_) => {
            return Json(ApiResponse::success("Version info", VersionInfo {
                current,
                latest: None,
                has_update: false,
                download_url: None,
            }));
        }
    };

    let resp = client
        .get("https://api.github.com/repos/xiechengqi/miao/releases/latest")
        .header("User-Agent", "miao")
        .send()
        .await;

    match resp {
        Ok(r) => {
            if let Ok(release) = r.json::<GitHubRelease>().await {
                let latest = release.tag_name.clone();
                let has_update = is_newer_version(&current, &latest);

                // Find download URL for current architecture
                let asset_name = if cfg!(target_arch = "x86_64") {
                    "miao-rust-linux-amd64"
                } else if cfg!(target_arch = "aarch64") {
                    "miao-rust-linux-arm64"
                } else {
                    ""
                };

                let download_url = release.assets.iter()
                    .find(|a| a.name == asset_name)
                    .map(|a| a.browser_download_url.clone());

                Json(ApiResponse::success("Version info", VersionInfo {
                    current,
                    latest: Some(latest),
                    has_update,
                    download_url,
                }))
            } else {
                Json(ApiResponse::success("Version info", VersionInfo {
                    current,
                    latest: None,
                    has_update: false,
                    download_url: None,
                }))
            }
        }
        Err(_) => {
            Json(ApiResponse::success("Version info", VersionInfo {
                current,
                latest: None,
                has_update: false,
                download_url: None,
            }))
        }
    }
}

/// POST /api/upgrade - Download and apply upgrade
async fn upgrade() -> Json<ApiResponse<String>> {
    // 1. Fetch latest release info
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build() {
        Ok(c) => c,
        Err(e) => return Json(ApiResponse::error(format!("Failed to create HTTP client: {}", e))),
    };

    let release: GitHubRelease = match client
        .get("https://api.github.com/repos/xiechengqi/miao/releases/latest")
        .header("User-Agent", "miao")
        .send()
        .await {
        Ok(r) => match r.json().await {
            Ok(rel) => rel,
            Err(e) => return Json(ApiResponse::error(format!("Failed to parse release info: {}", e))),
        },
        Err(e) => return Json(ApiResponse::error(format!("Failed to fetch release info: {}", e))),
    };

    // 移除版本检查，允许强制更新到 latest
    // 这样即使当前版本与 latest 相同也可以重新安装

    // 2. Find download URL for current architecture
    let asset_name = if cfg!(target_arch = "x86_64") {
        "miao-rust-linux-amd64"
    } else if cfg!(target_arch = "aarch64") {
        "miao-rust-linux-arm64"
    } else {
        return Json(ApiResponse::error("Unsupported architecture"));
    };

    let download_url = match release.assets.iter().find(|a| a.name == asset_name) {
        Some(a) => a.browser_download_url.clone(),
        None => return Json(ApiResponse::error("No binary found for current architecture")),
    };

    // 3. Download new binary to temp location
    println!("Downloading update from: {}", download_url);
    let binary_data = match client.get(&download_url).send().await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => return Json(ApiResponse::error(format!("Failed to download binary: {}", e))),
        },
        Err(e) => return Json(ApiResponse::error(format!("Failed to download: {}", e))),
    };

    let temp_path = "/tmp/miao-new";
    if let Err(e) = fs::write(temp_path, &binary_data) {
        return Json(ApiResponse::error(format!("Failed to write temp file: {}", e)));
    }

    // 4. Make it executable
    if let Err(e) = fs::set_permissions(temp_path, fs::Permissions::from_mode(0o755)) {
        return Json(ApiResponse::error(format!("Failed to set permissions: {}", e)));
    }

    // 5. Verify the new binary can run
    let verify = tokio::process::Command::new(temp_path)
        .arg("--help")
        .output()
        .await;

    if verify.is_err() {
        let _ = fs::remove_file(temp_path);
        return Json(ApiResponse::error("New binary verification failed"));
    }

    // 6. Get current executable path
    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return Json(ApiResponse::error(format!("Failed to get current exe path: {}", e))),
    };

    // 7. Stop sing-box before replacing and wait for it to exit
    println!("Stopping sing-box before upgrade...");
    stop_sing_internal_and_wait().await;

    // 8. Backup current binary (must succeed)
    let backup_path = format!("{}.bak", current_exe.display());
    if let Err(e) = fs::copy(&current_exe, &backup_path) {
        return Json(ApiResponse::error(format!("Failed to backup current binary: {}", e)));
    }

    // 9. Replace binary: delete first then copy (Linux allows deleting running executables)
    if let Err(e) = fs::remove_file(&current_exe) {
        return Json(ApiResponse::error(format!("Failed to remove old binary: {}", e)));
    }
    if let Err(e) = fs::copy(temp_path, &current_exe) {
        // Try to restore from backup
        let _ = fs::copy(&backup_path, &current_exe);
        return Json(ApiResponse::error(format!("Failed to copy new binary: {}", e)));
    }
    // Set executable permission
    if let Err(e) = fs::set_permissions(&current_exe, fs::Permissions::from_mode(0o755)) {
        // Try to restore from backup
        let _ = fs::remove_file(&current_exe);
        let _ = fs::copy(&backup_path, &current_exe);
        return Json(ApiResponse::error(format!("Failed to set permissions: {}", e)));
    }
    let _ = fs::remove_file(temp_path);

    println!("Upgrade successful! Restarting...");

    // 10. Restart:
    // - Prefer systemd restart (when deployed as a service)
    // - Fallback to exec() restart (for non-systemd environments / failures)
    let new_version = release.tag_name.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(500)).await;

        if try_restart_systemd("miao").await.is_ok() {
            return;
        }

        use std::os::unix::process::CommandExt;
        let args: Vec<String> = std::env::args().collect();
        let err = std::process::Command::new(&current_exe).args(&args[1..]).exec();

        // exec() only returns if there's an error, try to restore from backup
        eprintln!("Failed to exec new binary: {}", err);
        eprintln!("Attempting to restore from backup...");

        if fs::remove_file(&current_exe).is_ok() {
            if fs::copy(&backup_path, &current_exe).is_ok() {
                let _ = fs::set_permissions(&current_exe, fs::Permissions::from_mode(0o755));
                eprintln!("Restored from backup, restarting with old version...");
                let _ = std::process::Command::new(&current_exe).args(&args[1..]).exec();
            }
        }
        eprintln!("Failed to restore from backup, manual intervention required");
        std::process::exit(1);
    });

    Json(ApiResponse::success("Upgrade complete, restarting...", new_version))
}

async fn try_restart_systemd(unit: &str) -> Result<(), String> {
    // Prefer running the restart from a separate transient unit so it won't be killed when the
    // current service cgroup is stopped.
    let transient_unit = format!("miao-upgrade-restart-{}", std::process::id());
    let script = format!("sleep 0.5; systemctl restart {}", unit);

    let via_systemd_run = tokio::process::Command::new("systemd-run")
        .arg("--unit")
        .arg(&transient_unit)
        .arg("--property=Type=oneshot")
        .arg("--collect")
        .arg("/bin/sh")
        .arg("-c")
        .arg(&script)
        .output()
        .await;

    match via_systemd_run {
        Ok(out) if out.status.success() => return Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("systemd-run restart failed: {}", stderr.trim());
        }
        Err(e) => {
            eprintln!("systemd-run not available/failed: {}", e);
        }
    }

    // Fallback: direct systemctl (may still work, but can be killed if the unit stops quickly)
    let direct = tokio::process::Command::new("systemctl")
        .arg("restart")
        .arg(unit)
        .arg("--no-block")
        .output()
        .await
        .map_err(|e| format!("systemctl restart failed: {}", e))?;

    if direct.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&direct.stderr);
        Err(format!(
            "systemctl restart exited with {}: {}",
            direct.status,
            stderr.trim()
        ))
    }
}

// ============================================================================
// Subscription File Management APIs
// ============================================================================

/// GET /api/sub-files - Get all loaded subscription files
async fn get_sub_files(State(state): State<Arc<AppState>>) -> Json<ApiResponse<SubFilesResponse>> {
    let files = state.sub_files.lock().await.clone();
    let error = state.sub_dir_error.lock().await.clone();
    let sub_source = match &state.sub_source {
        SubSource::Path { value } => SubSourceResponse::Path {
            value: value.clone(),
        },
        SubSource::Git { url } => SubSourceResponse::Git {
            url: url.clone(),
            workdir: "sub".to_string(),
        },
    };
    Json(ApiResponse::success(
        "Subscription files loaded",
        SubFilesResponse {
            sub_dir: state.sub_dir.display().to_string(),
            sub_source,
            files,
            error,
        },
    ))
}

/// POST /api/sub-files/reload - Reload subscription files and restart
async fn reload_sub_files(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    match regenerate_and_restart(state).await {
        Ok(_) => Ok(Json(ApiResponse::success_no_data(
            "Subscription files reloaded and sing-box restarted",
        ))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e)),
        )),
    }
}

// ============================================================================
// Node Management APIs
// ============================================================================

/// GET /api/nodes - Get all manual nodes
async fn get_nodes(State(state): State<Arc<AppState>>) -> Json<ApiResponse<Vec<NodeInfo>>> {
    let config = state.config.lock().await;

    let nodes: Vec<NodeInfo> = config
        .nodes
        .iter()
        .filter_map(|s| {
            serde_json::from_str::<serde_json::Value>(s).ok().map(|v| NodeInfo {
                node_type: v.get("type").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                tag: v.get("tag").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                server: v.get("server").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                server_port: v.get("server_port").and_then(|p| p.as_u64()).unwrap_or(0) as u16,
                sni: v
                    .get("tls")
                    .and_then(|t| t.get("server_name"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string()),
            })
        })
        .collect();

    Json(ApiResponse::success("Nodes loaded", nodes))
}

/// GET /api/nodes/{tag} - Get a manual node detail (without password)
async fn get_node(
    State(state): State<Arc<AppState>>,
    Path(tag): Path<String>,
) -> Result<Json<ApiResponse<NodeDetailResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let config = state.config.lock().await;
    for node_str in &config.nodes {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(node_str) else {
            continue;
        };
        if v.get("tag").and_then(|t| t.as_str()) != Some(tag.as_str()) {
            continue;
        }

        let node_type = v
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown")
            .to_string();
        let server = v
            .get("server")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let server_port = v
            .get("server_port")
            .and_then(|p| p.as_u64())
            .unwrap_or(0) as u16;
        let sni = v
            .get("tls")
            .and_then(|t| t.get("server_name"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let cipher = v
            .get("method")
            .and_then(|m| m.as_str())
            .map(|m| m.to_string());
        let user = v
            .get("user")
            .and_then(|u| u.as_str())
            .map(|u| u.to_string());

        return Ok(Json(ApiResponse::success(
            "Node detail",
            NodeDetailResponse {
                node_type,
                tag,
                server,
                server_port,
                sni,
                cipher,
                user,
            },
        )));
    }

    Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Node not found"))))
}

/// POST /api/nodes - Add a node (Hysteria2/AnyTLS/Shadowsocks)
async fn add_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NodeRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;

        // Check if tag already exists
        for node_str in &config.nodes {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(node_str) {
                if v.get("tag").and_then(|t| t.as_str()) == Some(&req.tag) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ApiResponse::error("Node with this tag already exists")),
                    ));
                }
            }
        }

        // Build node based on type
        let node_type = req.node_type.as_deref().unwrap_or("hysteria2");
        let node_json = match node_type {
            "ssh" => {
                let mut node = serde_json::Map::new();
                node.insert("type".to_string(), serde_json::Value::String("ssh".to_string()));
                node.insert("tag".to_string(), serde_json::Value::String(req.tag));
                node.insert("server".to_string(), serde_json::Value::String(req.server));
                node.insert(
                    "server_port".to_string(),
                    serde_json::Value::Number(
                        u64::from(if req.server_port == 0 { 22 } else { req.server_port }).into(),
                    ),
                );
                if let Some(user) = req.user {
                    if !user.is_empty() {
                        node.insert("user".to_string(), serde_json::Value::String(user));
                    }
                }
                node.insert("password".to_string(), serde_json::Value::String(req.password));
                serde_json::to_string(&serde_json::Value::Object(node))
            }
            "anytls" => {
                let node = AnyTls {
                    outbound_type: "anytls".to_string(),
                    tag: req.tag,
                    server: req.server,
                    server_port: req.server_port,
                    password: req.password,
                    tls: Tls {
                        enabled: true,
                        server_name: req.sni,
                        insecure: true,
                    },
                };
                serde_json::to_string(&node)
            }
            "ss" => {
                let node = Shadowsocks {
                    outbound_type: "shadowsocks".to_string(),
                    tag: req.tag,
                    server: req.server,
                    server_port: req.server_port,
                    method: req.cipher.unwrap_or_else(|| "2022-blake3-aes-128-gcm".to_string()),
                    password: req.password,
                };
                serde_json::to_string(&node)
            }
            _ => {
                // Default to Hysteria2
                let node = Hysteria2 {
                    outbound_type: "hysteria2".to_string(),
                    tag: req.tag,
                    server: req.server,
                    server_port: req.server_port,
                    password: req.password,
                    up_mbps: 40,
                    down_mbps: 350,
                    tls: Tls {
                        enabled: true,
                        server_name: req.sni,
                        insecure: true,
                    },
                };
                serde_json::to_string(&node)
            }
        }.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to serialize node: {}", e))),
            )
        })?;

        config.nodes.push(node_json);

        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = regenerate_and_restart(state_clone).await {
            eprintln!("Background regenerate failed: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data("Node added, restarting...")))
}

/// PUT /api/nodes/{tag} - Update a manual node by tag (password optional)
async fn update_node(
    State(state): State<Arc<AppState>>,
    Path(original_tag): Path<String>,
    Json(req): Json<NodeUpdateRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;

        let mut found_index: Option<usize> = None;
        let mut existing: Option<serde_json::Value> = None;
        for (idx, node_str) in config.nodes.iter().enumerate() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(node_str) {
                if v.get("tag").and_then(|t| t.as_str()) == Some(original_tag.as_str()) {
                    found_index = Some(idx);
                    existing = Some(v);
                    break;
                }
            }
        }
        let Some(found_index) = found_index else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Node not found"))));
        };
        let existing = existing.unwrap_or(serde_json::Value::Null);

        let existing_type = existing
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("hysteria2");
        let node_type = req.node_type.as_deref().unwrap_or(existing_type);

        let new_tag = req
            .tag
            .clone()
            .unwrap_or_else(|| original_tag.clone());
        if new_tag != original_tag {
            for (idx, node_str) in config.nodes.iter().enumerate() {
                if idx == found_index {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(node_str) {
                    if v.get("tag").and_then(|t| t.as_str()) == Some(new_tag.as_str()) {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ApiResponse::error("Node with this tag already exists")),
                        ));
                    }
                }
            }
        }

        let server = req
            .server
            .clone()
            .or_else(|| {
                existing
                    .get("server")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        let server_port = req
            .server_port
            .or_else(|| existing.get("server_port").and_then(|p| p.as_u64()).map(|p| p as u16))
            .unwrap_or(0);

        let password = req.password.clone().and_then(|p| {
            if p.is_empty() {
                None
            } else {
                Some(p)
            }
        });

        let existing_password = existing
            .get("password")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
        let password = password.unwrap_or(existing_password);

        let node_json = match node_type {
            "ssh" => {
                let mut node = serde_json::Map::new();
                node.insert("type".to_string(), serde_json::Value::String("ssh".to_string()));
                node.insert("tag".to_string(), serde_json::Value::String(new_tag));
                node.insert("server".to_string(), serde_json::Value::String(server));
                node.insert(
                    "server_port".to_string(),
                    serde_json::Value::Number(u64::from(if server_port == 0 { 22 } else { server_port }).into()),
                );
                let user = req.user.clone().and_then(|u| if u.is_empty() { None } else { Some(u) }).or_else(|| {
                    existing.get("user").and_then(|u| u.as_str()).map(|u| u.to_string())
                });
                if let Some(user) = user {
                    node.insert("user".to_string(), serde_json::Value::String(user));
                }
                node.insert("password".to_string(), serde_json::Value::String(password));
                serde_json::to_string(&serde_json::Value::Object(node))
            }
            "anytls" => {
                let sni = req
                    .sni
                    .clone()
                    .or_else(|| {
                        existing
                            .get("tls")
                            .and_then(|t| t.get("server_name"))
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string())
                    });
                let node = AnyTls {
                    outbound_type: "anytls".to_string(),
                    tag: new_tag,
                    server,
                    server_port,
                    password,
                    tls: Tls {
                        enabled: true,
                        server_name: sni,
                        insecure: true,
                    },
                };
                serde_json::to_string(&node)
            }
            "ss" => {
                let method = req
                    .cipher
                    .clone()
                    .or_else(|| existing.get("method").and_then(|m| m.as_str()).map(|m| m.to_string()))
                    .unwrap_or_else(|| "2022-blake3-aes-128-gcm".to_string());
                let node = Shadowsocks {
                    outbound_type: "shadowsocks".to_string(),
                    tag: new_tag,
                    server,
                    server_port,
                    method,
                    password,
                };
                serde_json::to_string(&node)
            }
            _ => {
                let sni = req
                    .sni
                    .clone()
                    .or_else(|| {
                        existing
                            .get("tls")
                            .and_then(|t| t.get("server_name"))
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string())
                    });
                let node = Hysteria2 {
                    outbound_type: "hysteria2".to_string(),
                    tag: new_tag,
                    server,
                    server_port,
                    password,
                    up_mbps: existing.get("up_mbps").and_then(|v| v.as_u64()).unwrap_or(40) as u32,
                    down_mbps: existing.get("down_mbps").and_then(|v| v.as_u64()).unwrap_or(350) as u32,
                    tls: Tls {
                        enabled: true,
                        server_name: sni,
                        insecure: true,
                    },
                };
                serde_json::to_string(&node)
            }
        }
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to serialize node: {}", e))),
            )
        })?;

        config.nodes[found_index] = node_json;
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = regenerate_and_restart(state_clone).await {
            eprintln!("Background regenerate failed: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data("Node updated, restarting...")))
}

/// DELETE /api/nodes - Delete a node by tag
async fn delete_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeleteNodeRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;

        let original_len = config.nodes.len();
        config.nodes.retain(|node_str| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(node_str) {
                v.get("tag").and_then(|t| t.as_str()) != Some(&req.tag)
            } else {
                true
            }
        });

        if config.nodes.len() == original_len {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("Node not found")),
            ));
        }

        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = regenerate_and_restart(state_clone).await {
            eprintln!("Background regenerate failed: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data("Node deleted, restarting...")))
}

/// POST /api/node-test - Test a node connectivity (TCP connect only)
async fn test_node(
    Json(req): Json<NodeTestRequest>,
) -> Result<Json<ApiResponse<NodeTestResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let timeout_ms = req.timeout_ms.unwrap_or(3000);
    let addr = format!("{}:{}", req.server, req.server_port);

    let started = Instant::now();
    let connect = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        tokio::net::TcpStream::connect(addr),
    )
    .await;

    match connect {
        Ok(Ok(stream)) => {
            drop(stream);
            Ok(Json(ApiResponse::success(
                "Connected",
                NodeTestResponse {
                    latency_ms: started.elapsed().as_millis(),
                },
            )))
        }
        Ok(Err(e)) => Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiResponse::error(format!("Connect failed: {}", e))),
        )),
        Err(_) => Err((
            StatusCode::GATEWAY_TIMEOUT,
            Json(ApiResponse::error("Connect timeout")),
        )),
    }
}

async fn get_dns_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<DnsStatusResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let (stored_active, raw_candidates, interval_ms, fail_threshold, cooldown_ms) = {
        let config = state.config.lock().await;
        (
            config
                .dns_active
                .clone()
                .unwrap_or_else(|| DEFAULT_DNS_ACTIVE.to_string()),
            config
                .dns_candidates
                .clone()
                .unwrap_or_else(default_dns_candidates),
            config.dns_check_interval_ms.unwrap_or(180_000),
            config.dns_fail_threshold.unwrap_or(3),
            config.dns_cooldown_ms.unwrap_or(300_000),
        )
    };

    let candidates = normalize_dns_candidates(raw_candidates);
    let active = sanitize_dns_active(&stored_active);

    let now = Instant::now();
    let monitor = state.dns_monitor.lock().await;
    let last_check_secs_ago = monitor
        .last_check_at
        .map(|t| now.saturating_duration_since(t).as_secs());

    let mut health: HashMap<String, DnsHealthPublic> = HashMap::new();
    for tag in candidates.iter() {
        let entry = monitor.health.get(tag).cloned().unwrap_or_default();
        let cooldown_remaining_secs = entry
            .cooldown_until
            .and_then(|t| t.checked_duration_since(now).map(|d| d.as_secs()))
            .unwrap_or(0);
        let last_checked_secs_ago = entry
            .last_checked_at
            .map(|t| now.saturating_duration_since(t).as_secs());
        health.insert(
            tag.clone(),
            DnsHealthPublic {
                ok: entry.ok,
                failures: entry.failures,
                cooldown_remaining_secs,
                last_error: entry.last_error,
                last_checked_secs_ago,
            },
        );
    }

    Ok(Json(ApiResponse::success(
        "DNS status",
        DnsStatusResponse {
            active,
            candidates,
            interval_ms,
            fail_threshold,
            cooldown_ms,
            health,
            last_check_secs_ago,
        },
    )))
}

async fn check_dns_now(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<DnsStatusResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let (stored_active, fail_threshold, cooldown_ms) = {
        let config = state.config.lock().await;
        (
            config
                .dns_active
                .clone()
                .unwrap_or_else(|| DEFAULT_DNS_ACTIVE.to_string()),
            config.dns_fail_threshold.unwrap_or(3),
            config.dns_cooldown_ms.unwrap_or(300_000),
        )
    };
    let active = sanitize_dns_active(&stored_active);
    let candidates = vec![active];
    let _ = run_dns_checks(
        &state,
        &candidates,
        fail_threshold,
        cooldown_ms,
        Duration::from_millis(2_500),
    )
    .await;
    get_dns_status(State(state)).await
}

async fn switch_dns_active(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DnsSwitchRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let raw_candidates = {
        let config = state.config.lock().await;
        config
            .dns_candidates
            .clone()
            .unwrap_or_else(default_dns_candidates)
    };
    let candidates = normalize_dns_candidates(raw_candidates);

    if !candidates.iter().any(|c| c == &req.tag) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Unknown DNS tag")),
        ));
    }

    {
        let mut config = state.config.lock().await;
        config.dns_active = Some(req.tag.clone());
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    if is_sing_running().await {
        regenerate_and_restart(state.clone()).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to restart: {}", e))),
            )
        })?;
    }

    Ok(Json(ApiResponse::success_no_data("DNS switched")))
}

async fn get_proxy_status(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<ProxyStatusResponse>> {
    let (enabled, pool, active, interval_ms, timeout_ms, fail_threshold, window_size, window_fail_rate, pause_ms) = {
        let config = state.config.lock().await;
        (
            config.proxy_monitor_enabled.unwrap_or(true),
            config.proxy_pool.clone().unwrap_or_default(),
            config.selections.get("proxy").cloned(),
            config.proxy_check_interval_ms.unwrap_or(180_000),
            config.proxy_check_timeout_ms.unwrap_or(3_000),
            config.proxy_fail_threshold.unwrap_or(3),
            config.proxy_window_size.unwrap_or(10),
            config.proxy_window_fail_rate.unwrap_or(0.6),
            config.proxy_pause_ms.unwrap_or(60_000),
        )
    };

    let now = Instant::now();
    let monitor = state.proxy_monitor.lock().await;
    let last_check_secs_ago = monitor
        .last_check_at
        .map(|t| now.saturating_duration_since(t).as_secs());
    let paused_remaining_secs = monitor
        .paused_until
        .and_then(|t| t.checked_duration_since(now).map(|d| d.as_secs()))
        .unwrap_or(0);

    let mut health: HashMap<String, ProxyHealthPublic> = HashMap::new();
    for tag in pool.iter() {
        let entry = monitor.health.get(tag).cloned().unwrap_or_default();
        let last_checked_secs_ago = entry
            .last_checked_at
            .map(|t| now.saturating_duration_since(t).as_secs());
        let window_size_seen = entry.window.len();
        let window_fails = entry.window.iter().filter(|v| !**v).count() as u32;
        health.insert(
            tag.clone(),
            ProxyHealthPublic {
                ok: entry.ok,
                consecutive_failures: entry.consecutive_failures,
                window_fails,
                window_size: window_size_seen,
                last_error: entry.last_error,
                last_ip: entry.last_ip,
                last_location: entry.last_location,
                last_checked_secs_ago,
            },
        );
    }

    Json(ApiResponse::success(
        "Proxy status",
        ProxyStatusResponse {
            enabled,
            pool,
            active,
            interval_ms,
            timeout_ms,
            fail_threshold,
            window_size,
            window_fail_rate,
            pause_ms,
            paused_remaining_secs,
            health,
            last_check_secs_ago,
        },
    ))
}

async fn update_proxy_pool(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProxyPoolUpdateRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let pool = normalize_pool(&req.pool, 64);
    {
        let mut config = state.config.lock().await;
        config.proxy_pool = if pool.is_empty() { None } else { Some(pool.clone()) };
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    {
        let mut monitor = state.proxy_monitor.lock().await;
        monitor.paused_until = None;
        if pool.is_empty() {
            monitor.health.clear();
        } else {
            monitor.health.retain(|k, _| pool.iter().any(|n| n == k));
        }
    }

    if !pool.is_empty() && is_sing_running().await {
        let current_selected = { state.config.lock().await.selections.get("proxy").cloned() };
        let should_align = current_selected
            .as_ref()
            .map(|cur| !pool.iter().any(|n| n == cur))
            .unwrap_or(true);
        if should_align {
            if let Err(e) = switch_selector_and_save(&state, "proxy", &pool[0]).await {
                eprintln!("Failed to align active proxy after pool update: {}", e);
            }
        }
    }

    Ok(Json(ApiResponse::success_no_data("Proxy pool updated")))
}

fn redact_tunnel_auth(auth: &TcpTunnelAuth) -> TcpTunnelAuthPublic {
    match auth {
        TcpTunnelAuth::Password { .. } => TcpTunnelAuthPublic::Password,
        TcpTunnelAuth::PrivateKeyPath { path, .. } => TcpTunnelAuthPublic::PrivateKeyPath {
            path: path.clone(),
        },
    }
}

fn generate_tunnel_id() -> String {
    format!("t-{}", uuid::Uuid::new_v4())
}

fn normalize_tcp_tunnel(req: TcpTunnelUpsertRequest, id: String) -> Result<TcpTunnelConfig, String> {
    let local_addr = req.local_addr.unwrap_or_else(default_local_addr);
    let remote_bind_addr = req.remote_bind_addr.unwrap_or_else(default_remote_bind_addr);
    let ssh_port = req.ssh_port.unwrap_or_else(default_ssh_port);
    let strict_host_key_checking = req.strict_host_key_checking.unwrap_or(true);
    let host_key_fingerprint = req.host_key_fingerprint.unwrap_or_default();
    let allow_public_bind = req.allow_public_bind.unwrap_or(false);
    let connect_timeout_ms = req.connect_timeout_ms.unwrap_or_else(default_connect_timeout_ms);
    let keepalive_interval_ms = req
        .keepalive_interval_ms
        .unwrap_or_else(default_keepalive_interval_ms);
    let reconnect_backoff_ms = req.reconnect_backoff_ms.unwrap_or_else(default_tcp_tunnel_backoff);
    let enabled = req.enabled.unwrap_or(false);

    if req.remote_port == 0 {
        return Err("remote_port must be > 0".to_string());
    }
    if remote_bind_addr == "0.0.0.0" && !allow_public_bind {
        return Err("allow_public_bind must be true when remote_bind_addr is 0.0.0.0".to_string());
    }
    if strict_host_key_checking && host_key_fingerprint.trim().is_empty() {
        return Err("host_key_fingerprint is required when strict_host_key_checking is true".to_string());
    }

    Ok(TcpTunnelConfig {
        id,
        name: req.name,
        enabled,
        local_addr,
        local_port: req.local_port,
        remote_bind_addr,
        remote_port: req.remote_port,
        ssh_host: req.ssh_host,
        ssh_port,
        username: req.username,
        auth: req.auth,
        strict_host_key_checking,
        host_key_fingerprint,
        allow_public_bind,
        connect_timeout_ms,
        keepalive_interval_ms,
        reconnect_backoff_ms,
        managed_by: None,
    })
}

async fn apply_tunnels_from_config(state: &Arc<AppState>) {
    let tunnels = { state.config.lock().await.tcp_tunnels.clone() };
    state.tcp_tunnel.apply_config(&tunnels).await;
}

async fn apply_full_tunnel_sets_from_config(state: &Arc<AppState>) {
    let sets = { state.config.lock().await.tcp_tunnel_sets.clone() };
    state.full_tunnel.sync_from_config(state.clone(), sets).await;
}

fn build_tcp_tunnel_item(
    t: TcpTunnelConfig,
    status: tcp_tunnel::TunnelRuntimeStatus,
) -> TcpTunnelItem {
    TcpTunnelItem {
        id: t.id,
        name: t.name,
        enabled: t.enabled,
        local_addr: t.local_addr,
        local_port: t.local_port,
        remote_bind_addr: t.remote_bind_addr,
        remote_port: t.remote_port,
        ssh_host: t.ssh_host,
        ssh_port: t.ssh_port,
        username: t.username,
        auth: redact_tunnel_auth(&t.auth),
        strict_host_key_checking: t.strict_host_key_checking,
        host_key_fingerprint: t.host_key_fingerprint,
        allow_public_bind: t.allow_public_bind,
        connect_timeout_ms: t.connect_timeout_ms,
        keepalive_interval_ms: t.keepalive_interval_ms,
        reconnect_backoff_ms: t.reconnect_backoff_ms,
        status,
    }
}

async fn get_tcp_tunnels(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<TcpTunnelListResponse>> {
    let supported = state.tcp_tunnel.supported();
    let tunnels = { state.config.lock().await.tcp_tunnels.clone() };

    let mut items = Vec::with_capacity(tunnels.len());
    for t in tunnels {
        if matches!(&t.managed_by, Some(TcpTunnelManagedBy::FullTunnel { .. })) {
            continue;
        }
        let status = state
            .tcp_tunnel
            .get_status(&t.id)
            .await
            .unwrap_or_default();
        items.push(build_tcp_tunnel_item(t, status));
    }

    Json(ApiResponse::success(
        "TCP tunnels",
        TcpTunnelListResponse { supported, items },
    ))
}

async fn create_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TcpTunnelUpsertRequest>,
) -> Result<Json<ApiResponse<TcpTunnelItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let id = req.id.clone().unwrap_or_else(generate_tunnel_id);
    let cfg = normalize_tcp_tunnel(req, id.clone())
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    // Require secrets on create.
    match &cfg.auth {
        TcpTunnelAuth::Password { password } if password.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("password is required")),
            ));
        }
        TcpTunnelAuth::PrivateKeyPath { path, .. } if path.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("private key path is required")),
            ));
        }
        _ => {}
    }

    {
        let mut config = state.config.lock().await;
        if config.tcp_tunnels.iter().any(|t| t.id == id) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("Tunnel id already exists")),
            ));
        }
        config.tcp_tunnels.push(cfg.clone());
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    apply_tunnels_from_config(&state).await;

    let status = state.tcp_tunnel.get_status(&cfg.id).await.unwrap_or_default();
    Ok(Json(ApiResponse::success(
        "Tunnel created",
        TcpTunnelItem {
            id: cfg.id,
            name: cfg.name,
            enabled: cfg.enabled,
            local_addr: cfg.local_addr,
            local_port: cfg.local_port,
            remote_bind_addr: cfg.remote_bind_addr,
            remote_port: cfg.remote_port,
            ssh_host: cfg.ssh_host,
            ssh_port: cfg.ssh_port,
            username: cfg.username,
            auth: redact_tunnel_auth(&cfg.auth),
            strict_host_key_checking: cfg.strict_host_key_checking,
            host_key_fingerprint: cfg.host_key_fingerprint,
            allow_public_bind: cfg.allow_public_bind,
            connect_timeout_ms: cfg.connect_timeout_ms,
            keepalive_interval_ms: cfg.keepalive_interval_ms,
            reconnect_backoff_ms: cfg.reconnect_backoff_ms,
            status,
        },
    )))
}

async fn update_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<TcpTunnelUpsertRequest>,
) -> Result<Json<ApiResponse<TcpTunnelItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let existing = {
        let config = state.config.lock().await;
        config
            .tcp_tunnels
            .iter()
            .find(|t| t.id == id)
            .cloned()
    };
    let Some(existing) = existing else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
    };

    let mut cfg = normalize_tcp_tunnel(req, id.clone())
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    // Support "leave blank to keep unchanged" for secrets on update.
    match (&existing.auth, &mut cfg.auth) {
        (TcpTunnelAuth::Password { password: old }, TcpTunnelAuth::Password { password: new }) => {
            if new.is_empty() {
                *new = old.clone();
            }
        }
        (
            TcpTunnelAuth::PrivateKeyPath {
                path: old_path,
                passphrase: old_pass,
            },
            TcpTunnelAuth::PrivateKeyPath {
                path: new_path,
                passphrase: new_pass,
            },
        ) => {
            if new_path.is_empty() {
                *new_path = old_path.clone();
            }
            if new_pass.is_none() {
                *new_pass = old_pass.clone();
            }
        }
        _ => {}
    }

    // Require secrets after merge (important when switching auth types).
    match &cfg.auth {
        TcpTunnelAuth::Password { password } if password.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("password is required")),
            ));
        }
        TcpTunnelAuth::PrivateKeyPath { path, .. } if path.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("private key path is required")),
            ));
        }
        _ => {}
    }

    {
        let mut config = state.config.lock().await;
        let Some(pos) = config.tcp_tunnels.iter().position(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
        };
        config.tcp_tunnels[pos] = cfg.clone();
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    apply_tunnels_from_config(&state).await;
    let status = state.tcp_tunnel.get_status(&cfg.id).await.unwrap_or_default();
    Ok(Json(ApiResponse::success(
        "Tunnel updated",
        TcpTunnelItem {
            id: cfg.id,
            name: cfg.name,
            enabled: cfg.enabled,
            local_addr: cfg.local_addr,
            local_port: cfg.local_port,
            remote_bind_addr: cfg.remote_bind_addr,
            remote_port: cfg.remote_port,
            ssh_host: cfg.ssh_host,
            ssh_port: cfg.ssh_port,
            username: cfg.username,
            auth: redact_tunnel_auth(&cfg.auth),
            strict_host_key_checking: cfg.strict_host_key_checking,
            host_key_fingerprint: cfg.host_key_fingerprint,
            allow_public_bind: cfg.allow_public_bind,
            connect_timeout_ms: cfg.connect_timeout_ms,
            keepalive_interval_ms: cfg.keepalive_interval_ms,
            reconnect_backoff_ms: cfg.reconnect_backoff_ms,
            status,
        },
    )))
}

async fn delete_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let before = config.tcp_tunnels.len();
        config.tcp_tunnels.retain(|t| t.id != id);
        if config.tcp_tunnels.len() == before {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    apply_tunnels_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Tunnel deleted")))
}

async fn start_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let Some(t) = config.tcp_tunnels.iter_mut().find(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
        };
        t.enabled = true;
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_tunnels_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Tunnel started")))
}

async fn stop_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let Some(t) = config.tcp_tunnels.iter_mut().find(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
        };
        t.enabled = false;
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_tunnels_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Tunnel stopped")))
}

async fn restart_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let tunnel_cfg = {
        let mut config = state.config.lock().await;
        let Some(t) = config.tcp_tunnels.iter_mut().find(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
        };
        t.enabled = true;
        let cloned = t.clone();
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        cloned
    };
    // Restart with the latest config to guarantee "restart" also starts a previously stopped tunnel.
    let _ = state.tcp_tunnel.restart_with_config(tunnel_cfg).await;
    Ok(Json(ApiResponse::success_no_data("Tunnel restarted")))
}

async fn test_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TcpTunnelTestResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config = state.config.lock().await;
        config
            .tcp_tunnels
            .iter()
            .find(|t| t.id == id)
            .cloned()
    };

    let Some(cfg) = cfg else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
    };

    match state.tcp_tunnel.test(&cfg).await {
        Ok(()) => Ok(Json(ApiResponse::success(
            "Tunnel test ok",
            TcpTunnelTestResponse { ok: true },
        ))),
        Err((code, message)) => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("{code}: {message}"))),
        )),
    }
}

async fn get_tcp_tunnel_overview(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<TcpTunnelOverviewResponse>> {
    let supported = state.tcp_tunnel.supported();
    let (tunnels, sets) = {
        let config = state.config.lock().await;
        (config.tcp_tunnels.clone(), config.tcp_tunnel_sets.clone())
    };

    let mut items: Vec<TcpTunnelOverviewItem> = Vec::with_capacity(tunnels.len() + sets.len());

    for t in tunnels {
        if matches!(&t.managed_by, Some(TcpTunnelManagedBy::FullTunnel { .. })) {
            continue;
        }
        let status = state
            .tcp_tunnel
            .get_status(&t.id)
            .await
            .unwrap_or_default();
        items.push(TcpTunnelOverviewItem {
            mode: TcpTunnelOverviewMode::Single,
            id: t.id,
            name: t.name,
            enabled: t.enabled,
            ssh_host: t.ssh_host,
            ssh_port: t.ssh_port,
            username: t.username,
            remote_bind_addr: t.remote_bind_addr,
            remote_port: Some(t.remote_port),
            local_addr: Some(t.local_addr),
            local_port: Some(t.local_port),
            auth: Some(redact_tunnel_auth(&t.auth)),
            strict_host_key_checking: Some(t.strict_host_key_checking),
            host_key_fingerprint: Some(t.host_key_fingerprint),
            allow_public_bind: Some(t.allow_public_bind),
            connect_timeout_ms: Some(t.connect_timeout_ms),
            keepalive_interval_ms: Some(t.keepalive_interval_ms),
            reconnect_backoff_ms: Some(t.reconnect_backoff_ms),
            status,
        });
    }

    for s in sets {
        let mut status = tcp_tunnel::TunnelRuntimeStatus::default();
        let st = state.full_tunnel.get_status(&s.id).await;
        status.state = if !s.enabled {
            tcp_tunnel::TunnelState::Stopped
        } else if st.last_error.is_some() {
            tcp_tunnel::TunnelState::Error
        } else {
            tcp_tunnel::TunnelState::Forwarding
        };
        if let Some(e) = st.last_error {
            status.last_error = Some(tcp_tunnel::TunnelErrorInfo {
                code: "SCAN_FAILED".to_string(),
                message: e,
                at_ms: chrono::Utc::now().timestamp_millis(),
            });
        }
        items.push(TcpTunnelOverviewItem {
            mode: TcpTunnelOverviewMode::Full,
            id: s.id,
            name: s.name,
            enabled: s.enabled,
            ssh_host: s.ssh_host,
            ssh_port: s.ssh_port,
            username: s.username,
            remote_bind_addr: s.remote_bind_addr,
            remote_port: None,
            local_addr: None,
            local_port: None,
            auth: None,
            strict_host_key_checking: None,
            host_key_fingerprint: None,
            allow_public_bind: None,
            connect_timeout_ms: None,
            keepalive_interval_ms: None,
            reconnect_backoff_ms: None,
            status,
        });
    }

    Json(ApiResponse::success(
        "TCP tunnel overview",
        TcpTunnelOverviewResponse { supported, items },
    ))
}

async fn get_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TcpTunnelSetDetailResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let set = {
        let config = state.config.lock().await;
        config
            .tcp_tunnel_sets
            .iter()
            .find(|s| s.id == id)
            .cloned()
    };
    let Some(set) = set else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
    };
    Ok(Json(ApiResponse::success(
        "Set detail",
        TcpTunnelSetDetailResponse {
            id: set.id,
            name: set.name,
            enabled: set.enabled,
            remote_bind_addr: set.remote_bind_addr,
            ssh_host: set.ssh_host,
            ssh_port: set.ssh_port,
            username: set.username,
            auth: redact_tunnel_auth(&set.auth),
            strict_host_key_checking: set.strict_host_key_checking,
            host_key_fingerprint: set.host_key_fingerprint,
            exclude_ports: set.exclude_ports,
            scan_interval_ms: set.scan_interval_ms,
            debounce_ms: set.debounce_ms,
        },
    )))
}

async fn get_tcp_tunnel_set_tunnels(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TcpTunnelListResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let (supported, tunnels) = {
        let config = state.config.lock().await;
        let exists = config.tcp_tunnel_sets.iter().any(|s| s.id == id);
        if !exists {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
        }
        let tunnels: Vec<TcpTunnelConfig> = config
            .tcp_tunnels
            .iter()
            .filter(|t| {
                matches!(
                    &t.managed_by,
                    Some(TcpTunnelManagedBy::FullTunnel { set_id, .. }) if set_id == &id
                )
            })
            .cloned()
            .collect();
        (state.tcp_tunnel.supported(), tunnels)
    };

    let mut items = Vec::with_capacity(tunnels.len());
    for t in tunnels {
        let status = state
            .tcp_tunnel
            .get_status(&t.id)
            .await
            .unwrap_or_default();
        items.push(build_tcp_tunnel_item(t, status));
    }

    Ok(Json(ApiResponse::success(
        "Set tunnels",
        TcpTunnelListResponse { supported, items },
    )))
}

async fn start_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let Some(s) = config.tcp_tunnel_sets.iter_mut().find(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
        };
        s.enabled = true;
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Set started")))
}

async fn stop_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let Some(s) = config.tcp_tunnel_sets.iter_mut().find(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
        };
        s.enabled = false;
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Set stopped")))
}

async fn restart_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    // For now, "restart" means set enabled=true (controller will handle actual runtime when implemented).
    {
        let mut config = state.config.lock().await;
        let Some(s) = config.tcp_tunnel_sets.iter_mut().find(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
        };
        s.enabled = true;
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Set restarted")))
}

async fn update_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<TcpTunnelSetCreateRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let existing = {
        let config = state.config.lock().await;
        config
            .tcp_tunnel_sets
            .iter()
            .find(|s| s.id == id)
            .cloned()
    };
    let Some(existing) = existing else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
    };
    if existing.enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Set must be stopped before editing")),
        ));
    }

    let mut auth = req.auth;
    match (&existing.auth, &mut auth) {
        (TcpTunnelAuth::Password { password: old }, TcpTunnelAuth::Password { password: new }) => {
            if new.is_empty() {
                *new = old.clone();
            }
        }
        (
            TcpTunnelAuth::PrivateKeyPath {
                path: old_path,
                passphrase: old_pass,
            },
            TcpTunnelAuth::PrivateKeyPath {
                path: new_path,
                passphrase: new_pass,
            },
        ) => {
            if new_path.is_empty() {
                *new_path = old_path.clone();
            }
            if new_pass.is_none() {
                *new_pass = old_pass.clone();
            }
        }
        _ => {}
    }

    match &auth {
        TcpTunnelAuth::Password { password } if password.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("password is required")),
            ));
        }
        TcpTunnelAuth::PrivateKeyPath { path, .. } if path.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("private key path is required")),
            ));
        }
        _ => {}
    }

    let ssh_host = req.ssh_host.trim().to_string();
    if ssh_host.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("ssh_host is required"))));
    }
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("username is required"))));
    }

    let strict_host_key_checking = req
        .strict_host_key_checking
        .unwrap_or(existing.strict_host_key_checking);
    let host_key_fingerprint = req
        .host_key_fingerprint
        .unwrap_or_else(|| existing.host_key_fingerprint.clone());
    if strict_host_key_checking && host_key_fingerprint.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(
                "host_key_fingerprint is required when strict_host_key_checking is true",
            )),
        ));
    }

    let name = match req.name {
        Some(n) => {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(n)
            }
        }
        None => existing.name.clone(),
    };

    let updated = TcpTunnelSetConfig {
        id: existing.id.clone(),
        name,
        enabled: req.enabled.unwrap_or(existing.enabled),
        remote_bind_addr: req
            .remote_bind_addr
            .unwrap_or_else(|| existing.remote_bind_addr.clone()),
        ssh_host,
        ssh_port: req.ssh_port.unwrap_or(existing.ssh_port),
        username,
        auth: auth.clone(),
        strict_host_key_checking,
        host_key_fingerprint: host_key_fingerprint.clone(),
        exclude_ports: req.exclude_ports.unwrap_or_else(|| existing.exclude_ports.clone()),
        scan_interval_ms: req.scan_interval_ms.unwrap_or(existing.scan_interval_ms),
        debounce_ms: req.debounce_ms.unwrap_or(existing.debounce_ms),
    };

    {
        let mut config = state.config.lock().await;
        let Some(pos) = config.tcp_tunnel_sets.iter().position(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
        };
        config.tcp_tunnel_sets[pos] = updated.clone();

        for t in config.tcp_tunnels.iter_mut() {
            if let Some(TcpTunnelManagedBy::FullTunnel { set_id, .. }) = &t.managed_by {
                if set_id == &id {
                    t.enabled = updated.enabled;
                    t.remote_bind_addr = updated.remote_bind_addr.clone();
                    t.ssh_host = updated.ssh_host.clone();
                    t.ssh_port = updated.ssh_port;
                    t.username = updated.username.clone();
                    t.auth = auth.clone();
                    t.strict_host_key_checking = updated.strict_host_key_checking;
                    t.host_key_fingerprint = updated.host_key_fingerprint.clone();
                    t.allow_public_bind = updated.remote_bind_addr == "0.0.0.0";
                }
            }
        }

        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    apply_tunnels_from_config(&state).await;
    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Set updated")))
}

async fn create_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TcpTunnelSetCreateRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let id = generate_tunnel_set_id();
    let enabled = req.enabled.unwrap_or(false);
    let remote_bind_addr = req.remote_bind_addr.unwrap_or_else(default_remote_bind_addr);
    let ssh_port = req.ssh_port.unwrap_or_else(default_ssh_port);
    let strict_host_key_checking = req.strict_host_key_checking.unwrap_or(true);
    let host_key_fingerprint = req.host_key_fingerprint.unwrap_or_default();
    let exclude_ports = req.exclude_ports.unwrap_or_default();
    let scan_interval_ms = req.scan_interval_ms.unwrap_or(3_000);
    let debounce_ms = req.debounce_ms.unwrap_or(8_000);

    if strict_host_key_checking && host_key_fingerprint.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(
                "host_key_fingerprint is required when strict_host_key_checking is true",
            )),
        ));
    }

    match &req.auth {
        TcpTunnelAuth::Password { password } if password.is_empty() => {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("password is required"))));
        }
        TcpTunnelAuth::PrivateKeyPath { path, .. } if path.is_empty() => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("private key path is required")),
            ));
        }
        _ => {}
    }

    {
        let mut config = state.config.lock().await;
        config.tcp_tunnel_sets.push(TcpTunnelSetConfig {
            id,
            name: req.name,
            enabled,
            remote_bind_addr,
            ssh_host: req.ssh_host,
            ssh_port,
            username: req.username,
            auth: req.auth,
            strict_host_key_checking,
            host_key_fingerprint,
            exclude_ports,
            scan_interval_ms,
            debounce_ms,
        });
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Set created")))
}

async fn copy_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let Some(existing) = config.tcp_tunnel_sets.iter().find(|s| s.id == id).cloned() else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
        };
        let mut cloned = existing.clone();
        cloned.id = generate_tunnel_set_id();
        cloned.enabled = false;
        cloned.name = existing.name.as_ref().and_then(|n| {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("{trimmed}-copy"))
            }
        });
        config.tcp_tunnel_sets.push(cloned.clone());
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    };

    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Set copied")))
}

async fn test_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TcpTunnelTestResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let set = {
        let config = state.config.lock().await;
        config
            .tcp_tunnel_sets
            .iter()
            .find(|s| s.id == id)
            .cloned()
    };
    let Some(set) = set else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
    };

    let cfg = TcpTunnelConfig {
        id: "test".to_string(),
        name: None,
        enabled: true,
        local_addr: "127.0.0.1".to_string(),
        local_port: 0,
        remote_bind_addr: set.remote_bind_addr.clone(),
        remote_port: 0,
        ssh_host: set.ssh_host.clone(),
        ssh_port: set.ssh_port,
        username: set.username.clone(),
        auth: set.auth.clone(),
        strict_host_key_checking: set.strict_host_key_checking,
        host_key_fingerprint: set.host_key_fingerprint.clone(),
        allow_public_bind: set.remote_bind_addr == "0.0.0.0",
        connect_timeout_ms: default_connect_timeout_ms(),
        keepalive_interval_ms: default_keepalive_interval_ms(),
        reconnect_backoff_ms: default_tcp_tunnel_backoff(),
        managed_by: None,
    };

    match state.tcp_tunnel.test_ssh_only(&cfg).await {
        Ok(()) => Ok(Json(ApiResponse::success(
            "Set test ok",
            TcpTunnelTestResponse { ok: true },
        ))),
        Err((code, message)) => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("{code}: {message}"))),
        )),
    }
}

async fn delete_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let before = config.tcp_tunnel_sets.len();
        config.tcp_tunnel_sets.retain(|s| s.id != id);
        if config.tcp_tunnel_sets.len() == before {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Set not found"))));
        }
        // Also remove any managed tunnels for this set.
        config.tcp_tunnels.retain(|t| {
            !matches!(
                &t.managed_by,
                Some(TcpTunnelManagedBy::FullTunnel { set_id, .. }) if set_id == &id
            )
        });
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_tunnels_from_config(&state).await;
    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Set deleted")))
}

async fn bulk_start_tcp_tunnels(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkIdsRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        for id in req.ids.iter() {
            if let Some(t) = config.tcp_tunnels.iter_mut().find(|t| &t.id == id) {
                t.enabled = true;
            }
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_tunnels_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Tunnels started")))
}

async fn bulk_stop_tcp_tunnels(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkIdsRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        for id in req.ids.iter() {
            if let Some(t) = config.tcp_tunnels.iter_mut().find(|t| &t.id == id) {
                t.enabled = false;
            }
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_tunnels_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Tunnels stopped")))
}

async fn bulk_start_tcp_tunnel_sets(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkIdsRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        for id in req.ids.iter() {
            if let Some(s) = config.tcp_tunnel_sets.iter_mut().find(|s| &s.id == id) {
                s.enabled = true;
            }
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Sets started")))
}

async fn bulk_stop_tcp_tunnel_sets(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkIdsRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        for id in req.ids.iter() {
            if let Some(s) = config.tcp_tunnel_sets.iter_mut().find(|s| &s.id == id) {
                s.enabled = false;
            }
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success_no_data("Sets stopped")))
}

async fn copy_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TcpTunnelItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let mut config = state.config.lock().await;
        let Some(existing) = config.tcp_tunnels.iter().find(|t| t.id == id).cloned() else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Tunnel not found"))));
        };

        let mut cloned = existing.clone();
        cloned.id = generate_tunnel_id();
        cloned.enabled = false;
        cloned.name = existing.name.as_ref().and_then(|n| {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("{trimmed}-copy"))
            }
        });

        config.tcp_tunnels.push(cloned.clone());
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        cloned
    };

    apply_tunnels_from_config(&state).await;
    let status = state.tcp_tunnel.get_status(&cfg.id).await.unwrap_or_default();
    Ok(Json(ApiResponse::success(
        "Tunnel copied",
        TcpTunnelItem {
            id: cfg.id,
            name: cfg.name,
            enabled: cfg.enabled,
            local_addr: cfg.local_addr,
            local_port: cfg.local_port,
            remote_bind_addr: cfg.remote_bind_addr,
            remote_port: cfg.remote_port,
            ssh_host: cfg.ssh_host,
            ssh_port: cfg.ssh_port,
            username: cfg.username,
            auth: redact_tunnel_auth(&cfg.auth),
            strict_host_key_checking: cfg.strict_host_key_checking,
            host_key_fingerprint: cfg.host_key_fingerprint,
            allow_public_bind: cfg.allow_public_bind,
            connect_timeout_ms: cfg.connect_timeout_ms,
            keepalive_interval_ms: cfg.keepalive_interval_ms,
            reconnect_backoff_ms: cfg.reconnect_backoff_ms,
            status,
        },
    )))
}

// ============================================================================
// Internal Functions
// ============================================================================

/// Check if the location indicates mainland China (excluding Hong Kong, Macau, Taiwan)
fn is_mainland_china(location: &str) -> bool {
    if location.contains("中国") {
        // Hong Kong, Macau, Taiwan are considered as proxy nodes
        let is_special_region = location.contains("香港")
                             || location.contains("澳门")
                             || location.contains("台湾");
        !is_special_region
    } else {
        false
    }
}

/// Save config to config.yaml
async fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let yaml = serde_yaml::to_string(config)?;
    tokio::fs::write("config.yaml", yaml).await?;
    Ok(())
}

fn normalize_pool(input: &[String], max_len: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for raw in input.iter() {
        if out.len() >= max_len {
            break;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

async fn switch_selector_and_save(
    state: &Arc<AppState>,
    group: &str,
    desired: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    clash_switch_selector_resilient(&client, group, desired).await?;

    {
        let mut config = state.config.lock().await;
        config
            .selections
            .insert(group.to_string(), desired.to_string());
        if let Err(e) = save_config(&config).await {
            return Err(format!("Failed to save config: {}", e));
        }
    }

    Ok(())
}

fn build_node_type_map(config: &Config, subs: &LoadedSubscriptions) -> HashMap<String, String> {
    let mut node_type_by_tag = HashMap::new();

    for node_str in &config.nodes {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(node_str) {
            let Some(tag) = v.get("tag").and_then(|t| t.as_str()) else {
                continue;
            };
            let Some(typ) = v.get("type").and_then(|t| t.as_str()) else {
                continue;
            };
            node_type_by_tag.insert(tag.to_string(), typ.to_string());
        }
    }

    for outbound in &subs.outbounds {
        let Some(tag) = outbound.get("tag").and_then(|t| t.as_str()) else {
            continue;
        };
        let Some(typ) = outbound.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        node_type_by_tag.insert(tag.to_string(), typ.to_string());
    }

    node_type_by_tag
}

#[derive(Clone)]
struct DohCandidate {
    tag: &'static str,
    host: &'static str,
    ip: &'static str,
    path: &'static str,
}

#[derive(Clone, Default)]
struct DnsMonitorState {
    health: HashMap<String, DnsHealthEntry>,
    last_check_at: Option<Instant>,
}

#[derive(Clone, Default)]
struct DnsHealthEntry {
    ok: bool,
    failures: u32,
    cooldown_until: Option<Instant>,
    last_error: Option<String>,
    last_checked_at: Option<Instant>,
}

#[derive(Clone, Default)]
struct ProxyMonitorState {
    health: HashMap<String, ProxyHealthEntry>,
    last_check_at: Option<Instant>,
    paused_until: Option<Instant>,
}

#[derive(Clone, Default)]
struct ProxyHealthEntry {
    ok: bool,
    consecutive_failures: u32,
    window: VecDeque<bool>,
    last_error: Option<String>,
    last_checked_at: Option<Instant>,
    last_ip: Option<String>,
    last_location: Option<String>,
}

#[derive(Serialize)]
struct DnsStatusResponse {
    active: String,
    candidates: Vec<String>,
    interval_ms: u64,
    fail_threshold: u32,
    cooldown_ms: u64,
    health: HashMap<String, DnsHealthPublic>,
    last_check_secs_ago: Option<u64>,
}

#[derive(Serialize)]
struct DnsHealthPublic {
    ok: bool,
    failures: u32,
    cooldown_remaining_secs: u64,
    last_error: Option<String>,
    last_checked_secs_ago: Option<u64>,
}

#[derive(Serialize)]
struct ProxyStatusResponse {
    enabled: bool,
    pool: Vec<String>,
    active: Option<String>,
    interval_ms: u64,
    timeout_ms: u64,
    fail_threshold: u32,
    window_size: usize,
    window_fail_rate: f64,
    pause_ms: u64,
    paused_remaining_secs: u64,
    health: HashMap<String, ProxyHealthPublic>,
    last_check_secs_ago: Option<u64>,
}

#[derive(Serialize)]
struct ProxyHealthPublic {
    ok: bool,
    consecutive_failures: u32,
    window_fails: u32,
    window_size: usize,
    last_error: Option<String>,
    last_ip: Option<String>,
    last_location: Option<String>,
    last_checked_secs_ago: Option<u64>,
}

#[derive(Deserialize)]
struct ProxyPoolUpdateRequest {
    pool: Vec<String>,
}

#[derive(Deserialize)]
struct DnsSwitchRequest {
    tag: String,
}

fn default_dns_candidates() -> Vec<String> {
    vec![
        "doh-cf".to_string(),
        "doh-google".to_string(),
    ]
}

fn is_supported_dns_tag(tag: &str) -> bool {
    matches!(tag, "dns-direct" | "doh-cf" | "doh-google")
}

fn sanitize_dns_active(configured: &str) -> String {
    if configured == "dns-direct" {
        return DEFAULT_DNS_ACTIVE.to_string();
    }
    if is_supported_dns_tag(configured) {
        return configured.to_string();
    }
    DEFAULT_DNS_ACTIVE.to_string()
}

fn doh_candidates_by_tag() -> HashMap<&'static str, DohCandidate> {
    HashMap::from([
        (
            "doh-cf",
            DohCandidate {
                tag: "doh-cf",
                host: "dns.cloudflare.com",
                ip: "1.1.1.1",
                path: "/dns-query",
            },
        ),
        (
            "doh-google",
            DohCandidate {
                tag: "doh-google",
                host: "dns.google",
                ip: "8.8.8.8",
                path: "/dns-query",
            },
        ),
    ])
}

fn build_dns_query_bytes(domain: &str) -> Vec<u8> {
    // Minimal DNS query message for A record.
    let mut msg: Vec<u8> = Vec::with_capacity(64);
    msg.extend_from_slice(&0u16.to_be_bytes()); // id
    msg.extend_from_slice(&0x0100u16.to_be_bytes()); // recursion desired
    msg.extend_from_slice(&1u16.to_be_bytes()); // qdcount
    msg.extend_from_slice(&0u16.to_be_bytes()); // ancount
    msg.extend_from_slice(&0u16.to_be_bytes()); // nscount
    msg.extend_from_slice(&0u16.to_be_bytes()); // arcount
    for label in domain.trim_end_matches('.').split('.') {
        msg.push(label.len().min(63) as u8);
        msg.extend_from_slice(&label.as_bytes()[..label.len().min(63)]);
    }
    msg.push(0); // root
    msg.extend_from_slice(&1u16.to_be_bytes()); // QTYPE A
    msg.extend_from_slice(&1u16.to_be_bytes()); // QCLASS IN
    msg
}

fn build_dns_query_base64url(domain: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(build_dns_query_bytes(domain))
}

async fn check_doh(candidate: &DohCandidate, timeout: Duration) -> Result<(), String> {
    let dns = build_dns_query_base64url("example.com");
    let url = format!("https://{}{}?dns={}", candidate.host, candidate.path, dns);
    let socket_addr = format!("{}:443", candidate.ip);
    let addr = socket_addr
        .parse()
        .map_err(|e| format!("invalid socket address {}: {}", socket_addr, e))?;

    let client = reqwest::Client::builder()
        .resolve(candidate.host, addr)
        .timeout(timeout)
        .build()
        .map_err(|e| format!("build reqwest client: {}", e))?;

    let resp = client
        .get(url)
        .header("accept", "application/dns-message")
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

#[derive(Clone)]
struct UdpDnsCandidate {
    ip: &'static str,
    port: u16,
}

fn udp_dns_candidates_by_tag() -> HashMap<&'static str, UdpDnsCandidate> {
    HashMap::from([
        // Keep tags stable with sing-box template in get_config_template().
        ("dns-direct", UdpDnsCandidate { ip: "223.5.5.5", port: 53 }),
    ])
}

async fn check_udp_dns(candidate: &UdpDnsCandidate, timeout: Duration) -> Result<(), String> {
    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("bind UDP socket failed: {}", e))?;

    let msg = build_dns_query_bytes("example.com");
    tokio::time::timeout(timeout, sock.send_to(&msg, (candidate.ip, candidate.port)))
        .await
        .map_err(|_| "UDP send timeout".to_string())?
        .map_err(|e| format!("UDP send failed: {}", e))?;

    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(timeout, sock.recv_from(&mut buf))
        .await
        .map_err(|_| "UDP recv timeout".to_string())?
        .map_err(|e| format!("UDP recv failed: {}", e))?;

    if n < 12 {
        return Err("short DNS response".to_string());
    }
    Ok(())
}

async fn is_sing_running() -> bool {
    let mut lock = SING_PROCESS.lock().await;
    if let Some(proc) = lock.as_mut() {
        proc.child.try_wait().ok().flatten().is_none()
    } else {
        false
    }
}

async fn run_dns_checks(
    state: &Arc<AppState>,
    candidates: &[String],
    fail_threshold: u32,
    cooldown_ms: u64,
    timeout: Duration,
) -> HashMap<String, bool> {
    let candidate_map = doh_candidates_by_tag();
    let udp_candidate_map = udp_dns_candidates_by_tag();
    let now = Instant::now();

    let mut healthy: HashMap<String, bool> = HashMap::new();
    let mut monitor = state.dns_monitor.lock().await;
    monitor.last_check_at = Some(now);

    for tag in candidates.iter() {
        let entry = monitor.health.entry(tag.clone()).or_default();
        entry.last_checked_at = Some(now);

        if let Some(until) = entry.cooldown_until {
            if until > now {
                entry.ok = false;
                healthy.insert(tag.clone(), false);
                continue;
            }
        }

        if let Some(c) = candidate_map.get(tag.as_str()) {
            match check_doh(c, timeout).await {
                Ok(()) => {
                    entry.ok = true;
                    entry.failures = 0;
                    entry.cooldown_until = None;
                    entry.last_error = None;
                    healthy.insert(tag.clone(), true);
                }
                Err(e) => {
                    entry.ok = false;
                    entry.failures = entry.failures.saturating_add(1);
                    entry.last_error = Some(e);
                    if entry.failures >= fail_threshold {
                        entry.cooldown_until = Some(Instant::now() + Duration::from_millis(cooldown_ms));
                    }
                    healthy.insert(tag.clone(), false);
                }
            }
        } else if let Some(c) = udp_candidate_map.get(tag.as_str()) {
            match check_udp_dns(c, timeout).await {
                Ok(()) => {
                    entry.ok = true;
                    entry.failures = 0;
                    entry.cooldown_until = None;
                    entry.last_error = None;
                    healthy.insert(tag.clone(), true);
                }
                Err(e) => {
                    entry.ok = false;
                    entry.failures = entry.failures.saturating_add(1);
                    entry.last_error = Some(e);
                    if entry.failures >= fail_threshold {
                        entry.cooldown_until = Some(Instant::now() + Duration::from_millis(cooldown_ms));
                    }
                    healthy.insert(tag.clone(), false);
                }
            }
        } else {
            entry.ok = false;
            entry.last_error = Some("Unknown DNS candidate".to_string());
            healthy.insert(tag.clone(), false);
        }
    }

    healthy
}

fn normalize_dns_candidates(raw: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len() + 1);
    let mut seen: HashSet<String> = HashSet::new();

    for tag in raw.into_iter() {
        if tag == "dns-direct" {
            continue;
        }
        if !is_supported_dns_tag(&tag) {
            continue;
        }
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }

    // Always keep Cloudflare as the ultimate fallback.
    if !seen.contains(DEFAULT_DNS_ACTIVE) {
        out.push(DEFAULT_DNS_ACTIVE.to_string());
    }
    out
}

async fn dns_health_monitor(state: Arc<AppState>) {
    loop {
        let (raw_candidates, interval_ms, fail_threshold, cooldown_ms) = {
            let config = state.config.lock().await;
            (
                config
                    .dns_candidates
                    .clone()
                    .unwrap_or_else(default_dns_candidates),
                config.dns_check_interval_ms.unwrap_or(180_000),
                config.dns_fail_threshold.unwrap_or(3),
                config.dns_cooldown_ms.unwrap_or(300_000),
            )
        };

        let candidates = normalize_dns_candidates(raw_candidates);
        let _ = run_dns_checks(
            &state,
            &candidates,
            fail_threshold,
            cooldown_ms,
            Duration::from_millis(2_500),
        )
        .await;

        sleep(Duration::from_millis(interval_ms)).await;
    }
}

async fn check_proxy_health_via_3030(timeout: Duration) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("build reqwest client: {}", e))?;

    let resp = client
        .get("https://3.0.3.0/ips")
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    if resp.status() != StatusCode::OK {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

fn proxy_should_failover(
    entry: &ProxyHealthEntry,
    fail_threshold: u32,
    window_size: usize,
    window_fail_rate: f64,
) -> bool {
    if entry.consecutive_failures >= fail_threshold {
        return true;
    }
    if window_size > 0 && entry.window.len() >= window_size {
        let fails = entry.window.iter().filter(|v| !**v).count() as f64;
        let rate = fails / (entry.window.len() as f64);
        return rate >= window_fail_rate;
    }
    false
}

async fn record_proxy_check_result(
    state: &Arc<AppState>,
    node: &str,
    now: Instant,
    ok: bool,
    ip: Option<String>,
    location: Option<String>,
    error: Option<String>,
    window_size: usize,
) {
    let mut monitor = state.proxy_monitor.lock().await;
    monitor.last_check_at = Some(now);
    let entry = monitor.health.entry(node.to_string()).or_default();
    entry.last_checked_at = Some(now);
    entry.ok = ok;
    entry.last_ip = ip;
    entry.last_location = location;
    entry.last_error = error;
    if ok {
        entry.consecutive_failures = 0;
    } else {
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
    }
    entry.window.push_back(ok);
    while window_size > 0 && entry.window.len() > window_size {
        entry.window.pop_front();
    }
}

async fn proxy_health_monitor(state: Arc<AppState>) {
    loop {
        let (enabled, pool, interval_ms, timeout_ms, fail_threshold, window_size, window_fail_rate, pause_ms) = {
            let config = state.config.lock().await;
            let raw_pool = config.proxy_pool.clone().unwrap_or_default();
            (
                config.proxy_monitor_enabled.unwrap_or(true),
                normalize_pool(&raw_pool, 64),
                config.proxy_check_interval_ms.unwrap_or(180_000),
                config.proxy_check_timeout_ms.unwrap_or(3_000),
                config.proxy_fail_threshold.unwrap_or(3),
                config.proxy_window_size.unwrap_or(10),
                config.proxy_window_fail_rate.unwrap_or(0.6),
                config.proxy_pause_ms.unwrap_or(60_000),
            )
        };

        if !enabled || pool.len() < 2 || !is_sing_running().await {
            sleep(Duration::from_millis(interval_ms)).await;
            continue;
        }

        let now = Instant::now();
        {
            let mut monitor = state.proxy_monitor.lock().await;
            if let Some(until) = monitor.paused_until {
                if until > now {
                    sleep(Duration::from_millis(interval_ms)).await;
                    continue;
                }
                monitor.paused_until = None;
            }
        }

        // Best-effort: prune pool by current selector choices (e.g. subscription changed)
        let pool = {
            let client = reqwest::Client::new();
            if let Ok(choices) = clash_get_selector_choices(&client, "proxy").await {
                let choices_set: HashSet<String> = choices.into_iter().collect();
                let pruned: Vec<String> = pool
                    .iter()
                    .cloned()
                    .filter(|n| choices_set.contains(n))
                    .collect();
                if pruned.len() >= 2 { pruned } else { pool }
            } else {
                pool
            }
        };

        let active = { state.config.lock().await.selections.get("proxy").cloned() };
        let active = match active {
            Some(a) if pool.iter().any(|n| n == &a) => a,
            _ => {
                let desired = pool[0].clone();
                if let Err(e) = switch_selector_and_save(&state, "proxy", &desired).await {
                    eprintln!("Proxy monitor: failed to set initial active proxy: {}", e);
                    sleep(Duration::from_millis(interval_ms)).await;
                    continue;
                }
                desired
            }
        };

        let timeout = Duration::from_millis(timeout_ms);
        let check_now = Instant::now();
        let (ok, ip, location, err) = match check_proxy_health_via_3030(timeout).await {
            Ok(()) => (true, None, None, None),
            Err(e) => (false, None, None, Some(e)),
        };
        record_proxy_check_result(&state, &active, check_now, ok, ip, location, err, window_size).await;

        let should_failover = {
            let monitor = state.proxy_monitor.lock().await;
            let entry = monitor.health.get(&active).cloned().unwrap_or_default();
            proxy_should_failover(&entry, fail_threshold, window_size, window_fail_rate)
        };

        if !should_failover {
            sleep(Duration::from_millis(interval_ms)).await;
            continue;
        }

        let start_idx = pool.iter().position(|n| n == &active).unwrap_or(0);
        let mut switched_ok = false;
        for offset in 1..pool.len() {
            let idx = (start_idx + offset) % pool.len();
            let candidate = pool[idx].clone();

            if let Err(e) = switch_selector_and_save(&state, "proxy", &candidate).await {
                record_proxy_check_result(
                    &state,
                    &candidate,
                    Instant::now(),
                    false,
                    None,
                    None,
                    Some(format!("switch failed: {}", e)),
                    window_size,
                )
                .await;
                continue;
            }

            sleep(Duration::from_millis(500)).await;

            let check_now = Instant::now();
            let (ok, ip, location, err) = match check_proxy_health_via_3030(timeout).await {
                Ok(()) => (true, None, None, None),
                Err(e) => (false, None, None, Some(e)),
            };
            record_proxy_check_result(&state, &candidate, check_now, ok, ip, location, err, window_size).await;

            if ok {
                switched_ok = true;
                break;
            }
        }

        if !switched_ok {
            eprintln!("Proxy monitor: all candidates failed; pausing");
            let mut monitor = state.proxy_monitor.lock().await;
            monitor.paused_until = Some(Instant::now() + Duration::from_millis(pause_ms));
        }

        sleep(Duration::from_millis(interval_ms)).await;
    }
}

async fn apply_saved_selections(config: &Config) -> Result<(), String> {
    let proxy_selection = config
        .selections
        .get("proxy")
        .cloned()
        .or_else(|| config.proxy_pool.as_ref().and_then(|p| p.first().cloned()));
    let has_any = !config.selections.is_empty() || proxy_selection.is_some();
    if !has_any {
        return Ok(());
    }

    let client = reqwest::Client::new();

    let mut ordered: Vec<(String, String)> = Vec::with_capacity(config.selections.len() + 1);

    let desired_dns_selection = config
        .selections
        .get("_dns")
        .cloned()
        .unwrap_or_else(|| "proxy".to_string());

    if proxy_selection.is_some() || config.selections.contains_key("_dns") {
        ordered.push(("_dns".to_string(), desired_dns_selection));
    }
    if let Some(proxy) = proxy_selection {
        ordered.push(("proxy".to_string(), proxy));
    }
    for (group, name) in config.selections.iter() {
        if group == "proxy" || group == "_dns" {
            continue;
        }
        ordered.push((group.clone(), name.clone()));
    }

    for (group, name) in ordered.into_iter() {
        let mut last_err: Option<String> = None;

        for attempt in 1..=10 {
            match clash_switch_selector_resilient(&client, &group, &name).await {
                Ok(()) => {
                    println!("Restored selection: {} -> {}", group, name);
                    last_err = None;
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                }
            };

            // Clash API may not be ready right after sing-box starts
            if attempt < 10 {
                sleep(Duration::from_millis(500)).await;
            }
        }

        if let Some(e) = last_err {
            eprintln!("Failed to restore selection for {}: {}", group, e);
        }
    }

    Ok(())
}

/// Regenerate sing-box config and restart the service
async fn regenerate_and_restart(state: Arc<AppState>) -> Result<(), String> {
    let config_clone = { state.config.lock().await.clone() };

    if let SubSource::Git { .. } = &state.sub_source {
        let out = tokio::process::Command::new("git")
            .arg("-C")
            .arg("sub")
            .arg("pull")
            .arg("--ff-only")
            .output()
            .await
            .map_err(|e| format!("Failed to run git pull: {}", e))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(format!(
                "Git sync failed (git pull --ff-only): {}",
                if detail.is_empty() {
                    out.status.to_string()
                } else {
                    detail
                }
            ));
        }
    }

    let loaded = load_subscription_dir(&state.sub_dir).await;
    {
        let mut sub_files = state.sub_files.lock().await;
        *sub_files = loaded.files.clone();
    }
    {
        let mut sub_dir_error = state.sub_dir_error.lock().await;
        *sub_dir_error = loaded.dir_error.clone();
    }
    {
        let mut node_type_by_tag = state.node_type_by_tag.lock().await;
        *node_type_by_tag = build_node_type_map(&config_clone, &loaded);
    }

    gen_config(&config_clone, &state.sing_box_home, &loaded)
        .await
        .map_err(|e| format!("Failed to regenerate config: {}", e))?;
    println!("Config regenerated successfully");

    // Stop and restart sing-box
    stop_sing_internal().await;
    sleep(Duration::from_millis(500)).await;

    start_sing_internal(&state.sing_box_home)
        .await
        .map_err(|e| format!("Failed to restart sing-box: {}", e))?;
    let _ = apply_saved_selections(&config_clone).await;
    println!("sing-box restarted successfully");
    Ok(())
}

/// Extract embedded sing-box binary to current working directory
fn extract_sing_box() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let current_dir = std::env::current_dir()?;
    let sing_box_path = current_dir.join("sing-box");

    if !sing_box_path.exists() {
        println!("Extracting embedded sing-box binary to {:?}", sing_box_path);
        fs::write(&sing_box_path, SING_BOX_BINARY)?;
        fs::set_permissions(&sing_box_path, fs::Permissions::from_mode(0o755))?;
        println!("sing-box binary extracted successfully");
    }

    let dashboard_dir = current_dir.join("dashboard");
    if !dashboard_dir.exists() {
        fs::create_dir_all(&dashboard_dir)?;
    }

    Ok(current_dir)
}

async fn check_and_install_openwrt_dependencies(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !PathBuf::from("/etc/openwrt_release").exists() {
        return Ok(());
    }

    println!("OpenWrt system detected. Checking dependencies...");

    let output = tokio::process::Command::new("opkg")
        .arg("list-installed")
        .output()
        .await?;

    let installed_list = String::from_utf8_lossy(&output.stdout);
    let installed_set: std::collections::HashSet<&str> = installed_list
        .lines()
        .map(|line| line.split_whitespace().next().unwrap_or(""))
        .collect();

    let mut packages_to_install = Vec::new();

    if !installed_set.contains("kmod-tun") {
        packages_to_install.push("kmod-tun");
    }
    if !installed_set.contains("kmod-nft-queue") {
        packages_to_install.push("kmod-nft-queue");
    }

    if packages_to_install.is_empty() {
        println!("Required dependencies (kmod-tun, kmod-nft-queue) are already installed.");
        return Ok(());
    }

    println!(
        "Missing dependencies: {:?}. Installing...",
        packages_to_install
    );

    println!("Running 'opkg update'...");
    let update_status = tokio::process::Command::new("opkg")
        .arg("update")
        .status()
        .await?;

    if !update_status.success() {
        eprintln!("'opkg update' finished with error, but proceeding with installation attempt...");
    }

    for pkg in packages_to_install {
        println!("Installing {}...", pkg);
        let install_status = tokio::process::Command::new("opkg")
            .arg("install")
            .arg(pkg)
            .status()
            .await?;

        if !install_status.success() {
            return Err(
                format!("Failed to install {}. Please install it manually.", pkg).into(),
            );
        }
    }

    println!("Dependencies installed successfully.");
    Ok(())
}

async fn start_sing_internal(
    sing_box_home: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut lock = SING_PROCESS.lock().await;
    if let Some(ref mut proc) = *lock {
        if proc.child.try_wait()?.is_none() {
            return Err("already running!".into());
        }
    }

    let sing_box_path = PathBuf::from(sing_box_home).join("sing-box");
    let config_path = PathBuf::from(sing_box_home).join("config.json");

    println!("Starting sing-box from: {:?}", sing_box_path);
    println!("Using config: {:?}", config_path);

    let mut child = tokio::process::Command::new(&sing_box_path)
        .current_dir(sing_box_home)
        .arg("run")
        .arg("-c")
        .arg(&config_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    let pid = child.id();
    println!("sing-box process spawned with PID: {:?}", pid);

    // Wait a short moment to check if process exits immediately
    sleep(Duration::from_millis(500)).await;
    if let Some(exit_status) = child.try_wait()? {
        let code = exit_status.code().unwrap_or(-1);
        return Err(format!("sing-box exited immediately with code {}", code).into());
    }

    // Store the process first
    *lock = Some(SingBoxProcess {
        child,
        started_at: Instant::now(),
    });
    drop(lock); // Release lock before connectivity check

    // Wait for sing-box to fully initialize
    sleep(Duration::from_secs(5)).await;

    // Connectivity check using 3.0.3.0 IP service (3 attempts)
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    for attempt in 1..=3 {
        println!("Connectivity check attempt {}/3...", attempt);

        match client.get("https://3.0.3.0/ips").send().await {
            Ok(res) if res.status() == StatusCode::OK => {
                println!("✅ Connectivity check passed!");
                return Ok(());
            }
            Ok(res) => println!("Connectivity check: unexpected status {}", res.status()),
            Err(e) => println!("Connectivity check failed: {}", e),
        }

        if attempt < 3 {
            sleep(Duration::from_secs(2)).await;
        }
    }

    // Even if all checks failed, don't kill sing-box
    println!("⚠️  Warning: All connectivity checks failed, but sing-box is still running");
    println!("   You can verify connectivity manually via the web panel.");

    Ok(())
}

async fn stop_sing_internal() {
    let mut lock = SING_PROCESS.lock().await;
    if let Some(ref mut proc) = *lock {
        if proc.child.try_wait().ok().flatten().is_none() {
            // Use SIGTERM to allow sing-box to gracefully shutdown and cleanup nftables rules
            if let Some(pid) = proc.child.id() {
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                // Wait up to 3 seconds for graceful shutdown
                for _ in 0..30 {
                    sleep(Duration::from_millis(100)).await;
                    if proc.child.try_wait().ok().flatten().is_some() {
                        break;
                    }
                }
                // Force kill if still running
                if proc.child.try_wait().ok().flatten().is_none() {
                    proc.child.start_kill().ok();
                }
            }
        }
    }
    *lock = None;
}

/// Stop sing-box and wait for it to fully exit
async fn stop_sing_internal_and_wait() {
    let mut lock = SING_PROCESS.lock().await;
    if let Some(ref mut proc) = *lock {
        if proc.child.try_wait().ok().flatten().is_none() {
            // Use SIGTERM to allow sing-box to gracefully shutdown and cleanup nftables rules
            if let Some(pid) = proc.child.id() {
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                // Wait up to 3 seconds for graceful shutdown
                for _ in 0..30 {
                    sleep(Duration::from_millis(100)).await;
                    if proc.child.try_wait().ok().flatten().is_some() {
                        break;
                    }
                }
                // Force kill if still running
                if proc.child.try_wait().ok().flatten().is_none() {
                    proc.child.start_kill().ok();
                    let _ = proc.child.wait().await;
                }
            }
        }
    }
    *lock = None;
}

async fn gen_config(
    config: &Config,
    sing_box_home: &str,
    subs: &LoadedSubscriptions,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let my_outbounds: Vec<serde_json::Value> = config
        .nodes
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();
    let my_names: Vec<String> = my_outbounds
        .iter()
        .filter_map(|o| o.get("tag").and_then(|v| v.as_str()).map(String::from))
        .collect();

    let mut final_outbounds: Vec<serde_json::Value> = vec![];
    let mut final_node_names: Vec<String> = vec![];

    final_node_names.extend(subs.node_names.iter().cloned());
    final_outbounds.extend(subs.outbounds.iter().cloned());

    let total_nodes = my_outbounds.len() + final_outbounds.len();
    if total_nodes == 0 {
        return Err(
            "No nodes available: all subscriptions failed and no manual nodes configured".into(),
        );
    }

    let mut sing_box_config = get_config_template();
    if let Some(dns) = sing_box_config.get_mut("dns") {
        let configured = config.dns_active.as_deref().unwrap_or(DEFAULT_DNS_ACTIVE);
        let active = sanitize_dns_active(configured);
        dns["final"] = serde_json::Value::String(active);
    }
    if let Some(outbounds) = sing_box_config["outbounds"][0].get_mut("outbounds") {
        if let Some(arr) = outbounds.as_array_mut() {
            arr.extend(
                my_names
                    .into_iter()
                    .chain(final_node_names.into_iter())
                    .map(serde_json::Value::String),
            );
        }
    }
    if let Some(arr) = sing_box_config["outbounds"].as_array_mut() {
        arr.extend(my_outbounds.into_iter().chain(final_outbounds.into_iter()));
    }

    let config_output_loc = format!("{}/config.json", sing_box_home);
    tokio::fs::write(
        &config_output_loc,
        serde_json::to_string(&sing_box_config)?,
    )
    .await?;

    println!(
        "Generated sing-box config with {} outbounds at {}",
        sing_box_config["outbounds"].as_array().map(|a| a.len()).unwrap_or(0),
        config_output_loc
    );
    Ok(())
}



fn get_config_template() -> serde_json::Value {
    serde_json::json!({
        "log": {"disabled": false, "timestamp": true, "level": "info"},
        "experimental": {"clash_api": {"external_controller": "127.0.0.1:6262", "access_control_allow_origin": ["*"]}},
        "dns": {
            "final": DEFAULT_DNS_ACTIVE,
            "strategy": "prefer_ipv4",
            "independent_cache": true,
            "servers": [
                {"type": "udp", "tag": "dns-direct", "server": "223.5.5.5", "server_port": 53},
                {
                    "type": "https",
                    "tag": "doh-cf",
                    "server": "1.1.1.1",
                    "server_port": 443,
                    "path": "/dns-query",
                    "headers": {"Host": "dns.cloudflare.com"},
                    "tls": {"enabled": true, "server_name": "dns.cloudflare.com", "insecure": false},
                    "detour": "_dns",
                    "connect_timeout": "2s"
                },
                {
                    "type": "https",
                    "tag": "doh-google",
                    "server": "8.8.8.8",
                    "server_port": 443,
                    "path": "/dns-query",
                    "headers": {"Host": "dns.google"},
                    "tls": {"enabled": true, "server_name": "dns.google", "insecure": false},
                    "detour": "_dns",
                    "connect_timeout": "2s"
                }
            ]
        },
        "inbounds": [
            {"type": "tun", "tag": "tun-in", "interface_name": "sing-tun", "address": ["172.18.0.1/30", "fd00:172:18::1/126"], "mtu": 9000, "auto_route": true, "strict_route": true, "stack": "system", "sniff": true, "sniff_override_destination": false }
        ],
        "outbounds": [
            {"type": "selector", "tag": "proxy", "outbounds": []},
            {"type": "selector", "tag": "_dns", "outbounds": ["proxy", "direct"]},
            {"type": "direct", "tag": "direct"}
        ],
        "route": {
            "final": "proxy",
            "auto_detect_interface": true,
            "default_domain_resolver": "dns-direct",
            "rules": [
                {"action": "sniff"},
                {"protocol": "dns", "action": "hijack-dns"},
                {"network": "tcp", "port": 22, "action": "route", "outbound": "direct"},
                {"ip_cidr": ["100.64.0.0/10"], "action": "route", "outbound": "direct"},
                {"ip_is_private": true, "action": "route", "outbound": "direct"},
                {"network": "icmp", "action": "route", "outbound": "direct"}
            ]
        }
    })
}

fn parse_subscription_text(
    text: &str,
) -> Result<(Vec<String>, Vec<serde_json::Value>), Box<dyn std::error::Error + Send + Sync>> {
    // Try parsing as JSON first (sing-box format)
    if let Ok(json_obj) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(outbounds_arr) = json_obj.get("outbounds").and_then(|o| o.as_array()) {
            println!("Detected sing-box JSON format subscription");
            return parse_singbox_json(outbounds_arr);
        }
    }

    // Try parsing as Shadowsocks URL list (base64 encoded)
    if let Some(ss_result) = try_parse_ss_urls(text) {
        println!("Detected Shadowsocks URL format subscription");
        return ss_result;
    }

    // Fall back to YAML parsing (Clash format)
    println!("Parsing as Clash YAML format");
    let clash_obj: serde_yaml::Value = serde_yaml::from_str(text)?;
    let proxies = clash_obj
        .get("proxies")
        .and_then(|p| p.as_sequence())
        .unwrap_or(&vec![])
        .clone();

    let nodes: Vec<serde_yaml::Value> = proxies.into_iter().collect();
    let mut node_names = vec![];
    let mut outbounds = vec![];

    for node in nodes {
        let typ = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let name = node.get("name").and_then(|n| n.as_str()).unwrap_or("");
        match typ {
            "hysteria2" => {
                let hysteria2 = Hysteria2 {
                    outbound_type: "hysteria2".to_string(),
                    tag: name.to_string(),
                    server: node
                        .get("server")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    server_port: node.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16,
                    password: node
                        .get("password")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string(),
                    up_mbps: 40,
                    down_mbps: 350,
                    tls: Tls {
                        enabled: true,
                        server_name: node
                            .get("sni")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string()),
                        insecure: true,
                    },
                };
                node_names.push(name.to_string());
                outbounds.push(serde_json::to_value(hysteria2)?);
            }
            "anytls" => {
                let anytls = AnyTls {
                    outbound_type: "anytls".to_string(),
                    tag: name.to_string(),
                    server: node
                        .get("server")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    server_port: node.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16,
                    password: node
                        .get("password")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string(),
                    tls: Tls {
                        enabled: true,
                        server_name: node
                            .get("sni")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string()),
                        insecure: node
                            .get("skip-cert-verify")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    },
                };
                node_names.push(name.to_string());
                outbounds.push(serde_json::to_value(anytls)?);
            }
            _ => {}
        }
    }
    Ok((node_names, outbounds))
}

async fn load_subscription_file(
    path: &StdPath,
) -> Result<(Vec<String>, Vec<serde_json::Value>), Box<dyn std::error::Error + Send + Sync>> {
    let text = tokio::fs::read_to_string(path).await?;
    parse_subscription_text(&text)
}

async fn load_subscription_dir(sub_dir: &StdPath) -> LoadedSubscriptions {
    let mut file_paths: Vec<(String, PathBuf)> = vec![];
    let mut dir_error: Option<String> = None;

    match tokio::fs::read_dir(sub_dir).await {
        Ok(mut rd) => {
            loop {
                match rd.next_entry().await {
                    Ok(Some(ent)) => {
                        let md = match ent.metadata().await {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        if !md.is_file() {
                            continue;
                        }
                        let file_name = ent.file_name().to_string_lossy().to_string();
                        file_paths.push((file_name, ent.path()));
                    }
                    Ok(None) => break,
                    Err(e) => {
                        dir_error = Some(format!("Failed to scan sub dir {}: {}", sub_dir.display(), e));
                        break;
                    }
                }
            }
        }
        Err(e) => {
            dir_error = Some(format!("Failed to read sub dir {}: {}", sub_dir.display(), e));
        }
    }

    file_paths.sort_by(|a, b| a.0.cmp(&b.0));

    let mut file_statuses: Vec<SubFileStatus> = vec![];
    let mut merged_by_tag: HashMap<String, serde_json::Value> = HashMap::new();
    let mut tag_order: Vec<String> = vec![];

    for (file_name, path) in file_paths {
        match load_subscription_file(&path).await {
            Ok((_node_names, outbounds)) => {
                let node_count = outbounds.len();
                for outbound in outbounds {
                    let tag = outbound
                        .get("tag")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let Some(tag) = tag else {
                        continue;
                    };
                    if !merged_by_tag.contains_key(&tag) {
                        tag_order.push(tag.clone());
                    }
                    merged_by_tag.insert(tag, outbound);
                }
                file_statuses.push(SubFileStatus {
                    file_name,
                    file_path: path.display().to_string(),
                    loaded: true,
                    node_count,
                    error: None,
                });
            }
            Err(e) => {
                file_statuses.push(SubFileStatus {
                    file_name,
                    file_path: path.display().to_string(),
                    loaded: false,
                    node_count: 0,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let outbounds: Vec<serde_json::Value> = tag_order
        .iter()
        .filter_map(|tag| merged_by_tag.get(tag).cloned())
        .collect();

    LoadedSubscriptions {
        files: file_statuses,
        node_names: tag_order,
        outbounds,
        dir_error,
    }
}

/// Parse sing-box JSON format subscription
fn parse_singbox_json(
    outbounds: &Vec<serde_json::Value>,
) -> Result<(Vec<String>, Vec<serde_json::Value>), Box<dyn std::error::Error + Send + Sync>> {
    let mut node_names = vec![];
    let mut result_outbounds = vec![];

    // Filter out non-proxy outbounds (selector, urltest, direct, etc.)
    let proxy_types = ["hysteria2", "vmess", "vless", "trojan", "shadowsocks", "ss"];

    for outbound in outbounds {
        let typ = outbound.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let tag = outbound.get("tag").and_then(|t| t.as_str()).unwrap_or("");

        if !proxy_types.contains(&typ) {
            continue;
        }

        match typ {
            "hysteria2" => {
                // Extract hysteria2 node and convert to our format
                let server = outbound.get("server").and_then(|s| s.as_str()).unwrap_or("");
                let password = outbound.get("password").and_then(|p| p.as_str()).unwrap_or("");
                let server_port = outbound.get("server_port").and_then(|p| p.as_u64()).unwrap_or(443) as u16;

                let tls = outbound.get("tls");
                let server_name = tls
                    .and_then(|t| t.get("server_name"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());

                let hysteria2 = Hysteria2 {
                    outbound_type: "hysteria2".to_string(),
                    tag: tag.to_string(),
                    server: server.to_string(),
                    server_port,
                    password: password.to_string(),
                    up_mbps: 40,
                    down_mbps: 350,
                    tls: Tls {
                        enabled: true,
                        server_name,
                        insecure: true,
                    },
                };

                node_names.push(tag.to_string());
                result_outbounds.push(serde_json::to_value(hysteria2)?);
            }
            _ => {
                // For other types, use the outbound as-is
                // But skip if it's missing required fields
                if !tag.is_empty() {
                    node_names.push(tag.to_string());
                    result_outbounds.push(outbound.clone());
                }
            }
        }
    }

    println!("Parsed {} nodes from sing-box JSON", node_names.len());
    Ok((node_names, result_outbounds))
}

/// Try to detect if the text is base64 encoded SS URLs
fn try_parse_ss_urls(text: &str) -> Option<Result<(Vec<String>, Vec<serde_json::Value>), Box<dyn std::error::Error + Send + Sync>>> {
    // Check if the text looks like base64 (only contains valid base64 characters)
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Try to decode as base64
    match base64_decode(trimmed) {
        Ok(decoded) => {
            let decoded_str = String::from_utf8_lossy(&decoded);
            // Check if decoded content contains ss:// URLs
            if decoded_str.contains("ss://") {
                Some(parse_ss_url_list(&decoded_str))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Base64 decode helper
fn base64_decode(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    // Handle URL-safe base64 as well
    let input = input.trim();
    if input.contains('_') || input.contains('-') {
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE, input)
    } else {
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, input)
    }
}

/// Parse a list of SS URLs (base64 decoded content)
fn parse_ss_url_list(content: &str) -> Result<(Vec<String>, Vec<serde_json::Value>), Box<dyn std::error::Error + Send + Sync>> {
    let mut node_names = vec![];
    let mut outbounds = vec![];

    // Split by newlines and parse each SS URL
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with("ss://") {
            continue;
        }

        if let Some((name, ss)) = parse_single_ss_url(line) {
            node_names.push(name.clone());
            outbounds.push(ss);
        }
    }

    println!("Parsed {} nodes from Shadowsocks URLs", node_names.len());
    Ok((node_names, outbounds))
}

/// Parse a single SS URL
/// Format: ss://BASE64(method:password)@server:port#name
/// Or with plugin: ss://BASE64(method:password)@server:port?plugin=xxx#name
fn parse_single_ss_url(url: &str) -> Option<(String, serde_json::Value)> {
    // Remove the ss:// prefix
    let url = url.strip_prefix("ss://")?;

    // Find the fragment (#) for the name
    let (url_part, name) = match url.rsplit_once('#') {
        Some((u, n)) => (u, url_decode(n)),
        None => (url, String::new()),
    };

    // Find the @ sign separating userinfo from server:port
    let (userinfo, server_part) = match url_part.split_once('@') {
        Some((u, s)) => (u, s),
        None => return None,
    };

    // Parse server:port
    let (server, port) = match server_part.rsplit_once(':') {
        Some((s, p)) => (s, p.parse::<u16>().ok()?),
        None => return None,
    };

    // Decode userinfo (method:password)
    let decoded_userinfo = match base64_decode(userinfo) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => return None,
    };

    let (method, password) = match decoded_userinfo.split_once(':') {
        Some((m, p)) => (m.to_string(), p.to_string()),
        None => {
            // Handle case where password contains ':'
            let parts: Vec<&str> = decoded_userinfo.splitn(2, ':').collect();
            if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                return None;
            }
        }
    };

    // Create shadowsocks outbound
    let ss = Shadowsocks {
        outbound_type: "shadowsocks".to_string(),
        tag: if name.is_empty() { format!("{}:{}", server, port) } else { name },
        server: server.to_string(),
        server_port: port,
        method,
        password,
    };

    Some((ss.tag.clone(), serde_json::to_value(ss).ok()?))
}

/// URL decode helper - handles UTF-8 multi-byte sequences (including emoji)
fn url_decode(input: &str) -> String {
    percent_encoding::percent_decode_str(input)
        .decode_utf8()
        .unwrap_or_else(|_| input.to_string().into())
        .into_owned()
}

// ============================================================================
// Authentication Middleware
// ============================================================================

// JWT 认证中间件
async fn auth_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 从 header 中获取 Authorization
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    if let Some(auth) = auth_header {
        // 检查是否是 Bearer token 格式
        if let Some(token) = auth.strip_prefix("Bearer ") {
            // 验证 token
            if verify_token(token).is_ok() {
                return Ok(next.run(req).await);
            }
        }
    }

    // 认证失败
    Err(StatusCode::UNAUTHORIZED)
}

fn looks_like_git_url(value: &str) -> bool {
    value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("ssh://")
        || (value.starts_with("git@") && value.contains(':'))
}

async fn remove_path_if_exists(path: &StdPath) -> Result<(), String> {
    let md = match tokio::fs::symlink_metadata(path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Failed to stat {}: {}", path.display(), e)),
    };

    if md.is_dir() {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|e| format!("Failed to remove dir {}: {}", path.display(), e))?;
    } else {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| format!("Failed to remove file {}: {}", path.display(), e))?;
    }
    Ok(())
}

async fn prepare_sub_dir_from_git(url: &str) -> Result<(), String> {
    let tmp = StdPath::new(".sub");
    let final_dir = StdPath::new("sub");

    remove_path_if_exists(tmp).await?;

    let out = tokio::process::Command::new("git")
        .arg("clone")
        .arg(url)
        .arg(".sub")
        .output()
        .await
        .map_err(|e| format!("Failed to run git clone: {}", e))?;

    if !out.status.success() {
        let _ = remove_path_if_exists(tmp).await;
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!(
            "Git clone failed: {}",
            if detail.is_empty() {
                out.status.to_string()
            } else {
                detail
            }
        ));
    }

    remove_path_if_exists(final_dir).await?;
    tokio::fs::rename(tmp, final_dir)
        .await
        .map_err(|e| format!("Failed to move .sub -> sub: {}", e))?;
    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // CLI args (pre-parse for help; help should not require root)
    let mut raw_args = std::env::args();
    let arg0 = raw_args.next().unwrap_or_else(|| "miao".to_string());
    let program_name = std::path::Path::new(&arg0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("miao");
    let rest_args: Vec<String> = raw_args.collect();
    if rest_args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Miao - sing-box 管理器\n\n\
用法:\n  {program} [OPTIONS]\n\n\
选项:\n  --sub <PATH|GIT_URL>   订阅文件目录或 Git 仓库（默认 ./sub）\n  -h, --help             显示帮助并退出\n\n\
说明:\n  - 配置文件为当前目录下的 ./config.yaml\n  - 正常运行需要 root 权限（--help 例外）",
            program = program_name
        );
        return Ok(());
    }

    // Check for root privileges
    if !Uid::effective().is_root() {
        eprintln!("Error: This application must be run as root.");
        std::process::exit(1);
    }

    // CLI args
    let mut sub_dir = PathBuf::from("sub");
    let mut sub_source = SubSource::Path {
        value: "sub".to_string(),
    };
    let mut args = rest_args.into_iter();
    while let Some(arg) = args.next() {
        if let Some(v) = arg.strip_prefix("--sub=") {
            if looks_like_git_url(v) {
                prepare_sub_dir_from_git(v)
                    .await
                    .map_err(|e| format!("Failed to prepare subscription dir from git: {}", e))?;
                sub_dir = PathBuf::from("sub");
                sub_source = SubSource::Git { url: v.to_string() };
            } else {
                sub_dir = PathBuf::from(v);
                sub_source = SubSource::Path {
                    value: v.to_string(),
                };
            }
        } else if arg == "--sub" {
            if let Some(v) = args.next() {
                if looks_like_git_url(&v) {
                    prepare_sub_dir_from_git(&v)
                        .await
                        .map_err(|e| format!("Failed to prepare subscription dir from git: {}", e))?;
                    sub_dir = PathBuf::from("sub");
                    sub_source = SubSource::Git { url: v };
                } else {
                    sub_dir = PathBuf::from(&v);
                    sub_source = SubSource::Path { value: v };
                }
            }
        }
    }

    println!("Reading configuration...");
    let (config, setup_required) = match tokio::fs::read_to_string("config.yaml").await {
        Ok(text) => (serde_yaml::from_str::<Config>(&text)?, false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            Config {
                port: Some(DEFAULT_PORT),
                sing_box_home: None,
                password: None,
                selections: HashMap::new(),
                nodes: vec![],
                dns_active: None,
                dns_candidates: None,
                dns_check_interval_ms: None,
                dns_fail_threshold: None,
                dns_cooldown_ms: None,
                proxy_pool: None,
                proxy_monitor_enabled: None,
                proxy_check_interval_ms: None,
                proxy_check_timeout_ms: None,
                proxy_fail_threshold: None,
                proxy_window_size: None,
                proxy_window_fail_rate: None,
                proxy_pause_ms: None,
                tcp_tunnels: vec![],
                tcp_tunnel_sets: vec![],
            },
            true,
        ),
        Err(e) => return Err(e.into()),
    };

    let port = config.port.unwrap_or(DEFAULT_PORT);

    // Extract embedded sing-box binary and determine working directory
    let sing_box_home = if let Some(custom_home) = &config.sing_box_home {
        custom_home.clone()
    } else {
        extract_sing_box()?.to_string_lossy().to_string()
    };

    println!("Loading subscription files from: {}", sub_dir.display());
    let loaded_subs = load_subscription_dir(&sub_dir).await;
    let node_type_by_tag = build_node_type_map(&config, &loaded_subs);

    if !setup_required {
        // Generate initial config
        println!("Generating initial config...");
        match gen_config(&config, &sing_box_home, &loaded_subs).await {
            Ok(_) => {
                // Check OpenWrt dependencies
                println!("Checking dependencies...");
                if let Err(e) = check_and_install_openwrt_dependencies().await {
                    eprintln!("Failed to check or install OpenWrt dependencies: {}", e);
                }

                // Start sing-box
                match start_sing_internal(&sing_box_home).await {
                    Ok(_) => {
                        let _ = apply_saved_selections(&config).await;
                        println!("sing-box started successfully")
                    }
                    Err(e) => eprintln!("Failed to start sing-box: {}", e),
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to generate config: {}. Please add subscription files under {} and reload.",
                    e,
                    sub_dir.display()
                );
            }
        }

    } else {
        println!("No config.yaml found, entering setup mode at http://localhost:{}", port);
    }

    let app_state = Arc::new(AppState {
        config: Mutex::new(config.clone()),
        sing_box_home: sing_box_home.clone(),
        sub_dir: sub_dir.clone(),
        sub_source,
        sub_files: Mutex::new(loaded_subs.files.clone()),
        sub_dir_error: Mutex::new(loaded_subs.dir_error.clone()),
        node_type_by_tag: Mutex::new(node_type_by_tag),
        dns_monitor: Mutex::new(DnsMonitorState::default()),
        proxy_monitor: Mutex::new(ProxyMonitorState::default()),
        setup_required: AtomicBool::new(setup_required),
        tcp_tunnel: tcp_tunnel::TunnelManager::new(),
        full_tunnel: full_tunnel::FullTunnelManager::new(),
    });

    // Apply initial TCP tunnel config (best-effort).
    {
        let cfg = app_state.config.lock().await;
        app_state.tcp_tunnel.apply_config(&cfg.tcp_tunnels).await;
        app_state
            .full_tunnel
            .sync_from_config(app_state.clone(), cfg.tcp_tunnel_sets.clone())
            .await;
    }

    // DNS health monitor (best-effort). It updates config.dns_active and restarts sing-box when needed.
    {
        let state_clone = app_state.clone();
        tokio::spawn(async move { dns_health_monitor(state_clone).await });
    }

    // Proxy health monitor (best-effort). It periodically checks current proxy via 3.0.3.0 and fails over within proxy_pool.
    {
        let state_clone = app_state.clone();
        tokio::spawn(async move { proxy_health_monitor(state_clone).await });
    }



    // Build router with API endpoints

    // 需要认证的路由
    let protected_routes = Router::new()
        // Status and service control
        .route("/api/status", get(get_status))
        .route("/api/service/start", post(start_service))
        .route("/api/service/stop", post(stop_service))
        // Connectivity test
        .route("/api/connectivity", post(test_connectivity))
        // Upgrade (protected)
        .route("/api/upgrade", post(upgrade))
        // Clash API proxy (protected HTTP)
        .route("/api/clash/proxies", get(clash_get_proxies))
        .route("/api/clash/proxies/{group}", put(clash_switch_proxy))
        .route("/api/clash/proxies/{node}/delay", get(clash_test_delay))
        .route("/api/selections", get(get_selections))
        .route("/api/proxy/status", get(get_proxy_status))
        .route("/api/proxy/pool", put(update_proxy_pool))
        // Subscription file management
        .route("/api/sub-files", get(get_sub_files))
        .route("/api/sub-files/reload", post(reload_sub_files))
        // Node management
        .route("/api/nodes", get(get_nodes))
        .route("/api/nodes", post(add_node))
        .route("/api/nodes", delete(delete_node))
        // Use a standalone endpoint to avoid colliding with node tags (e.g. tag == "test")
        .route("/api/node-test", post(test_node))
        .route("/api/nodes/{tag}", get(get_node).put(update_node))
        .route("/api/dns/status", get(get_dns_status))
        .route("/api/dns/check", post(check_dns_now))
        .route("/api/dns/switch", post(switch_dns_active))
        // TCP reverse tunnels (SSH -R)
        .route("/api/tcp-tunnels", get(get_tcp_tunnels))
        .route("/api/tcp-tunnels", post(create_tcp_tunnel))
        .route("/api/tcp-tunnels/{id}", put(update_tcp_tunnel))
        .route("/api/tcp-tunnels/{id}", delete(delete_tcp_tunnel))
        .route("/api/tcp-tunnels/{id}/start", post(start_tcp_tunnel))
        .route("/api/tcp-tunnels/{id}/stop", post(stop_tcp_tunnel))
        .route("/api/tcp-tunnels/{id}/restart", post(restart_tcp_tunnel))
        .route("/api/tcp-tunnels/{id}/test", post(test_tcp_tunnel))
        .route("/api/tcp-tunnels/{id}/copy", post(copy_tcp_tunnel))
        .route("/api/tcp-tunnels/bulk/start", post(bulk_start_tcp_tunnels))
        .route("/api/tcp-tunnels/bulk/stop", post(bulk_stop_tcp_tunnels))
        .route("/api/tcp-tunnel/overview", get(get_tcp_tunnel_overview))
        .route("/api/tcp-tunnel-sets", post(create_tcp_tunnel_set))
        .route("/api/tcp-tunnel-sets/{id}", get(get_tcp_tunnel_set).put(update_tcp_tunnel_set).delete(delete_tcp_tunnel_set))
        .route("/api/tcp-tunnel-sets/{id}/start", post(start_tcp_tunnel_set))
        .route("/api/tcp-tunnel-sets/{id}/stop", post(stop_tcp_tunnel_set))
        .route("/api/tcp-tunnel-sets/{id}/restart", post(restart_tcp_tunnel_set))
        .route("/api/tcp-tunnel-sets/{id}/tunnels", get(get_tcp_tunnel_set_tunnels))
        .route("/api/tcp-tunnel-sets/{id}/copy", post(copy_tcp_tunnel_set))
        .route("/api/tcp-tunnel-sets/{id}/test", post(test_tcp_tunnel_set))
        .route("/api/tcp-tunnel-sets/bulk/start", post(bulk_start_tcp_tunnel_sets))
        .route("/api/tcp-tunnel-sets/bulk/stop", post(bulk_stop_tcp_tunnel_sets))
        .route_layer(middleware::from_fn(auth_middleware));  // 应用认证中间件

    // 公开路由（不需要认证）
    let ws_routes = Router::new()
        .route("/api/clash/ws/traffic", get(clash_ws_traffic))
        .route("/api/clash/ws/logs", get(clash_ws_logs));

    let app = Router::new()
        .route("/", get(serve_index))           // 首页可访问（前端会检查）
        .route("/icon.svg", get(serve_icon))    // 网站图标
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup/init", post(setup_init))
        .route("/api/login", post(login))       // 登录接口
        .route("/api/version", get(get_version)) // 版本信息与更新检查（公开，便于探活）
        .merge(ws_routes)
        .merge(protected_routes)                // 合并受保护的路由
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    println!("✅ Miao 控制面板已启动: http://localhost:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
