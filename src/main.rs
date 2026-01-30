use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Json, Response},
    routing::{delete, get, post, put},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use lazy_static::lazy_static;
use machine_info::Machine;
use nix::sys::signal::{kill, Signal};
use nix::unistd::{Pid, Uid};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path as StdPath, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio_tungstenite::connect_async;
use tokio::sync::{broadcast, Mutex};
use tokio::task::spawn_blocking;
use tokio::time::sleep;
use tokio::io::AsyncWriteExt;
use base64::Engine;
use chrono::Utc;
use rust_embed::RustEmbed;
use axum::response::IntoResponse;

mod tcp_tunnel;
mod full_tunnel;
mod sync;

// Version embedded at compile time
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Embed sing-box binary based on target architecture
#[cfg(target_arch = "x86_64")]
const SING_BOX_BINARY: &[u8] = include_bytes!("../embedded/sing-box-amd64");

#[cfg(target_arch = "aarch64")]
const SING_BOX_BINARY: &[u8] = include_bytes!("../embedded/sing-box-arm64");

// Embed gotty binary based on target architecture
#[cfg(target_arch = "x86_64")]
const GOTTY_BINARY: &[u8] = include_bytes!("../embedded/gotty-amd64");

#[cfg(target_arch = "aarch64")]
const GOTTY_BINARY: &[u8] = include_bytes!("../embedded/gotty-arm64");

// Embed static assets (Next.js build output) at compile time
#[derive(RustEmbed)]
#[folder = "public/"]
struct StaticAssets;

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

fn default_tunnel_set_connect_timeout_ms() -> u64 {
    10_000
}

fn default_keepalive_interval_ms() -> u64 {
    10_000
}

fn default_tunnel_set_start_batch_size() -> u64 {
    5
}

fn default_tunnel_set_start_batch_interval_ms() -> u64 {
    500
}

fn default_schedule_timezone() -> String {
    "Asia/Shanghai".to_string()
}

fn default_metrics_enabled() -> bool {
    true
}

fn default_metrics_storage_path() -> String {
    "./metrics.sqlite".to_string()
}

fn default_metrics_retention_days() -> u32 {
    7
}

fn default_metrics_sample_interval_secs() -> u64 {
    5
}

fn default_terminal_addr() -> String {
    "127.0.0.1".to_string()
}

fn default_terminal_port() -> u16 {
    DEFAULT_TERMINAL_PORT
}

fn default_terminal_command() -> String {
    "/bin/bash".to_string()
}

fn default_terminal_extra_args() -> Vec<String> {
    vec!["-w".to_string(), "--enable-idle-alert".to_string()]
}

fn default_vnc_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_vnc_port() -> u16 {
    DEFAULT_VNC_PORT
}

fn default_vnc_display() -> String {
    ":10".to_string()
}

fn default_vnc_resolution() -> String {
    "1920x1080".to_string()
}

fn default_vnc_depth() -> u16 {
    24
}

fn default_vnc_frame_rate() -> u16 {
    24
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
    SshAgent,
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
    #[serde(default = "default_tunnel_set_connect_timeout_ms")]
    connect_timeout_ms: u64,
    #[serde(default = "default_tunnel_set_start_batch_size")]
    start_batch_size: u64,
    #[serde(default = "default_tunnel_set_start_batch_interval_ms")]
    start_batch_interval_ms: u64,

}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum SyncPathKind {
    File,
    Dir,
    Missing,
}

