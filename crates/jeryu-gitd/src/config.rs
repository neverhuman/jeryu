//! Runtime configuration for jeryu_gitd.

use crate::error::{GitdError, Result};
use crate::protection::ProtectedRefRule;
use std::env;
use std::path::PathBuf;

/// Git server configuration.
#[derive(Clone, Debug)]
pub struct GitdConfig {
    /// Root directory containing owner/repo.git bare repositories.
    pub storage_root: PathBuf,
    /// HTTP listen address.
    pub http_addr: String,
    /// Git executable path.
    pub git_bin: String,
    /// Maximum accepted LFS object size for direct basic upload.
    pub lfs_max_object_bytes: u64,
    /// Protected-ref rules evaluated before ref updates.
    pub protected_refs: Vec<ProtectedRefRule>,
}

impl GitdConfig {
    /// Build a config with safe local defaults.
    #[must_use]
    pub fn new(storage_root: impl Into<PathBuf>) -> Self {
        Self {
            storage_root: storage_root.into(),
            http_addr: "127.0.0.1:8080".to_string(),
            git_bin: env::var("GITD_GIT_BIN").unwrap_or_else(|_| "git".to_string()),
            lfs_max_object_bytes: 2 * 1024 * 1024 * 1024,
            protected_refs: ProtectedRefRule::default_phase1_rules(),
        }
    }

    /// Load a minimal key-value config file.
    ///
    /// Supported lines are `storage_root=...`, `http_addr=...`, `git_bin=...`,
    /// and `lfs_max_object_bytes=...`. Empty lines and `#` comments are ignored.
    pub fn from_file(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let text = std::fs::read_to_string(&path)?;
        let mut config = Self::new("/var/lib/jeryu/repos");
        for (idx, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(GitdError::InvalidInput(format!(
                    "{}:{}: expected key=value",
                    path.display(),
                    idx + 1
                )));
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "storage_root" => config.storage_root = PathBuf::from(value),
                "http_addr" => config.http_addr = value.to_string(),
                "git_bin" => config.git_bin = value.to_string(),
                "lfs_max_object_bytes" => {
                    config.lfs_max_object_bytes = value.parse().map_err(|_| {
                        GitdError::InvalidInput(format!(
                            "invalid lfs_max_object_bytes at {}:{}",
                            path.display(),
                            idx + 1
                        ))
                    })?;
                }
                other => {
                    return Err(GitdError::InvalidInput(format!(
                        "{}:{}: unknown config key {other}",
                        path.display(),
                        idx + 1
                    )));
                }
            }
        }
        Ok(config)
    }
}
