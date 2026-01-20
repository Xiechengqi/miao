use crate::{
    extract_sy, SyncConfig, SyncErrorInfo, SyncOptions, SyncRuntimeStatus, SyncSchedule, SyncState,
    TcpTunnelAuth,
};
use chrono::Utc;
use chrono_tz::Tz;
use cron::Schedule;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};

#[derive(Clone)]
pub struct SyncManager {
    inner: Arc<SyncManagerInner>,
}

struct SyncManagerInner {
    runtimes: Mutex<HashMap<String, SyncRuntime>>,
    schedules: Mutex<HashMap<String, SyncScheduleHandle>>,
}

struct SyncRuntime {
    status: Arc<RwLock<SyncRuntimeStatus>>,
    stop_tx: watch::Sender<bool>,
    join: Option<tokio::task::JoinHandle<()>>,
    current_pid: Arc<Mutex<Option<i32>>>,
}

struct SyncScheduleHandle {
    stop_tx: watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
    cron: String,
    timezone: String,
}

impl SyncRuntime {
    fn new() -> Self {
        let (stop_tx, _) = watch::channel(false);
        Self {
            status: Arc::new(RwLock::new(SyncRuntimeStatus::default())),
            stop_tx,
            join: None,
            current_pid: Arc::new(Mutex::new(None)),
        }
    }
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SyncManagerInner {
                runtimes: Mutex::new(HashMap::new()),
                schedules: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn apply_config(&self, configs: &[SyncConfig]) {
        let desired_ids: Vec<String> = configs.iter().map(|c| c.id.clone()).collect();
        {
            let mut runtimes = self.inner.runtimes.lock().await;
            for id in desired_ids.iter() {
                runtimes.entry(id.clone()).or_insert_with(SyncRuntime::new);
            }
        }

        let existing_ids: Vec<String> = {
            let runtimes = self.inner.runtimes.lock().await;
            runtimes.keys().cloned().collect()
        };

        for id in existing_ids {
            if !desired_ids.contains(&id) {
                let _ = self.stop(&id).await;
                let mut runtimes = self.inner.runtimes.lock().await;
                runtimes.remove(&id);
            }
        }

        for cfg in configs {
            if !cfg.enabled {
                let _ = self.stop(&cfg.id).await;
            }
        }

        self.apply_schedules(configs).await;
    }

    pub async fn start(&self, cfg: SyncConfig, dry_run: bool) -> Result<(), String> {
        let (status, current_pid) = {
            let mut runtimes = self.inner.runtimes.lock().await;
            let entry = runtimes
                .entry(cfg.id.clone())
                .or_insert_with(SyncRuntime::new);
            (entry.status.clone(), entry.current_pid.clone())
        };

        {
            let status = status.read().await;
            if status.state == SyncState::Running {
                return Err("Sync is already running".to_string());
            }
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        {
            let mut runtimes = self.inner.runtimes.lock().await;
            if let Some(entry) = runtimes.get_mut(&cfg.id) {
                entry.stop_tx = stop_tx.clone();
            }
        }

        let status = status.clone();
        let pid_slot = current_pid.clone();
        let join = tokio::spawn(async move {
            run_sync_task(cfg, status, stop_rx, pid_slot, dry_run).await;
        });

        {
            let mut runtimes = self.inner.runtimes.lock().await;
            if let Some(entry) = runtimes.get_mut(&cfg.id) {
                entry.join = Some(join);
            }
        }

        Ok(())
    }

    pub async fn stop(&self, id: &str) -> Result<(), String> {
        let (stop_tx, status, current_pid) = {
            let runtimes = self.inner.runtimes.lock().await;
            let Some(runtime) = runtimes.get(id) else {
                return Err("Sync not found".to_string());
            };
            (
                runtime.stop_tx.clone(),
                runtime.status.clone(),
                runtime.current_pid.clone(),
            )
        };

        let _ = stop_tx.send(true);
        if let Some(pid) = current_pid.lock().await.take() {
            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
        }

        {
            let mut status = status.write().await;
            status.state = SyncState::Stopped;
        }

        Ok(())
    }

    pub async fn get_status(&self, id: &str) -> SyncRuntimeStatus {
        let runtimes = self.inner.runtimes.lock().await;
        let Some(runtime) = runtimes.get(id) else {
            return SyncRuntimeStatus::default();
        };
        runtime.status.read().await.clone()
    }

    async fn apply_schedules(&self, configs: &[SyncConfig]) {
        let desired: HashMap<String, (String, String)> = configs
            .iter()
            .filter_map(|cfg| {
                let Some(schedule) = cfg.schedule.as_ref() else {
                    return None;
                };
                if !cfg.enabled || !schedule.enabled {
                    return None;
                }
                if schedule.cron.trim().is_empty() {
                    return None;
                }
                Some((
                    cfg.id.clone(),
                    (schedule.cron.clone(), schedule.timezone.clone()),
                ))
            })
            .collect();

        let existing_ids: Vec<String> = {
            let schedules = self.inner.schedules.lock().await;
            schedules.keys().cloned().collect()
        };

        for id in existing_ids {
            if !desired.contains_key(&id) {
                self.stop_schedule(&id).await;
            }
        }

        for (id, (cron, timezone)) in desired {
            let mut schedules = self.inner.schedules.lock().await;
            if let Some(existing) = schedules.get(&id) {
                if existing.cron == cron && existing.timezone == timezone {
                    continue;
                }
            }
            drop(schedules);
            self.stop_schedule(&id).await;

            let Some(cfg) = configs.iter().find(|c| c.id == id).cloned() else {
                continue;
            };
            let (stop_tx, stop_rx) = watch::channel(false);
            let manager = self.clone();
            let join = tokio::spawn(async move {
                run_schedule_loop(manager, cfg, stop_rx).await;
            });
            let mut schedules = self.inner.schedules.lock().await;
            schedules.insert(
                id.clone(),
                SyncScheduleHandle {
                    stop_tx,
                    join,
                    cron,
                    timezone,
                },
            );
        }
    }

    async fn stop_schedule(&self, id: &str) {
        let handle = {
            let mut schedules = self.inner.schedules.lock().await;
            schedules.remove(id)
        };
        if let Some(handle) = handle {
            let _ = handle.stop_tx.send(true);
            let _ = handle.join.await;
        }
    }
}

async fn run_schedule_loop(
    manager: SyncManager,
    cfg: SyncConfig,
    mut stop_rx: watch::Receiver<bool>,
) {
    let Some(schedule_cfg) = cfg.schedule.clone() else {
        return;
    };

    let timezone = parse_timezone(&schedule_cfg).unwrap_or(chrono_tz::Asia::Shanghai);
    let schedule = match Schedule::from_str(&schedule_cfg.cron) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid cron schedule for {}: {}", cfg.id, e);
            return;
        }
    };