impl Default for SyncPathKind {
    fn default() -> Self {
        SyncPathKind::Missing
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SyncLocalPath {
    path: String,
    #[serde(default)]
    kind: SyncPathKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SyncSshConfig {
    host: String,
    #[serde(default = "default_ssh_port")]
    port: u16,
    username: String,
    auth: TcpTunnelAuth,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
struct SyncOptions {
    #[serde(default)]
    delete: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    include: Vec<String>,
    #[serde(default = "default_compression_level")]
    compression_level: u8,
    #[serde(default)]
    compression_threads: u8,
    #[serde(default)]
    incremental: bool,
    #[serde(default)]
    preserve_permissions: bool,
    #[serde(default)]
    follow_symlinks: bool,
}

fn default_compression_level() -> u8 { 3 }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
struct SyncSchedule {
    #[serde(default = "default_true")]
    enabled: bool,
    cron: String,
    #[serde(default = "default_schedule_timezone")]
    timezone: String,
}

impl Default for SyncSchedule {
    fn default() -> Self {
        SyncSchedule {
            enabled: true,
            cron: String::new(),
            timezone: default_schedule_timezone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
struct SyncConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    local_paths: Vec<SyncLocalPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote_path: Option<String>,
    ssh: SyncSshConfig,
    #[serde(default)]
    options: SyncOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schedule: Option<SyncSchedule>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            id: String::new(),
            name: None,
            enabled: false,
            local_paths: Vec::new(),
            remote_path: None,
            ssh: SyncSshConfig {
                host: String::new(),
                port: default_ssh_port(),
                username: String::new(),
                auth: TcpTunnelAuth::Password {
                    password: String::new(),
                },
            },
            options: SyncOptions::default(),
            schedule: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
struct TerminalConfigLegacy {
    enabled: bool,
    addr: String,
    port: u16,
    command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    command_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_password: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extra_args: Vec<String>,
}

impl Default for TerminalConfigLegacy {
    fn default() -> Self {
        TerminalConfigLegacy {
            enabled: false,
            addr: default_terminal_addr(),
            port: default_terminal_port(),
            command: default_terminal_command(),
            command_args: Vec::new(),
            auth_username: None,
            auth_password: None,
            extra_args: default_terminal_extra_args(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
struct TerminalNodeConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    enabled: bool,
    addr: String,
    port: u16,
    command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    command_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_password: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extra_args: Vec<String>,
}

impl Default for TerminalNodeConfig {
    fn default() -> Self {
        TerminalNodeConfig {
            id: String::new(),
            name: None,
            enabled: false,
            addr: default_terminal_addr(),
            port: default_terminal_port(),
            command: default_terminal_command(),
            command_args: Vec::new(),
            auth_username: None,
            auth_password: None,
            extra_args: default_terminal_extra_args(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
struct VncSessionConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    enabled: bool,
    addr: String,
    port: u16,
    display: String,
    resolution: String,
    depth: u16,
    frame_rate: u16,
    #[serde(default)]
    view_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

impl Default for VncSessionConfig {
    fn default() -> Self {
        VncSessionConfig {
            id: String::new(),
            name: None,
            enabled: false,
            addr: default_vnc_addr(),
            port: default_vnc_port(),
            display: default_vnc_display(),
            resolution: default_vnc_resolution(),
            depth: default_vnc_depth(),
            frame_rate: default_vnc_frame_rate(),
            view_only: false,
            password: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
struct AppConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vnc_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display: Option<String>,
    #[serde(default)]
    command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    env: HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            id: String::new(),
            name: None,
            enabled: false,
            vnc_session_id: None,
            display: None,
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct MetricsConfig {
    #[serde(default = "default_metrics_enabled")]
    enabled: bool,
    #[serde(default = "default_metrics_storage_path")]
    storage_path: String,
    #[serde(default = "default_metrics_retention_days")]
    retention_days: u32,
    #[serde(default = "default_metrics_sample_interval_secs")]
    sample_interval_secs: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        MetricsConfig {
            enabled: default_metrics_enabled(),
            storage_path: default_metrics_storage_path(),
            retention_days: default_metrics_retention_days(),
            sample_interval_secs: default_metrics_sample_interval_secs(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum SubscriptionSource {
    Url { url: String },
    Git { repo: String },
    Path { path: String },
}

#[derive(Clone, Serialize, Deserialize)]
struct SubscriptionConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(flatten)]
    source: SubscriptionSource,
}

// Host configuration for SSH connections
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostAuth {
    Password {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
    },
    PrivateKeyPath {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    SshAgent,
}

fn default_private_key_path() -> Option<String> {
    let candidates = ["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"];
    for name in candidates {
        let path = format!("/root/.ssh/{}", name);
        if fs::metadata(&path).map(|meta| meta.is_file()).unwrap_or(false) {
            return Some(path);
        }
    }
    None
}

fn resolve_private_key_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    default_private_key_path().ok_or_else(|| "Private key path is required".to_string())
}

async fn test_host_connection(cfg: &HostConfig) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new("ssh");
    cmd.arg("-o").arg("BatchMode=yes")
       .arg("-o").arg("ConnectTimeout=5")
       .arg("-o").arg("StrictHostKeyChecking=no")
       .arg("-p").arg(cfg.port.to_string());

    match &cfg.auth {
        HostAuth::PrivateKeyPath { path, .. } => {
            let resolved_path = resolve_private_key_path(path)?;
            cmd.arg("-i").arg(resolved_path);
        }
        _ => {}
    }

    cmd.arg(format!("{}@{}", cfg.username, cfg.host))
       .arg("echo")
       .arg("ok");

    let output = cmd.output().await.map_err(|e| format!("Failed to run SSH: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Connection failed: {}", stderr.trim()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct HostConfig {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    host: String,
    #[serde(default = "default_ssh_port")]
    port: u16,
    username: String,
    auth: HostAuth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sing_box_home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,  // 登录密码
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal: Option<TerminalConfigLegacy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    terminals: Vec<TerminalNodeConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    vnc_sessions: Vec<VncSessionConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    apps: Vec<AppConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    syncs: Vec<SyncConfig>,
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

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    subscriptions: Vec<SubscriptionConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hosts: Vec<HostConfig>,

    #[serde(default)]
    metrics: MetricsConfig,
}

const DEFAULT_PORT: u16 = 6161;
const DEFAULT_TERMINAL_PORT: u16 = 7681;
const DEFAULT_VNC_PORT: u16 = 7900;
const DEFAULT_DNS_ACTIVE: &str = "doh-cf";
const KASMVNC_USER: &str = "user";
const KASMVNC_HTTPD_DIR: &str = "/usr/share/kasmvnc/www";
const KASMVNC_DEFAULTS_JS: &str = "/usr/share/kasmvnc/www/kasmvnc-defaults.js";
const KASMVNC_BASE_HOME: &str = "/app/kasmvnc";

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

#[derive(Deserialize)]
struct PasswordChangeRequest {
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
    Password { password: String },
    PrivateKeyPath { path: String },
    SshAgent,
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
struct TcpTunnelSaveResponse {
    item: TcpTunnelItem,
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
    host_id: Option<String>,
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
    #[serde(default)]
    connect_timeout_ms: Option<u64>,
    #[serde(default)]
    start_batch_size: Option<u64>,
    #[serde(default)]
    start_batch_interval_ms: Option<u64>,
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
    connect_timeout_ms: u64,
    start_batch_size: u64,
    start_batch_interval_ms: u64,
}

#[derive(Serialize)]
struct TcpTunnelSetSaveResponse {
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum SyncState {
    Stopped,
    Running,
    Error,
}

#[derive(Clone, Debug, Serialize)]
struct SyncErrorInfo {
    message: String,
    at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
struct SyncRuntimeStatus {
    state: SyncState,
    running_path: Option<String>,
    last_run_at_ms: Option<i64>,
    last_ok_at_ms: Option<i64>,
    last_error: Option<SyncErrorInfo>,
}

impl Default for SyncRuntimeStatus {
    fn default() -> Self {
        Self {
            state: SyncState::Stopped,
            running_path: None,
            last_run_at_ms: None,
            last_ok_at_ms: None,
            last_error: None,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SyncAuthPublic {
    Password { password: String },
    PrivateKeyPath { path: String },
    SshAgent,
}

#[derive(Serialize)]
struct SyncItem {
    id: String,
    name: Option<String>,
    enabled: bool,
    local_paths: Vec<SyncLocalPath>,
    remote_path: Option<String>,
    ssh: SyncSshInfo,
    auth: SyncAuthPublic,
    options: SyncOptions,
    schedule: Option<SyncSchedule>,
    status: SyncRuntimeStatus,
}

#[derive(Serialize)]
struct SyncSshInfo {
    host: String,
    port: u16,
    username: String,
}

#[derive(Serialize)]
struct SyncListResponse {
    items: Vec<SyncItem>,
}

#[derive(Serialize)]
struct SyncSaveResponse {
    item: SyncItem,
}

#[derive(Serialize)]
struct SyncTestResponse {
    ok: bool,
}

#[derive(Deserialize)]
struct SyncUpsertRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    host_id: Option<String>,
    local_paths: Vec<String>,
    #[serde(default)]
    remote_path: Option<String>,
    ssh_host: String,
    #[serde(default)]
    ssh_port: Option<u16>,
    username: String,
    auth: TcpTunnelAuth,
    #[serde(default)]
    options: SyncOptions,
    #[serde(default)]
    schedule: Option<SyncSchedule>,
}

// Host API types
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum HostAuthTypePublic {
    Password,
    PrivateKeyPath,
    SshAgent,
}

#[derive(Serialize)]
struct HostItem {
    id: String,
    name: Option<String>,
    host: String,
    port: u16,
    username: String,
    auth_type: HostAuthTypePublic,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key_path: Option<String>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
}

#[derive(Serialize)]
struct HostListResponse {
    items: Vec<HostItem>,
}

#[derive(Deserialize)]
struct HostUpsertRequest {
    #[serde(default)]
    name: Option<String>,
    host: String,
    #[serde(default)]
    port: Option<u16>,
    username: String,
    auth_type: String,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    private_key_path: Option<String>,
    #[serde(default)]
    private_key_passphrase: Option<String>,
}

#[derive(Serialize)]
struct HostDefaultKeyPathResponse {
    path: Option<String>,
}

fn generate_host_id() -> String {
    format!("h-{}", uuid::Uuid::new_v4())
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

struct SystemMonitor {
    machine: Mutex<Machine>,
    info_cache: Mutex<Option<serde_json::Value>>,
    status_cache: Mutex<Option<serde_json::Value>>,
}

impl SystemMonitor {
    fn new() -> Self {
        Self {
            machine: Mutex::new(Machine::new()),
            info_cache: Mutex::new(None),
            status_cache: Mutex::new(None),
        }
    }
}

struct AppState {
    config: Mutex<Config>,
    sing_box_home: String,
    subscriptions_root: PathBuf,
    subscription_status: Mutex<HashMap<String, SubscriptionRuntime>>,
    node_type_by_tag: Mutex<HashMap<String, String>>,
    dns_monitor: Mutex<DnsMonitorState>,
    proxy_monitor: Mutex<ProxyMonitorState>,
    setup_required: AtomicBool,
    tcp_tunnel: tcp_tunnel::TunnelManager,
    full_tunnel: full_tunnel::FullTunnelManager,
    sync_manager: sync::SyncManager,
    system_monitor: SystemMonitor,
    metrics_config: MetricsConfig,
}

#[derive(Serialize)]
struct SubscriptionRuntime {
    files: Vec<SubFileStatus>,
    error: Option<String>,
    updated_at: Option<i64>,
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
struct TerminalRuntimeStatus {
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uptime_secs: Option<u64>,
}

#[derive(Serialize, Clone)]
struct VncRuntimeStatus {
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uptime_secs: Option<u64>,
}

#[derive(Serialize, Clone)]
struct AppRuntimeStatus {
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uptime_secs: Option<u64>,
}

#[derive(Serialize)]
struct TerminalItem {
    id: String,
    name: Option<String>,
    enabled: bool,
    addr: String,
    port: u16,
    command: String,
    command_args: Vec<String>,
    auth_username: Option<String>,
    auth_password: Option<String>,
    extra_args: Vec<String>,
    status: TerminalRuntimeStatus,
}

#[derive(Serialize)]
struct VncSessionItem {
    id: String,
    name: Option<String>,
    enabled: bool,
    addr: String,
    port: u16,
    display: String,
    resolution: String,
    depth: u16,
    frame_rate: u16,
    view_only: bool,
    password: Option<String>,
    status: VncRuntimeStatus,
}

#[derive(Serialize)]
struct AppItem {
    id: String,
    name: Option<String>,
    enabled: bool,
    vnc_session_id: Option<String>,
    display: Option<String>,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    status: AppRuntimeStatus,
}

#[derive(Serialize)]
struct TerminalListResponse {
    items: Vec<TerminalItem>,
}

#[derive(Serialize)]
struct VncSessionListResponse {
    items: Vec<VncSessionItem>,
}

#[derive(Serialize)]
struct AppListResponse {
    items: Vec<AppItem>,
}

#[derive(Deserialize)]
struct TerminalUpsertRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    addr: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    command_args: Option<Vec<String>>,
    #[serde(default)]
    auth_username: Option<String>,
    #[serde(default)]
    auth_password: Option<String>,
    #[serde(default)]
    extra_args: Option<Vec<String>>,
    #[serde(default)]
    restart: bool,
    #[serde(default)]
    clear_auth: bool,
}

#[derive(Deserialize)]
struct VncSessionUpsertRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    addr: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    display: Option<String>,
    #[serde(default)]
    resolution: Option<String>,
    #[serde(default)]
    depth: Option<u16>,
    #[serde(default)]
    frame_rate: Option<u16>,
    #[serde(default)]
    view_only: Option<bool>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    restart: bool,
}

#[derive(Deserialize)]
struct AppUpsertRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    vnc_session_id: Option<String>,
    #[serde(default)]
    display: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    env: Option<HashMap<String, String>>,
    #[serde(default)]
    restart: bool,
}

#[derive(Serialize)]
struct AppTemplateItem {
    id: String,
    name: String,
    description: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

#[derive(Serialize)]
struct AppTemplateListResponse {
    items: Vec<AppTemplateItem>,
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
    subscription_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct SubFilesResponse {
    sub_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sub_source: Option<SubscriptionSourceResponse>,
    files: Vec<SubFileStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum SubscriptionSourceResponse {
    Url { url: String },
    Git { repo: String, workdir: String },
    Path { path: String },
}

#[derive(Serialize)]
struct SubscriptionItem {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    enabled: bool,
    source: SubscriptionSourceResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
    files: Vec<SubFileStatus>,
}

#[derive(Serialize)]
struct SubscriptionListResponse {
    items: Vec<SubscriptionItem>,
}

#[derive(Serialize)]
struct SubscriptionSaveResponse {
    item: SubscriptionItem,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum SubscriptionSourceInput {
    Url { url: String },
    Git { repo: String },
    Path { path: String },
}

#[derive(Deserialize)]
struct SubscriptionUpsertRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(flatten)]
    source: SubscriptionSourceInput,
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

struct GottyProcess {
    child: tokio::process::Child,
    started_at: Instant,
}

struct VncProcess {
    child: tokio::process::Child,
    started_at: Instant,
}

struct AppProcess {
    child: tokio::process::Child,
    started_at: Instant,
}

lazy_static! {
    static ref SING_PROCESS: Mutex<Option<SingBoxProcess>> = Mutex::new(None);
    static ref GOTTY_PROCESSES: Mutex<HashMap<String, GottyProcess>> = Mutex::new(HashMap::new());
    static ref VNC_PROCESSES: Mutex<HashMap<String, VncProcess>> = Mutex::new(HashMap::new());
    static ref APP_PROCESSES: Mutex<HashMap<String, AppProcess>> = Mutex::new(HashMap::new());
    static ref WS_CONNECT_ERROR_LOGS: Mutex<HashMap<String, Instant>> = Mutex::new(HashMap::new());
    static ref LOG_BROADCAST: broadcast::Sender<String> = {
        let (tx, _rx) = broadcast::channel(1000);
        tx
    };
    static ref LOG_BUFFER: StdMutex<VecDeque<String>> = StdMutex::new(VecDeque::with_capacity(1000));
}

// ============================================================================
// Logging Infrastructure
// ============================================================================

fn broadcast_log(level: &str, message: &str) {
    use chrono::FixedOffset;
    let utc8 = FixedOffset::east_opt(8 * 3600).unwrap();
    let time_str = Utc::now().with_timezone(&utc8).format("%Y-%m-%d %H:%M:%S").to_string();
    let entry = serde_json::json!({
        "time": time_str,
        "level": level,
        "message": message
    });
    let entry_str = entry.to_string();
    {
        let mut buffer = LOG_BUFFER.lock().expect("log buffer lock poisoned");
        buffer.push_back(entry_str.clone());
        if buffer.len() > 1000 {
            buffer.pop_front();
        }
    }
    let _ = LOG_BROADCAST.send(entry_str);
}

macro_rules! log_info {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
        crate::broadcast_log("info", &msg);
    }};
}

macro_rules! log_error {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        eprintln!("{}", msg);
        crate::broadcast_log("error", &msg);
    }};
}

macro_rules! log_warning {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
        crate::broadcast_log("warning", &msg);
    }};
}

/// Spawns a child process with stdout/stderr piped and captured to the log broadcast.
/// Returns the spawned Child. The caller is responsible for storing/managing the child.
fn spawn_with_log_capture(
    command: &mut tokio::process::Command,
    process_name: String,
) -> Result<tokio::process::Child, std::io::Error> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;

    // Capture stdout
    if let Some(stdout) = child.stdout.take() {
        let name = process_name.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("[{}] {}", name, line);
                let _ = std::io::stdout().flush();
                broadcast_log("info", &format!("[{}] {}", name, line));
            }
        });
    }

    // Capture stderr
    if let Some(stderr) = child.stderr.take() {
        let name = process_name;
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("[{}] {}", name, line);
                let _ = std::io::stderr().flush();
                broadcast_log("error", &format!("[{}] {}", name, line));
            }
        });
    }

    Ok(child)
}

// ============================================================================
// API Handlers
// ============================================================================

/// Serve static files from embedded assets
async fn serve_static(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/');

    // 1. Try exact path match
    if let Some(content) = StaticAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, mime.as_ref())],
            content.data.into_owned(),
        ).into_response();
    }

    // 2. Try with .html extension (for Next.js static export)
    let html_path = format!("{}.html", path);
    if let Some(content) = StaticAssets::get(&html_path) {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/html")],
            content.data.into_owned(),
        ).into_response();
    }

    // 3. Try with /index.html (for directory index)
    let index_path = format!("{}/index.html", path);
    if let Some(content) = StaticAssets::get(&index_path) {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/html")],
            content.data.into_owned(),
        ).into_response();
    }

    // 4. Fall back to SPA routing (serve root index.html)
    spa_fallback().await
}

/// SPA fallback: serve index.html for all unmatched routes (client-side routing)
async fn spa_fallback() -> Response {
    match StaticAssets::get("index.html") {
        Some(content) => {
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/html")],
                content.data.into_owned(),
            ).into_response()
        }
        None => {
            (StatusCode::INTERNAL_SERVER_ERROR, "index.html not found").into_response()
        }
    }
}

/// POST /api/login - User login
async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Json<ApiResponse<LoginResponse>> {
    let config = state.config.lock().await;

    // 获取配置中的密码，如果未设置则使用默认密码 "admin123"
    let expected_password = config.password.as_deref().unwrap_or("admin123");

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

/// POST /api/password - Update login password
async fn update_password(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PasswordChangeRequest>,
) -> Json<ApiResponse<()>> {
    let password = req.password.trim();
    if password.len() < 4 {
        return Json(ApiResponse::error("密码至少 4 位"));
    }

    let mut config = state.config.lock().await;
    config.password = Some(password.to_string());
    if let Err(e) = save_config(&config).await {
        return Json(ApiResponse::error(format!("保存配置失败: {}", e)));
    }

    Json(ApiResponse::success_no_data("密码已更新"))
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

async fn refresh_system_metrics(state: &AppState) -> Result<(), String> {
    let mut machine = state.system_monitor.machine.lock().await;
    let mut info = machine.system_info();
    let status = machine
        .system_status()
        .map_err(|e| format!("Failed to read system status: {}", e))?;
    let graphics = machine.graphics_status();
    drop(machine);

    if info.processor.brand.trim().is_empty() {
        if let Some(fallback) = read_cpu_brand_fallback(&info) {
            info.processor.brand = fallback;
        }
    }

    let sample_period_secs = state.metrics_config.sample_interval_secs.max(1);
    let (primary_disk_used, primary_disk_total) =
        select_primary_disk(&info).unwrap_or((0, 0));
    let gpu_percent = average_gpu_percent(&graphics);

    let mut seen_mounts: HashSet<String> = HashSet::new();
    let disks_usage = info
        .disks
        .iter()
        .filter(|disk| {
            let key = if disk.mount_point.is_empty() {
                &disk.name
            } else {
                &disk.mount_point
            };
            seen_mounts.insert(key.to_string())
        })
        .map(|disk| {
            json!({
                "name": disk.name,
                "used": disk.size.saturating_sub(disk.available),
                "total": disk.size
            })
        })
        .collect::<Vec<_>>();

    let info_value = serde_json::to_value(&info)
        .map_err(|e| format!("Failed to serialize system info: {}", e))?;
    let uptime_secs = read_uptime_secs();
    let status_value = json!({
        "timestamp": chrono::Utc::now().timestamp(),
        "samplePeriodSecs": sample_period_secs,
        "cpuPercent": status.cpu,
        "memoryUsedKb": status.memory,
        "uptimeSecs": uptime_secs,
        "graphics": graphics,
        "disks": disks_usage,
        "nvidiaAvailable": !graphics.is_empty()
    });

    *state.system_monitor.info_cache.lock().await = Some(info_value);
    *state.system_monitor.status_cache.lock().await = Some(status_value);

    if state.metrics_config.enabled {
        let record = MetricsRecord {
            timestamp: chrono::Utc::now().timestamp(),
            cpu_percent: status.cpu,
            memory_used_kb: status.memory,
            gpu_percent,
            disk_used_bytes: primary_disk_used,
            disk_total_bytes: primary_disk_total,
        };
        write_metrics_record(&state.metrics_config, record).await?;
    }

    Ok(())
}

fn read_uptime_secs() -> Option<u64> {
    let contents = fs::read_to_string("/proc/uptime").ok()?;
    let first = contents.split_whitespace().next()?;
    let seconds = first.parse::<f64>().ok()?;
    if seconds.is_finite() && seconds >= 0.0 {
        Some(seconds.floor() as u64)
    } else {
        None
    }
}

fn read_cpu_brand_fallback(info: &machine_info::SystemInfo) -> Option<String> {
    if let Some(model) = info.model.as_ref() {
        let trimmed = model.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let cpuinfo = fs::read_to_string("/proc/cpuinfo").ok()?;
    let mut hardware = None;
    let mut processor = None;

    for line in cpuinfo.lines() {
        if let Some(value) = line.strip_prefix("model name") {
            let name = value.trim_start_matches(':').trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        if let Some(value) = line.strip_prefix("Hardware") {
            let name = value.trim_start_matches(':').trim();
            if !name.is_empty() {
                hardware = Some(name.to_string());
            }
        }
        if let Some(value) = line.strip_prefix("Processor") {
            let name = value.trim_start_matches(':').trim();
            if !name.is_empty() {
                processor = Some(name.to_string());
            }
        }
    }

    hardware.or(processor)
}

#[derive(Serialize)]
struct MetricsPoint {
    timestamp: i64,
    cpu_percent: i32,
    memory_used_kb: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    gpu_percent: Option<i32>,
    disk_used_bytes: u64,
    disk_total_bytes: u64,
}

struct MetricsRecord {
    timestamp: i64,
    cpu_percent: i32,
    memory_used_kb: i32,
    gpu_percent: Option<i32>,
    disk_used_bytes: u64,
    disk_total_bytes: u64,
}

#[derive(Deserialize)]
struct MetricsQuery {
    range: Option<String>,
    step: Option<String>,
}

fn parse_duration_to_secs(input: &str) -> Option<i64> {
    if input.len() < 2 {
        return None;
    }
    let (value, unit) = input.split_at(input.len() - 1);
    let value = value.parse::<i64>().ok()?;
    match unit {
        "s" => Some(value),
        "m" => Some(value * 60),
        "h" => Some(value * 3600),
        "d" => Some(value * 86400),
        _ => None,
    }
}

fn default_step_label(range_secs: i64) -> String {
    if range_secs <= 3600 {
        "30s".to_string()
    } else if range_secs <= 21600 {
        "1m".to_string()
    } else if range_secs <= 86400 {
        "5m".to_string()
    } else {
        "15m".to_string()
    }
}

fn select_primary_disk(info: &machine_info::SystemInfo) -> Option<(u64, u64)> {
    if let Some(disk) = info.disks.iter().find(|disk| disk.mount_point == "/") {
        return Some((disk.size.saturating_sub(disk.available), disk.size));
    }
    info.disks
        .first()
        .map(|disk| (disk.size.saturating_sub(disk.available), disk.size))
}

fn average_gpu_percent(graphics: &[machine_info::GraphicsUsage]) -> Option<i32> {
    if graphics.is_empty() {
        return None;
    }
    let sum: u32 = graphics.iter().map(|g| g.gpu).sum();
    Some((sum / graphics.len() as u32) as i32)
}

fn init_metrics_db(path: &str) -> Result<(), String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open metrics db: {}", e))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS system_metrics (
            timestamp INTEGER NOT NULL,
            cpu_percent INTEGER NOT NULL,
            memory_used_kb INTEGER NOT NULL,
            gpu_percent INTEGER,
            disk_used_bytes INTEGER NOT NULL,
            disk_total_bytes INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_system_metrics_ts ON system_metrics(timestamp);",
    )
    .map_err(|e| format!("Failed to init metrics db: {}", e))?;
    Ok(())
}

fn insert_metrics_record(path: &str, record: &MetricsRecord) -> Result<(), String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open metrics db: {}", e))?;
    conn.execute(
        "INSERT INTO system_metrics (timestamp, cpu_percent, memory_used_kb, gpu_percent, disk_used_bytes, disk_total_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            record.timestamp,
            record.cpu_percent,
            record.memory_used_kb,
            record.gpu_percent,
            record.disk_used_bytes as i64,
            record.disk_total_bytes as i64,
        ],
    )
    .map_err(|e| format!("Failed to insert metrics: {}", e))?;
    Ok(())
}

fn prune_metrics(path: &str, cutoff_ts: i64) -> Result<(), String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open metrics db: {}", e))?;
    conn.execute(
        "DELETE FROM system_metrics WHERE timestamp < ?1",
        params![cutoff_ts],
    )
    .map_err(|e| format!("Failed to prune metrics: {}", e))?;
    Ok(())
}

async fn write_metrics_record(
    config: &MetricsConfig,
    record: MetricsRecord,
) -> Result<(), String> {
    let storage_path = config.storage_path.clone();
    let retention_days = config.retention_days;
    let cutoff_ts = record.timestamp - (retention_days as i64 * 86400);
    spawn_blocking(move || {
        init_metrics_db(&storage_path)?;
        insert_metrics_record(&storage_path, &record)?;
        if retention_days > 0 {
            prune_metrics(&storage_path, cutoff_ts)?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Metrics task failed: {}", e))??;
    Ok(())
}

fn load_metrics_series(
    path: &str,
    start_ts: i64,
    end_ts: i64,
    step_secs: i64,
) -> Result<Vec<MetricsPoint>, String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open metrics db: {}", e))?;
    let mut stmt = conn
        .prepare(
            "WITH bucketed AS (
                SELECT
                    timestamp,
                    cpu_percent,
                    memory_used_kb,
                    gpu_percent,
                    disk_used_bytes,
                    disk_total_bytes,
                    (timestamp / ?1) * ?1 AS bucket_ts
                FROM system_metrics
                WHERE timestamp >= ?2 AND timestamp <= ?3
            ),
            latest_in_bucket AS (
                SELECT bucket_ts, MAX(timestamp) AS latest_ts
                FROM bucketed
                GROUP BY bucket_ts
            )
            SELECT
                b.bucket_ts AS timestamp,
                CAST(AVG(b.cpu_percent) AS INTEGER) AS cpu_percent,
                CAST(AVG(b.gpu_percent) AS INTEGER) AS gpu_percent,
                b2.memory_used_kb AS memory_used_kb,
                b2.disk_used_bytes AS disk_used_bytes,
                b2.disk_total_bytes AS disk_total_bytes
            FROM bucketed b
            JOIN latest_in_bucket l ON b.bucket_ts = l.bucket_ts
            JOIN bucketed b2 ON b2.bucket_ts = l.bucket_ts AND b2.timestamp = l.latest_ts
            GROUP BY b.bucket_ts, b2.memory_used_kb, b2.disk_used_bytes, b2.disk_total_bytes
            ORDER BY b.bucket_ts ASC",
        )
        .map_err(|e| format!("Failed to prepare metrics query: {}", e))?;
    let rows = stmt
        .query_map(params![step_secs, start_ts, end_ts], |row| {
            Ok(MetricsPoint {
                timestamp: row.get(0)?,
                cpu_percent: row.get(1)?,
                gpu_percent: row.get(2)?,
                memory_used_kb: row.get(3)?,
                disk_used_bytes: row.get::<_, i64>(4)? as u64,
                disk_total_bytes: row.get::<_, i64>(5)? as u64,
            })
        })
        .map_err(|e| format!("Failed to load metrics: {}", e))?;

    let mut points = Vec::new();
    for row in rows {
        points.push(row.map_err(|e| format!("Failed to parse metrics row: {}", e))?);
    }
    Ok(points)
}

async fn get_system_info(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    {
        let cache = state.system_monitor.info_cache.lock().await;
        if let Some(value) = cache.as_ref() {
            return Json(ApiResponse::success("System info", value.clone()));
        }
    }

    if let Err(e) = refresh_system_metrics(&state).await {
        return Json(ApiResponse::error(e));
    }

    let cache = state.system_monitor.info_cache.lock().await;
    match cache.as_ref() {
        Some(value) => Json(ApiResponse::success("System info", value.clone())),
        None => Json(ApiResponse::error("System info not available")),
    }
}

async fn get_system_status(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<serde_json::Value>> {
    {
        let cache = state.system_monitor.status_cache.lock().await;
        if let Some(value) = cache.as_ref() {
            return Json(ApiResponse::success("System status", value.clone()));
        }
    }

    if let Err(e) = refresh_system_metrics(&state).await {
        return Json(ApiResponse::error(e));
    }

    let cache = state.system_monitor.status_cache.lock().await;
    match cache.as_ref() {
        Some(value) => Json(ApiResponse::success("System status", value.clone())),
        None => Json(ApiResponse::error("System status not available")),
    }
}

async fn get_system_metrics(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MetricsQuery>,
) -> Json<ApiResponse<serde_json::Value>> {
    if !state.metrics_config.enabled {
        return Json(ApiResponse::error("Metrics storage is disabled"));
    }

    let range_label = query.range.unwrap_or_else(|| "1h".to_string());
    let range_secs = match parse_duration_to_secs(&range_label) {
        Some(value) if value > 0 => value,
        _ => return Json(ApiResponse::error("Invalid range")),
    };

    let step_label = query
        .step
        .unwrap_or_else(|| default_step_label(range_secs));
    let step_secs = match parse_duration_to_secs(&step_label) {
        Some(value) if value > 0 => value,
        _ => return Json(ApiResponse::error("Invalid step")),
    };

    if step_secs > range_secs {
        return Json(ApiResponse::error("Step must be <= range"));
    }

    let end_ts = chrono::Utc::now().timestamp();
    let start_ts = end_ts - range_secs;
    let storage_path = state.metrics_config.storage_path.clone();

    let result = spawn_blocking(move || {
        init_metrics_db(&storage_path)?;
        load_metrics_series(&storage_path, start_ts, end_ts, step_secs)
    })
    .await
    .map_err(|e| format!("Metrics task failed: {}", e));

    let series = match result {
        Ok(Ok(series)) => series,
        Ok(Err(err)) => return Json(ApiResponse::error(err)),
        Err(err) => return Json(ApiResponse::error(err)),
    };

    Json(ApiResponse::success(
        "System metrics",
        json!({
            "range": range_label,
            "step": step_label,
            "series": series
        }),
    ))
}

async fn get_terminal_runtime_status(id: &str) -> TerminalRuntimeStatus {
    let mut lock = GOTTY_PROCESSES.lock().await;
    if let Some(proc) = lock.get_mut(id) {
        match proc.child.try_wait() {
            Ok(Some(_)) => {
                lock.remove(id);
                TerminalRuntimeStatus {
                    running: false,
                    pid: None,
                    uptime_secs: None,
                }
            }
            Ok(None) => TerminalRuntimeStatus {
                running: true,
                pid: proc.child.id(),
                uptime_secs: Some(proc.started_at.elapsed().as_secs()),
            },
            Err(_) => {
                lock.remove(id);
                TerminalRuntimeStatus {
                    running: false,
                    pid: None,
                    uptime_secs: None,
                }
            }
        }
    } else {
        TerminalRuntimeStatus {
            running: false,
            pid: None,
            uptime_secs: None,
        }
    }
}

async fn get_vnc_runtime_status(id: &str) -> VncRuntimeStatus {
    let mut lock = VNC_PROCESSES.lock().await;
    if let Some(proc) = lock.get_mut(id) {
        match proc.child.try_wait() {
            Ok(Some(_)) => {
                lock.remove(id);
                VncRuntimeStatus {
                    running: false,
                    pid: None,
                    uptime_secs: None,
                }
            }
            Ok(None) => VncRuntimeStatus {
                running: true,
                pid: proc.child.id(),
                uptime_secs: Some(proc.started_at.elapsed().as_secs()),
            },
            Err(_) => {
                lock.remove(id);
                VncRuntimeStatus {
                    running: false,
                    pid: None,
                    uptime_secs: None,
                }
            }
        }
    } else {
        VncRuntimeStatus {
            running: false,
            pid: None,
            uptime_secs: None,
        }
    }
}

async fn get_app_runtime_status(id: &str) -> AppRuntimeStatus {
    let mut lock = APP_PROCESSES.lock().await;
    if let Some(proc) = lock.get_mut(id) {
        match proc.child.try_wait() {
            Ok(Some(_)) => {
                lock.remove(id);
                AppRuntimeStatus {
                    running: false,
                    pid: None,
                    uptime_secs: None,
                }
            }
            Ok(None) => AppRuntimeStatus {
                running: true,
                pid: proc.child.id(),
                uptime_secs: Some(proc.started_at.elapsed().as_secs()),
            },
            Err(_) => {
                lock.remove(id);
                AppRuntimeStatus {
                    running: false,
                    pid: None,
                    uptime_secs: None,
                }
            }
        }
    } else {
        AppRuntimeStatus {
            running: false,
            pid: None,
            uptime_secs: None,
        }
    }
}

fn build_terminal_item(cfg: TerminalNodeConfig, status: TerminalRuntimeStatus) -> TerminalItem {
    TerminalItem {
        id: cfg.id,
        name: cfg.name,
        enabled: cfg.enabled,
        addr: cfg.addr,
        port: cfg.port,
        command: cfg.command,
        command_args: cfg.command_args,
        auth_username: cfg.auth_username,
        auth_password: cfg.auth_password,
        extra_args: cfg.extra_args,
        status,
    }
}

fn build_vnc_session_item(cfg: VncSessionConfig, status: VncRuntimeStatus) -> VncSessionItem {
    VncSessionItem {
        id: cfg.id,
        name: cfg.name,
        enabled: cfg.enabled,
        addr: cfg.addr,
        port: cfg.port,
        display: cfg.display,
        resolution: cfg.resolution,
        depth: cfg.depth,
        frame_rate: cfg.frame_rate,
        view_only: cfg.view_only,
        password: cfg.password,
        status,
    }
}

fn build_app_item(cfg: AppConfig, status: AppRuntimeStatus) -> AppItem {
    AppItem {
        id: cfg.id,
        name: cfg.name,
        enabled: cfg.enabled,
        vnc_session_id: cfg.vnc_session_id,
        display: cfg.display,
        command: cfg.command,
        args: cfg.args,
        env: cfg.env,
        status,
    }
}

async fn get_terminals(State(state): State<Arc<AppState>>) -> Json<ApiResponse<TerminalListResponse>> {
    let terminals = { state.config.lock().await.terminals.clone() };
    let mut items = Vec::with_capacity(terminals.len());
    for t in terminals {
        let status = get_terminal_runtime_status(&t.id).await;
        items.push(build_terminal_item(t, status));
    }
    Json(ApiResponse::success("Terminals", TerminalListResponse { items }))
}

async fn get_vnc_sessions(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<VncSessionListResponse>> {
    let sessions = { state.config.lock().await.vnc_sessions.clone() };
    let mut items = Vec::with_capacity(sessions.len());
    for s in sessions {
        let status = get_vnc_runtime_status(&s.id).await;
        items.push(build_vnc_session_item(s, status));
    }
    Json(ApiResponse::success(
        "VNC 会话列表",
        VncSessionListResponse { items },
    ))
}

async fn get_apps(State(state): State<Arc<AppState>>) -> Json<ApiResponse<AppListResponse>> {
    let apps = { state.config.lock().await.apps.clone() };
    let mut items = Vec::with_capacity(apps.len());
    for a in apps {
        let status = get_app_runtime_status(&a.id).await;
        items.push(build_app_item(a, status));
    }
    Json(ApiResponse::success("Apps", AppListResponse { items }))
}

async fn get_app_templates_handler() -> Json<ApiResponse<AppTemplateListResponse>> {
    Json(ApiResponse::success(
        "App templates",
        AppTemplateListResponse {
            items: app_templates(),
        },
    ))
}

async fn create_vnc_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VncSessionUpsertRequest>,
) -> Result<Json<ApiResponse<VncSessionItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let id = generate_vnc_session_id();
    let mut cfg = normalize_vnc_session_request(req, id.clone(), None)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    {
        let mut config_guard = state.config.lock().await;
        if let Some(err) = vnc_bind_conflict(&cfg.id, &cfg, &config_guard.vnc_sessions) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        config_guard.vnc_sessions.push(cfg.clone());
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    if cfg.enabled {
        if let Err(e) = start_vnc_internal(&cfg.id, &cfg).await {
            let mut config_guard = state.config.lock().await;
            if let Some(v) = config_guard.vnc_sessions.iter_mut().find(|v| v.id == cfg.id) {
                v.enabled = false;
                cfg.enabled = false;
            }
            let _ = save_config(&config_guard).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!("Failed to start: {}", e))),
            ));
        }
    }

    let status = get_vnc_runtime_status(&cfg.id).await;
    Ok(Json(ApiResponse::success(
        "VNC 会话已创建",
        build_vnc_session_item(cfg, status),
    )))
}

async fn update_vnc_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<VncSessionUpsertRequest>,
) -> Result<Json<ApiResponse<VncSessionItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let existing = {
        let config_guard = state.config.lock().await;
        config_guard
            .vnc_sessions
            .iter()
            .find(|v| v.id == id)
            .cloned()
    };
    let Some(existing) = existing else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("VNC session not found")),
        ));
    };

    let restart = req.restart;
    let mut cfg = normalize_vnc_session_request(req, id.clone(), Some(&existing))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    {
        let mut config_guard = state.config.lock().await;
        if let Some(err) = vnc_bind_conflict(&cfg.id, &cfg, &config_guard.vnc_sessions) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        let Some(pos) = config_guard.vnc_sessions.iter().position(|v| v.id == id) else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("VNC session not found")),
            ));
        };
        config_guard.vnc_sessions[pos] = cfg.clone();
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    let restart = restart && cfg.enabled;
    if restart {
        cfg.enabled = true;
        {
            let mut config_guard = state.config.lock().await;
            if let Some(v) = config_guard.vnc_sessions.iter_mut().find(|v| v.id == id) {
                v.enabled = true;
            }
            let _ = save_config(&config_guard).await;
        }
        let _ = stop_vnc_internal(&cfg.id, &existing.display).await;
        if let Err(e) = start_vnc_internal(&cfg.id, &cfg).await {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!("Failed to restart: {}", e))),
            ));
        }
    } else {
        let status = get_vnc_runtime_status(&cfg.id).await;
        if cfg.enabled && !status.running {
            if let Err(e) = start_vnc_internal(&cfg.id, &cfg).await {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error(format!("Failed to start: {}", e))),
                ));
            }
        }
        if !cfg.enabled && status.running {
            let _ = stop_vnc_internal(&cfg.id, &existing.display).await;
        }
    }

    let status = get_vnc_runtime_status(&cfg.id).await;
    Ok(Json(ApiResponse::success(
        "VNC 会话已更新",
        build_vnc_session_item(cfg, status),
    )))
}

