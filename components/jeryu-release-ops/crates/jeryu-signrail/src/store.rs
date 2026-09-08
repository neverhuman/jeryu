//! Content-addressed artifact and receipt storage.

use crate::artifact::Artifact;
use crate::error::{Result, SignRailError};
use std::fs;
use std::path::{Path, PathBuf};

/// Local filesystem artifact store.
#[derive(Clone, Debug)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    /// Open or create an artifact store.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("artifacts"))?;
        fs::create_dir_all(root.join("receipts"))?;
        fs::create_dir_all(root.join("sboms"))?;
        Ok(Self { root })
    }

    /// Return the root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Store an artifact by digest and return the stored path.
    pub fn put_artifact(&self, artifact: &Artifact) -> Result<PathBuf> {
        if artifact.digest.len() < 2 {
            return Err(SignRailError::InvalidInput(
                "artifact digest is too short".to_string(),
            ));
        }
        let shard = &artifact.digest[..2];
        let dir = self
            .root
            .join("artifacts")
            .join(shard)
            .join(&artifact.digest);
        fs::create_dir_all(&dir)?;
        let target = dir.join(&artifact.name);
        fs::copy(&artifact.path, &target)?;
        Ok(target)
    }

    /// Store JSON content under a named namespace.
    pub fn put_json(&self, namespace: &str, name: &str, json: &str) -> Result<PathBuf> {
        let safe_name = name.replace('/', "_");
        let dir = self.root.join(namespace);
        fs::create_dir_all(&dir)?;
        let target = dir.join(format!("{safe_name}.json"));
        fs::write(&target, json)?;
        Ok(target)
    }
}
