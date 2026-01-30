use std::fmt;

#[derive(Debug, Clone)]
pub enum SyncError {
    ScanError(String),
    ArchiveError(String),
    CompressError(String),
    SshConnectError(String),
    SshAuthError(String),
    SshExecError {
        command: String,
        exit_code: i32,
        stderr: String,
    },
    RemoteError(String),
    Cancelled,
    IoError(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::ScanError(msg) => write!(f, "Scan error: {}", msg),
            SyncError::ArchiveError(msg) => write!(f, "Archive error: {}", msg),
            SyncError::CompressError(msg) => write!(f, "Compress error: {}", msg),
            SyncError::SshConnectError(msg) => write!(f, "SSH connect error: {}", msg),
            SyncError::SshAuthError(msg) => write!(f, "SSH auth error: {}", msg),
            SyncError::SshExecError { command, exit_code, stderr } => {
                write!(f, "SSH exec error: command '{}' exited with code {}: {}",
                       command, exit_code, stderr)
            }
            SyncError::RemoteError(msg) => write!(f, "Remote error: {}", msg),
            SyncError::Cancelled => write!(f, "Operation cancelled"),
            SyncError::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        SyncError::IoError(err.to_string())
    }
}