async fn delete_vnc_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let display = {
        let mut config_guard = state.config.lock().await;
        if config_guard
            .apps
            .iter()
            .any(|app| app.vnc_session_id.as_deref() == Some(id.as_str()))
        {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(
                    "VNC 会话已绑定应用，无法删除",
                )),
            ));
        }
        let mut display = None;
        let before = config_guard.vnc_sessions.len();
        if let Some(pos) = config_guard.vnc_sessions.iter().position(|v| v.id == id) {
            display = Some(config_guard.vnc_sessions[pos].display.clone());
            config_guard.vnc_sessions.remove(pos);
        }
        if config_guard.vnc_sessions.len() == before {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("VNC session not found")),
            ));
        }
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        display.unwrap_or_else(default_vnc_display)
    };
    let _ = stop_vnc_internal(&id, &display).await;
    Ok(Json(ApiResponse::success_no_data("VNC 会话已删除")))
}

async fn start_vnc_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config_guard = state.config.lock().await;
        let Some(v) = config_guard.vnc_sessions.iter().find(|v| v.id == id) else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("VNC session not found")),
            ));
        };
        if let Some(err) = vnc_bind_conflict(&id, v, &config_guard.vnc_sessions) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        v.clone()
    };
    if let Err(e) = start_vnc_internal(&cfg.id, &cfg).await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("Failed to start: {}", e))),
        ));
    }
    {
        let mut config_guard = state.config.lock().await;
        if let Some(v) = config_guard.vnc_sessions.iter_mut().find(|v| v.id == id) {
            v.enabled = true;
            if let Err(e) = save_config(&config_guard).await {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to save config: {}", e))),
                ));
            }
        }
    }
    Ok(Json(ApiResponse::success_no_data("VNC 会话已启动")))
}

async fn stop_vnc_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let display = {
        let mut config_guard = state.config.lock().await;
        let Some(v) = config_guard.vnc_sessions.iter_mut().find(|v| v.id == id) else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("VNC session not found")),
            ));
        };
        v.enabled = false;
        let display = v.display.clone();
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        display
    };
    let _ = stop_vnc_internal(&id, &display).await;
    Ok(Json(ApiResponse::success_no_data("VNC 会话已停止")))
}

async fn restart_vnc_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let (cfg, display) = {
        let config_guard = state.config.lock().await;
        let Some(v) = config_guard.vnc_sessions.iter().find(|v| v.id == id) else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error("VNC session not found")),
            ));
        };
        if let Some(err) = vnc_bind_conflict(&id, v, &config_guard.vnc_sessions) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        (v.clone(), v.display.clone())
    };
    let _ = stop_vnc_internal(&id, &display).await;
    if let Err(e) = start_vnc_internal(&cfg.id, &cfg).await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("Failed to restart: {}", e))),
        ));
    }
    {
        let mut config_guard = state.config.lock().await;
        if let Some(v) = config_guard.vnc_sessions.iter_mut().find(|v| v.id == id) {
            v.enabled = true;
            if let Err(e) = save_config(&config_guard).await {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to save config: {}", e))),
                ));
            }
        }
    }
    Ok(Json(ApiResponse::success_no_data("VNC 会话已重启")))
}

async fn create_app(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AppUpsertRequest>,
) -> Result<Json<ApiResponse<AppItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let id = generate_app_id();
    let mut cfg = normalize_app_request(req, id.clone(), None)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    {
        let mut config_guard = state.config.lock().await;
        if let Some(vnc_id) = &cfg.vnc_session_id {
            if !config_guard.vnc_sessions.iter().any(|v| v.id == *vnc_id) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error("VNC 会话不存在")),
                ));
            }
            if let Some(err) = app_vnc_conflict(&cfg.id, vnc_id, &config_guard.apps) {
                return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
            }
        }
        config_guard.apps.push(cfg.clone());
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    if cfg.enabled {
        let config_snapshot = { state.config.lock().await.clone() };
        if let Err(e) = start_app_internal(&cfg, &config_snapshot).await {
            let mut config_guard = state.config.lock().await;
            if let Some(a) = config_guard.apps.iter_mut().find(|a| a.id == cfg.id) {
                a.enabled = false;
                cfg.enabled = false;
            }
            let _ = save_config(&config_guard).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!("Failed to start: {}", e))),
            ));
        }
    }

    let status = get_app_runtime_status(&cfg.id).await;
    Ok(Json(ApiResponse::success(
        "应用已创建",
        build_app_item(cfg, status),
    )))
}

async fn update_app(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AppUpsertRequest>,
) -> Result<Json<ApiResponse<AppItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let existing = {
        let config_guard = state.config.lock().await;
        config_guard.apps.iter().find(|a| a.id == id).cloned()
    };
    let Some(existing) = existing else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("App not found"))));
    };

    let restart = req.restart;
    let mut cfg = normalize_app_request(req, id.clone(), Some(&existing))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    {
        let mut config_guard = state.config.lock().await;
        if let Some(vnc_id) = &cfg.vnc_session_id {
            if !config_guard.vnc_sessions.iter().any(|v| v.id == *vnc_id) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error("VNC 会话不存在")),
                ));
            }
            if let Some(err) = app_vnc_conflict(&cfg.id, vnc_id, &config_guard.apps) {
                return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
            }
        }
        let Some(pos) = config_guard.apps.iter().position(|a| a.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("App not found"))));
        };
        config_guard.apps[pos] = cfg.clone();
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    let restart = restart && cfg.enabled;
    if restart {
        cfg.enabled = true;
        {
            let mut config_guard = state.config.lock().await;
            if let Some(a) = config_guard.apps.iter_mut().find(|a| a.id == id) {
                a.enabled = true;
            }
            let _ = save_config(&config_guard).await;
        }
        let _ = stop_app_internal(&cfg.id).await;
        let config_snapshot = { state.config.lock().await.clone() };
        if let Err(e) = start_app_internal(&cfg, &config_snapshot).await {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!("Failed to restart: {}", e))),
            ));
        }
    } else {
        let status = get_app_runtime_status(&cfg.id).await;
        if cfg.enabled && !status.running {
            let config_snapshot = { state.config.lock().await.clone() };
            if let Err(e) = start_app_internal(&cfg, &config_snapshot).await {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error(format!("Failed to start: {}", e))),
                ));
            }
        }
        if !cfg.enabled && status.running {
            let _ = stop_app_internal(&cfg.id).await;
        }
    }

    let status = get_app_runtime_status(&cfg.id).await;
    Ok(Json(ApiResponse::success(
        "应用已更新",
        build_app_item(cfg, status),
    )))
}

async fn delete_app(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config_guard = state.config.lock().await;
        let before = config_guard.apps.len();
        config_guard.apps.retain(|a| a.id != id);
        if config_guard.apps.len() == before {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("App not found"))));
        }
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    let _ = stop_app_internal(&id).await;
    Ok(Json(ApiResponse::success_no_data("应用已删除")))
}

async fn start_app(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config_guard = state.config.lock().await;
        let Some(a) = config_guard.apps.iter().find(|a| a.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("App not found"))));
        };
        a.clone()
    };
    let config_snapshot = { state.config.lock().await.clone() };
    if let Err(e) = start_app_internal(&cfg, &config_snapshot).await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("Failed to start: {}", e))),
        ));
    }
    {
        let mut config_guard = state.config.lock().await;
        if let Some(a) = config_guard.apps.iter_mut().find(|a| a.id == id) {
            a.enabled = true;
            if let Err(e) = save_config(&config_guard).await {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to save config: {}", e))),
                ));
            }
        }
    }
    Ok(Json(ApiResponse::success_no_data("应用已启动")))
}

async fn stop_app(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config_guard = state.config.lock().await;
        let Some(a) = config_guard.apps.iter_mut().find(|a| a.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("App not found"))));
        };
        a.enabled = false;
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    let _ = stop_app_internal(&id).await;
    Ok(Json(ApiResponse::success_no_data("应用已停止")))
}

async fn restart_app(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config_guard = state.config.lock().await;
        let Some(a) = config_guard.apps.iter().find(|a| a.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("App not found"))));
        };
        a.clone()
    };
    let _ = stop_app_internal(&id).await;
    let config_snapshot = { state.config.lock().await.clone() };
    if let Err(e) = start_app_internal(&cfg, &config_snapshot).await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("Failed to restart: {}", e))),
        ));
    }
    {
        let mut config_guard = state.config.lock().await;
        if let Some(a) = config_guard.apps.iter_mut().find(|a| a.id == id) {
            a.enabled = true;
            if let Err(e) = save_config(&config_guard).await {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to save config: {}", e))),
                ));
            }
        }
    }
    Ok(Json(ApiResponse::success_no_data("应用已重启")))
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
                Json(ApiResponse::error("sing-box 正在运行中")),
            ));
        }
    }

    drop(lock);

    match start_sing_internal(&state.sing_box_home).await {
        Ok(_) => {
            let config = state.config.lock().await;
            let _ = apply_saved_selections(&config).await;
            Ok(Json(ApiResponse::success_no_data(
                "sing-box 启动成功",
            )))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e)),
        )),
    }
}

/// POST /api/service/stop - Stop sing-box
async fn stop_service() -> Json<ApiResponse<()>> {
    stop_sing_internal().await;
    Json(ApiResponse::success_no_data("sing-box stopped"))
}

/// POST /api/service/restart - Restart sing-box with regenerated config
async fn restart_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    if !sing_box_running().await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("sing-box is not running")),
        ));
    }
    regenerate_and_restart(state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))))?;
    Ok(Json(ApiResponse::success_no_data("sing-box restarted")))
}

fn normalize_terminal_request(
    req: TerminalUpsertRequest,
    id: String,
    existing: Option<&TerminalNodeConfig>,
) -> Result<TerminalNodeConfig, String> {
    let mut cfg = existing
        .cloned()
        .unwrap_or_else(|| terminal_node_default(id.clone()));
    cfg.id = id;

    if let Some(name) = req.name {
        let trimmed = name.trim();
        cfg.name = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    if let Some(enabled) = req.enabled {
        cfg.enabled = enabled;
    }
    if let Some(addr) = req.addr {
        let trimmed = addr.trim();
        cfg.addr = if trimmed.is_empty() {
            default_terminal_addr()
        } else {
            trimmed.to_string()
        };
    }
    if let Some(port) = req.port {
        if port == 0 {
            return Err("terminal port is required".to_string());
        }
        cfg.port = port;
    }
    if let Some(command) = req.command {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err("terminal command is required".to_string());
        }
        cfg.command = trimmed.to_string();
    }
    if let Some(command_args) = req.command_args {
        cfg.command_args = command_args
            .into_iter()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();
    }
    if req.clear_auth {
        cfg.auth_username = None;
        cfg.auth_password = None;
    } else {
        if let Some(username) = req.auth_username {
            let trimmed = username.trim().to_string();
            cfg.auth_username = if trimmed.is_empty() { None } else { Some(trimmed) };
        }
        if let Some(password) = req.auth_password {
            let trimmed = password.trim().to_string();
            cfg.auth_password = if trimmed.is_empty() { None } else { Some(trimmed) };
        }
    }
    if let Some(extra_args) = req.extra_args {
        cfg.extra_args = extra_args
            .into_iter()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();
    }

    if cfg.command.trim().is_empty() {
        return Err("terminal command is required".to_string());
    }
    if cfg.port == 0 {
        return Err("terminal port is required".to_string());
    }

    Ok(cfg)
}

