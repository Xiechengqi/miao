use crate::sync::error::SyncError;
use crate::sync::manifest::BackupManifest;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub rel_path: PathBuf,
    pub abs_path: PathBuf,
    pub size: u64,
    pub mtime_ms: i64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

pub struct Scanner {
    exclude_patterns: Vec<String>,
    include_patterns: Vec<String>,
    follow_symlinks: bool,
}

impl Scanner {
    pub fn new(
        exclude_patterns: Vec<String>,
        include_patterns: Vec<String>,
        follow_symlinks: bool,
    ) -> Self {
        Self {
            exclude_patterns,
            include_patterns,
            follow_symlinks,
        }
    }

    pub fn scan(
        &self,
        root: &Path,
        manifest: Option<&BackupManifest>,
    ) -> Result<Vec<FileEntry>, SyncError> {
        let root = root
            .canonicalize()
            .map_err(|e| SyncError::ScanError(format!("canonicalize root: {e}")))?;

        let mut builder = WalkBuilder::new(&root);
        builder
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .follow_links(self.follow_symlinks)
            .threads(num_cpus::get());

        let walker = builder.build();
        let mut entries = Vec::new();

        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Walk error: {e}");
                    continue;
                }
            };

            let abs_path = entry.path().to_path_buf();

            // Skip root directory itself
            if abs_path == root {
                continue;
            }

            let rel_path = match abs_path.strip_prefix(&root) {
                Ok(p) => p.to_path_buf(),
                Err(_) => continue,
            };

            let rel_str = rel_path.to_string_lossy();

            // Apply exclude patterns
            if self.should_exclude(&rel_str) {
                continue;
            }

            // Apply include patterns (if any)
            if !self.include_patterns.is_empty() && !self.should_include(&rel_str) {
                continue;
            }

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mtime_ms = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            let file_entry = FileEntry {
                rel_path: rel_path.clone(),
                abs_path,
                size: metadata.len(),
                mtime_ms,
                is_dir: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
            };

            // Incremental mode: skip unchanged files
            if let Some(m) = manifest {
                if !file_entry.is_dir && !self.is_changed(&file_entry, m) {
                    continue;
                }
            }

            entries.push(file_entry);
        }

        // Sort by path for deterministic order
        entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        Ok(entries)
    }

    fn should_exclude(&self, path: &str) -> bool {
        for pattern in &self.exclude_patterns {
            if pattern.is_empty() {
                continue;
            }
            if Self::matches_pattern(path, pattern) {
                return true;
            }
        }
        false
    }

    fn should_include(&self, path: &str) -> bool {
        for pattern in &self.include_patterns {
            if pattern.is_empty() {
                continue;
            }
            if Self::matches_pattern(path, pattern) {
                return true;
            }
        }
        false
    }

    fn matches_pattern(path: &str, pattern: &str) -> bool {
        // Simple glob matching
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let (prefix, suffix) = (parts[0], parts[1]);
                return path.starts_with(prefix) && path.ends_with(suffix);
            }
            // Handle ** for recursive match
            if pattern == "**" {
                return true;
            }
        }

        // Exact match or prefix match for directories
        path == pattern || path.starts_with(&format!("{}/", pattern))
    }

    fn is_changed(&self, entry: &FileEntry, manifest: &BackupManifest) -> bool {
        let key = entry.rel_path.to_string_lossy().to_string();
        match manifest.entries.get(&key) {
            None => true, // New file
            Some(prev) => {
                entry.mtime_ms != prev.mtime_ms || entry.size != prev.size
            }
        }
    }
}