    loop {
        if *stop_rx.borrow() {
            break;
        }

        let now = Utc::now().with_timezone(&timezone);
        let Some(next) = schedule.upcoming(timezone).next() else {
            break;
        };
        let wait = match (next - now).to_std() {
            Ok(d) => d,
            Err(_) => std::time::Duration::from_secs(0),
        };

        tokio::select! {
            _ = tokio::time::sleep(wait) => {
                if *stop_rx.borrow() {
                    break;
                }
                if let Err(e) = manager.start(cfg.clone(), false).await {
                    println!("Sync schedule skipped for {}: {}", cfg.id, e);
                }
            }
            changed = stop_rx.changed() => {
                if changed.is_err() {
                    break;
                }
            }
        }
    }
}

async fn run_sync_task(
    cfg: SyncConfig,
    status: Arc<RwLock<SyncRuntimeStatus>>,
    mut stop_rx: watch::Receiver<bool>,
    current_pid: Arc<Mutex<Option<i32>>>,
    dry_run: bool,
) {
    {
        let mut status_guard = status.write().await;
        status_guard.state = SyncState::Running;
        status_guard.last_run_at_ms = Some(Utc::now().timestamp_millis());
        status_guard.last_error = None;
    }

    let mut had_error = false;
    let local_paths = cfg.local_paths.clone();

    for local in local_paths {
        if *stop_rx.borrow() {
            break;
        }

        let (source, dest) = match build_paths(&cfg, &local.path) {
            Ok(v) => v,
            Err(e) => {
                record_sync_error(&status, e).await;
                had_error = true;
                break;
            }
        };

        let ssh_runtime = match prepare_ssh_runtime(&cfg, &local.path).await {
            Ok(p) => p,
            Err(e) => {
                record_sync_error(&status, e).await;
                had_error = true;
                break;
            }
        };

        let sy_path = match extract_sy() {
            Ok(p) => p,
            Err(e) => {
                record_sync_error(&status, format!("Failed to extract sy: {}", e)).await;
                had_error = true;
                let _ = cleanup_ssh_home(&ssh_runtime.home_dir).await;
                break;
            }
        };

        {
            let mut status_guard = status.write().await;
            status_guard.running_path = Some(local.path.clone());
        }

        let mut command = tokio::process::Command::new(&sy_path);
        let args = build_sy_args(&cfg.options, dry_run, &source, &dest);
        command.args(args);
        command.env("HOME", ssh_runtime.home_dir.display().to_string());
        if let Some(askpass) = ssh_runtime.askpass_path.as_ref() {
            command.env("SSH_ASKPASS", askpass.display().to_string());
            command.env("SSH_ASKPASS_REQUIRE", "force");
            command.env("DISPLAY", "miao-sync");
        }
        command.stdout(std::process::Stdio::inherit());
        command.stderr(std::process::Stdio::inherit());

        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                record_sync_error(&status, format!("Failed to spawn sy: {}", e)).await;
                had_error = true;
                let _ = cleanup_ssh_home(&ssh_runtime.home_dir).await;
                break;
            }
        };

        if let Some(pid) = child.id() {
            let mut pid_guard = current_pid.lock().await;
            *pid_guard = Some(pid as i32);
        }

        let result = tokio::select! {
            res = child.wait() => res,
            changed = stop_rx.changed() => {
                if changed.is_ok() && *stop_rx.borrow() {
                    if let Some(pid) = current_pid.lock().await.take() {
                        let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
                    }
                }
                child.wait().await
            }
        };

        {
            let mut pid_guard = current_pid.lock().await;
            *pid_guard = None;
        }

        let _ = cleanup_ssh_home(&ssh_runtime.home_dir).await;

        match result {
            Ok(exit) if exit.success() => {}
            Ok(exit) => {
                let code = exit.code().unwrap_or(-1);
                record_sync_error(&status, format!("sy exited with code {}", code)).await;
                had_error = true;
                break;
            }
            Err(e) => {
                record_sync_error(&status, format!("sy wait failed: {}", e)).await;
                had_error = true;
                break;
            }
        }
    }

    let mut status_guard = status.write().await;
    status_guard.running_path = None;
    status_guard.state = if had_error {
        SyncState::Error
    } else {
        SyncState::Stopped
    };
    if !had_error && !*stop_rx.borrow() {
        status_guard.last_ok_at_ms = Some(Utc::now().timestamp_millis());
    }
}

