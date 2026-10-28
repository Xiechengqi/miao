//! Pinned, verified sing-box distribution and configuration validation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
pub struct Release {
    pub version: String,
    pub sha256: HashMap<String, String>,
}

pub fn release() -> Release {
    serde_json::from_str(include_str!("../sing-box-release.json"))
        .expect("valid bundled release manifest")
}

pub fn verify_archive(bytes: &[u8], arch: &str) -> Result<(), String> {
    let manifest = release();
    let expected = manifest
        .sha256
        .get(arch)
        .ok_or("Unsupported sing-box architecture")?;
    if format!("{:x}", Sha256::digest(bytes)) != *expected {
        return Err("sing-box archive SHA-256 mismatch".into());
    }
    Ok(())
}

pub struct StagedBinary {
    pub dir: PathBuf,
    pub binary: PathBuf,
}
impl Drop for StagedBinary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub async fn stage_download(home: &Path) -> Result<StagedBinary, String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return Err("Unsupported sing-box architecture".into()),
    };
    let version = release().version;
    let url = format!(
        "https://github.com/SagerNet/sing-box/releases/download/v{0}/sing-box-{0}-linux-{1}.tar.gz",
        version, arch
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Failed to download sing-box")?
        .error_for_status()
        .map_err(|_| "sing-box download returned an HTTP error")?
        .bytes()
        .await
        .map_err(|_| "Failed to read sing-box download")?;
    verify_archive(&bytes, arch)?;
    std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
    let dir = home.join(format!(".sing-box-stage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).map_err(|e| format!("Create staging directory: {}", e))?;
    let staged = StagedBinary {
        binary: dir.join("sing-box"),
        dir,
    };
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..]));
    let expected = format!("sing-box-{}-linux-{}/sing-box", version, arch);
    let mut found = false;
    for entry in archive.entries().map_err(|_| "Invalid sing-box archive")? {
        let mut entry = entry.map_err(|_| "Invalid sing-box archive entry")?;
        if entry.path().map_err(|_| "Invalid archive path")?.as_ref() == Path::new(&expected)
            && entry.header().entry_type().is_file()
        {
            let mut file = std::fs::File::create(&staged.binary).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&staged.binary, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
            found = true;
            break;
        }
    }
    if !found {
        return Err("sing-box binary missing from archive".into());
    }
    let info = version_info(&staged.binary)
        .await
        .ok_or("Cannot read sing-box version")?;
    if info.version != version {
        return Err("Downloaded sing-box version does not match pinned release".into());
    }
    if ["with_quic", "with_utls", "with_clash_api"]
        .iter()
        .any(|tag| !info.tags.iter().any(|t| t == tag))
    {
        return Err("sing-box build requires with_quic, with_utls and with_clash_api".into());
    }
    Ok(staged)
}

#[derive(Debug, Serialize)]
pub struct VersionInfo {
    pub version: String,
    pub tags: Vec<String>,
}

pub async fn version_info(binary: &Path) -> Option<VersionInfo> {
    let binary = std::fs::canonicalize(binary).ok()?;
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(binary)
            .arg("version")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let info = String::from_utf8(output.stdout).ok()?;
    let version = info
        .lines()
        .find_map(|l| l.strip_prefix("sing-box version "))?
        .trim()
        .to_owned();
    let tags = info
        .lines()
        .find_map(|l| l.strip_prefix("Tags: "))
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    Some(VersionInfo { version, tags })
}

pub async fn check_config(binary: &Path, config: &Path) -> Result<(), String> {
    check_config_in(
        binary,
        config,
        config.parent().unwrap_or_else(|| Path::new(".")),
    )
    .await
}

