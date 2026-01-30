use crate::sync::archiver::StreamingArchiver;
use crate::sync::compressor::StreamingCompressor;
use crate::sync::error::SyncError;
use crate::sync::manifest::BackupManifest;
use crate::sync::scanner::{FileEntry, Scanner};
use crate::sync::transport::SshTransport;
use crate::{SyncConfig, SyncOptions, SyncRuntimeStatus};
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

pub struct BackupPipeline {
    config: SyncConfig,
}

impl BackupPipeline {
    pub fn new(config: SyncConfig) -> Self {
        Self { config }
    }

    pub async fn run(
        &self,
        local_path: &str,
        status: Arc<RwLock<SyncRuntimeStatus>>,
        stop_rx: watch::Receiver<bool>,
    ) -> Result<(), SyncError> {
        let options = &self.config.options;
        let remote_path = self.config.remote_path.as_deref().unwrap_or("/");

        let mut transport = SshTransport::connect(&self.config.ssh).await?;
        self.ensure_remote_tools(&mut transport).await?;

        let manifest = if options.incremental {
            self.load_remote_manifest(&mut transport, remote_path).await.ok()
        } else {
            None
        };

        let scanner = Scanner::new(
            options.exclude.clone(),
            options.include.clone(),
            options.follow_symlinks,
        );

        let root = Path::new(local_path);
        let entries = scanner.scan(root, manifest.as_ref())?;

        if entries.is_empty() {
            transport.disconnect().await;
            return Ok(());
        }

        {
            let mut s = status.write().await;
            s.running_path = Some(local_path.to_string());
        }

        let compressed_data = self.create_compressed_archive(root, &entries, options, &stop_rx)?;

        if *stop_rx.borrow() {
            transport.disconnect().await;
            return Err(SyncError::Cancelled);
        }

        self.transfer_and_extract(&mut transport, remote_path, compressed_data, options).await?;

        let new_manifest = BackupManifest::from_entries(local_path, remote_path, &entries);
        self.save_remote_manifest(&mut transport, remote_path, &new_manifest).await?;

        if options.delete {
            self.delete_remote_orphans(&mut transport, remote_path, &new_manifest).await?;
        }

        transport.disconnect().await;
        Ok(())
    }

    async fn ensure_remote_tools(&self, transport: &mut SshTransport) -> Result<(), SyncError> {
        let result = transport.exec("command -v zstd").await?;
        if result.exit_code != 0 {
            let install_cmd = "command -v apt-get >/dev/null && apt-get update && apt-get install -y zstd || command -v yum >/dev/null && yum install -y zstd || command -v apk >/dev/null && apk add zstd || exit 1";
            let result = transport.exec(install_cmd).await?;
            if result.exit_code != 0 {
                return Err(SyncError::RemoteError("zstd not found and auto-install failed".to_string()));
            }
        }

        let result = transport.exec("command -v tar").await?;
        if result.exit_code != 0 {
            return Err(SyncError::RemoteError("tar not found on remote".to_string()));
        }
        Ok(())
    }

    fn create_compressed_archive(
        &self,
        root: &Path,
        entries: &[FileEntry],
        options: &SyncOptions,
        stop_rx: &watch::Receiver<bool>,
    ) -> Result<Vec<u8>, SyncError> {
        let mut tar_data = Vec::new();
        let archiver = StreamingArchiver::new(options.preserve_permissions);
        archiver.archive(root, entries, &mut tar_data, stop_rx)?;

        if *stop_rx.borrow() {
            return Err(SyncError::Cancelled);
        }

        let level = if options.compression_level == 0 { 3 } else { options.compression_level };
        let compressor = StreamingCompressor::new(level, options.compression_threads);
        let mut compressed_data = Vec::new();
        compressor.compress(Cursor::new(tar_data), &mut compressed_data)?;
        Ok(compressed_data)
    }

    async fn transfer_and_extract(
        &self,
        transport: &mut SshTransport,
        remote_path: &str,
        data: Vec<u8>,
        options: &SyncOptions,
    ) -> Result<(), SyncError> {
        let preserve_flag = if options.preserve_permissions { "p" } else { "" };
        let cmd = format!(
            "mkdir -p {} && cd {} && zstd -d | tar -x{}",
            shell_escape(remote_path),
            shell_escape(remote_path),
            preserve_flag
        );

        let cursor = Cursor::new(data);
        let result = transport.exec_with_stdin(&cmd, cursor).await?;

        if result.exit_code != 0 {
            return Err(SyncError::SshExecError {
                command: cmd,
                exit_code: result.exit_code,
                stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            });
        }
        Ok(())
    }

    async fn load_remote_manifest(
        &self,
        transport: &mut SshTransport,
        remote_path: &str,
    ) -> Result<BackupManifest, SyncError> {
        let manifest_path = format!("{}/{}", remote_path, BackupManifest::FILENAME);
        let data = transport.download_file(&manifest_path).await?;

        if data.is_empty() {
            return Err(SyncError::RemoteError("manifest not found".to_string()));
        }

        let json = String::from_utf8_lossy(&data);
        BackupManifest::from_json(&json)
            .map_err(|e| SyncError::RemoteError(format!("parse manifest: {e}")))
    }

    async fn save_remote_manifest(
        &self,
        transport: &mut SshTransport,
        remote_path: &str,
        manifest: &BackupManifest,
    ) -> Result<(), SyncError> {
        let json = manifest.to_json()
            .map_err(|e| SyncError::RemoteError(format!("serialize manifest: {e}")))?;
        let manifest_path = format!("{}/{}", remote_path, BackupManifest::FILENAME);
        transport.upload_file(&manifest_path, json.as_bytes()).await
    }

    async fn delete_remote_orphans(
        &self,
        transport: &mut SshTransport,
        remote_path: &str,
        manifest: &BackupManifest,
    ) -> Result<(), SyncError> {
        let cmd = format!("cd {} && find . -type f 2>/dev/null || true", shell_escape(remote_path));
        let result = transport.exec(&cmd).await?;

        let stdout = String::from_utf8_lossy(&result.stdout);
        let orphans: Vec<&str> = stdout
            .lines()
            .map(|l| l.trim_start_matches("./"))
            .filter(|l| !l.is_empty() && *l != BackupManifest::FILENAME)
            .filter(|f| !manifest.entries.contains_key(*f))
            .collect();

        if orphans.is_empty() {
            return Ok(());
        }

        for chunk in orphans.chunks(100) {
            let files: Vec<String> = chunk.iter().map(|f| shell_escape(f)).collect();
            let cmd = format!("cd {} && rm -f {}", shell_escape(remote_path), files.join(" "));
            let _ = transport.exec(&cmd).await;
        }
        Ok(())
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
