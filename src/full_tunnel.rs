use crate::{save_config, AppState, TcpTunnelConfig, TcpTunnelManagedBy, TcpTunnelSetConfig};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, watch};
use tokio::time::{sleep, Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct FullTunnelSetRuntime {
    pub enabled: bool,
    pub last_scan_at: Option<Instant>,
    pub last_error: Option<String>,
}

struct SetHandle {
    stop_tx: watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct FullTunnelManager {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    handles: Mutex<HashMap<String, SetHandle>>,
    status: Mutex<HashMap<String, FullTunnelSetRuntime>>,
}

impl FullTunnelManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_status(&self, set_id: &str) -> FullTunnelSetRuntime {
        let m = self.inner.status.lock().await;
        m.get(set_id).cloned().unwrap_or_default()
    }

    pub async fn sync_from_config(&self, state: Arc<AppState>, sets: Vec<TcpTunnelSetConfig>) {
        let mut handles = self.inner.handles.lock().await;
        let mut desired: HashMap<String, TcpTunnelSetConfig> = HashMap::new();
        for s in sets {
            desired.insert(s.id.clone(), s);
        }

        let mut to_join: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        // Stop removed sets
        let existing_ids: Vec<String> = handles.keys().cloned().collect();
        for id in existing_ids {
            if !desired.contains_key(&id) {
                if let Some(h) = handles.remove(&id) {
                    let _ = h.stop_tx.send(true);
                    to_join.push(h.join);
                }
                let mut st = self.inner.status.lock().await;
                st.remove(&id);
            }
        }

        // Start/stop/update existing
        for (id, cfg) in desired {
            if !cfg.enabled {
                if let Some(h) = handles.remove(&id) {
                    let _ = h.stop_tx.send(true);
                    to_join.push(h.join);
                }
                let mut st = self.inner.status.lock().await;
                st.entry(id.clone()).or_default().enabled = false;
                continue;
            }

            if handles.contains_key(&id) {
                // already running; keep it
                let mut st = self.inner.status.lock().await;
                st.entry(id.clone()).or_default().enabled = true;
                continue;
            }

            let (stop_tx, stop_rx) = watch::channel(false);
            let state_clone = state.clone();
            let manager = self.clone();
            let join = tokio::spawn(async move {
                run_set_loop(manager, state_clone, cfg, stop_rx).await;
            });

            handles.insert(id.clone(), SetHandle { stop_tx, join });
            let mut st = self.inner.status.lock().await;
            st.entry(id.clone()).or_default().enabled = true;
        }

        drop(handles);
        for j in to_join {
            let _ = j.await;
        }
    }
}

async fn run_set_loop(
    manager: FullTunnelManager,
    state: Arc<AppState>,
    set_cfg: TcpTunnelSetConfig,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut missing_since: HashMap<u16, Instant> = HashMap::new();
    let scan_interval = Duration::from_millis(set_cfg.scan_interval_ms.max(500).min(60_000));
    let debounce = Duration::from_millis(set_cfg.debounce_ms.max(0).min(300_000));

    loop {
        if *stop_rx.borrow() {
            let mut st = manager.inner.status.lock().await;
            st.entry(set_cfg.id.clone()).or_default().enabled = false;
            break;
        }

        {
            let mut st = manager.inner.status.lock().await;
            let entry = st.entry(set_cfg.id.clone()).or_default();
            entry.enabled = true;
            entry.last_scan_at = Some(Instant::now());
            entry.last_error = None;
        }

        let ports_now = match scan_listen_ports().await {
            Ok(p) => p,
            Err(e) => {
                let mut st = manager.inner.status.lock().await;
                st.entry(set_cfg.id.clone()).or_default().last_error = Some(e);
                tokio::select! {
                    _ = sleep(scan_interval) => {},
                    _ = stop_rx.changed() => {},
                }
                continue;
            }
        };

        let mut ports_now: HashSet<u16> = ports_now
            .into_iter()
            .filter(|p| !set_cfg.exclude_ports.iter().any(|x| x == p))
            .collect();
        if set_cfg.include_ports_enabled {
            let include: HashSet<u16> = set_cfg.include_ports.iter().cloned().collect();
            ports_now.retain(|p| include.contains(p));
        }

        // Build managed map (port -> tunnel id)
        let (managed_map, all_tunnels) = {
            let cfg = state.config.lock().await;
            let mut map: HashMap<u16, String> = HashMap::new();
            for t in cfg.tcp_tunnels.iter() {
                if let Some(TcpTunnelManagedBy::FullTunnel { set_id, managed_port }) = &t.managed_by
                {
                    if set_id == &set_cfg.id {
                        map.insert(*managed_port, t.id.clone());
                    }
                }
            }
            (map, cfg.tcp_tunnels.clone())
        };

        // For "exists" check (set dimension)
        let managed_ports: HashSet<u16> = managed_map.keys().cloned().collect();

        // Mark missing and delete after debounce
        for p in managed_ports.iter() {
            if ports_now.contains(p) {
                missing_since.remove(p);
                continue;
            }
            let first = missing_since.entry(*p).or_insert_with(Instant::now);
            if debounce.is_zero() || first.elapsed() >= debounce {
                // Stop+delete the managed tunnel for this port
                let tunnel_id = managed_map.get(p).cloned();
                if let Some(tid) = tunnel_id {
                    let mut need_apply = false;
                    {
                        let mut cfg = state.config.lock().await;
                        let before = cfg.tcp_tunnels.len();
                        cfg.tcp_tunnels.retain(|t| t.id != tid);
                        if cfg.tcp_tunnels.len() != before {
                            if save_config(&cfg).await.is_ok() {
                                need_apply = true;
                            }
                        }
                    }
                    if need_apply {
                        let tunnels = { state.config.lock().await.tcp_tunnels.clone() };
                        state.tcp_tunnel.apply_config(&tunnels).await;
                    }
                }
                missing_since.remove(p);
            }
        }

        // Add new ports: only if no existing managed tunnel for this set+port
        let mut to_add: Vec<u16> = ports_now
            .iter()
            .filter(|p| !managed_map.contains_key(p))
            .cloned()
            .collect();

        if !to_add.is_empty() {
            to_add.sort_unstable();
            let batch_size = set_cfg.start_batch_size.max(1).min(128) as usize;
            let batch_interval_ms = set_cfg.start_batch_interval_ms.min(60_000);
            let batch_interval = Duration::from_millis(batch_interval_ms);

            let mut idx = 0usize;
            while idx < to_add.len() {
                let end = (idx + batch_size).min(to_add.len());
                let chunk = &to_add[idx..end];
                let mut changed = false;
                {
                    let mut cfg = state.config.lock().await;
                    for p in chunk {
                        // set-dimension existence check is enough; do not touch existing entries
                        let exists = cfg.tcp_tunnels.iter().any(|t| {
                            matches!(
                                &t.managed_by,
                                Some(TcpTunnelManagedBy::FullTunnel {
                                    set_id,
                                    managed_port
                                }) if set_id == &set_cfg.id && *managed_port == *p
                            )
                        });
                        if exists {
                            continue;
                        }

                        let id = crate::generate_tunnel_id();
                        cfg.tcp_tunnels.push(TcpTunnelConfig {
                            id,
                            name: None,
                            enabled: set_cfg.enabled,
                            local_addr: "127.0.0.1".to_string(),
                            local_port: *p,
                            remote_bind_addr: set_cfg.remote_bind_addr.clone(),
                            remote_port: *p,
                            ssh_host: set_cfg.ssh_host.clone(),
                            ssh_port: set_cfg.ssh_port,
                            username: set_cfg.username.clone(),
                            auth: set_cfg.auth.clone(),
                            strict_host_key_checking: set_cfg.strict_host_key_checking,
                            host_key_fingerprint: set_cfg.host_key_fingerprint.clone(),
                            allow_public_bind: set_cfg.remote_bind_addr == "0.0.0.0",
                            connect_timeout_ms: set_cfg.connect_timeout_ms,
                            keepalive_interval_ms: 10_000,
                            reconnect_backoff_ms: crate::default_tcp_tunnel_backoff(),
                            managed_by: Some(TcpTunnelManagedBy::FullTunnel {
                                set_id: set_cfg.id.clone(),
                                managed_port: *p,
                            }),
                        });
                        changed = true;
                    }
                    if changed {
                        let _ = save_config(&cfg).await;
                    }
                }
                if changed {
                    let tunnels = { state.config.lock().await.tcp_tunnels.clone() };
                    state.tcp_tunnel.apply_config(&tunnels).await;
                }

                idx = end;
                if idx < to_add.len() {
                    tokio::select! {
                        _ = sleep(batch_interval) => {},
                        _ = stop_rx.changed() => {},
                    }
                    if *stop_rx.borrow() {
                        break;
                    }
                }
            }
        }

        // Avoid unused warning for all_tunnels (kept for debugging future expansions)
        let _ = all_tunnels;

        tokio::select! {
            _ = sleep(scan_interval) => {},
            _ = stop_rx.changed() => {},
        }
    }
}

async fn scan_listen_ports() -> Result<HashSet<u16>, String> {
    if let Ok(p) = scan_from_ss().await {
        return Ok(p);
    }
    if let Ok(p) = scan_from_netstat().await {
        return Ok(p);
    }
    Err("Failed to scan ports: ss and netstat both failed".to_string())
}

async fn scan_from_ss() -> Result<HashSet<u16>, String> {
    let out = tokio::process::Command::new("ss")
        .args(["-plunt"])
        .output()
        .await
        .map_err(|e| format!("ss exec failed: {e}"))?;
    if !out.status.success() {
        return Err(format!("ss failed: {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_ss_output(&text)
}

fn parse_ss_output(text: &str) -> Result<HashSet<u16>, String> {
    let mut ports: HashSet<u16> = HashSet::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("Netid") {
            continue;
        }
        let cols: Vec<&str> = l.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        let proto = cols[0];
        let state = cols[1];
        if proto != "tcp" || state != "LISTEN" {
            continue;
        }
        // Local Address:Port is usually at index 4 for ss -plunt
        if let Some(p) = extract_port(cols[4]) {
            ports.insert(p);
        }
    }
    Ok(ports)
}

async fn scan_from_netstat() -> Result<HashSet<u16>, String> {
    let out = tokio::process::Command::new("netstat")
        .args(["-anltp"])
        .output()
        .await
        .map_err(|e| format!("netstat exec failed: {e}"))?;
    if !out.status.success() {
        return Err(format!("netstat failed: {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_netstat_output(&text)
}

fn parse_netstat_output(text: &str) -> Result<HashSet<u16>, String> {
    let mut ports: HashSet<u16> = HashSet::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("Proto") || l.starts_with("Active") {
            continue;
        }
        let cols: Vec<&str> = l.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let proto = cols[0];
        let local = cols[3];
        let state = cols[5];
        if proto != "tcp" || state != "LISTEN" {
            continue;
        }
        if let Some(p) = extract_port(local) {
            ports.insert(p);
        }
    }
    Ok(ports)
}

fn extract_port(s: &str) -> Option<u16> {
    let s = s.trim();
    let s = s.strip_prefix('[').unwrap_or(s);
    let s = s.strip_suffix(']').unwrap_or(s);
    let mut it = s.rsplitn(2, ':');
    let port_str = it.next()?;
    port_str.parse::<u16>().ok()
}
