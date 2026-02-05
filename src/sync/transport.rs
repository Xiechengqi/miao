use crate::sync::error::SyncError;
use crate::{SyncSshConfig, TcpTunnelAuth};
use russh::client::{self, Handle};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::load_secret_key;
use russh::ChannelMsg;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::time::Duration;

const CONNECT_TIMEOUT_MS: u64 = 30000;
const BUFFER_SIZE: usize = 64 * 1024;

pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

struct TransportHandler;

impl client::Handler for TransportHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Skip host key verification for backup operations
        Ok(true)
    }
}

pub struct SshTransport {
    session: Handle<TransportHandler>,
}

impl SshTransport {
    pub async fn connect(cfg: &SyncSshConfig) -> Result<Self, SyncError> {
        let handler = TransportHandler;

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

        let addr = (cfg.host.as_str(), cfg.port);
        let connect_timeout = Duration::from_millis(CONNECT_TIMEOUT_MS);

        let mut session = tokio::time::timeout(
            connect_timeout,
            client::connect(client_cfg, addr, handler),
        )
        .await
        .map_err(|_| SyncError::SshConnectError("connect timeout".to_string()))?
        .map_err(|e| SyncError::SshConnectError(format!("{e:?}")))?;

        // Authenticate
        Self::authenticate(&mut session, cfg, connect_timeout).await?;

        Ok(Self { session })
    }

    async fn authenticate(
        session: &mut Handle<TransportHandler>,
        cfg: &SyncSshConfig,
        timeout: Duration,
    ) -> Result<(), SyncError> {
        match &cfg.auth {
            TcpTunnelAuth::Password { password } => {
                if !password.is_empty() {
                    let auth = tokio::time::timeout(
                        timeout,
                        session.authenticate_password(cfg.username.clone(), password.clone()),
                    )
                    .await
                    .map_err(|_| SyncError::SshAuthError("auth timeout".to_string()))?
                    .map_err(|e| SyncError::SshAuthError(format!("{e:?}")))?;

                    if !auth.success() {
                        return Err(SyncError::SshAuthError("password auth failed".to_string()));
                    }
                    return Ok(());
                }

                // Try default SSH keys
                Self::try_default_keys(session, cfg, timeout).await
            }
            TcpTunnelAuth::PrivateKeyPath { path, passphrase } => {
                let key = load_secret_key(path, passphrase.as_deref())
                    .map_err(|e| SyncError::SshAuthError(format!("load key: {e:?}")))?;

                let rsa_hash = tokio::time::timeout(timeout, session.best_supported_rsa_hash())
                    .await
                    .map_err(|_| SyncError::SshAuthError("auth timeout".to_string()))?
                    .map_err(|e| SyncError::SshAuthError(format!("{e:?}")))?
                    .flatten();

                let auth = tokio::time::timeout(
                    timeout,
                    session.authenticate_publickey(
                        cfg.username.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
                    ),
                )
                .await
                .map_err(|_| SyncError::SshAuthError("auth timeout".to_string()))?
                .map_err(|e| SyncError::SshAuthError(format!("{e:?}")))?;

                if !auth.success() {
                    return Err(SyncError::SshAuthError("key auth failed".to_string()));
                }
                Ok(())
            }
        }
    }

    async fn try_default_keys(
        session: &mut Handle<TransportHandler>,
        cfg: &SyncSshConfig,
        timeout: Duration,
    ) -> Result<(), SyncError> {
        let key_paths = default_ssh_key_paths();
        if key_paths.is_empty() {
            return Err(SyncError::SshAuthError(
                "no password and no default ssh keys".to_string(),
            ));
        }

        for path in key_paths {
            if !path.exists() {
                continue;
            }

            let key = match load_secret_key(&path, None) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let rsa_hash = match tokio::time::timeout(timeout, session.best_supported_rsa_hash())
                .await
            {
                Ok(Ok(v)) => v.flatten(),
                _ => continue,
            };

            let auth = tokio::time::timeout(
                timeout,
                session.authenticate_publickey(
                    cfg.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
                ),
            )
            .await;

            if let Ok(Ok(auth)) = auth {
                if auth.success() {
                    return Ok(());
                }
            }
        }

        Err(SyncError::SshAuthError("default key auth failed".to_string()))
    }

