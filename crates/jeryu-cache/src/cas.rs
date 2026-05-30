//! Content-addressed storage backends.

use crate::digest::{Digest, digest_bytes};
use crate::error::{Result, VaultError};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Minimal CAS backend contract.
pub trait CasBackend: Send + Sync {
    /// Stores bytes and returns their content digest.
    fn put(&self, bytes: &[u8]) -> Result<Digest>;
    /// Reads bytes by digest. Missing object returns `Ok(None)`.
    fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>>;
    /// Returns whether an object exists.
    fn exists(&self, digest: &Digest) -> Result<bool>;
}

/// Filesystem CAS with testable availability switch.
#[derive(Debug, Clone)]
pub struct FsCas {
    root: PathBuf,
    available: Arc<AtomicBool>,
}

impl FsCas {
    /// Opens a filesystem CAS.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("objects"))?;
        Ok(Self {
            root,
            available: Arc::new(AtomicBool::new(true)),
        })
    }

    /// Sets availability for outage tests.
    pub fn set_available(&self, available: bool) {
        self.available.store(available, Ordering::SeqCst);
    }

    fn ensure_available(&self) -> Result<()> {
        if self.available.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(VaultError::StoreUnavailable)
        }
    }

    fn path_for(&self, digest: &Digest) -> PathBuf {
        let hex = &digest.as_str()[4..];
        self.root
            .join("objects")
            .join(&hex[0..2])
            .join(digest.as_str())
    }
}

impl CasBackend for FsCas {
    fn put(&self, bytes: &[u8]) -> Result<Digest> {
        self.ensure_available()?;
        let digest = digest_bytes(bytes);
        let path = self.path_for(&digest);
        if path.exists() {
            return Ok(digest);
        }
        let parent = path
            .parent()
            .ok_or_else(|| VaultError::Io("object path has no parent".to_string()))?;
        fs::create_dir_all(parent)?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, path)?;
        Ok(digest)
    }

    fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
        self.ensure_available()?;
        let path = self.path_for(digest);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(fs::read(path)?))
    }

    fn exists(&self, digest: &Digest) -> Result<bool> {
        self.ensure_available()?;
        Ok(self.path_for(digest).exists())
    }
}
