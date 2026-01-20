use crate::TcpTunnelConfig;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, watch, Mutex};
use tokio::time::{sleep, Duration};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelState {
    Stopped,
    Connecting,
    Forwarding,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct TunnelErrorInfo {
    pub code: String,
    pub message: String,
    pub at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TunnelRuntimeStatus {
    pub state: TunnelState,
    pub active_conns: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub last_ok_at_ms: Option<i64>,
    pub last_error: Option<TunnelErrorInfo>,
}

impl Default for TunnelRuntimeStatus {
    fn default() -> Self {
        Self {
            state: TunnelState::Stopped,
            active_conns: 0,
            bytes_in: 0,
            bytes_out: 0,
            last_ok_at_ms: None,
            last_error: None,
        }
    }
}

#[derive(Clone)]
pub struct TunnelManager {
    inner: Arc<InnerManager>,
}

struct InnerManager {
    tunnels: Mutex<HashMap<String, TunnelHandle>>,
}

struct TunnelHandle {
    config: TcpTunnelConfig,
    status: Arc<RwLock<TunnelRuntimeStatus>>,
    stop_tx: watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
}

fn runtime_config_equal(a: &TcpTunnelConfig, b: &TcpTunnelConfig) -> bool {
    a.enabled == b.enabled
        && a.local_addr == b.local_addr
        && a.local_port == b.local_port
        && a.remote_bind_addr == b.remote_bind_addr
        && a.remote_port == b.remote_port
        && a.ssh_host == b.ssh_host
        && a.ssh_port == b.ssh_port
        && a.username == b.username
        && a.auth == b.auth
        && a.strict_host_key_checking == b.strict_host_key_checking
        && a.host_key_fingerprint == b.host_key_fingerprint
        && a.allow_public_bind == b.allow_public_bind
        && a.connect_timeout_ms == b.connect_timeout_ms
        && a.keepalive_interval_ms == b.keepalive_interval_ms
        && a.reconnect_backoff_ms == b.reconnect_backoff_ms
}

impl TunnelManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InnerManager {
                tunnels: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn supported(&self) -> bool {
        cfg!(feature = "tcp_tunnel")
    }

    pub async fn apply_config(&self, configs: &[TcpTunnelConfig]) {
        let mut to_join: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        let mut guard = self.inner.tunnels.lock().await;
        let mut desired: HashMap<String, TcpTunnelConfig> = HashMap::new();
        for c in configs {
            desired.insert(c.id.clone(), c.clone());
        }

        let existing_ids: Vec<String> = guard.keys().cloned().collect();
        for id in existing_ids {
            if !desired.contains_key(&id) {
                if let Some(handle) = guard.remove(&id) {
                    let _ = handle.stop_tx.send(true);
                    to_join.push(handle.join);
                }
            }
        }

        for (id, cfg) in desired {
            if !guard.contains_key(&id) {
                {
                    let handle = spawn_tunnel(cfg).await;
                    guard.insert(id, handle);
                }
                continue;
            }

            let existing = guard.get_mut(&id).expect("exists");

            // Avoid restarting the runtime for metadata-only changes (e.g. name).
            // This also prevents UI hangs when a restart would wait on long-lived connections.
            if !runtime_config_equal(&existing.config, &cfg) {
                let old = guard.remove(&id).expect("exists");
                let _ = old.stop_tx.send(true);
                to_join.push(old.join);
                let handle = spawn_tunnel(cfg).await;
                guard.insert(id, handle);
                continue;
            }

            existing.config = cfg.clone();
            if !cfg.enabled {
                let _ = existing.stop_tx.send(true);
            } else {
                let _ = existing.stop_tx.send(false);
            }
        }

        drop(guard);
        for j in to_join {
            let _ = j.await;
        }
    }

    pub async fn start(&self, id: &str) -> Result<(), String> {
        let guard = self.inner.tunnels.lock().await;
        let Some(handle) = guard.get(id) else {
            return Err("Tunnel not found".to_string());
        };
        let _ = handle.stop_tx.send(false);
        Ok(())
    }

    pub async fn stop(&self, id: &str) -> Result<(), String> {
        let guard = self.inner.tunnels.lock().await;
        let Some(handle) = guard.get(id) else {
            return Err("Tunnel not found".to_string());
        };
        let _ = handle.stop_tx.send(true);
        Ok(())
    }

    pub async fn restart(&self, id: &str) -> Result<(), String> {
        let cfg = {
            let guard = self.inner.tunnels.lock().await;
            let Some(old) = guard.get(id) else {
                return Err("Tunnel not found".to_string());
            };
            old.config.clone()
        };
        self.restart_with_config(cfg).await
    }

    pub async fn restart_with_config(&self, cfg: TcpTunnelConfig) -> Result<(), String> {
        let mut join: Option<tokio::task::JoinHandle<()>> = None;

        {
            let mut guard = self.inner.tunnels.lock().await;
            if let Some(old) = guard.remove(&cfg.id) {
                let _ = old.stop_tx.send(true);
                join = Some(old.join);
            }
        }

        if let Some(j) = join {
            let _ = j.await;
        }

        let mut guard = self.inner.tunnels.lock().await;
        let new_handle = spawn_tunnel(cfg.clone()).await;
        guard.insert(cfg.id.clone(), new_handle);
        Ok(())
    }

    pub async fn get_status(&self, id: &str) -> Option<TunnelRuntimeStatus> {
        let guard = self.inner.tunnels.lock().await;
        let handle = guard.get(id)?;
        let out = handle.status.read().await.clone();
        Some(out)
    }

    pub async fn list(&self) -> Vec<(TcpTunnelConfig, TunnelRuntimeStatus)> {
        let guard = self.inner.tunnels.lock().await;
        let mut out = Vec::with_capacity(guard.len());
        for handle in guard.values() {
            out.push((handle.config.clone(), handle.status.read().await.clone()));
        }
        out
    }

    pub async fn test(&self, cfg: &TcpTunnelConfig) -> Result<(), (String, String)> {
        if !cfg!(feature = "tcp_tunnel") {
            let _ = cfg;
            return Err((
                "NOT_SUPPORTED".to_string(),
                "tcp_tunnel feature not enabled".to_string(),
            ));
        }
        #[cfg(feature = "tcp_tunnel")]
        {
            return test_once(cfg).await;
        }
        #[cfg(not(feature = "tcp_tunnel"))]
        unreachable!();
    }

    pub async fn test_ssh_only(&self, cfg: &TcpTunnelConfig) -> Result<(), (String, String)> {
        if !cfg!(feature = "tcp_tunnel") {
            let _ = cfg;
            return Err((
                "NOT_SUPPORTED".to_string(),
                "tcp_tunnel feature not enabled".to_string(),
            ));
        }
        #[cfg(feature = "tcp_tunnel")]
        {
            return test_ssh_only_once(cfg).await;
        }
        #[cfg(not(feature = "tcp_tunnel"))]
        unreachable!();
    }
}

async fn spawn_tunnel(cfg: TcpTunnelConfig) -> TunnelHandle {
    let status = Arc::new(RwLock::new(TunnelRuntimeStatus::default()));
    let (stop_tx, stop_rx) = watch::channel(!cfg.enabled);
    let status_clone = status.clone();
    let cfg_clone = cfg.clone();
    let join = tokio::spawn(async move {
        run_tunnel(cfg_clone, status_clone, stop_rx).await;
    });
    TunnelHandle {
        config: cfg,
        status,
        stop_tx,
        join,
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn validate(cfg: &TcpTunnelConfig) -> Result<(), (String, String)> {
    if cfg.remote_port == 0 {
        return Err((
            "CONFIG_INVALID".to_string(),
            "remote_port must be > 0".to_string(),
        ));
    }
    if cfg.remote_bind_addr == "0.0.0.0" && !cfg.allow_public_bind {
        return Err((
            "PUBLIC_BIND_NOT_ALLOWED".to_string(),
            "allow_public_bind must be true when remote_bind_addr is 0.0.0.0".to_string(),
        ));
    }
    if cfg.strict_host_key_checking && cfg.host_key_fingerprint.trim().is_empty() {
        return Err((
            "HOSTKEY_MISSING".to_string(),
            "host_key_fingerprint is required when strict_host_key_checking is true".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "tcp_tunnel")]
fn default_ssh_key_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let base = PathBuf::from(home).join(".ssh");
    vec![
        base.join("id_ed25519"),
        base.join("id_rsa"),
        base.join("id_ecdsa"),
    ]
}

#[cfg(feature = "tcp_tunnel")]
async fn authenticate_session(
    session: &mut russh::client::Handle<TunnelClientHandler>,
    cfg: &TcpTunnelConfig,
    connect_timeout: Duration,
) -> Result<russh::client::AuthResult, (String, String)> {
    use crate::TcpTunnelAuth;
    use russh::keys::key::PrivateKeyWithHashAlg;
    use russh::keys::load_secret_key;

    match &cfg.auth {
        TcpTunnelAuth::Password { password } => {
            if !password.is_empty() {
                return tokio::time::timeout(
                    connect_timeout,
                    session.authenticate_password(cfg.username.clone(), password.clone()),
                )
                .await
                .map_err(|_| ("AUTH_TIMEOUT".to_string(), "authentication timeout".to_string()))?
                .map_err(|e| ("AUTH_FAILED".to_string(), format!("{e:?}")));
            }

            let mut last_err: Option<String> = None;
            let key_paths = default_ssh_key_paths();
            if key_paths.is_empty() {
                return Err((
                    "AUTH_MISSING".to_string(),
                    "password is empty and no default ssh keys found".to_string(),
                ));
            }

            for path in key_paths {
                if !path.exists() {
                    continue;
                }
                let key = match load_secret_key(&path, None) {
                    Ok(k) => k,
                    Err(e) => {
                        last_err = Some(format!("{e:?}"));
                        continue;
                    }
                };
                let rsa_hash = match tokio::time::timeout(
                    connect_timeout,
                    session.best_supported_rsa_hash(),
                )
                .await
                {
                    Ok(Ok(v)) => v.flatten(),
                    Ok(Err(e)) => {
                        last_err = Some(format!("{e:?}"));
                        continue;
                    }
                    Err(_) => {
                        return Err((
                            "AUTH_TIMEOUT".to_string(),
                            "authentication timeout".to_string(),
                        ));
                    }
                };

                let auth = tokio::time::timeout(
                    connect_timeout,
                    session.authenticate_publickey(
                        cfg.username.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
                    ),
                )
                .await
                .map_err(|_| ("AUTH_TIMEOUT".to_string(), "authentication timeout".to_string()))?
                .map_err(|e| ("AUTH_FAILED".to_string(), format!("{e:?}")))?;

                if auth.success() {
                    return Ok(auth);
                }
                last_err = Some("authentication failed".to_string());
            }

            Err((
                "AUTH_FAILED".to_string(),
                last_err.unwrap_or_else(|| "authentication failed".to_string()),
            ))
        }
        TcpTunnelAuth::PrivateKeyPath { path, passphrase } => {
            let key = load_secret_key(path, passphrase.as_deref())
                .map_err(|e| ("AUTH_FAILED".to_string(), format!("{e:?}")))?;
            let rsa_hash = tokio::time::timeout(connect_timeout, session.best_supported_rsa_hash())
                .await
                .map_err(|_| ("AUTH_TIMEOUT".to_string(), "authentication timeout".to_string()))?
                .map_err(|e| ("AUTH_FAILED".to_string(), format!("{e:?}")))?
                .flatten();
            tokio::time::timeout(
                connect_timeout,
                session.authenticate_publickey(
                    cfg.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
                ),
            )
            .await
            .map_err(|_| ("AUTH_TIMEOUT".to_string(), "authentication timeout".to_string()))?
            .map_err(|e| ("AUTH_FAILED".to_string(), format!("{e:?}")))
        }
    }
}

async fn set_error(
    status: &Arc<RwLock<TunnelRuntimeStatus>>,
    code: &str,
    message: &str,
) {
    let mut s = status.write().await;
    s.state = TunnelState::Error;
    s.last_error = Some(TunnelErrorInfo {
        code: code.to_string(),
        message: message.to_string(),
        at_ms: now_ms(),
    });
}

async fn record_last_error(status: &Arc<RwLock<TunnelRuntimeStatus>>, code: &str, message: &str) {
    let mut s = status.write().await;
    s.last_error = Some(TunnelErrorInfo {
        code: code.to_string(),
        message: message.to_string(),
        at_ms: now_ms(),
    });
}

async fn set_state(status: &Arc<RwLock<TunnelRuntimeStatus>>, st: TunnelState) {
    let mut s = status.write().await;
    s.state = st.clone();
    if matches!(st, TunnelState::Forwarding) {
        s.last_ok_at_ms = Some(now_ms());
        s.last_error = None;
    }
}

fn backoff(cfg: &TcpTunnelConfig, attempt: u32) -> Duration {
    let base_ms = cfg.reconnect_backoff_ms.base_ms;
    let max_ms = cfg.reconnect_backoff_ms.max_ms;
    let shift = attempt.min(16);
    let mul = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let d = base_ms.saturating_mul(mul).min(max_ms);
    Duration::from_millis(d.max(200))
}

async fn run_tunnel(
    cfg: TcpTunnelConfig,
    status: Arc<RwLock<TunnelRuntimeStatus>>,
    mut stop_rx: watch::Receiver<bool>,
) {
    if let Err((c, m)) = validate(&cfg) {
        set_error(&status, &c, &m).await;
    }

    let mut attempt: u32 = 0;

    loop {
        if *stop_rx.borrow() {
            set_state(&status, TunnelState::Stopped).await;
            if stop_rx.changed().await.is_err() {
                break;
            }
            attempt = 0;
            continue;
        }

        set_state(&status, TunnelState::Connecting).await;

        match connect_and_forward(&cfg, &status, &mut stop_rx).await {
            Ok(()) => {
                set_state(&status, TunnelState::Stopped).await;
                attempt = 0;
            }
            Err((code, message, retryable)) => {
                set_error(&status, &code, &message).await;
                if !retryable {
                    let _ = stop_rx.changed().await;
                    attempt = 0;
                    continue;
                }
                let wait = backoff(&cfg, attempt);
                attempt = attempt.saturating_add(1);
                tokio::select! {
                    _ = stop_rx.changed() => {},
                    _ = sleep(wait) => {},
                }
            }
        }
    }
}

#[cfg(feature = "tcp_tunnel")]
async fn connect_and_forward(
    cfg: &TcpTunnelConfig,
    status: &Arc<RwLock<TunnelRuntimeStatus>>,
    stop_rx: &mut watch::Receiver<bool>,
) -> Result<(), (String, String, bool)> {
    use russh::client;
    use russh::Disconnect;
    use std::borrow::Cow;

    validate(cfg).map_err(|(c, m)| (c, m, false))?;

    let handler = TunnelClientHandler::new(cfg.clone(), status.clone());

    let client_cfg = client::Config {
        nodelay: true,
        inactivity_timeout: None,
        preferred: russh::Preferred {
            kex: Cow::Owned(vec![
                russh::kex::CURVE25519_PRE_RFC_8731,
                russh::kex::EXTENSION_SUPPORT_AS_CLIENT,
            ]),
            ..Default::default()
        },
        ..Default::default()
    };
    let client_cfg = Arc::new(client_cfg);

    let addr = (cfg.ssh_host.as_str(), cfg.ssh_port);
    let connect_timeout = Duration::from_millis(cfg.connect_timeout_ms);
    let mut session = tokio::time::timeout(connect_timeout, client::connect(client_cfg, addr, handler))
        .await
        .map_err(|_| ("SSH_CONNECT_TIMEOUT".to_string(), "connect timeout".to_string(), true))?
        .map_err(|e| ("SSH_CONNECT_FAILED".to_string(), format!("{e:?}"), true))?;

    let auth_ok = authenticate_session(&mut session, cfg, connect_timeout)
        .await
        .map_err(|(c, m)| (c, m, false))?;

    if !auth_ok.success() {
        return Err((
            "AUTH_FAILED".to_string(),
            "authentication failed".to_string(),
            false,
        ));
    }

    tokio::time::timeout(
        connect_timeout,
        session.tcpip_forward(cfg.remote_bind_addr.clone(), cfg.remote_port as u32),
    )
    .await
    .map_err(|_| ("TCPIP_FORWARD_TIMEOUT".to_string(), "tcpip_forward timeout".to_string(), false))?
    .map_err(|e| match e {
        russh::Error::RequestDenied => (
            "REMOTE_PORT_CONFLICT".to_string(),
            "tcpip_forward denied (port in use or server policy)".to_string(),
            false,
        ),
        _ => ("TCPIP_FORWARD_FAILED".to_string(), format!("{e:?}"), false),
    })?;

    set_state(status, TunnelState::Forwarding).await;

    let keepalive_interval = Duration::from_millis(cfg.keepalive_interval_ms);

    loop {
        tokio::select! {
            r = stop_rx.changed() => {
                let _ = r;
                if *stop_rx.borrow() {
                    let _ = session.cancel_tcpip_forward(cfg.remote_bind_addr.clone(), cfg.remote_port as u32).await;
                    let _ = session.disconnect(Disconnect::ByApplication, "stop", "en").await;
                    break;
                }
            }
            _ = sleep(keepalive_interval) => {
                if session.is_closed() {
                    return Err(("SSH_DISCONNECTED".to_string(), "session closed".to_string(), true));
                }
                let _ = session.send_keepalive(false).await;
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "tcp_tunnel"))]
async fn connect_and_forward(
    _cfg: &TcpTunnelConfig,
    status: &Arc<RwLock<TunnelRuntimeStatus>>,
    stop_rx: &mut watch::Receiver<bool>,
) -> Result<(), (String, String, bool)> {
    set_error(status, "NOT_SUPPORTED", "tcp_tunnel feature not enabled").await;
    while stop_rx.changed().await.is_ok() {
        if *stop_rx.borrow() {
            break;
        }
    }
    Ok(())
}

#[cfg(feature = "tcp_tunnel")]
struct TunnelClientHandler {
    cfg: TcpTunnelConfig,
    status: Arc<RwLock<TunnelRuntimeStatus>>,
}

#[cfg(feature = "tcp_tunnel")]
impl TunnelClientHandler {
    fn new(cfg: TcpTunnelConfig, status: Arc<RwLock<TunnelRuntimeStatus>>) -> Self {
        Self { cfg, status }
    }
}

#[cfg(feature = "tcp_tunnel")]
impl russh::client::Handler for TunnelClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        if !self.cfg.strict_host_key_checking {
            return Ok(true);
        }
        let expected = self.cfg.host_key_fingerprint.trim();
        if expected.is_empty() {
            set_error(
                &self.status,
                "HOSTKEY_MISSING",
                "host_key_fingerprint is required",
            )
            .await;
            return Ok(false);
        }
        let actual = compute_openssh_sha256_fingerprint(server_public_key)
            .unwrap_or_else(|_| "<unknown>".to_string());
        if expected == actual {
            Ok(true)
        } else {
            set_error(
                &self.status,
                "HOSTKEY_MISMATCH",
                &format!("expected {expected}, got {actual}"),
            )
            .await;
            Ok(false)
        }
    }

    fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut russh::client::Session,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        let local_addr = format!("{}:{}", self.cfg.local_addr, self.cfg.local_port);
        let status = self.status.clone();
        async move {
            // Important: do not block the SSH session handler with a long-lived copy loop.
            // If we await I/O here, the underlying session task may stop processing packets,
            // resulting in deadlocks (no data forwarded; disconnect/cancel not applied).
            tokio::spawn(async move {
                {
                    let mut s = status.write().await;
                    s.active_conns = s.active_conns.saturating_add(1);
                }

                let result = tokio::net::TcpStream::connect(&local_addr).await;
                match result {
                    Ok(mut stream) => {
                        let mut channel_stream = channel.into_stream();
                        let copy_res =
                            tokio::io::copy_bidirectional(&mut channel_stream, &mut stream).await;
                        let _ = tokio::io::AsyncWriteExt::shutdown(&mut channel_stream).await;
                        if let Ok((a, b)) = copy_res {
                            let mut s = status.write().await;
                            s.bytes_in = s.bytes_in.saturating_add(a);
                            s.bytes_out = s.bytes_out.saturating_add(b);
                        }
                    }
                    Err(e) => {
                        record_last_error(&status, "LOCAL_CONNECT_FAILED", &format!("{e}")).await;
                        let _ = channel.close().await;
                    }
                }

                {
                    let mut s = status.write().await;
                    s.active_conns = s.active_conns.saturating_sub(1);
                }
            });
            Ok(())
        }
    }
}

#[cfg(feature = "tcp_tunnel")]
fn compute_openssh_sha256_fingerprint(
    key: &russh::keys::ssh_key::PublicKey,
) -> Result<String, russh::Error> {
    use base64::engine::general_purpose::STANDARD_NO_PAD;
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let openssh = key.to_openssh()?;
    let b64 = openssh
        .split_whitespace()
        .nth(1)
        .ok_or(russh::Error::KexInit)?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|_| russh::Error::KexInit)?;
    let digest = Sha256::digest(decoded);
    Ok(format!("SHA256:{}", STANDARD_NO_PAD.encode(digest)))
}

#[cfg(feature = "tcp_tunnel")]
async fn test_once(cfg: &TcpTunnelConfig) -> Result<(), (String, String)> {
    use russh::client;
    use std::borrow::Cow;

    validate(cfg)?;

    let status = Arc::new(RwLock::new(TunnelRuntimeStatus::default()));
    let handler = TunnelClientHandler::new(cfg.clone(), status.clone());

    let client_cfg = client::Config {
        nodelay: true,
        inactivity_timeout: None,
        preferred: russh::Preferred {
            kex: Cow::Owned(vec![
                russh::kex::CURVE25519_PRE_RFC_8731,
                russh::kex::EXTENSION_SUPPORT_AS_CLIENT,
            ]),
            ..Default::default()
        },
        ..Default::default()
    };

    let client_cfg = Arc::new(client_cfg);
    let addr = (cfg.ssh_host.as_str(), cfg.ssh_port);
    let connect_timeout = Duration::from_millis(cfg.connect_timeout_ms);
    let mut session = tokio::time::timeout(connect_timeout, client::connect(client_cfg, addr, handler))
        .await
        .map_err(|_| ("SSH_CONNECT_TIMEOUT".to_string(), "connect timeout".to_string()))?
        .map_err(|e| ("SSH_CONNECT_FAILED".to_string(), format!("{e:?}")))?;

    let auth_ok = authenticate_session(&mut session, cfg, connect_timeout).await?;

    if !auth_ok.success() {
        return Err(("AUTH_FAILED".to_string(), "authentication failed".to_string()));
    }

    tokio::time::timeout(
        connect_timeout,
        session.tcpip_forward(cfg.remote_bind_addr.clone(), cfg.remote_port as u32),
    )
    .await
    .map_err(|_| ("TCPIP_FORWARD_TIMEOUT".to_string(), "tcpip_forward timeout".to_string()))?
    .map_err(|e| match e {
        russh::Error::RequestDenied => (
            "REMOTE_PORT_CONFLICT".to_string(),
            "tcpip_forward denied (port in use or server policy)".to_string(),
        ),
        _ => ("TCPIP_FORWARD_FAILED".to_string(), format!("{e:?}")),
    })?;

    let _ = session
        .cancel_tcpip_forward(cfg.remote_bind_addr.clone(), cfg.remote_port as u32)
        .await;
    let _ = session.disconnect(russh::Disconnect::ByApplication, "test done", "en").await;
    Ok(())
}

#[cfg(feature = "tcp_tunnel")]
async fn test_ssh_only_once(cfg: &TcpTunnelConfig) -> Result<(), (String, String)> {
    use crate::TcpTunnelAuth;
    use russh::client;
    use std::borrow::Cow;

    if cfg.ssh_host.trim().is_empty() {
        return Err(("SSH_HOST_MISSING".to_string(), "ssh_host is required".to_string()));
    }
    if cfg.username.trim().is_empty() {
        return Err(("USERNAME_MISSING".to_string(), "username is required".to_string()));
    }
    if cfg.strict_host_key_checking && cfg.host_key_fingerprint.trim().is_empty() {
        return Err((
            "HOSTKEY_MISSING".to_string(),
            "host_key_fingerprint is required".to_string(),
        ));
    }
    match &cfg.auth {
        TcpTunnelAuth::PrivateKeyPath { path, .. } if path.is_empty() => {
            return Err((
                "AUTH_MISSING".to_string(),
                "private key path is required".to_string(),
            ));
        }
        _ => {}
    }

    let status = Arc::new(RwLock::new(TunnelRuntimeStatus::default()));
    let handler = TunnelClientHandler::new(cfg.clone(), status.clone());

    let client_cfg = client::Config {
        nodelay: true,
        inactivity_timeout: None,
        preferred: russh::Preferred {
            kex: Cow::Owned(vec![
                russh::kex::CURVE25519_PRE_RFC_8731,
                russh::kex::EXTENSION_SUPPORT_AS_CLIENT,
            ]),
            ..Default::default()
        },
        ..Default::default()
    };

    let client_cfg = Arc::new(client_cfg);
    let addr = (cfg.ssh_host.as_str(), cfg.ssh_port);
    let connect_timeout = Duration::from_millis(cfg.connect_timeout_ms);
    let mut session = tokio::time::timeout(connect_timeout, client::connect(client_cfg, addr, handler))
        .await
        .map_err(|_| ("SSH_CONNECT_TIMEOUT".to_string(), "connect timeout".to_string()))?
        .map_err(|e| ("SSH_CONNECT_FAILED".to_string(), format!("{e:?}")))?;

    let auth_ok = authenticate_session(&mut session, cfg, connect_timeout).await?;

    if !auth_ok.success() {
        return Err(("AUTH_FAILED".to_string(), "authentication failed".to_string()));
    }

    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "test done", "en")
        .await;
    Ok(())
}