    /// Execute a command and return result
    pub async fn exec(&mut self, command: &str) -> Result<ExecResult, SyncError> {
        let mut channel = self
            .session
            .channel_open_session()
            .await
            .map_err(|e| SyncError::SshExecError {
                command: command.to_string(),
                exit_code: -1,
                stderr: format!("open channel: {e:?}"),
            })?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| SyncError::SshExecError {
                command: command.to_string(),
                exit_code: -1,
                stderr: format!("exec: {e:?}"),
            })?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = 0i32;

        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExtendedData { data, ext }) => {
                    if ext == 1 {
                        stderr.extend_from_slice(&data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = exit_status as i32;
                }
                Some(ChannelMsg::Eof) | None => break,
                _ => {}
            }
        }

        Ok(ExecResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Execute command with stdin data streaming
    pub async fn exec_with_stdin(
        &mut self,
        command: &str,
        mut stdin_data: impl tokio::io::AsyncRead + Unpin,
    ) -> Result<ExecResult, SyncError> {
        let mut channel = self
            .session
            .channel_open_session()
            .await
            .map_err(|e| SyncError::SshExecError {
                command: command.to_string(),
                exit_code: -1,
                stderr: format!("open channel: {e:?}"),
            })?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| SyncError::SshExecError {
                command: command.to_string(),
                exit_code: -1,
                stderr: format!("exec: {e:?}"),
            })?;

        let (mut read_half, write_half) = channel.split();
        let reader = tokio::spawn(async move {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_code = 0i32;

            loop {
                match read_half.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        stdout.extend_from_slice(&data);
                    }
                    Some(ChannelMsg::ExtendedData { data, ext }) => {
                        if ext == 1 {
                            stderr.extend_from_slice(&data);
                        }
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        exit_code = exit_status as i32;
                    }
                    Some(ChannelMsg::Eof) | None => break,
                    _ => {}
                }
            }

            (stdout, stderr, exit_code)
        });

        write_half
            .data(&mut stdin_data)
            .await
            .map_err(|e| SyncError::SshExecError {
                command: command.to_string(),
                exit_code: -1,
                stderr: format!("send data: {e:?}"),
            })?;

        write_half.eof().await.map_err(|e| SyncError::SshExecError {
            command: command.to_string(),
            exit_code: -1,
            stderr: format!("send eof: {e:?}"),
        })?;

        let (stdout, stderr, exit_code) = reader
            .await
            .map_err(|e| SyncError::SshExecError {
                command: command.to_string(),
                exit_code: -1,
                stderr: format!("join reader: {e:?}"),
            })?;

        Ok(ExecResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Download file content from remote
    pub async fn download_file(&mut self, path: &str) -> Result<Vec<u8>, SyncError> {
        let cmd = format!("cat {} 2>/dev/null || true", shell_escape(path));
        let result = self.exec(&cmd).await?;
        Ok(result.stdout)
    }

    /// Upload file content to remote
    pub async fn upload_file(&mut self, path: &str, content: &[u8]) -> Result<(), SyncError> {
        let cmd = format!("cat > {}", shell_escape(path));
        let cursor = std::io::Cursor::new(content.to_vec());
        let result = self.exec_with_stdin(&cmd, cursor).await?;

        if result.exit_code != 0 {
            return Err(SyncError::SshExecError {
                command: cmd,
                exit_code: result.exit_code,
                stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            });
        }
        Ok(())
    }

    /// Check if session is still alive
    #[allow(dead_code)]
    pub fn is_closed(&self) -> bool {
        self.session.is_closed()
    }

    /// Disconnect the session
    pub async fn disconnect(self) {
        let _ = self
            .session
            .disconnect(russh::Disconnect::ByApplication, "done", "en")
            .await;
    }
}

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

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