pub async fn check_config_in(
    binary: &Path,
    config: &Path,
    working_dir: &Path,
) -> Result<(), String> {
    let binary = std::fs::canonicalize(binary).map_err(|e| format!("sing-box binary: {}", e))?;
    let config = std::fs::canonicalize(config).map_err(|e| format!("sing-box config: {}", e))?;
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(binary)
            .arg("check")
            .arg("-c")
            .arg(&config)
            .current_dir(working_dir)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "sing-box config check timed out")?
    .map_err(|e| format!("Run sing-box check: {}", e))?;
    if !output.status.success() {
        // The native error may include passwords or other source content.
        return Err("sing-box rejected the generated configuration; check protocol parameters and core compatibility".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_checksums_are_required() {
        assert!(verify_archive(b"tampered", "amd64").is_err());
        assert!(verify_archive(b"tampered", "arm64").is_err());
        assert!(verify_archive(b"tampered", "unknown").is_err());
        assert_eq!(release().sha256.len(), 2);
    }
}

/// Snapshot before stopping the service. Backups survive staging cleanup and failed rollback.
pub struct Replacement {
    home: PathBuf,
    had_binary: bool,
    had_config: bool,
}
impl Replacement {
    pub fn prepare(home: &Path) -> Result<Self, String> {
        let transaction = Self {
            home: home.to_owned(),
            had_binary: home.join("sing-box").exists(),
            had_config: home.join("config.json").exists(),
        };
        for (exists, name) in [
            (transaction.had_binary, "sing-box"),
            (transaction.had_config, "config.json"),
        ] {
            if exists {
                let backup = home.join(format!("{}.previous", name));
                std::fs::copy(home.join(name), &backup)
                    .map_err(|e| format!("Back up {}: {}", name, e))?;
                if name == "config.json" {
                    std::fs::set_permissions(backup, std::fs::Permissions::from_mode(0o600))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(transaction)
    }
    pub fn activate(&self, staged: &StagedBinary) -> Result<(), String> {
        std::fs::rename(&staged.binary, self.home.join("sing-box"))
            .map_err(|e| format!("Replace sing-box: {}", e))?;
        std::fs::rename(
            staged.dir.join("config.json"),
            self.home.join("config.json"),
        )
        .map_err(|e| format!("Replace config: {}", e))?;
        Ok(())
    }
    pub fn rollback(&self) -> Result<(), String> {
        // Attempt both restorations even if one fails.
        let results: Vec<_> = [
            (self.had_binary, "sing-box"),
            (self.had_config, "config.json"),
        ]
        .into_iter()
        .map(|(exists, name)| {
            let target = self.home.join(name);
            if exists {
                std::fs::copy(self.home.join(format!("{}.previous", name)), target).map(|_| ())
            } else {
                match std::fs::remove_file(target) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                }
            }
        })
        .collect();
        for result in results {
            result.map_err(|e| format!("Restore previous core/config: {}", e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod replacement_tests {
    use super::*;
    fn setup() -> (PathBuf, StagedBinary) {
        let home =
            std::env::temp_dir().join(format!("miao-replacement-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&home).unwrap();
        let dir = home.join("stage");
        std::fs::create_dir(&dir).unwrap();
        let staged = StagedBinary {
            binary: dir.join("sing-box"),
            dir,
        };
        std::fs::write(&staged.binary, "new-core").unwrap();
        (home, staged)
    }
    #[test]
    fn failed_second_rename_restores_both_files() {
        let (home, staged) = setup();
        std::fs::write(home.join("sing-box"), "old-core").unwrap();
        std::fs::write(home.join("config.json"), "old-config").unwrap();
        let transaction = Replacement::prepare(&home).unwrap();
        assert!(transaction.activate(&staged).is_err());
        assert_eq!(
            std::fs::read_to_string(home.join("sing-box")).unwrap(),
            "new-core"
        );
        transaction.rollback().unwrap();
        assert_eq!(
            std::fs::read_to_string(home.join("sing-box")).unwrap(),
            "old-core"
        );
        assert_eq!(
            std::fs::read_to_string(home.join("config.json")).unwrap(),
            "old-config"
        );
        assert!(home.join("sing-box.previous").exists());
        drop(staged);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn new_install_and_successful_activation_can_be_rolled_back() {
        let (home, staged) = setup();
        std::fs::write(staged.dir.join("config.json"), "new-config").unwrap();
        let transaction = Replacement::prepare(&home).unwrap();
        transaction.activate(&staged).unwrap();
        assert_eq!(
            std::fs::read_to_string(home.join("config.json")).unwrap(),
            "new-config"
        );
        transaction.rollback().unwrap();
        assert!(!home.join("sing-box").exists());
        assert!(!home.join("config.json").exists());
        drop(staged);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[cfg(test)]
mod check_tests {
    use super::*;
    #[tokio::test]
    async fn staged_check_uses_runtime_home_for_relative_resources() {
        let home = std::env::temp_dir().join(format!("miao-check-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&home).unwrap();
        let staged = home.join("stage");
        std::fs::create_dir(&staged).unwrap();
        let binary = staged.join("sing-box");
        std::fs::write(&binary, "#!/bin/sh\ntest -f relative-resource\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(home.join("relative-resource"), "resource").unwrap();
        let config = staged.join("config.json");
        std::fs::write(&config, "{}").unwrap();
        assert!(check_config(&binary, &config).await.is_err());
        assert!(check_config_in(&binary, &config, &home).await.is_ok());
        std::fs::remove_dir_all(home).unwrap();
    }
}
