use crate::sync::error::SyncError;
use crate::sync::scanner::FileEntry;
use std::io::Write;
use std::path::Path;
use tar::{Builder, Header};
use tokio::sync::watch;

pub struct StreamingArchiver {
    preserve_permissions: bool,
}

impl StreamingArchiver {
    pub fn new(preserve_permissions: bool) -> Self {
        Self { preserve_permissions }
    }

    /// Create tar archive from file entries, writing to the provided writer
    pub fn archive<W: Write>(
        &self,
        root: &Path,
        entries: &[FileEntry],
        writer: W,
        stop_rx: &watch::Receiver<bool>,
    ) -> Result<(), SyncError> {
        let mut builder = Builder::new(writer);

        for entry in entries {
            if *stop_rx.borrow() {
                return Err(SyncError::Cancelled);
            }

            if entry.is_dir {
                self.append_dir(&mut builder, entry)?;
            } else if entry.is_symlink {
                self.append_symlink(&mut builder, entry)?;
            } else {
                self.append_file(&mut builder, root, entry)?;
            }
        }

        builder
            .finish()
            .map_err(|e| SyncError::ArchiveError(format!("finish tar: {e}")))?;

        Ok(())
    }

    fn append_dir<W: Write>(
        &self,
        builder: &mut Builder<W>,
        entry: &FileEntry,
    ) -> Result<(), SyncError> {
        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mtime((entry.mtime_ms / 1000) as u64);

        if self.preserve_permissions {
            header.set_mode(0o755);
        } else {
            header.set_mode(0o755);
        }

        let path = entry.rel_path.to_string_lossy();
        let path_with_slash = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{}/", path)
        };

        header.set_cksum();

        builder
            .append_data(&mut header, &path_with_slash, std::io::empty())
            .map_err(|e| SyncError::ArchiveError(format!("append dir {}: {e}", path)))?;

        Ok(())
    }

    fn append_symlink<W: Write>(
        &self,
        builder: &mut Builder<W>,
        entry: &FileEntry,
    ) -> Result<(), SyncError> {
        let target = std::fs::read_link(&entry.abs_path)
            .map_err(|e| SyncError::ArchiveError(format!("read symlink: {e}")))?;

        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mtime((entry.mtime_ms / 1000) as u64);
        header.set_mode(0o777);
        header.set_cksum();

        let path = entry.rel_path.to_string_lossy();

        builder
            .append_link(&mut header, &*path, &target)
            .map_err(|e| SyncError::ArchiveError(format!("append symlink {}: {e}", path)))?;

        Ok(())
    }

    fn append_file<W: Write>(
        &self,
        builder: &mut Builder<W>,
        _root: &Path,
        entry: &FileEntry,
    ) -> Result<(), SyncError> {
        let file = std::fs::File::open(&entry.abs_path)
            .map_err(|e| SyncError::ArchiveError(format!("open file: {e}")))?;

        let metadata = file
            .metadata()
            .map_err(|e| SyncError::ArchiveError(format!("file metadata: {e}")))?;

        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(metadata.len());
        header.set_mtime((entry.mtime_ms / 1000) as u64);

        if self.preserve_permissions {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                header.set_mode(metadata.permissions().mode());
            }
            #[cfg(not(unix))]
            {
                header.set_mode(0o644);
            }
        } else {
            header.set_mode(0o644);
        }

        header.set_cksum();

        let path = entry.rel_path.to_string_lossy();

        builder
            .append_data(&mut header, &*path, file)
            .map_err(|e| SyncError::ArchiveError(format!("append file {}: {e}", path)))?;

        Ok(())
    }
}
