pub mod archiver;
pub mod compressor;
pub mod error;
pub mod manifest;
pub mod pipeline;
pub mod scanner;
pub mod transport;

use crate::{SyncConfig, SyncErrorInfo, SyncRuntimeStatus, SyncState};
use chrono::Utc;
use chrono_tz::Tz;
use cron::Schedule;
use error::SyncError;
use pipeline::BackupPipeline;
use std::collections::HashMap;
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
    _join: Option<tokio::task::JoinHandle<()>>,
}

struct SyncScheduleHandle {
    stop_tx: watch::Sender<bool>,
    _join: tokio::task::JoinHandle<()>,
    cron: String,
    timezone: String,
}

impl SyncRuntime {
    fn new() -> Self {
        let (stop_tx, _) = watch::channel(false);
        Self {
            status: Arc::new(RwLock::new(SyncRuntimeStatus::default())),
            stop_tx,
            _join: None,
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

    pub async fn start(&self, cfg: SyncConfig) -> Result<(), String> {
        let status = {
            let mut runtimes = self.inner.runtimes.lock().await;
            let entry = runtimes.entry(cfg.id.clone()).or_insert_with(SyncRuntime::new);
            entry.status.clone()
        };

        {
            let s = status.read().await;
            if s.state == SyncState::Running {
                return Err("Sync is already running".to_string());
            }
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        {
            let mut runtimes = self.inner.runtimes.lock().await;
            if let Some(entry) = runtimes.get_mut(&cfg.id) {
                entry.stop_tx = stop_tx;
            }
        }

        let status_clone = status.clone();
        let cfg_id = cfg.id.clone();
        let join = tokio::spawn(async move {
            run_sync_task(cfg, status_clone, stop_rx).await;
        });

        {
            let mut runtimes = self.inner.runtimes.lock().await;
            if let Some(entry) = runtimes.get_mut(&cfg_id) {
                entry._join = Some(join);
            }
        }

        Ok(())
    }

    pub async fn stop(&self, id: &str) -> Result<(), String> {
        let stop_tx = {
            let runtimes = self.inner.runtimes.lock().await;
            let Some(runtime) = runtimes.get(id) else {
                return Err("Sync not found".to_string());
            };
            runtime.stop_tx.clone()
        };

        let _ = stop_tx.send(true);
        Ok(())
    }

    pub async fn get_status(&self, id: &str) -> SyncRuntimeStatus {
        let runtimes = self.inner.runtimes.lock().await;
        let Some(runtime) = runtimes.get(id) else {
            return SyncRuntimeStatus::default();
        };
        let result = runtime.status.read().await.clone();
        result
    }

    pub async fn test_sync(&self, cfg: &SyncConfig) -> Result<(), String> {
        use transport::SshTransport;

        let mut transport = SshTransport::connect(&cfg.ssh)
            .await
            .map_err(|e| e.to_string())?;

        // Check remote tools
        let result = transport.exec("command -v zstd && command -v tar")
            .await
            .map_err(|e| e.to_string())?;

        if result.exit_code != 0 {
            return Err("Remote missing zstd or tar".to_string());
        }

        transport.disconnect().await;
        Ok(())
    }

    async fn apply_schedules(&self, configs: &[SyncConfig]) {
        let desired: HashMap<String, (String, String)> = configs
            .iter()
            .filter_map(|cfg| {
                let schedule = cfg.schedule.as_ref()?;
                if !cfg.enabled || !schedule.enabled || schedule.cron.trim().is_empty() {
                    return None;
                }
                Some((cfg.id.clone(), (schedule.cron.clone(), schedule.timezone.clone())))
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
            let needs_restart = {
                let schedules = self.inner.schedules.lock().await;
                match schedules.get(&id) {
                    Some(existing) => existing.cron != cron || existing.timezone != timezone,
                    None => true,
                }
            };

            if !needs_restart {
                continue;
            }

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
            schedules.insert(id, SyncScheduleHandle {
                stop_tx,
                _join: join,
                cron,
                timezone,
            });
        }
    }
    async fn stop_schedule(&self, id: &str) {
        let handle = {
            let mut schedules = self.inner.schedules.lock().await;
            schedules.remove(id)
        };
        if let Some(handle) = handle {
            let _ = handle.stop_tx.send(true);
        }
    }
}

async fn run_sync_task(
    cfg: SyncConfig,
    status: Arc<RwLock<SyncRuntimeStatus>>,
    stop_rx: watch::Receiver<bool>,
) {
    {
        let mut s = status.write().await;
        s.state = SyncState::Running;
        s.last_run_at_ms = Some(Utc::now().timestamp_millis());
        s.last_error = None;
    }

    let local_paths = cfg.local_paths.clone();
    let mut had_error = false;

    for local in local_paths {
        if *stop_rx.borrow() {
            break;
        }

        let pipeline = BackupPipeline::new(cfg.clone());
        match pipeline.run(&local.path, status.clone(), stop_rx.clone()).await {
            Ok(()) => {}
            Err(SyncError::Cancelled) => break,
            Err(e) => {
                let mut s = status.write().await;
                s.last_error = Some(SyncErrorInfo {
                    message: e.to_string(),
                    at_ms: Utc::now().timestamp_millis(),
                });
                had_error = true;
                break;
            }
        }
    }

    let mut s = status.write().await;
    s.running_path = None;
    s.state = if had_error { SyncState::Error } else { SyncState::Stopped };
    if !had_error && !*stop_rx.borrow() {
        s.last_ok_at_ms = Some(Utc::now().timestamp_millis());
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

    let timezone = Tz::from_str(schedule_cfg.timezone.trim())
        .unwrap_or(chrono_tz::Asia::Shanghai);

    let expr = schedule_cfg.cron.trim();
    let cron_expr = if expr.split_whitespace().count() == 5 {
        format!("0 {}", expr)
    } else {
        expr.to_string()
    };

    let schedule = match Schedule::from_str(&cron_expr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid cron for {}: {}", cfg.id, e);
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
                let _ = manager.start(cfg.clone()).await;
            }
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    break;
                }
            }
        }
    }
}