fn normalize_vnc_session_request(
    req: VncSessionUpsertRequest,
    id: String,
    existing: Option<&VncSessionConfig>,
) -> Result<VncSessionConfig, String> {
    let mut cfg = existing.cloned().unwrap_or_else(|| {
        let mut v = VncSessionConfig::default();
        v.id = id.clone();
        v
    });
    cfg.id = id;

    if let Some(name) = req.name {
        let trimmed = name.trim();
        cfg.name = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    if let Some(enabled) = req.enabled {
        cfg.enabled = enabled;
    }
    if let Some(addr) = req.addr {
        let trimmed = addr.trim();
        cfg.addr = if trimmed.is_empty() {
            default_vnc_addr()
        } else {
            trimmed.to_string()
        };
    }
    if let Some(port) = req.port {
        if port == 0 {
            return Err("VNC 端口不能为空".to_string());
        }
        cfg.port = port;
    }
    if let Some(display) = req.display {
        let trimmed = display.trim();
        if trimmed.is_empty() {
            return Err("VNC DISPLAY 不能为空".to_string());
        }
        cfg.display = normalize_display_value(trimmed);
    } else {
        cfg.display = normalize_display_value(&cfg.display);
    }
    if let Some(resolution) = req.resolution {
        let trimmed = resolution.trim();
        cfg.resolution = if trimmed.is_empty() {
            default_vnc_resolution()
        } else {
            trimmed.to_string()
        };
    }
    if let Some(depth) = req.depth {
        if depth == 0 {
            return Err("VNC 色深必须大于 0".to_string());
        }
        cfg.depth = depth;
    }
    if let Some(frame_rate) = req.frame_rate {
        if frame_rate == 0 {
            return Err("VNC 帧率必须大于 0".to_string());
        }
        cfg.frame_rate = frame_rate;
    }
    if let Some(view_only) = req.view_only {
        cfg.view_only = view_only;
    }
    if let Some(password) = req.password {
        let trimmed = password.trim().to_string();
        cfg.password = if trimmed.is_empty() { None } else { Some(trimmed) };
    }

    if cfg.port == 0 {
        return Err("VNC 端口不能为空".to_string());
    }
    if cfg.display.trim().is_empty() {
        return Err("VNC DISPLAY 不能为空".to_string());
    }
    if cfg.resolution.trim().is_empty() {
        return Err("VNC 分辨率不能为空".to_string());
    }

    Ok(cfg)
}

fn normalize_app_request(
    req: AppUpsertRequest,
    id: String,
    existing: Option<&AppConfig>,
) -> Result<AppConfig, String> {
    let mut cfg = existing.cloned().unwrap_or_else(|| {
        let mut a = AppConfig::default();
        a.id = id.clone();
        a
    });
    cfg.id = id;

    if let Some(name) = req.name {
        let trimmed = name.trim();
        cfg.name = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    if let Some(enabled) = req.enabled {
        cfg.enabled = enabled;
    }
    if let Some(vnc_session_id) = req.vnc_session_id {
        let trimmed = vnc_session_id.trim();
        cfg.vnc_session_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    if let Some(display) = req.display {
        let trimmed = display.trim();
        cfg.display = if trimmed.is_empty() {
            None
        } else {
            Some(normalize_display_value(trimmed))
        };
    }
    if let Some(command) = req.command {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err("应用启动命令不能为空".to_string());
        }
        cfg.command = trimmed.to_string();
    }
    if let Some(args) = req.args {
        cfg.args = args
            .into_iter()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();
    }
    if let Some(env) = req.env {
        let mut normalized = HashMap::new();
        for (k, v) in env {
            let key = k.trim();
            if key.is_empty() {
                continue;
            }
            normalized.insert(key.to_string(), v.trim().to_string());
        }
        cfg.env = normalized;
    }

    if cfg.command.trim().is_empty() {
        return Err("应用启动命令不能为空".to_string());
    }
    if cfg.vnc_session_id.is_none() {
        if cfg
            .display
            .as_ref()
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
        {
            return Err("未绑定 VNC 时必须填写 DISPLAY".to_string());
        }
    }

    Ok(cfg)
}

fn app_templates() -> Vec<AppTemplateItem> {
    vec![
        AppTemplateItem {
            id: "chromium".to_string(),
            name: "Chromium".to_string(),
            description: "Chromium (X11) with common flags".to_string(),
            command: "chromium".to_string(),
            args: vec![
                "--no-sandbox".to_string(),
                "--no-first-run".to_string(),
                "--disable-dev-shm-usage".to_string(),
                "--disable-popup-blocking".to_string(),
                "--disable-infobars".to_string(),
                "--disable-gpu".to_string(),
                "--start-maximized".to_string(),
                "--no-default-browser-check".to_string(),
                "--ozone-platform=x11".to_string(),
                "--password-store=basic".to_string(),
                "--enable-features=NetworkService,NetworkServiceInProcess,LoadCryptoTokenExtension,PermuteTLSExtensions".to_string(),
                "--disable-features=FlashDeprecationWarning,EnablePasswordsAccountStorage,CommandLineFlagSecurityWarningsEnabled".to_string(),
                "--enable-blink-features=IdleDetection,Fledge,Parakeet".to_string(),
                "--lang=zh-CN".to_string(),
                "--user-data-dir=/app/chromium".to_string(),
                "--remote-debugging-port=9222".to_string(),
                "--simulate-outdated-no-au=Tue, 31 Dec 2099 23:59:59 GMT".to_string(),
            ],
            env: HashMap::new(),
        },
        AppTemplateItem {
            id: "cc-switch".to_string(),
            name: "CCSwitch".to_string(),
            description: "CCSwitch desktop app".to_string(),
            command: "cc-switch".to_string(),
            args: vec![],
            env: HashMap::new(),
        },
    ]
}

async fn create_terminal(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TerminalUpsertRequest>,
) -> Result<Json<ApiResponse<TerminalItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let id = generate_terminal_id();
    let mut cfg = normalize_terminal_request(req, id.clone(), None)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    {
        let mut config_guard = state.config.lock().await;
        if let Some(err) = terminal_bind_conflict(&cfg.id, &cfg, &config_guard.terminals) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        config_guard.terminals.push(cfg.clone());
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    if cfg.enabled {
        if let Err(e) = start_terminal_internal(&cfg.id, &cfg).await {
            let mut config_guard = state.config.lock().await;
            if let Some(t) = config_guard.terminals.iter_mut().find(|t| t.id == cfg.id) {
                t.enabled = false;
                cfg.enabled = false;
            }
            let _ = save_config(&config_guard).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!("Failed to start: {}", e))),
            ));
        }
    }

    let status = get_terminal_runtime_status(&cfg.id).await;
    Ok(Json(ApiResponse::success(
        "Terminal created",
        build_terminal_item(cfg, status),
    )))
}

async fn update_terminal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<TerminalUpsertRequest>,
) -> Result<Json<ApiResponse<TerminalItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let existing = {
        let config_guard = state.config.lock().await;
        config_guard
            .terminals
            .iter()
            .find(|t| t.id == id)
            .cloned()
    };
    let Some(existing) = existing else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Terminal not found"))));
    };

    let restart = req.restart;
    let mut cfg = normalize_terminal_request(req, id.clone(), Some(&existing))
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    {
        let mut config_guard = state.config.lock().await;
        if let Some(err) = terminal_bind_conflict(&cfg.id, &cfg, &config_guard.terminals) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        let Some(pos) = config_guard.terminals.iter().position(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Terminal not found"))));
        };
        config_guard.terminals[pos] = cfg.clone();
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    let restart = restart && cfg.enabled;
    if restart {
        cfg.enabled = true;
        {
            let mut config_guard = state.config.lock().await;
            if let Some(t) = config_guard.terminals.iter_mut().find(|t| t.id == id) {
                t.enabled = true;
            }
            let _ = save_config(&config_guard).await;
        }
        let _ = stop_terminal_internal(&cfg.id).await;
        if let Err(e) = start_terminal_internal(&cfg.id, &cfg).await {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!("Failed to restart: {}", e))),
            ));
        }
    } else {
        let status = get_terminal_runtime_status(&cfg.id).await;
        if cfg.enabled && !status.running {
            if let Err(e) = start_terminal_internal(&cfg.id, &cfg).await {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error(format!("Failed to start: {}", e))),
                ));
            }
        }
        if !cfg.enabled && status.running {
            let _ = stop_terminal_internal(&cfg.id).await;
        }
    }

    let status = get_terminal_runtime_status(&cfg.id).await;
    Ok(Json(ApiResponse::success(
        "Terminal updated",
        build_terminal_item(cfg, status),
    )))
}

async fn delete_terminal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config_guard = state.config.lock().await;
        let before = config_guard.terminals.len();
        config_guard.terminals.retain(|t| t.id != id);
        if config_guard.terminals.len() == before {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Terminal not found"))));
        }
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    let _ = stop_terminal_internal(&id).await;
    Ok(Json(ApiResponse::success_no_data("Terminal deleted")))
}

async fn start_terminal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config_guard = state.config.lock().await;
        let Some(t) = config_guard.terminals.iter().find(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Terminal not found"))));
        };
        if let Some(err) = terminal_bind_conflict(&id, t, &config_guard.terminals) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        t.clone()
    };
    if let Err(e) = start_terminal_internal(&cfg.id, &cfg).await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("Failed to start: {}", e))),
        ));
    }
    {
        let mut config_guard = state.config.lock().await;
        if let Some(t) = config_guard.terminals.iter_mut().find(|t| t.id == id) {
            t.enabled = true;
            if let Err(e) = save_config(&config_guard).await {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to save config: {}", e))),
                ));
            }
        }
    }
    Ok(Json(ApiResponse::success_no_data("terminal started")))
}

async fn stop_terminal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config_guard = state.config.lock().await;
        let Some(t) = config_guard.terminals.iter_mut().find(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Terminal not found"))));
        };
        t.enabled = false;
        if let Err(e) = save_config(&config_guard).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }
    let _ = stop_terminal_internal(&id).await;
    Ok(Json(ApiResponse::success_no_data("terminal stopped")))
}

async fn restart_terminal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config_guard = state.config.lock().await;
        let Some(t) = config_guard.terminals.iter().find(|t| t.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Terminal not found"))));
        };
        if let Some(err) = terminal_bind_conflict(&id, t, &config_guard.terminals) {
            return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(err))));
        }
        t.clone()
    };
    let _ = stop_terminal_internal(&id).await;
    if let Err(e) = start_terminal_internal(&cfg.id, &cfg).await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!("Failed to restart: {}", e))),
        ));
    }
    {
        let mut config_guard = state.config.lock().await;
        if let Some(t) = config_guard.terminals.iter_mut().find(|t| t.id == id) {
            t.enabled = true;
            if let Err(e) = save_config(&config_guard).await {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to save config: {}", e))),
                ));
            }
        }
    }
    Ok(Json(ApiResponse::success_no_data("terminal restarted")))
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
            log_error!("Background regenerate failed after setup: {}", e);
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

#[derive(Deserialize)]
struct BatchDelayRequest {
    nodes: Vec<String>,
    url: Option<String>,
    timeout: Option<u32>,
}

#[derive(Serialize)]
struct BatchDelayResponse {
    results: Vec<BatchDelayItem>,
    total: usize,
    success: usize,
}

#[derive(Serialize)]
struct BatchDelayItem {
    node: String,
    delay: Option<u64>,
    success: bool,
}

async fn clash_test_batch_delay(
    Json(req): Json<BatchDelayRequest>,
) -> Result<Json<ApiResponse<BatchDelayResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(format!("Failed to create client: {}", e)))))?;

    // 并行测试所有节点延迟
    let mut tasks = Vec::with_capacity(req.nodes.len());

    for node in &req.nodes {
        let client = client.clone();
        let node = node.clone();
        let timeout = req.timeout;
        let url = req.url.clone();

        tasks.push(tokio::spawn(async move {
            let mut result_url = format!("{}/proxies/{}/delay", CLASH_HTTP_BASE,
                percent_encoding::utf8_percent_encode(&node, percent_encoding::NON_ALPHANUMERIC));

            let mut params: Vec<(&str, String)> = Vec::new();
            if let Some(t) = timeout {
                params.push(("timeout", t.to_string()));
            }
            if let Some(ref test_url) = url {
                params.push(("url", test_url.clone()));
            }

            if !params.is_empty() {
                let query_string: String = params.iter()
                    .map(|(k, v)| format!("{}={}", k, percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)))
                    .collect::<Vec<_>>()
                    .join("&");
                result_url = format!("{}?{}", result_url, query_string);
            }

            let resp = client.get(&result_url).send().await;

            let delay_result = match resp {
                Ok(r) if r.status().is_success() => {
                    match r.json::<serde_json::Value>().await {
                        Ok(json) => json.get("delay").and_then(|d| d.as_u64()),
                        Err(_) => None,
                    }
                }
                _ => None,
            };

            BatchDelayItem {
                node,
                delay: delay_result,
                success: delay_result.is_some(),
            }
        }));
    }

    // 等待所有任务完成
    let mut results = Vec::with_capacity(tasks.len());
    let mut success_count = 0;

    for task in tasks {
        match task.await {
            Ok(item) => {
                if item.success {
                    success_count += 1;
                }
                results.push(item);
            }
            Err(_) => {
                // 任务panic，添加空结果
                results.push(BatchDelayItem {
                    node: String::new(),
                    delay: None,
                    success: false,
                });
            }
        }
    }

    Ok(Json(ApiResponse::success("Batch delay test completed", BatchDelayResponse {
        results,
        total: req.nodes.len(),
        success: success_count,
    })))
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
    Ok(ws.on_upgrade(move |socket| handle_logs_websocket(socket, level)))
}

async fn handle_logs_websocket(mut socket: WebSocket, min_level: String) {
    let mut rx = LOG_BROADCAST.subscribe();

    // Helper to check if log level passes the filter
    fn level_passes(log_level: &str, min_level: &str) -> bool {
        let level_priority = |l: &str| match l.to_lowercase().as_str() {
            "debug" => 0,
            "info" => 1,
            "warning" => 2,
            "error" => 3,
            _ => 1,
        };
        level_priority(log_level) >= level_priority(min_level)
    }

    let history: Vec<String> = {
        let buffer = LOG_BUFFER.lock().expect("log buffer lock poisoned");
        buffer.iter().cloned().collect()
    };
    for msg in history {
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&msg) {
            if let Some(level) = entry.get("level").and_then(|v| v.as_str()) {
                if !level_passes(level, &min_level) {
                    continue;
                }
            }
        }
        if socket.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        // Parse JSON to check level filter
                        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&msg) {
                            if let Some(level) = entry.get("level").and_then(|v| v.as_str()) {
                                if !level_passes(level, &min_level) {
                                    continue;
                                }
                            }
                        }
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Client is too slow, some messages were dropped
                        use chrono::FixedOffset;
                        let utc8 = FixedOffset::east_opt(8 * 3600).unwrap();
                        let time_str = Utc::now().with_timezone(&utc8).format("%Y-%m-%d %H:%M:%S").to_string();
                        let warning = serde_json::json!({
                            "time": time_str,
                            "level": "warning",
                            "message": format!("Dropped {} log messages (client too slow)", n)
                        });
                        let _ = socket.send(Message::Text(warning.to_string().into())).await;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn should_log_ws_connect_error(url: &str) -> bool {
    let mut guard = WS_CONNECT_ERROR_LOGS.lock().await;
    let now = Instant::now();
    if let Some(last) = guard.get_mut(url) {
        if now.duration_since(*last) >= Duration::from_secs(30) {
            *last = now;
            return true;
        }
        return false;
    }
    guard.insert(url.to_string(), now);
    true
}