fn build_sy_args(options: &SyncOptions, dry_run: bool, source: &str, dest: &str) -> Vec<String> {
    let mut args = Vec::new();
    if dry_run {
        args.push("--dry-run".to_string());
    }
    if options.delete {
        args.push("--delete".to_string());
    }
    if options.verify {
        args.push("--verify".to_string());
    }
    if options.compress {
        args.push("--compress".to_string());
    }
    if let Some(bwlimit) = options.bwlimit.as_ref() {
        if !bwlimit.trim().is_empty() {
            args.push("--bwlimit".to_string());
            args.push(bwlimit.trim().to_string());
        }
    }
    for pattern in &options.exclude {
        if !pattern.trim().is_empty() {
            args.push("--exclude".to_string());
            args.push(pattern.trim().to_string());
        }
    }
    for pattern in &options.include {
        if !pattern.trim().is_empty() {
            args.push("--include".to_string());
            args.push(pattern.trim().to_string());
        }
    }
    if let Some(parallel) = options.parallel {
        if parallel > 0 {
            args.push("--parallel".to_string());
            args.push(parallel.to_string());
        }
    }
    if options.watch {
        args.push("--watch".to_string());
    }
    for extra in &options.extra_args {
        if !extra.trim().is_empty() {
            args.push(extra.trim().to_string());
        }
    }
    args.push(source.to_string());
    args.push(dest.to_string());
    args
}

