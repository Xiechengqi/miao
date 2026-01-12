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
use std::collections::HashMap;
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
    sub_files: Mutex<Vec<SubFileStatus>>,
    sub_dir_error: Mutex<Option<String>>,
    node_type_by_tag: Mutex<HashMap<String, String>>,
    dns_monitor: Mutex<DnsMonitorState>,
    setup_required: AtomicBool,
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
            let node_type_by_tag = state.node_type_by_tag.lock().await;
            let _ = apply_saved_selections(&config, Some(&*node_type_by_tag)).await;
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
    let client = reqwest::Client::new();

    let previous_proxy_selection = if group == "proxy" {
        let config = state.config.lock().await;
        config.selections.get("proxy").cloned()
    } else {
        None
    };

    clash_switch_selector(&client, &group, &req.name)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(ApiResponse::error(e))))?;

    let mut dns_selection: Option<String> = None;
    if group == "proxy" {
        let is_ssh = {
            let node_type_by_tag = state.node_type_by_tag.lock().await;
            node_type_by_tag
                .get(&req.name)
                .map(|t| t == "ssh")
                .unwrap_or(false)
        };
        let target = if is_ssh { "direct" } else { "proxy" };
        match clash_switch_selector_resilient(&client, "_dns", target).await {
            Ok(()) => {
                dns_selection = Some(target.to_string());
            }
            Err(e) => {
                if is_ssh {
                    if let Some(previous) = previous_proxy_selection {
                        if let Err(rollback_err) =
                            clash_switch_selector(&client, "proxy", &previous).await
                        {
                            eprintln!("Failed to rollback proxy selector: {}", rollback_err);
                        }
                    }
                    return Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ApiResponse::error(format!(
                            "Switched proxy to SSH, but failed to switch DNS selector to direct: {}",
                            e
                        ))),
                    ));
                }
                eprintln!("Failed to switch _dns selector: {}", e);
            }
        }
    }

    {
        let mut config = state.config.lock().await;
        config.selections.insert(group, req.name);
        if let Some(dns_selection) = dns_selection {
            config.selections.insert("_dns".to_string(), dns_selection);
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

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
    Json(ApiResponse::success(
        "Subscription files loaded",
        SubFilesResponse {
            sub_dir: state.sub_dir.display().to_string(),
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
    let (active, candidates, interval_ms, fail_threshold, cooldown_ms) = {
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
            config.dns_check_interval_ms.unwrap_or(30_000),
            config.dns_fail_threshold.unwrap_or(3),
            config.dns_cooldown_ms.unwrap_or(300_000),
        )
    };

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
    let (candidates, fail_threshold, cooldown_ms) = {
        let config = state.config.lock().await;
        (
            config
                .dns_candidates
                .clone()
                .unwrap_or_else(default_dns_candidates),
            config.dns_fail_threshold.unwrap_or(3),
            config.dns_cooldown_ms.unwrap_or(300_000),
        )
    };
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
    let candidates = {
        let config = state.config.lock().await;
        config
            .dns_candidates
            .clone()
            .unwrap_or_else(default_dns_candidates)
    };
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

#[derive(Deserialize)]
struct DnsSwitchRequest {
    tag: String,
}

fn default_dns_candidates() -> Vec<String> {
    vec![
        "doh-cf".to_string(),
        "doh-google".to_string(),
        "doh-quad9".to_string(),
        "dns-direct".to_string(),
    ]
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
        (
            "doh-quad9",
            DohCandidate {
                tag: "doh-quad9",
                host: "dns.quad9.net",
                ip: "9.9.9.9",
                path: "/dns-query",
            },
        ),
    ])
}

fn build_dns_query_base64url(domain: &str) -> String {
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

    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(msg)
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

        if tag == "dns-direct" {
            entry.ok = true;
            entry.failures = 0;
            entry.last_error = None;
            healthy.insert(tag.clone(), true);
            continue;
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
        } else {
            entry.ok = false;
            entry.last_error = Some("Unknown DNS candidate".to_string());
            healthy.insert(tag.clone(), false);
        }
    }

    healthy
}

async fn dns_health_monitor(state: Arc<AppState>) {
    loop {
        let (candidates, interval_ms, fail_threshold, cooldown_ms, active) = {
            let config = state.config.lock().await;
            (
                config
                    .dns_candidates
                    .clone()
                    .unwrap_or_else(default_dns_candidates),
                config.dns_check_interval_ms.unwrap_or(30_000),
                config.dns_fail_threshold.unwrap_or(3),
                config.dns_cooldown_ms.unwrap_or(300_000),
                config
                    .dns_active
                    .clone()
                    .unwrap_or_else(|| DEFAULT_DNS_ACTIVE.to_string()),
            )
        };

        let healthy = run_dns_checks(
            &state,
            &candidates,
            fail_threshold,
            cooldown_ms,
            Duration::from_millis(2_500),
        )
        .await;

        let mut desired = active.clone();
        if !healthy.get(&active).copied().unwrap_or(false) {
            for tag in candidates.iter() {
                if healthy.get(tag).copied().unwrap_or(false) {
                    desired = tag.clone();
                    break;
                }
            }
        }

        if desired != active {
            {
                let mut config = state.config.lock().await;
                config.dns_active = Some(desired.clone());
                if let Err(e) = save_config(&config).await {
                    eprintln!("Failed to save config while switching DNS: {}", e);
                }
            }
            if is_sing_running().await {
                if let Err(e) = regenerate_and_restart(state.clone()).await {
                    eprintln!("DNS switch triggered restart failed: {}", e);
                } else {
                    println!("Switched DNS active server: {}", desired);
                }
            }
        }

        sleep(Duration::from_millis(interval_ms)).await;
    }
}

async fn apply_saved_selections(
    config: &Config,
    node_type_by_tag: Option<&HashMap<String, String>>,
) -> Result<(), String> {
    if config.selections.is_empty() {
        return Ok(());
    }

    let client = reqwest::Client::new();

    let mut ordered: Vec<(String, String)> = Vec::with_capacity(config.selections.len());

    let proxy_selection = config.selections.get("proxy").cloned();
    let proxy_is_ssh = proxy_selection
        .as_ref()
        .and_then(|name| node_type_by_tag.and_then(|m| m.get(name)))
        .map(|t| t == "ssh")
        .unwrap_or(false);

    let desired_dns_selection = if proxy_is_ssh {
        "direct".to_string()
    } else {
        config
            .selections
            .get("_dns")
            .cloned()
            .unwrap_or_else(|| "proxy".to_string())
    };

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
    let node_type_by_tag = state.node_type_by_tag.lock().await;
    let _ = apply_saved_selections(&config_clone, Some(&*node_type_by_tag)).await;
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

        match client.get("http://3.0.3.0").send().await {
            Ok(res) if res.status().is_success() => {
                match res.text().await {
                    Ok(text) => {
                        // Try to parse JSON response
                        if let Ok(info) = serde_json::from_str::<IpCheckResponse>(&text) {
                            if is_mainland_china(&info.location) {
                                println!("⚠️  Warning: Traffic appears to be using direct connection");
                                println!("   Current IP: {}", info.ip);
                                println!("   Location: {}", info.location);
                                println!("   This may indicate proxy is not working properly.");
                            } else {
                                println!("✅ Connectivity check passed!");
                                println!("   Current IP: {}", info.ip);
                                println!("   Location: {}", info.location);
                            }
                        } else {
                            println!("✅ Network is reachable (could not parse response)");
                        }
                        // Always return Ok, don't kill sing-box
                        return Ok(());
                    }
                    Err(e) => {
                        println!("✅ Network is reachable (could not read body: {})", e);
                        return Ok(());
                    }
                }
            }
            Ok(res) => {
                println!("Connectivity check: unexpected status {}", res.status());
            }
            Err(e) => {
                println!("Connectivity check failed: {}", e);
            }
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
        let active = config
            .dns_active
            .as_deref()
            .unwrap_or(DEFAULT_DNS_ACTIVE)
            .to_string();
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
                {"type": "udp", "tag": "114", "server": "114.114.114.114", "server_port": 53},
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
                },
                {
                    "type": "https",
                    "tag": "doh-quad9",
                    "server": "9.9.9.9",
                    "server_port": 443,
                    "path": "/dns-query",
                    "headers": {"Host": "dns.quad9.net"},
                    "tls": {"enabled": true, "server_name": "dns.quad9.net", "insecure": false},
                    "detour": "_dns",
                    "connect_timeout": "2s"
                }
            ]
        },
        "inbounds": [
            {"type": "tun", "tag": "tun-in", "interface_name": "sing-tun", "address": ["172.18.0.1/30"], "mtu": 9000, "auto_route": true, "strict_route": true, "stack": "system", "sniff": true, "sniff_override_destination": false }
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

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Check for root privileges
    if !Uid::effective().is_root() {
        eprintln!("Error: This application must be run as root.");
        std::process::exit(1);
    }

    // CLI args
    let mut sub_dir = PathBuf::from("sub");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(v) = arg.strip_prefix("--sub-dir=") {
            sub_dir = PathBuf::from(v);
        } else if arg == "--sub-dir" {
            if let Some(v) = args.next() {
                sub_dir = PathBuf::from(v);
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
                        let _ = apply_saved_selections(&config, Some(&node_type_by_tag)).await;
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
        sub_files: Mutex::new(loaded_subs.files.clone()),
        sub_dir_error: Mutex::new(loaded_subs.dir_error.clone()),
        node_type_by_tag: Mutex::new(node_type_by_tag),
        dns_monitor: Mutex::new(DnsMonitorState::default()),
        setup_required: AtomicBool::new(setup_required),
    });

    // DNS health monitor (best-effort). It updates config.dns_active and restarts sing-box when needed.
    {
        let state_clone = app_state.clone();
        tokio::spawn(async move { dns_health_monitor(state_clone).await });
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