async fn proxy_websocket(mut client_socket: WebSocket, upstream_url: String) {
    let upstream = connect_async(&upstream_url).await;
    let (upstream_ws, _) = match upstream {
        Ok(v) => v,
        Err(e) => {
            // Only log error if sing-box is running (to avoid spam when service is stopped)
            if sing_box_running().await && should_log_ws_connect_error(&upstream_url).await {
                log_error!("Failed to connect upstream websocket {}: {}", upstream_url, e);
            }
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
                    log_error!("Upstream websocket error: {}", e);
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
                let has_update = true;

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
    log_info!("Downloading update from: {}", download_url);
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
    log_info!("Stopping sing-box before upgrade...");
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

    // Mark embedded binaries for forced re-extraction on next start.
    if let Ok(current_dir) = std::env::current_dir() {
        let _ = fs::write(current_dir.join(".force_extract_sing_box"), b"1");
        let _ = fs::write(current_dir.join(".force_extract_gotty"), b"1");
    }

    log_info!("Upgrade successful! Restarting...");

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
        log_error!("Failed to exec new binary: {}", err);
        log_error!("Attempting to restore from backup...");

        if fs::remove_file(&current_exe).is_ok() {
            if fs::copy(&backup_path, &current_exe).is_ok() {
                let _ = fs::set_permissions(&current_exe, fs::Permissions::from_mode(0o755));
                log_error!("Restored from backup, restarting with old version...");
                let _ = std::process::Command::new(&current_exe).args(&args[1..]).exec();
            }
        }
        log_error!("Failed to restore from backup, manual intervention required");
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
            log_error!("systemd-run restart failed: {}", stderr.trim());
        }
        Err(e) => {
            log_error!("systemd-run not available/failed: {}", e);
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
    let status = state.subscription_status.lock().await;
    let files = status
        .values()
        .flat_map(|entry| entry.files.clone())
        .collect::<Vec<_>>();
    let error = status.values().find_map(|entry| entry.error.clone());
    Json(ApiResponse::success(
        "Subscription files loaded",
        SubFilesResponse {
            sub_dir: state.subscriptions_root.display().to_string(),
            sub_source: None,
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

fn build_subscription_source_response(
    sub: &SubscriptionConfig,
    root: &StdPath,
) -> SubscriptionSourceResponse {
    match &sub.source {
        SubscriptionSource::Url { url } => SubscriptionSourceResponse::Url { url: url.clone() },
        SubscriptionSource::Git { repo } => SubscriptionSourceResponse::Git {
            repo: repo.clone(),
            workdir: root.join(&sub.id).display().to_string(),
        },
        SubscriptionSource::Path { path } => SubscriptionSourceResponse::Path { path: path.clone() },
    }
}

fn build_subscription_item(
    sub: &SubscriptionConfig,
    runtime: Option<&SubscriptionRuntime>,
    root: &StdPath,
) -> SubscriptionItem {
    SubscriptionItem {
        id: sub.id.clone(),
        name: sub.name.clone(),
        enabled: sub.enabled,
        source: build_subscription_source_response(sub, root),
        updated_at: runtime.and_then(|value| value.updated_at),
        last_error: runtime.and_then(|value| value.error.clone()),
        files: runtime.map(|value| value.files.clone()).unwrap_or_default(),
    }
}

fn normalize_subscription_name(name: Option<String>) -> Option<String> {
    name.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn validate_subscription_source(input: &SubscriptionSourceInput) -> Result<SubscriptionSource, String> {
    match input {
        SubscriptionSourceInput::Url { url } => {
            let trimmed = url.trim();
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                Ok(SubscriptionSource::Url { url: trimmed.to_string() })
            } else {
                Err("Subscription URL must start with http:// or https://".to_string())
            }
        }
        SubscriptionSourceInput::Git { repo } => {
            let trimmed = repo.trim();
            if looks_like_git_url(trimmed) {
                Ok(SubscriptionSource::Git { repo: trimmed.to_string() })
            } else {
                Err("Git repository URL must be https://, ssh://, or git@".to_string())
            }
        }
        SubscriptionSourceInput::Path { path } => {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                Err("Subscription path is required".to_string())
            } else {
                Ok(SubscriptionSource::Path { path: trimmed.to_string() })
            }
        }
    }
}

async fn list_subscriptions(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<SubscriptionListResponse>> {
    let config = state.config.lock().await;
    let status = state.subscription_status.lock().await;
    let root = state.subscriptions_root.clone();
    let items = config
        .subscriptions
        .iter()
        .map(|sub| build_subscription_item(sub, status.get(&sub.id), &root))
        .collect::<Vec<_>>();
    Json(ApiResponse::success(
        "Subscriptions",
        SubscriptionListResponse { items },
    ))
}

async fn create_subscription(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubscriptionUpsertRequest>,
) -> Result<Json<ApiResponse<SubscriptionSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let source = validate_subscription_source(&req.source)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;
    let cfg = SubscriptionConfig {
        id: generate_subscription_id(),
        name: normalize_subscription_name(req.name),
        enabled: req.enabled.unwrap_or(true),
        source,
    };

    {
        let mut config = state.config.lock().await;
        config.subscriptions.push(cfg.clone());
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    if let Err(e) = regenerate_and_restart(state.clone()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e)),
        ));
    }

    let status = state.subscription_status.lock().await;
    let item = build_subscription_item(&cfg, status.get(&cfg.id), &state.subscriptions_root);
    Ok(Json(ApiResponse::success(
        "Subscription created",
        SubscriptionSaveResponse { item },
    )))
}

async fn update_subscription(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SubscriptionUpsertRequest>,
) -> Result<Json<ApiResponse<SubscriptionSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let source = validate_subscription_source(&req.source)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    let updated = {
        let mut config = state.config.lock().await;
        let Some(pos) = config.subscriptions.iter().position(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Subscription not found"))));
        };
        let existing = config.subscriptions[pos].clone();
        let name = if req.name.is_some() {
            normalize_subscription_name(req.name)
        } else {
            existing.name
        };
        let cfg = SubscriptionConfig {
            id: existing.id,
            name,
            enabled: req.enabled.unwrap_or(existing.enabled),
            source,
        };
        config.subscriptions[pos] = cfg.clone();
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        cfg
    };

    if let Err(e) = regenerate_and_restart(state.clone()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e)),
        ));
    }

    let status = state.subscription_status.lock().await;
    let item = build_subscription_item(&updated, status.get(&updated.id), &state.subscriptions_root);
    Ok(Json(ApiResponse::success(
        "Subscription updated",
        SubscriptionSaveResponse { item },
    )))
}

async fn delete_subscription(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let removed = {
        let mut config = state.config.lock().await;
        let Some(pos) = config.subscriptions.iter().position(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Subscription not found"))));
        };
        let removed = config.subscriptions.remove(pos);
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        removed
    };

    if let SubscriptionSource::Url { .. } | SubscriptionSource::Git { .. } = removed.source {
        let _ = remove_path_if_exists(&state.subscriptions_root.join(&removed.id)).await;
    }

    if let Err(e) = regenerate_and_restart(state.clone()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e)),
        ));
    }

    Ok(Json(ApiResponse::success_no_data("Subscription deleted")))
}

async fn reload_subscription(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let exists = {
        let config = state.config.lock().await;
        config.subscriptions.iter().any(|s| s.id == id)
    };
    if !exists {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Subscription not found"))));
    }
    match regenerate_and_restart(state).await {
        Ok(_) => Ok(Json(ApiResponse::success_no_data(
            "Subscription reloaded and sing-box restarted",
        ))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e)),
        )),
    }
}

async fn reload_subscriptions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    match regenerate_and_restart(state).await {
        Ok(_) => Ok(Json(ApiResponse::success_no_data(
            "Subscriptions reloaded and sing-box restarted",
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

    let running = sing_box_running().await;
    let state_clone = state.clone();
    tokio::spawn(async move {
        if running {
            if let Err(e) = regenerate_and_restart(state_clone).await {
                log_error!("Background regenerate failed: {}", e);
            }
        } else if let Err(e) = regenerate_config(state_clone).await {
            log_error!("Background regenerate failed: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data(if running {
        "Node added, restarting..."
    } else {
        "Node added, pending apply"
    })))
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

    let running = sing_box_running().await;
    let state_clone = state.clone();
    tokio::spawn(async move {
        if running {
            if let Err(e) = regenerate_and_restart(state_clone).await {
                log_error!("Background regenerate failed: {}", e);
            }
        } else if let Err(e) = regenerate_config(state_clone).await {
            log_error!("Background regenerate failed: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data(if running {
        "Node updated, restarting..."
    } else {
        "Node updated, pending apply"
    })))
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

    let running = sing_box_running().await;
    let state_clone = state.clone();
    tokio::spawn(async move {
        if running {
            if let Err(e) = regenerate_and_restart(state_clone).await {
                log_error!("Background regenerate failed: {}", e);
            }
        } else if let Err(e) = regenerate_config(state_clone).await {
            log_error!("Background regenerate failed: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data(if running {
        "Node deleted, restarting..."
    } else {
        "Node deleted, pending apply"
    })))
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
                log_error!("Failed to align active proxy after pool update: {}", e);
            }
        }
    }

    Ok(Json(ApiResponse::success_no_data("Proxy pool updated")))
}

fn redact_tunnel_auth(auth: &TcpTunnelAuth) -> TcpTunnelAuthPublic {
    match auth {
        TcpTunnelAuth::Password { password } => TcpTunnelAuthPublic::Password {
            password: password.clone(),
        },
        TcpTunnelAuth::PrivateKeyPath { path, .. } => TcpTunnelAuthPublic::PrivateKeyPath {
            path: path.clone(),
        },
        TcpTunnelAuth::SshAgent => TcpTunnelAuthPublic::SshAgent,
    }
}

fn redact_sync_auth(auth: &TcpTunnelAuth) -> SyncAuthPublic {
    match auth {
        TcpTunnelAuth::Password { password } => SyncAuthPublic::Password {
            password: password.clone(),
        },
        TcpTunnelAuth::PrivateKeyPath { path, .. } => SyncAuthPublic::PrivateKeyPath {
            path: path.clone(),
        },
        TcpTunnelAuth::SshAgent => SyncAuthPublic::SshAgent,
    }
}

fn generate_tunnel_id() -> String {
    format!("t-{}", uuid::Uuid::new_v4())
}

fn generate_sync_id() -> String {
    format!("sync-{}", uuid::Uuid::new_v4())
}

fn generate_subscription_id() -> String {
    format!("sub-{}", uuid::Uuid::new_v4())
}

fn generate_terminal_id() -> String {
    format!("term-{}", uuid::Uuid::new_v4())
}

fn generate_vnc_session_id() -> String {
    format!("vnc-{}", uuid::Uuid::new_v4())
}

fn generate_app_id() -> String {
    format!("app-{}", uuid::Uuid::new_v4())
}

fn normalize_display_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        ":0".to_string()
    } else if trimmed.starts_with(':') {
        trimmed.to_string()
    } else {
        format!(":{}", trimmed)
    }
}

fn terminal_legacy_is_default(cfg: &TerminalConfigLegacy) -> bool {
    cfg.enabled == false
        && cfg.addr == default_terminal_addr()
        && cfg.port == default_terminal_port()
        && cfg.command == default_terminal_command()
        && cfg.command_args.is_empty()
        && cfg.auth_username.is_none()
        && cfg.auth_password.is_none()
        && (cfg.extra_args.is_empty() || cfg.extra_args == default_terminal_extra_args())
}

fn terminal_node_default(id: String) -> TerminalNodeConfig {
    let mut cfg = TerminalNodeConfig::default();
    cfg.id = id;
    cfg
}

fn terminal_bind_conflict(
    id: &str,
    cfg: &TerminalNodeConfig,
    terminals: &[TerminalNodeConfig],
) -> Option<String> {
    let addr = if cfg.addr.trim().is_empty() {
        "127.0.0.1"
    } else {
        cfg.addr.as_str()
    };
    let port = cfg.port;
    for t in terminals {
        if t.id == id {
            continue;
        }
        if t.port != port {
            continue;
        }
        let other_addr = if t.addr.trim().is_empty() {
            "127.0.0.1"
        } else {
            t.addr.as_str()
        };
        let conflicts = addr == other_addr || addr == "0.0.0.0" || other_addr == "0.0.0.0";
        if conflicts {
            let name = t.name.clone().unwrap_or_else(|| t.id.clone());
            return Some(format!("terminal port already in use by {}", name));
        }
    }
    None
}

fn vnc_bind_conflict(
    id: &str,
    cfg: &VncSessionConfig,
    sessions: &[VncSessionConfig],
) -> Option<String> {
    let addr = if cfg.addr.trim().is_empty() {
        "0.0.0.0"
    } else {
        cfg.addr.as_str()
    };
    let port = cfg.port;
    let display = normalize_display_value(&cfg.display);
    for s in sessions {
        if s.id == id {
            continue;
        }
        if s.port == port {
            let other_addr = if s.addr.trim().is_empty() {
                "0.0.0.0"
            } else {
                s.addr.as_str()
            };
            let conflicts = addr == other_addr || addr == "0.0.0.0" || other_addr == "0.0.0.0";
            if conflicts {
                let name = s.name.clone().unwrap_or_else(|| s.id.clone());
            return Some(format!("VNC 端口已被 {} 使用", name));
            }
        }
        if normalize_display_value(&s.display) == display {
            let name = s.name.clone().unwrap_or_else(|| s.id.clone());
            return Some(format!("VNC DISPLAY 已被 {} 使用", name));
        }
    }
    None
}

fn app_vnc_conflict(id: &str, vnc_session_id: &str, apps: &[AppConfig]) -> Option<String> {
    for app in apps {
        if app.id == id {
            continue;
        }
        if app.vnc_session_id.as_deref() == Some(vnc_session_id) {
            let name = app.name.clone().unwrap_or_else(|| app.id.clone());
            return Some(format!("VNC 会话已绑定应用 {}", name));
        }
    }
    None
}

fn migrate_terminals(config: &mut Config) {
    if !config.terminals.is_empty() {
        for t in &mut config.terminals {
            if t.id.trim().is_empty() {
                t.id = generate_terminal_id();
            }
        }
        config.terminal = None;
        return;
    }
    let legacy = config.terminal.take();
    let Some(legacy) = legacy else {
        return;
    };
    if legacy.enabled || !terminal_legacy_is_default(&legacy) {
        let mut node = terminal_node_default(generate_terminal_id());
        node.enabled = legacy.enabled;
        node.addr = legacy.addr;
        node.port = legacy.port;
        node.command = legacy.command;
        node.command_args = legacy.command_args;
        node.auth_username = legacy.auth_username;
        node.auth_password = legacy.auth_password;
        node.extra_args = legacy.extra_args;
        config.terminals.push(node);
    }
}

fn normalize_subscriptions(config: &mut Config) -> bool {
    let mut changed = false;
    for sub in &mut config.subscriptions {
        if sub.id.trim().is_empty() {
            sub.id = generate_subscription_id();
            changed = true;
        }
        if let Some(name) = &sub.name {
            if name.trim().is_empty() {
                sub.name = None;
                changed = true;
            }
        }
    }
    changed
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

async fn sing_box_running() -> bool {
    let mut lock = SING_PROCESS.lock().await;
    if let Some(ref mut proc) = *lock {
        match proc.child.try_wait() {
            Ok(Some(_)) => {
                *lock = None;
                false
            }
            Ok(None) => true,
            Err(_) => false,
        }
    } else {
        false
    }
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
) -> Result<Json<ApiResponse<TcpTunnelSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let id = req.id.clone().unwrap_or_else(generate_tunnel_id);
    let mut req = req;
    if let Some(host_id) = req.host_id.as_ref() {
        let config = state.config.lock().await;
        let host = config
            .hosts
            .iter()
            .find(|h| h.id == *host_id)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error("Host not found")),
                )
            })?;
        let auth = resolve_host_auth(host).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e)),
            )
        })?;
        req.ssh_host = host.host.clone();
        req.ssh_port = Some(host.port);
        req.username = host.username.clone();
        req.auth = auth;
    }

    let cfg = normalize_tcp_tunnel(req, id.clone())
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    // Require secrets on create.
    match &cfg.auth {
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
        TcpTunnelSaveResponse {
            item: TcpTunnelItem {
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
        },
    )))
}

async fn update_tcp_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<TcpTunnelUpsertRequest>,
) -> Result<Json<ApiResponse<TcpTunnelSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
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

    let mut req = req;
    if let Some(host_id) = req.host_id.as_ref() {
        let config = state.config.lock().await;
        let host = config
            .hosts
            .iter()
            .find(|h| h.id == *host_id)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error("Host not found")),
                )
            })?;
        let auth = resolve_host_auth(host).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e)),
            )
        })?;
        req.ssh_host = host.host.clone();
        req.ssh_port = Some(host.port);
        req.username = host.username.clone();
        req.auth = auth;
    }

    let mut cfg = normalize_tcp_tunnel(req, id.clone())
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    // Support "leave blank to keep unchanged" for private-key secrets on update.
    match (&existing.auth, &mut cfg.auth) {
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
        TcpTunnelSaveResponse {
            item: TcpTunnelItem {
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
            connect_timeout_ms: set.connect_timeout_ms,
            start_batch_size: set.start_batch_size,
            start_batch_interval_ms: set.start_batch_interval_ms,
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
) -> Result<Json<ApiResponse<TcpTunnelSetSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
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

    let ssh_port = req.ssh_port.unwrap_or(existing.ssh_port);

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
        ssh_port,
        username,
        auth: auth.clone(),
        strict_host_key_checking,
        host_key_fingerprint: host_key_fingerprint.clone(),
        exclude_ports: req.exclude_ports.unwrap_or_else(|| existing.exclude_ports.clone()),
        scan_interval_ms: req.scan_interval_ms.unwrap_or(existing.scan_interval_ms),
        debounce_ms: req.debounce_ms.unwrap_or(existing.debounce_ms),
        connect_timeout_ms: req
            .connect_timeout_ms
            .unwrap_or(existing.connect_timeout_ms),
        start_batch_size: req.start_batch_size.unwrap_or(existing.start_batch_size),
        start_batch_interval_ms: req
            .start_batch_interval_ms
            .unwrap_or(existing.start_batch_interval_ms),
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
                    t.connect_timeout_ms = updated.connect_timeout_ms;
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
    Ok(Json(ApiResponse::success(
        "Set updated",
        TcpTunnelSetSaveResponse {},
    )))
}

async fn create_tcp_tunnel_set(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TcpTunnelSetCreateRequest>,
) -> Result<Json<ApiResponse<TcpTunnelSetSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let id = generate_tunnel_set_id();
    let enabled = req.enabled.unwrap_or(false);
    let remote_bind_addr = req.remote_bind_addr.unwrap_or_else(default_remote_bind_addr);
    let ssh_port = req.ssh_port.unwrap_or_else(default_ssh_port);
    let strict_host_key_checking = req.strict_host_key_checking.unwrap_or(true);
    let host_key_fingerprint = req.host_key_fingerprint.unwrap_or_default();
    let exclude_ports = req.exclude_ports.unwrap_or_default();
    let scan_interval_ms = req.scan_interval_ms.unwrap_or(3_000);
    let debounce_ms = req.debounce_ms.unwrap_or(8_000);
    let connect_timeout_ms = req
        .connect_timeout_ms
        .unwrap_or_else(default_tunnel_set_connect_timeout_ms);
    let start_batch_size = req
        .start_batch_size
        .unwrap_or_else(default_tunnel_set_start_batch_size);
    let start_batch_interval_ms = req
        .start_batch_interval_ms
        .unwrap_or_else(default_tunnel_set_start_batch_interval_ms);

    if strict_host_key_checking && host_key_fingerprint.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(
                "host_key_fingerprint is required when strict_host_key_checking is true",
            )),
        ));
    }

    match &req.auth {
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
            connect_timeout_ms,
            start_batch_size,
            start_batch_interval_ms,
        });
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
    }

    apply_full_tunnel_sets_from_config(&state).await;
    Ok(Json(ApiResponse::success(
        "Set created",
        TcpTunnelSetSaveResponse {},
    )))
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
        connect_timeout_ms: set.connect_timeout_ms,
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
// Host API Handlers
// ============================================================================

fn build_host_item(cfg: &HostConfig) -> HostItem {
    let (auth_type, private_key_path) = match &cfg.auth {
        HostAuth::Password { .. } => (HostAuthTypePublic::Password, None),
        HostAuth::PrivateKeyPath { path, .. } => (HostAuthTypePublic::PrivateKeyPath, Some(path.clone())),
        HostAuth::SshAgent => (HostAuthTypePublic::SshAgent, None),
    };
    HostItem {
        id: cfg.id.clone(),
        name: cfg.name.clone(),
        host: cfg.host.clone(),
        port: cfg.port,
        username: cfg.username.clone(),
        auth_type,
        private_key_path,
        created_at: cfg.created_at,
        updated_at: cfg.updated_at,
    }
}

async fn get_hosts(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<HostListResponse>> {
    let hosts = { state.config.lock().await.hosts.clone() };
    let items: Vec<HostItem> = hosts.iter().map(build_host_item).collect();
    Json(ApiResponse::success("ok", HostListResponse { items }))
}

async fn get_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<HostItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let hosts = { state.config.lock().await.hosts.clone() };
    let Some(cfg) = hosts.iter().find(|h| h.id == id) else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Host not found"))));
    };
    Ok(Json(ApiResponse::success("ok", build_host_item(cfg))))
}

async fn get_host_default_key_path() -> Json<ApiResponse<HostDefaultKeyPathResponse>> {
    let path = default_private_key_path();
    Json(ApiResponse::success("ok", HostDefaultKeyPathResponse { path }))
}