fn build_paths(cfg: &SyncConfig, local_path: &str) -> Result<(String, String), String> {
    let remote_path = if cfg.local_paths.len() == 1 {
        cfg.remote_path
            .as_ref()
            .and_then(|p| {
                let trimmed = p.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .unwrap_or_else(|| local_path.to_string())
    } else {
        local_path.to_string()
    };

    let alias = format!("miao-sync-{}", cfg.id);
    let dest = format!("{}:{}", alias, remote_path);
    Ok((local_path.to_string(), dest))
}

struct SyncSshRuntime {
    home_dir: PathBuf,
    askpass_path: Option<PathBuf>,
}

async fn prepare_ssh_runtime(cfg: &SyncConfig, local_path: &str) -> Result<SyncSshRuntime, String> {
    let temp_dir = std::env::temp_dir();
    let dir = temp_dir.join(format!("miao-sync-{}-{}", cfg.id, uuid::Uuid::new_v4()));
    let ssh_dir = dir.join(".ssh");
    tokio::fs::create_dir_all(&ssh_dir)
        .await
        .map_err(|e| format!("Create ssh dir failed: {}", e))?;

    let alias = format!("miao-sync-{}", cfg.id);
    let mut config_lines = Vec::new();
    config_lines.push(format!("Host {}", alias));
    config_lines.push(format!("  HostName {}", cfg.ssh.host));
    config_lines.push(format!("  User {}", cfg.ssh.username));
    config_lines.push(format!("  Port {}", cfg.ssh.port));
    let password = match &cfg.ssh.auth {
        TcpTunnelAuth::Password { password } => password.trim().to_string(),
        TcpTunnelAuth::PrivateKeyPath { .. } => {
            return Err(format!(
                "sync only supports password auth for {}",
                local_path
            ));
        }
    };

    if !password.is_empty() {
        config_lines.push("  PreferredAuthentications password".to_string());
        config_lines.push("  PubkeyAuthentication no".to_string());
    }

    config_lines.push("  StrictHostKeyChecking no".to_string());
    config_lines.push("  UserKnownHostsFile /dev/null".to_string());

    let config_path = ssh_dir.join("config");
    tokio::fs::write(&config_path, config_lines.join("\n"))
        .await
        .map_err(|e| format!("Write ssh config failed: {}", e))?;

    let askpass_path = if password.is_empty() {
        None
    } else {
        let askpass = ssh_dir.join("askpass.sh");
        let script = format!(
            "#!/bin/sh\nprintf '%s' \"{}\"\n",
            escape_shell_arg(&password)
        );
        tokio::fs::write(&askpass, script)
            .await
            .map_err(|e| format!("Write askpass failed: {}", e))?;
        tokio::fs::set_permissions(&askpass, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|e| format!("Set askpass permissions failed: {}", e))?;
        Some(askpass)
    };

    Ok(SyncSshRuntime {
        home_dir: dir,
        askpass_path,
    })
}

async fn cleanup_ssh_home(dir: &PathBuf) -> Result<(), String> {
    if dir.exists() {
        tokio::fs::remove_dir_all(dir)
            .await
            .map_err(|e| format!("Remove temp ssh dir failed: {}", e))?;
    }
    Ok(())
}

fn escape_shell_arg(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
}

async fn record_sync_error(status: &Arc<RwLock<SyncRuntimeStatus>>, message: String) {
    let mut guard = status.write().await;
    guard.state = SyncState::Error;
    guard.last_error = Some(SyncErrorInfo {
        message,
        at_ms: Utc::now().timestamp_millis(),
    });
}

fn parse_timezone(schedule: &SyncSchedule) -> Option<Tz> {
    if schedule.timezone.trim().is_empty() {
        return None;
    }
    Tz::from_str(schedule.timezone.trim()).ok()
}
