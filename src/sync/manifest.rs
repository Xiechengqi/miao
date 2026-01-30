use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::sync::scanner::FileEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: u32,
    pub created_at_ms: i64,
    pub local_path: String,
    pub remote_path: String,
    pub entries: HashMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub size: u64,
    pub mtime_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

impl BackupManifest {
    pub const FILENAME: &'static str = ".miao-backup-manifest.json";
    pub const VERSION: u32 = 1;

    pub fn new(local_path: &str, remote_path: &str) -> Self {
        Self {
            version: Self::VERSION,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            local_path: local_path.to_string(),
            remote_path: remote_path.to_string(),
            entries: HashMap::new(),
        }
    }

    pub fn from_entries(
        local_path: &str,
        remote_path: &str,
        entries: &[FileEntry],
    ) -> Self {
        let mut manifest = Self::new(local_path, remote_path);
        for entry in entries {
            if entry.is_dir {
                continue;
            }
            let key = entry.rel_path.to_string_lossy().to_string();
            manifest.entries.insert(
                key,
                ManifestEntry {
                    size: entry.size,
                    mtime_ms: entry.mtime_ms,
                    checksum: None,
                },
            );
        }
        manifest
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