async fn test_host_config(
    Json(req): Json<HostUpsertRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let host = req.host.trim().to_string();
    if host.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Host is required"))));
    }
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Username is required"))));
    }

    let auth = match req.auth_type.as_str() {
        "password" => HostAuth::Password { password: req.password },
        "private_key_path" => {
            let path = req.private_key_path.unwrap_or_default();
            let resolved = resolve_private_key_path(&path)
                .map_err(|msg| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(msg))))?;
            HostAuth::PrivateKeyPath { path: resolved, passphrase: req.private_key_passphrase }
        }
        "ssh_agent" => HostAuth::SshAgent,
        _ => return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Invalid auth type")))),
    };

    let cfg = HostConfig {
        id: "test".to_string(),
        name: req.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        host,
        port: req.port.unwrap_or(default_ssh_port()),
        username,
        auth,
        created_at: None,
        updated_at: None,
    };

    match test_host_connection(&cfg).await {
        Ok(()) => Ok(Json(ApiResponse::success_no_data("Connection successful"))),
        Err(msg) => Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(msg)))),
    }
}

async fn create_host(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HostUpsertRequest>,
) -> Result<Json<ApiResponse<HostItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let host = req.host.trim().to_string();
    if host.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Host is required"))));
    }
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Username is required"))));
    }

    let auth = match req.auth_type.as_str() {
        "password" => HostAuth::Password { password: req.password },
        "private_key_path" => {
            let path = req.private_key_path.unwrap_or_default();
            let resolved = resolve_private_key_path(&path)
                .map_err(|msg| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(msg))))?;
            HostAuth::PrivateKeyPath { path: resolved, passphrase: req.private_key_passphrase }
        }
        "ssh_agent" => HostAuth::SshAgent,
        _ => return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Invalid auth type")))),
    };

    let now = Utc::now().timestamp();
    let cfg = HostConfig {
        id: generate_host_id(),
        name: req.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        host,
        port: req.port.unwrap_or(default_ssh_port()),
        username,
        auth,
        created_at: Some(now),
        updated_at: Some(now),
    };

    if let Err(msg) = test_host_connection(&cfg).await {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(msg))));
    }

    {
        let mut config = state.config.lock().await;
        config.hosts.push(cfg.clone());
        if let Err(e) = save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(format!("Failed to save config: {}", e)))));
        }
    }

    // Regenerate sing-box config; restart only if service is running.
    let state_clone = state.clone();
    tokio::spawn(async move {
        if sing_box_running().await {
            if let Err(e) = regenerate_and_restart(state_clone).await {
                eprintln!("Failed to restart after host creation: {}", e);
            }
        } else if let Err(e) = regenerate_config(state_clone).await {
            eprintln!("Failed to regenerate config after host creation: {}", e);
        }
    });

    Ok(Json(ApiResponse::success("Host created", build_host_item(&cfg))))
}

async fn update_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<HostUpsertRequest>,
) -> Result<Json<ApiResponse<HostItem>>, (StatusCode, Json<ApiResponse<()>>)> {
    let host = req.host.trim().to_string();
    if host.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Host is required"))));
    }
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Username is required"))));
    }

    let updated = {
        let mut config = state.config.lock().await;
        let Some(pos) = config.hosts.iter().position(|h| h.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Host not found"))));
        };
        let existing = &config.hosts[pos];

        let auth = match req.auth_type.as_str() {
            "password" => {
                let password = if req.password.as_ref().map(|p| p.is_empty()).unwrap_or(true) {
                    if let HostAuth::Password { password } = &existing.auth {
                        password.clone()
                    } else {
                        None
                    }
                } else {
                    req.password
                };
                HostAuth::Password { password }
            }
            "private_key_path" => {
                let path = req.private_key_path.unwrap_or_default();
                let resolved = if path.trim().is_empty() {
                    if let HostAuth::PrivateKeyPath { path, .. } = &existing.auth {
                        path.clone()
                    } else {
                        resolve_private_key_path(&path)
                            .map_err(|msg| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(msg))))?
                    }
                } else {
                    path.trim().to_string()
                };
                let passphrase = if req.private_key_passphrase.as_ref().map(|p| p.is_empty()).unwrap_or(true) {
                    if let HostAuth::PrivateKeyPath { passphrase, .. } = &existing.auth {
                        passphrase.clone()
                    } else {
                        None
                    }
                } else {
                    req.private_key_passphrase
                };
                HostAuth::PrivateKeyPath { path: resolved, passphrase }
            }
            "ssh_agent" => HostAuth::SshAgent,
            _ => return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error("Invalid auth type")))),
        };

        let now = Utc::now().timestamp();
        let cfg = HostConfig {
            id: id.clone(),
            name: req.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
            host,
            port: req.port.unwrap_or(default_ssh_port()),
            username,
            auth,
            created_at: existing.created_at,
            updated_at: Some(now),
        };

        config.hosts[pos] = cfg.clone();
        if let Err(e) = save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(format!("Failed to save config: {}", e)))));
        }
        cfg
    };

    // Regenerate sing-box config; restart only if service is running.
    let state_clone = state.clone();
    tokio::spawn(async move {
        if sing_box_running().await {
            if let Err(e) = regenerate_and_restart(state_clone).await {
                eprintln!("Failed to restart after host update: {}", e);
            }
        } else if let Err(e) = regenerate_config(state_clone).await {
            eprintln!("Failed to regenerate config after host update: {}", e);
        }
    });

    Ok(Json(ApiResponse::success("Host updated", build_host_item(&updated))))
}

async fn delete_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    {
        let mut config = state.config.lock().await;
        let Some(pos) = config.hosts.iter().position(|h| h.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Host not found"))));
        };
        config.hosts.remove(pos);
        if let Err(e) = save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(format!("Failed to save config: {}", e)))));
        }
    }

    // Regenerate sing-box config; restart only if service is running.
    let state_clone = state.clone();
    tokio::spawn(async move {
        if sing_box_running().await {
            if let Err(e) = regenerate_and_restart(state_clone).await {
                eprintln!("Failed to restart after host deletion: {}", e);
            }
        } else if let Err(e) = regenerate_config(state_clone).await {
            eprintln!("Failed to regenerate config after host deletion: {}", e);
        }
    });

    Ok(Json(ApiResponse::success_no_data("Host deleted")))
}

async fn test_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let host_cfg = {
        let config = state.config.lock().await;
        config.hosts.iter().find(|h| h.id == id).cloned()
    };
    let Some(cfg) = host_cfg else {
        return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Host not found"))));
    };

    match test_host_connection(&cfg).await {
        Ok(()) => Ok(Json(ApiResponse::success_no_data("Connection successful"))),
        Err(msg) => Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(msg)))),
    }
}

async fn get_syncs(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<SyncListResponse>> {
    let syncs = { state.config.lock().await.syncs.clone() };
    let mut items = Vec::with_capacity(syncs.len());
    for cfg in syncs {
        let status = state.sync_manager.get_status(&cfg.id).await;
        items.push(build_sync_item(&cfg, status));
    }
    Json(ApiResponse::success("ok", SyncListResponse { items }))
}

async fn create_sync(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SyncUpsertRequest>,
) -> Result<Json<ApiResponse<SyncSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let mut req = req;
    if let Some(host_id) = req.host_id.as_ref() {
        let config = state.config.lock().await;
        let host = config
            .hosts
            .iter()
            .find(|h| h.id == *host_id)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error("Host not found")),
                )
            })?;
        let auth = resolve_host_auth(host).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e)),
            )
        })?;
        req.ssh_host = host.host.clone();
        req.ssh_port = Some(host.port);
        req.username = host.username.clone();
        req.auth = auth;
    }

    let local_paths = build_sync_local_paths(&req.local_paths)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;
    let remote_path = normalize_sync_remote_path(req.remote_path);
    if local_paths.len() > 1 && remote_path.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Multiple local paths cannot set remote path")),
        ));
    }
    let schedule = normalize_sync_schedule(req.schedule)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    let host = req.ssh_host.trim().to_string();
    if host.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("SSH host is required")),
        ));
    }
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("SSH username is required")),
        ));
    }

    let options = normalize_sync_options(req.options);
    let cfg = SyncConfig {
        id: generate_sync_id(),
        name: normalize_sync_name(req.name),
        enabled: req.enabled.unwrap_or(true),
        local_paths,
        remote_path,
        ssh: SyncSshConfig {
            host,
            port: req.ssh_port.unwrap_or(default_ssh_port()),
            username,
            auth: req.auth,
        },
        options,
        schedule,
    };

    match &cfg.ssh.auth {
        TcpTunnelAuth::PrivateKeyPath { .. } => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("sync only supports password or ssh-agent auth")),
            ));
        }
        TcpTunnelAuth::Password { .. } | TcpTunnelAuth::SshAgent => {}
    }

    let syncs_snapshot = {
        let mut config = state.config.lock().await;
        config.syncs.push(cfg.clone());
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        config.syncs.clone()
    };
    state.sync_manager.apply_config(&syncs_snapshot).await;

    let status = state.sync_manager.get_status(&cfg.id).await;
    Ok(Json(ApiResponse::success(
        "Sync created",
        SyncSaveResponse {
            item: build_sync_item(&cfg, status),
        },
    )))
}

async fn update_sync(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SyncUpsertRequest>,
) -> Result<Json<ApiResponse<SyncSaveResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let mut req = req;
    if let Some(host_id) = req.host_id.as_ref() {
        let config = state.config.lock().await;
        let host = config
            .hosts
            .iter()
            .find(|h| h.id == *host_id)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error("Host not found")),
                )
            })?;
        let auth = resolve_host_auth(host).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e)),
            )
        })?;
        req.ssh_host = host.host.clone();
        req.ssh_port = Some(host.port);
        req.username = host.username.clone();
        req.auth = auth;
    }

    let local_paths = build_sync_local_paths(&req.local_paths)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;
    let remote_path = normalize_sync_remote_path(req.remote_path);
    if local_paths.len() > 1 && remote_path.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Multiple local paths cannot set remote path")),
        ));
    }
    let schedule = normalize_sync_schedule(req.schedule)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))))?;

    let host = req.ssh_host.trim().to_string();
    if host.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("SSH host is required")),
        ));
    }
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("SSH username is required")),
        ));
    }

    let (updated, syncs_snapshot) = {
        let mut config = state.config.lock().await;
        let Some(pos) = config.syncs.iter().position(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Sync not found"))));
        };
        let existing = config.syncs[pos].clone();
        let enabled = req.enabled.unwrap_or(existing.enabled);
        let name = if req.name.is_some() {
            normalize_sync_name(req.name)
        } else {
            existing.name.clone()
        };
        let auth = req.auth;

        match &auth {
            TcpTunnelAuth::PrivateKeyPath { .. } => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse::error("sync only supports password or ssh-agent auth")),
                ));
            }
            TcpTunnelAuth::Password { .. } | TcpTunnelAuth::SshAgent => {}
        }
        let cfg = SyncConfig {
            id: id.clone(),
            name,
            enabled,
            local_paths,
            remote_path,
            ssh: SyncSshConfig {
                host,
                port: req.ssh_port.unwrap_or(default_ssh_port()),
                username,
                auth,
            },
            options: normalize_sync_options(req.options),
            schedule,
        };
        config.syncs[pos] = cfg.clone();
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        (cfg, config.syncs.clone())
    };
    state.sync_manager.apply_config(&syncs_snapshot).await;

    let status = state.sync_manager.get_status(&updated.id).await;
    Ok(Json(ApiResponse::success(
        "Sync updated",
        SyncSaveResponse {
            item: build_sync_item(&updated, status),
        },
    )))
}

async fn delete_sync(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let syncs_snapshot = {
        let mut config = state.config.lock().await;
        let before = config.syncs.len();
        config.syncs.retain(|s| s.id != id);
        if config.syncs.len() == before {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Sync not found"))));
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        config.syncs.clone()
    };
    state.sync_manager.apply_config(&syncs_snapshot).await;
    let _ = state.sync_manager.stop(&id).await;
    Ok(Json(ApiResponse::success_no_data("Sync deleted")))
}

async fn start_sync(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config = state.config.lock().await;
        let Some(sync) = config.syncs.iter().find(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Sync not found"))));
        };
        sync.clone()
    };
    if let Err(e) = state.sync_manager.start(cfg.clone()).await {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))));
    }
    let syncs_snapshot = {
        let mut config = state.config.lock().await;
        if let Some(s) = config.syncs.iter_mut().find(|s| s.id == id) {
            s.enabled = true;
        }
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        config.syncs.clone()
    };
    state.sync_manager.apply_config(&syncs_snapshot).await;
    Ok(Json(ApiResponse::success_no_data("Sync started")))
}

async fn stop_sync(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    let syncs_snapshot = {
        let mut config = state.config.lock().await;
        let Some(sync) = config.syncs.iter_mut().find(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Sync not found"))));
        };
        sync.enabled = false;
        if let Err(e) = save_config(&config).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("Failed to save config: {}", e))),
            ));
        }
        config.syncs.clone()
    };
    state.sync_manager.apply_config(&syncs_snapshot).await;
    let _ = state.sync_manager.stop(&id).await;
    Ok(Json(ApiResponse::success_no_data("Sync stopped")))
}

async fn test_sync(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<SyncTestResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let cfg = {
        let config = state.config.lock().await;
        let Some(sync) = config.syncs.iter().find(|s| s.id == id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiResponse::error("Sync not found"))));
        };
        sync.clone()
    };
    if let Err(e) = state.sync_manager.test_sync(&cfg).await {
        return Err((StatusCode::BAD_REQUEST, Json(ApiResponse::error(e))));
    }
    Ok(Json(ApiResponse::success(
        "Sync test ok",
        SyncTestResponse { ok: true },
    )))
}

// ============================================================================
// Save config to config.yaml
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

fn normalize_sync_name(name: Option<String>) -> Option<String> {
    name.and_then(|n| {
        let trimmed = n.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_sync_remote_path(remote_path: Option<String>) -> Option<String> {
    remote_path.and_then(|p| {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_sync_options(mut options: SyncOptions) -> SyncOptions {
    options.exclude = options
        .exclude
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    options.include = options
        .include
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    options
}

async fn build_sync_local_paths(paths: &[String]) -> Result<Vec<SyncLocalPath>, String> {
    let mut items = Vec::new();
    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kind = match tokio::fs::metadata(trimmed).await {
            Ok(meta) if meta.is_dir() => SyncPathKind::Dir,
            Ok(meta) if meta.is_file() => SyncPathKind::File,
            Ok(_) => SyncPathKind::Missing,
            Err(_) => SyncPathKind::Missing,
        };
        items.push(SyncLocalPath {
            path: trimmed.to_string(),
            kind,
        });
    }
    if items.is_empty() {
        return Err("Local paths are required".to_string());
    }
    Ok(items)
}

fn normalize_sync_schedule(schedule: Option<SyncSchedule>) -> Result<Option<SyncSchedule>, String> {
    let Some(mut schedule) = schedule else {
        return Ok(None);
    };
    if schedule.timezone.trim().is_empty() {
        schedule.timezone = default_schedule_timezone();
    }
    if schedule.cron.trim().is_empty() {
        return Err("Cron expression is required".to_string());
    }
    let expr = schedule.cron.trim();
    let cron_expr = if expr.split_whitespace().count() == 5 {
        format!("0 {}", expr)
    } else {
        expr.to_string()
    };
    let _ = cron::Schedule::from_str(&cron_expr)
        .map_err(|e| format!("Invalid cron expression: {}", e))?;
    schedule.cron = schedule.cron.trim().to_string();
    Ok(Some(schedule))
}

fn build_sync_item(cfg: &SyncConfig, status: SyncRuntimeStatus) -> SyncItem {
    SyncItem {
        id: cfg.id.clone(),
        name: cfg.name.clone(),
        enabled: cfg.enabled,
        local_paths: cfg.local_paths.clone(),
        remote_path: cfg.remote_path.clone(),
        ssh: SyncSshInfo {
            host: cfg.ssh.host.clone(),
            port: cfg.ssh.port,
            username: cfg.ssh.username.clone(),
        },
        auth: redact_sync_auth(&cfg.ssh.auth),
        options: cfg.options.clone(),
        schedule: cfg.schedule.clone(),
        status,
    }
}

fn resolve_host_auth(host: &HostConfig) -> Result<TcpTunnelAuth, String> {
    match &host.auth {
        HostAuth::Password { password } => {
            let Some(pwd) = password.clone().filter(|p| !p.trim().is_empty()) else {
                return Err("Host password is required".to_string());
            };
            Ok(TcpTunnelAuth::Password { password: pwd })
        }
        HostAuth::PrivateKeyPath { path, passphrase } => {
            let resolved_path = resolve_private_key_path(path)
                .map_err(|_| "Host private key path is required".to_string())?;
            Ok(TcpTunnelAuth::PrivateKeyPath {
                path: resolved_path,
                passphrase: passphrase.clone(),
            })
        }
        HostAuth::SshAgent => Ok(TcpTunnelAuth::SshAgent),
    }
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
                host: "dns.cloudflare.com",
                ip: "1.1.1.1",
                path: "/dns-query",
            },
        ),
        (
            "doh-google",
            DohCandidate {
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
                    log_error!("Proxy monitor: failed to set initial active proxy: {}", e);
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
            log_error!("Proxy monitor: all candidates failed; pausing");
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
                    log_info!("Restored selection: {} -> {}", group, name);
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
            log_error!("Failed to restore selection for {}: {}", group, e);
        }
    }

    Ok(())
}

/// Regenerate sing-box config without restarting the service.
async fn regenerate_config(state: Arc<AppState>) -> Result<Config, String> {
    let config_clone = { state.config.lock().await.clone() };
    let loaded = load_subscriptions_and_update_state(&state, &config_clone).await;
    {
        let mut node_type_by_tag = state.node_type_by_tag.lock().await;
        *node_type_by_tag = build_node_type_map(&config_clone, &loaded);
    }

    gen_config(&config_clone, &state.sing_box_home, &loaded)
        .await
        .map_err(|e| format!("Failed to regenerate config: {}", e))?;
    log_info!("Config regenerated successfully");
    Ok(config_clone)
}

/// Regenerate sing-box config and restart the service
async fn regenerate_and_restart(state: Arc<AppState>) -> Result<(), String> {
    let config_clone = regenerate_config(state.clone()).await?;

    // Stop and restart sing-box
    stop_sing_internal().await;
    sleep(Duration::from_millis(500)).await;

    start_sing_internal(&state.sing_box_home)
        .await
        .map_err(|e| format!("重启 sing-box 失败: {}", e))?;
    let _ = apply_saved_selections(&config_clone).await;
    log_info!("sing-box restarted successfully");
    Ok(())
}

async fn load_subscriptions(
    config: &Config,
    root: &StdPath,
) -> (LoadedSubscriptions, HashMap<String, SubscriptionRuntime>) {
    let mut status_map: HashMap<String, SubscriptionRuntime> = HashMap::new();
    let mut merged_by_tag: HashMap<String, serde_json::Value> = HashMap::new();
    let mut tag_order: Vec<String> = vec![];
    let mut files: Vec<SubFileStatus> = vec![];
    let mut dir_error: Option<String> = None;
    let now_ts = chrono::Utc::now().timestamp();

    for sub in config.subscriptions.iter().filter(|s| s.enabled) {
        match prepare_subscription_dir(sub, root).await {
            Ok(dir) => {
                let loaded = load_subscription_dir(&dir, Some(&sub.id)).await;
                if dir_error.is_none() {
                    dir_error = loaded.dir_error.clone();
                }
                files.extend(loaded.files.clone());
                for outbound in loaded.outbounds {
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
                status_map.insert(
                    sub.id.clone(),
                    SubscriptionRuntime {
                        files: loaded.files,
                        error: loaded.dir_error.clone(),
                        updated_at: Some(now_ts),
                    },
                );
            }
            Err(err) => {
                if dir_error.is_none() {
                    dir_error = Some(err.clone());
                }
                status_map.insert(
                    sub.id.clone(),
                    SubscriptionRuntime {
                        files: vec![],
                        error: Some(err),
                        updated_at: None,
                    },
                );
            }
        }
    }

    let outbounds: Vec<serde_json::Value> = tag_order
        .iter()
        .filter_map(|tag| merged_by_tag.get(tag).cloned())
        .collect();

    (
        LoadedSubscriptions {
            files,
            outbounds,
            node_names: tag_order,
            dir_error,
        },
        status_map,
    )
}

async fn load_subscriptions_and_update_state(
    state: &Arc<AppState>,
    config: &Config,
) -> LoadedSubscriptions {
    let (loaded, status_map) = load_subscriptions(config, &state.subscriptions_root).await;
    {
        let mut guard = state.subscription_status.lock().await;
        *guard = status_map;
    }
    loaded
}

/// Extract embedded sing-box binary to current working directory
fn extract_sing_box() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let current_dir = std::env::current_dir()?;
    let sing_box_path = current_dir.join("sing-box");
    let force_marker = current_dir.join(".force_extract_sing_box");

    if force_marker.exists() || !sing_box_path.exists() {
        log_info!("Extracting embedded sing-box binary to {:?}", sing_box_path);
        fs::write(&sing_box_path, SING_BOX_BINARY)?;
        fs::set_permissions(&sing_box_path, fs::Permissions::from_mode(0o755))?;
        log_info!("sing-box binary extracted successfully");
        let _ = fs::remove_file(&force_marker);
    }

    let dashboard_dir = current_dir.join("dashboard");
    if !dashboard_dir.exists() {
        fs::create_dir_all(&dashboard_dir)?;
    }

    Ok(current_dir)
}

/// Extract embedded gotty binary to current working directory
fn extract_gotty() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let current_dir = std::env::current_dir()?;
    let gotty_path = current_dir.join("gotty");
    let force_marker = current_dir.join(".force_extract_gotty");

    if force_marker.exists() || !gotty_path.exists() {
        log_info!("Extracting embedded gotty binary to {:?}", gotty_path);
        fs::write(&gotty_path, GOTTY_BINARY)?;
        fs::set_permissions(&gotty_path, fs::Permissions::from_mode(0o755))?;
        log_info!("gotty binary extracted successfully");
        let _ = fs::remove_file(&force_marker);
    }

    Ok(gotty_path)
}

async fn check_and_install_openwrt_dependencies(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !PathBuf::from("/etc/openwrt_release").exists() {
        return Ok(());
    }

    log_info!("OpenWrt system detected. Checking dependencies...");

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
        log_info!("Required dependencies (kmod-tun, kmod-nft-queue) are already installed.");
        return Ok(());
    }

    log_info!(
        "Missing dependencies: {:?}. Installing...",
        packages_to_install
    );

    log_info!("Running 'opkg update'...");
    let update_status = tokio::process::Command::new("opkg")
        .arg("update")
        .status()
        .await?;

    if !update_status.success() {
        log_warning!("'opkg update' finished with error, but proceeding with installation attempt...");
    }

    for pkg in packages_to_install {
        log_info!("Installing {}...", pkg);
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

    log_info!("Dependencies installed successfully.");
    Ok(())
}

async fn start_sing_internal(
    sing_box_home: &str,
) -> Result<(), String> {
    let mut lock = SING_PROCESS.lock().await;
    if let Some(ref mut proc) = *lock {
        if proc.child.try_wait().map_err(|e| format!("等待进程失败: {}", e))?.is_none() {
            return Err("sing-box 正在运行中".to_string());
        }
    }

    let sing_box_path = PathBuf::from(sing_box_home).join("sing-box");
    let config_path = PathBuf::from(sing_box_home).join("config.json");

    // Check if sing-box binary exists
    if !sing_box_path.exists() {
        return Err(format!(
            "sing-box 二进制文件不存在: {:?}。请重新编译项目: bash ./build.sh",
            sing_box_path
        ));
    }

    // Check if config file exists
    if !config_path.exists() {
        return Err(format!(
            "sing-box 配置文件不存在: {:?}。请检查订阅或手动节点配置是否正确",
            config_path
        ));
    }

    log_info!("Starting sing-box from: {:?}", sing_box_path);
    log_info!("Using config: {:?}", config_path);

    let mut command = tokio::process::Command::new(&sing_box_path);
    command
        .current_dir(sing_box_home)
        .arg("run")
        .arg("-c")
        .arg(&config_path);

    let mut child = spawn_with_log_capture(&mut command, "sing-box".to_string())
        .map_err(|e| format!("启动 sing-box 进程失败: {}", e))?;
    let pid = child.id();
    log_info!("sing-box process spawned with PID: {:?}", pid);

    // Wait a short moment to check if process exits immediately
    sleep(Duration::from_millis(500)).await;
    if let Some(exit_status) = child.try_wait().map_err(|e| format!("等待进程失败: {}", e))? {
        let code = exit_status.code().unwrap_or(-1);
        // Try to read config for more details
        let config_content = tokio::fs::read_to_string(&config_path).await.ok();
        let config_hint = match &config_content {
            Some(content) => {
                if content.contains("outbounds") && content.contains("[]") {
                    "可能是没有可用节点，请检查订阅链接是否有效或添加手动节点".to_string()
                } else {
                    "配置文件可能存在语法错误".to_string()
                }
            }
            None => "配置文件读取失败".to_string(),
        };
        return Err(format!(
            "sing-box 启动后立即退出 (退出码: {})。{}",
            code, config_hint
        ));
    }

    // Store the process first
    *lock = Some(SingBoxProcess {
        child,
        started_at: Instant::now(),
    });
    drop(lock); // Release lock before connectivity check

    // Wait for sing-box to fully initialize
    sleep(Duration::from_secs(5)).await;

    // Check if sing-box is actually running by testing the API
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    match client.get("http://127.0.0.1:6262/proxies").send().await {
        Ok(_) => {
            log_info!("sing-box started successfully with Clash API available");
        }
        Err(e) => {
            log_warning!("sing-box started but Clash API not responding: {}", e);
            // Still consider it started, just log the warning
        }
    }

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

async fn start_terminal_internal(
    id: &str,
    config: &TerminalNodeConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut lock = GOTTY_PROCESSES.lock().await;
    if let Some(proc) = lock.get_mut(id) {
        if proc.child.try_wait().map_err(|e| format!("等待进程失败: {}", e))?.is_none() {
            return Err("terminal already running".into());
        }
        lock.remove(id);
    }

    let gotty_path = extract_gotty()?;
    log_info!("Starting gotty from: {:?}", gotty_path);

    let mut command = tokio::process::Command::new(&gotty_path);
    command
        .arg("-a")
        .arg(&config.addr)
        .arg("-p")
        .arg(config.port.to_string());

    if let (Some(user), Some(pass)) = (&config.auth_username, &config.auth_password) {
        if !user.trim().is_empty() && !pass.trim().is_empty() {
            command.arg("-c").arg(format!("{}:{}", user, pass));
        }
    }

    for arg in &config.extra_args {
        command.arg(arg);
    }

    command.arg(&config.command);
    for arg in &config.command_args {
        command.arg(arg);
    }

    let mut child = spawn_with_log_capture(&mut command, format!("gotty-{}", id))?;
    let pid = child.id();
    log_info!("gotty process spawned with PID: {:?}", pid);

    sleep(Duration::from_millis(300)).await;
    if let Some(exit_status) = child.try_wait().map_err(|e| format!("等待进程失败: {}", e))? {
        let code = exit_status.code().unwrap_or(-1);
        return Err(format!("gotty exited immediately with code {}", code).into());
    }

    lock.insert(
        id.to_string(),
        GottyProcess {
            child,
            started_at: Instant::now(),
        },
    );
    Ok(())
}

async fn stop_terminal_internal(id: &str) -> Result<(), String> {
    let mut lock = GOTTY_PROCESSES.lock().await;
    let Some(proc) = lock.get_mut(id) else {
        return Ok(());
    };
    if proc.child.try_wait().ok().flatten().is_none() {
        if let Some(pid) = proc.child.id() {
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            for _ in 0..30 {
                sleep(Duration::from_millis(100)).await;
                if proc.child.try_wait().ok().flatten().is_some() {
                    break;
                }
            }
            if proc.child.try_wait().ok().flatten().is_none() {
                proc.child.start_kill().ok();
            }
        }
    }
    lock.remove(id);
    Ok(())
}

/// POST /api/gotty/upgrade - Download and apply gotty binary upgrade
async fn upgrade_gotty() -> Json<ApiResponse<String>> {
    // 1. Stop all running terminals first
    log_info!("Stopping all terminals before gotty upgrade...");
    {
        let mut lock = GOTTY_PROCESSES.lock().await;
        for id in lock.keys().cloned().collect::<Vec<_>>() {
            drop(lock);
            if let Err(e) = stop_terminal_internal(&id).await {
                log_error!("Failed to stop terminal {}: {}", id, e);
            }
            lock = GOTTY_PROCESSES.lock().await;
        }
    }

    // 2. Download latest gotty binary
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build() {
        Ok(c) => c,
        Err(e) => return Json(ApiResponse::error(format!("Failed to create HTTP client: {}", e))),
    };

    let download_url = if cfg!(target_arch = "x86_64") {
        "https://github.com/Xiechengqi/gotty/releases/download/latest/gotty-linux-amd64"
    } else if cfg!(target_arch = "aarch64") {
        "https://github.com/Xiechengqi/gotty/releases/download/latest/gotty-linux-arm64"
    } else {
        return Json(ApiResponse::error("Unsupported architecture"));
    };

    log_info!("Downloading gotty from {}", download_url);

    match client.get(download_url).send().await {
        Ok(response) => {
            if !response.status().is_success() {
                return Json(ApiResponse::error(format!(
                    "Download failed with status: {}",
                    response.status()
                )));
            }

            match response.bytes().await {
                Ok(bytes) => {
                    let current_dir = match std::env::current_dir() {
                        Ok(d) => d,
                        Err(e) => return Json(ApiResponse::error(format!(
                            "Failed to get current directory: {}",
                            e
                        ))),
                    };
                    let gotty_path = current_dir.join("gotty");

                    // Backup current binary
                    let backup_path = format!("{}.bak", gotty_path.display());
                    if gotty_path.exists() {
                        if let Err(e) = fs::copy(&gotty_path, &backup_path) {
                            return Json(ApiResponse::error(format!(
                                "Failed to backup current gotty: {}",
                                e
                            )));
                        }
                        log_info!("Backed up current gotty to {:?}", backup_path);
                    }

                    // Write new binary
                    if let Err(e) = fs::write(&gotty_path, &bytes) {
                        // Try to restore backup
                        let _ = fs::copy(&backup_path, &gotty_path);
                        return Json(ApiResponse::error(format!(
                            "Failed to write new gotty binary: {}",
                            e
                        )));
                    }

                    // Set executable permissions
                    if let Err(e) = fs::set_permissions(&gotty_path, fs::Permissions::from_mode(0o755)) {
                        // Try to restore backup
                        let _ = fs::copy(&backup_path, &gotty_path);
                        return Json(ApiResponse::error(format!(
                            "Failed to set permissions: {}",
                            e
                        )));
                    }

                    log_info!("Gotty binary upgraded successfully to {:?}", gotty_path);

                    // Clean up backup if successful
                    if PathBuf::from(&backup_path).exists() {
                        let _ = fs::remove_file(&backup_path);
                    }

                    Json(ApiResponse::success(
                        "Gotty binary upgraded successfully. Please restart any running terminals.",
                        "success".to_string(),
                    ))
                }
                Err(e) => Json(ApiResponse::error(format!(
                    "Failed to download gotty binary: {}",
                    e
                ))),
            }
        }
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to download gotty: {}",
            e
        ))),
    }
}

fn binary_exists(cmd: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    for path in env::split_paths(&paths) {
        let candidate = path.join(cmd);
        if let Ok(metadata) = fs::metadata(&candidate) {
            if metadata.is_file() && (metadata.permissions().mode() & 0o111 != 0) {
                return true;
            }
        }
    }
    false
}

fn ensure_vnc_dependencies() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !binary_exists("vncserver") {
        return Err("vncserver not found in PATH".into());
    }
    if !binary_exists("vncpasswd") {
        return Err("vncpasswd not found in PATH".into());
    }
    if !PathBuf::from(KASMVNC_HTTPD_DIR).exists() {
        return Err(format!("kasmvnc httpd dir not found: {}", KASMVNC_HTTPD_DIR).into());
    }
    Ok(())
}

fn ensure_kasmvnc_web_defaults() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !PathBuf::from(KASMVNC_DEFAULTS_JS).exists() {
        let defaults_js = r#"(function() {
  var defaults = {
    'enable_ime': true,
    'resize': 'remote'
  };
  for (var key in defaults) {
    if (localStorage.getItem(key) === null) {
      localStorage.setItem(key, JSON.stringify(defaults[key]));
    }
  }
})();
"#;
        if let Err(e) = fs::write(KASMVNC_DEFAULTS_JS, defaults_js) {
            log_error!("Failed to write kasmvnc defaults js: {}", e);
            return Ok(());
        }
        let vnc_html_path = PathBuf::from(KASMVNC_HTTPD_DIR).join("vnc.html");
        if let Ok(contents) = fs::read_to_string(&vnc_html_path) {
            if !contents.contains("kasmvnc-defaults.js") {
                let updated = contents.replace(
                    "</head>",
                    "<script src=\"./kasmvnc-defaults.js\"></script></head>",
                );
                if let Err(e) = fs::write(&vnc_html_path, updated) {
                    log_error!("Failed to inject kasmvnc defaults js: {}", e);
                }
            }
        }
    }
    Ok(())
}

async fn start_vnc_internal(
    id: &str,
    config: &VncSessionConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut lock = VNC_PROCESSES.lock().await;
    if let Some(proc) = lock.get_mut(id) {
        if proc.child.try_wait().map_err(|e| format!("等待进程失败: {}", e))?.is_none() {
            return Err("vnc already running".into());
        }
        lock.remove(id);
    }

    ensure_vnc_dependencies()?;
    ensure_kasmvnc_web_defaults()?;

    let display = normalize_display_value(&config.display);
    let home_dir = PathBuf::from(KASMVNC_BASE_HOME).join(id);
    let vnc_dir = home_dir.join(".vnc");
    fs::create_dir_all(&vnc_dir)?;
    fs::write(vnc_dir.join(".de-was-selected"), "")?;

    let kasmvnc_yaml = r#"logging:
  log_writer_name: all
  log_dest: logfile
  level: 100

network:
  udp:
    public_ip: 127.0.0.1
  ssl:
    require_ssl: false
"#;
    fs::write(vnc_dir.join("kasmvnc.yaml"), kasmvnc_yaml)?;

    let password = config
        .password
        .clone()
        .unwrap_or_else(|| "kasmvnc".to_string());
    let password = if password.trim().is_empty() {
        "kasmvnc".to_string()
    } else {
        password
    };

    let mut pass_cmd = tokio::process::Command::new("vncpasswd");
    pass_cmd.arg("-u").arg(KASMVNC_USER);
    if !config.view_only {
        pass_cmd.arg("-w");
    }
    pass_cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .env("HOME", &home_dir);
    let mut child = pass_cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(format!("{}\n{}\n\n", password, password).as_bytes())
            .await?;
    }
    let status = child.wait().await?;
    if !status.success() {
        return Err("vncpasswd failed".into());
    }

    let _ = tokio::process::Command::new("vncserver")
        .arg("-kill")
        .arg(&display)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    let mut command = tokio::process::Command::new("vncserver");
    command
        .arg(&display)
        .arg("-fg")
        .arg("-noxstartup")
        .arg("-ac")
        .arg("-depth")
        .arg(config.depth.to_string())
        .arg("-geometry")
        .arg(&config.resolution)
        .arg("-websocketPort")
        .arg(config.port.to_string())
        .arg(format!("-FrameRate={}", config.frame_rate))
        .arg("-interface")
        .arg(&config.addr)
        .arg("-httpd")
        .arg(KASMVNC_HTTPD_DIR)
        .env("HOME", &home_dir)
        .current_dir(&home_dir);

    let mut child = spawn_with_log_capture(&mut command, format!("vnc-{}", id))?;
    let pid = child.id();
    log_info!("kasmvnc process spawned with PID: {:?}", pid);

    sleep(Duration::from_millis(500)).await;
    if let Some(exit_status) = child.try_wait().map_err(|e| format!("等待进程失败: {}", e))? {
        let code = exit_status.code().unwrap_or(-1);
        return Err(format!("kasmvnc exited immediately with code {}", code).into());
    }

    lock.insert(
        id.to_string(),
        VncProcess {
            child,
            started_at: Instant::now(),
        },
    );
    Ok(())
}

async fn stop_vnc_internal(id: &str, display: &str) -> Result<(), String> {
    let display = normalize_display_value(display);
    let _ = tokio::process::Command::new("vncserver")
        .arg("-kill")
        .arg(&display)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    let mut lock = VNC_PROCESSES.lock().await;
    let Some(proc) = lock.get_mut(id) else {
        return Ok(());
    };
    if proc.child.try_wait().ok().flatten().is_none() {
        if let Some(pid) = proc.child.id() {
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            for _ in 0..30 {
                sleep(Duration::from_millis(100)).await;
                if proc.child.try_wait().ok().flatten().is_some() {
                    break;
                }
            }
            if proc.child.try_wait().ok().flatten().is_none() {
                proc.child.start_kill().ok();
            }
        }
    }
    lock.remove(id);
    Ok(())
}

async fn start_app_internal(
    app: &AppConfig,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut lock = APP_PROCESSES.lock().await;
    if let Some(proc) = lock.get_mut(&app.id) {
        if proc.child.try_wait().map_err(|e| format!("等待进程失败: {}", e))?.is_none() {
            return Err("应用已在运行".into());
        }
        lock.remove(&app.id);
    }
    drop(lock);

    let mut display = app.display.clone();
    if let Some(vnc_id) = &app.vnc_session_id {
        let vnc = config
            .vnc_sessions
            .iter()
            .find(|v| v.id == *vnc_id)
            .cloned()
            .ok_or_else(|| format!("VNC 会话不存在: {}", vnc_id))?;
        if let Some(err) = app_vnc_conflict(&app.id, vnc_id, &config.apps) {
            return Err(err.into());
        }
        display = Some(vnc.display.clone());
        let vnc_status = get_vnc_runtime_status(&vnc.id).await;
        if !vnc_status.running {
            if let Some(err) = vnc_bind_conflict(&vnc.id, &vnc, &config.vnc_sessions) {
                return Err(err.into());
            }
            start_vnc_internal(&vnc.id, &vnc).await?;
        }
    }

    let display = display.ok_or_else(|| "未绑定 VNC 时必须填写 DISPLAY".to_string())?;
    let display = normalize_display_value(&display);

    if app.command.trim().is_empty() {
        return Err("应用启动命令不能为空".into());
    }

    let mut command = tokio::process::Command::new(&app.command);
    for arg in &app.args {
        command.arg(arg);
    }
    command.env("DISPLAY", &display);
    for (k, v) in &app.env {
        command.env(k, v);
    }

    let mut child = spawn_with_log_capture(&mut command, format!("app-{}", app.id))?;
    let pid = child.id();
    log_info!("app process spawned with PID: {:?}", pid);

    sleep(Duration::from_millis(300)).await;
    if let Some(exit_status) = child.try_wait().map_err(|e| format!("等待进程失败: {}", e))? {
        let code = exit_status.code().unwrap_or(-1);
        return Err(format!("app exited immediately with code {}", code).into());
    }

    let mut lock = APP_PROCESSES.lock().await;
    lock.insert(
        app.id.clone(),
        AppProcess {
            child,
            started_at: Instant::now(),
        },
    );
    Ok(())
}

async fn stop_app_internal(id: &str) -> Result<(), String> {
    let mut lock = APP_PROCESSES.lock().await;
    let Some(proc) = lock.get_mut(id) else {
        return Ok(());
    };
    if proc.child.try_wait().ok().flatten().is_none() {
        if let Some(pid) = proc.child.id() {
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            for _ in 0..30 {
                sleep(Duration::from_millis(100)).await;
                if proc.child.try_wait().ok().flatten().is_some() {
                    break;
                }
            }
            if proc.child.try_wait().ok().flatten().is_none() {
                proc.child.start_kill().ok();
            }
        }
    }
    lock.remove(id);
    Ok(())
}

// Generate SSH outbounds from hosts configuration
fn generate_ssh_outbounds_from_hosts(hosts: &[HostConfig]) -> (Vec<String>, Vec<serde_json::Value>) {
    let mut names = Vec::new();
    let mut outbounds = Vec::new();

    for host in hosts {
        let tag = host.name.clone().unwrap_or_else(|| format!("ssh-{}", host.host));
        names.push(tag.clone());

        let mut outbound = serde_json::json!({
            "type": "ssh",
            "tag": tag,
            "server": host.host,
            "server_port": host.port,
            "user": host.username,
        });

        match &host.auth {
            HostAuth::Password { password } => {
                if let Some(pwd) = password {
                    outbound["password"] = serde_json::Value::String(pwd.clone());
                }
            }
            HostAuth::PrivateKeyPath { path, passphrase } => {
                if let Ok(resolved_path) = resolve_private_key_path(path) {
                    outbound["private_key_path"] = serde_json::Value::String(resolved_path);
                }
                if let Some(pp) = passphrase {
                    outbound["private_key_passphrase"] = serde_json::Value::String(pp.clone());
                }
            }
            HostAuth::SshAgent => {
                // SSH agent doesn't need additional config
            }
        }

        outbounds.push(outbound);
    }

    (names, outbounds)
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

    // Generate SSH outbounds from hosts
    let (host_names, host_outbounds) = generate_ssh_outbounds_from_hosts(&config.hosts);

    let mut final_outbounds: Vec<serde_json::Value> = vec![];
    let mut final_node_names: Vec<String> = vec![];

    final_node_names.extend(subs.node_names.iter().cloned());
    final_outbounds.extend(subs.outbounds.iter().cloned());

    // Add host SSH outbounds
    final_node_names.extend(host_names);
    final_outbounds.extend(host_outbounds);

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
                {"ip_cidr": ["100.64.0.0/10"], "action": "route", "outbound": "direct"},
                {"ip_is_private": true, "action": "route", "outbound": "direct"},
                {"protocol": "ssh", "action": "route", "outbound": "direct"},
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
            log_info!("Detected sing-box JSON format subscription");
            return parse_singbox_json(outbounds_arr);
        }
    }

    // Try parsing as Shadowsocks URL list (base64 encoded)
    if let Some(ss_result) = try_parse_ss_urls(text) {
        log_info!("Detected Shadowsocks URL format subscription");
        return ss_result;
    }

    // Fall back to YAML parsing (Clash format)
    log_info!("Parsing as Clash YAML format");
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

async fn load_subscription_dir(sub_dir: &StdPath, subscription_id: Option<&str>) -> LoadedSubscriptions {
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
                    subscription_id: subscription_id.map(|value| value.to_string()),
                    error: None,
                });
            }
            Err(e) => {
                file_statuses.push(SubFileStatus {
                    file_name,
                    file_path: path.display().to_string(),
                    loaded: false,
                    node_count: 0,
                    subscription_id: subscription_id.map(|value| value.to_string()),
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

    log_info!("Parsed {} nodes from sing-box JSON", node_names.len());
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

    log_info!("Parsed {} nodes from Shadowsocks URLs", node_names.len());
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

async fn sync_git_repo(repo: &str, target: &StdPath) -> Result<(), String> {
    let git_dir = target.join(".git");
    if tokio::fs::metadata(&git_dir).await.is_ok() {
        let out = tokio::process::Command::new("git")
            .arg("-C")
            .arg(target)
            .arg("pull")
            .arg("--ff-only")
            .output()
            .await
            .map_err(|e| format!("Failed to run git pull: {}", e))?;
        if out.status.success() {
            return Ok(());
        }
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

    remove_path_if_exists(target).await?;
    let out = tokio::process::Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(repo)
        .arg(target)
        .output()
        .await
        .map_err(|e| format!("Failed to run git clone: {}", e))?;

    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!(
        "Git clone failed: {}",
        if detail.is_empty() {
            out.status.to_string()
        } else {
            detail
        }
    ))
}

async fn fetch_subscription_url(url: &str, dest_dir: &StdPath) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(dest_dir)
        .await
        .map_err(|e| format!("Failed to create dir {}: {}", dest_dir.display(), e))?;
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;
    if !resp.status().is_success() {
        return Err(format!("Failed to fetch {}: {}", url, resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    let target = dest_dir.join("subscription.yaml");
    tokio::fs::write(&target, bytes)
        .await
        .map_err(|e| format!("Failed to write {}: {}", target.display(), e))?;
    Ok(target)
}

async fn prepare_subscription_dir(
    sub: &SubscriptionConfig,
    root: &StdPath,
) -> Result<PathBuf, String> {
    match &sub.source {
        SubscriptionSource::Url { url } => {
            let dir = root.join(&sub.id);
            let _ = fetch_subscription_url(url, &dir).await?;
            Ok(dir)
        }
        SubscriptionSource::Git { repo } => {
            let dir = root.join(&sub.id);
            tokio::fs::create_dir_all(root)
                .await
                .map_err(|e| format!("Failed to create dir {}: {}", root.display(), e))?;
            sync_git_repo(repo, &dir).await?;
            Ok(dir)
        }
        SubscriptionSource::Path { path } => {
            let dir = PathBuf::from(path);
            tokio::fs::metadata(&dir)
                .await
                .map_err(|e| format!("Subscription path {} is not available: {}", dir.display(), e))?;
            Ok(dir)
        }
    }
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
选项:\n  -h, --help             显示帮助并退出\n\n\
说明:\n  - 配置文件为当前目录下的 ./config.yaml\n  - 正常运行需要 root 权限（--help 例外）",
            program = program_name
        );
        return Ok(());
    }

    // Check for root privileges
    if !Uid::effective().is_root() {
        log_error!("Error: This application must be run as root.");
        std::process::exit(1);
    }

    let subscriptions_root = PathBuf::from("sub");

    log_info!("Reading configuration...");
    let (mut config, setup_required) = match tokio::fs::read_to_string("config.yaml").await {
        Ok(text) => (serde_yaml::from_str::<Config>(&text)?, false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            Config {
                port: Some(DEFAULT_PORT),
                sing_box_home: None,
                password: None,
                terminal: None,
                terminals: vec![],
                vnc_sessions: vec![],
                apps: vec![],
                syncs: vec![],
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
                subscriptions: vec![],
                hosts: vec![],
                metrics: MetricsConfig::default(),
            },
            true,
        ),
        Err(e) => return Err(e.into()),
    };

    migrate_terminals(&mut config);
    let mut subscriptions_changed = normalize_subscriptions(&mut config);
    if config.subscriptions.is_empty()
        && tokio::fs::metadata(&subscriptions_root).await.is_ok()
    {
        config.subscriptions.push(SubscriptionConfig {
            id: generate_subscription_id(),
            name: Some("本地订阅".to_string()),
            enabled: true,
            source: SubscriptionSource::Path {
                path: subscriptions_root.display().to_string(),
            },
        });
        subscriptions_changed = true;
    }
    if subscriptions_changed && !setup_required {
        let _ = save_config(&config).await;
    }

    let port = config.port.unwrap_or(DEFAULT_PORT);

    // Extract embedded sing-box binary and determine working directory
    let sing_box_home = if let Some(custom_home) = &config.sing_box_home {
        custom_home.clone()
    } else {
        extract_sing_box()?.to_string_lossy().to_string()
    };

    log_info!("Loading subscriptions from: {}", subscriptions_root.display());
    let (loaded_subs, subscription_status) = load_subscriptions(&config, &subscriptions_root).await;
    let node_type_by_tag = build_node_type_map(&config, &loaded_subs);

    if !setup_required {
        // Generate initial config
        log_info!("Generating initial config...");
        match gen_config(&config, &sing_box_home, &loaded_subs).await {
            Ok(_) => {
                // Check OpenWrt dependencies
                log_info!("Checking dependencies...");
                if let Err(e) = check_and_install_openwrt_dependencies().await {
                    log_error!("Failed to check or install OpenWrt dependencies: {}", e);
                }

                // Start sing-box
                match start_sing_internal(&sing_box_home).await {
                    Ok(_) => {
                        let _ = apply_saved_selections(&config).await;
                        log_info!("sing-box started successfully")
                    }
                    Err(e) => log_error!("Failed to start sing-box: {}", e),
                }
            }
            Err(e) => {
                log_error!(
                    "Failed to generate config: {}. Please add subscription files under {} and reload.",
                    e,
                    subscriptions_root.display()
                );
            }
        }

        for terminal in &config.terminals {
            if !terminal.enabled {
                continue;
            }
            match start_terminal_internal(&terminal.id, terminal).await {
                Ok(_) => log_info!("gotty started successfully"),
                Err(e) => log_error!("Failed to start gotty: {}", e),
            }
        }

        for vnc in &config.vnc_sessions {
            if !vnc.enabled {
                continue;
            }
            match start_vnc_internal(&vnc.id, vnc).await {
                Ok(_) => log_info!("kasmvnc started successfully"),
                Err(e) => log_error!("Failed to start kasmvnc: {}", e),
            }
        }

        let config_snapshot = config.clone();
        for app in &config_snapshot.apps {
            if !app.enabled {
                continue;
            }
            match start_app_internal(app, &config_snapshot).await {
                Ok(_) => log_info!("应用启动成功"),
                Err(e) => log_error!("Failed to start app {}: {}", app.id, e),
            }
        }

    } else {
        log_info!("No config.yaml found, entering setup mode at http://localhost:{}", port);
    }

    let app_state = Arc::new(AppState {
        config: Mutex::new(config.clone()),
        sing_box_home: sing_box_home.clone(),
        subscriptions_root: subscriptions_root.clone(),
        subscription_status: Mutex::new(subscription_status),
        node_type_by_tag: Mutex::new(node_type_by_tag),
        dns_monitor: Mutex::new(DnsMonitorState::default()),
        proxy_monitor: Mutex::new(ProxyMonitorState::default()),
        setup_required: AtomicBool::new(setup_required),
        tcp_tunnel: tcp_tunnel::TunnelManager::new(),
        full_tunnel: full_tunnel::FullTunnelManager::new(),
        sync_manager: sync::SyncManager::new(),
        system_monitor: SystemMonitor::new(),
        metrics_config: config.metrics.clone(),
    });

    // Apply initial TCP tunnel config (best-effort).
    {
        let cfg = app_state.config.lock().await;
        app_state.tcp_tunnel.apply_config(&cfg.tcp_tunnels).await;
        app_state
            .full_tunnel
            .sync_from_config(app_state.clone(), cfg.tcp_tunnel_sets.clone())
            .await;
        app_state.sync_manager.apply_config(&cfg.syncs).await;
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

    {
        let state_clone = app_state.clone();
        let interval_secs = app_state.metrics_config.sample_interval_secs.max(1);
        tokio::spawn(async move {
            if let Err(e) = refresh_system_metrics(&state_clone).await {
                log_error!("Failed to refresh system metrics: {}", e);
            }
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                if let Err(e) = refresh_system_metrics(&state_clone).await {
                    log_error!("Failed to refresh system metrics: {}", e);
                }
            }
        });
    }



    // Build router with API endpoints

    // 需要认证的路由
    let protected_routes = Router::new()
        // Status and service control
        .route("/api/status", get(get_status))
        .route("/api/system/info", get(get_system_info))
        .route("/api/system/status", get(get_system_status))
        .route("/api/system/metrics", get(get_system_metrics))
        .route("/api/password", post(update_password))
        .route("/api/service/start", post(start_service))
        .route("/api/service/stop", post(stop_service))
        .route("/api/service/restart", post(restart_service))
        .route("/api/terminals", get(get_terminals))
        .route("/api/terminals", post(create_terminal))
        .route("/api/terminals/{id}", put(update_terminal).delete(delete_terminal))
        .route("/api/terminals/{id}/start", post(start_terminal))
        .route("/api/terminals/{id}/stop", post(stop_terminal))
        .route("/api/terminals/{id}/restart", post(restart_terminal))
        .route("/api/gotty/upgrade", post(upgrade_gotty))
        .route("/api/vnc-sessions", get(get_vnc_sessions))
        .route("/api/vnc-sessions", post(create_vnc_session))
        .route("/api/vnc-sessions/{id}", put(update_vnc_session).delete(delete_vnc_session))
        .route("/api/vnc-sessions/{id}/start", post(start_vnc_session))
        .route("/api/vnc-sessions/{id}/stop", post(stop_vnc_session))
        .route("/api/vnc-sessions/{id}/restart", post(restart_vnc_session))
        .route("/api/apps/templates", get(get_app_templates_handler))
        .route("/api/apps", get(get_apps))
        .route("/api/apps", post(create_app))
        .route("/api/apps/{id}", put(update_app).delete(delete_app))
        .route("/api/apps/{id}/start", post(start_app))
        .route("/api/apps/{id}/stop", post(stop_app))
        .route("/api/apps/{id}/restart", post(restart_app))
        // Connectivity test
        .route("/api/connectivity", post(test_connectivity))
        // Upgrade (protected)
        .route("/api/upgrade", post(upgrade))
        // Clash API proxy (protected HTTP)
        .route("/api/clash/proxies", get(clash_get_proxies))
        .route("/api/clash/proxies/{group}", put(clash_switch_proxy))
        .route("/api/clash/proxies/{node}/delay", get(clash_test_delay))
        .route("/api/clash/proxies/delay", post(clash_test_batch_delay))
        .route("/api/selections", get(get_selections))
        .route("/api/proxy/status", get(get_proxy_status))
        .route("/api/proxy/pool", put(update_proxy_pool))
        // Subscription file management
        .route("/api/sub-files", get(get_sub_files))
        .route("/api/sub-files/reload", post(reload_sub_files))
        .route("/api/subscriptions", get(list_subscriptions))
        .route("/api/subscriptions", post(create_subscription))
        .route("/api/subscriptions/{id}", put(update_subscription).delete(delete_subscription))
        .route("/api/subscriptions/{id}/reload", post(reload_subscription))
        .route("/api/subscriptions/reload", post(reload_subscriptions))
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
        .route("/api/syncs", get(get_syncs))
        .route("/api/syncs", post(create_sync))
        .route("/api/syncs/{id}", put(update_sync).delete(delete_sync))
        .route("/api/syncs/{id}/start", post(start_sync))
        .route("/api/syncs/{id}/stop", post(stop_sync))
        .route("/api/syncs/{id}/test", post(test_sync))
        // Host management
        .route("/api/hosts", get(get_hosts).post(create_host))
        .route("/api/hosts/default-key-path", get(get_host_default_key_path))
        .route("/api/hosts/test", post(test_host_config))
        .route("/api/hosts/{id}", get(get_host).put(update_host).delete(delete_host))
        .route("/api/hosts/{id}/test", post(test_host))
        .route_layer(middleware::from_fn(auth_middleware));  // 应用认证中间件

    // 公开路由（不需要认证）
    let ws_routes = Router::new()
        .route("/api/clash/ws/traffic", get(clash_ws_traffic))
        .route("/api/clash/ws/logs", get(clash_ws_logs));

    let app = Router::new()
        // API routes (highest priority)
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup/init", post(setup_init))
        .route("/api/login", post(login))
        .route("/api/version", get(get_version))
        .merge(ws_routes)
        .merge(protected_routes)
        // Static assets route (matches files in public/)
        .route("/{*path}", get(serve_static))
        .with_state(app_state)
        // SPA fallback (must be last, catches all unmatched routes)
        .fallback(spa_fallback);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    log_info!("✅ Miao 控制面板已启动: http://localhost:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
