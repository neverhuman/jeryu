use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::error::{CrateVaultError, Result};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CasObject {
    pub digest: Digest,
    pub bytes: u64,
}

#[derive(Clone, Debug)]
pub struct ContentAddressedStore {
    root: PathBuf,
}

impl ContentAddressedStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("objects"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn object_path(&self, digest: &Digest) -> PathBuf {
        let (a, b, rest) = digest.shard_path();
        self.root.join("objects").join(a).join(b).join(rest)
    }

    pub fn put_bytes(&self, bytes: &[u8]) -> Result<CasObject> {
        let digest = Digest::from_bytes(bytes);
        let path = self.object_path(&digest);
        if path.exists() {
            return Ok(CasObject {
                digest,
                bytes: bytes.len() as u64,
            });
        }
        let parent = path.parent().expect("CAS object path has parent");
        fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(".{}.tmp", digest));
        {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(CasObject {
            digest,
            bytes: bytes.len() as u64,
        })
    }

    pub fn put_file(&self, path: impl AsRef<Path>) -> Result<CasObject> {
        let bytes = fs::read(path)?;
        self.put_bytes(&bytes)
    }

    pub fn get_bytes(&self, digest: &Digest) -> Result<Vec<u8>> {
        let path = self.object_path(digest);
        if !path.exists() {
            return Err(CrateVaultError::CacheMiss(digest.to_string()));
        }
        let bytes = fs::read(path)?;
        let actual = Digest::from_bytes(&bytes);
        if &actual != digest {
            return Err(CrateVaultError::FalseHit(format!(
                "CAS object path {digest} contained {actual}"
            )));
        }
        Ok(bytes)
    }

    pub fn write_to(&self, digest: &Digest, output: impl AsRef<Path>) -> Result<()> {
        let bytes = self.get_bytes(digest)?;
        if let Some(parent) = output.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, bytes)?;
        Ok(())
    }

    pub fn contains_verified(&self, digest: &Digest) -> Result<bool> {
        match self.get_bytes(digest) {
            Ok(_) => Ok(true),
            Err(CrateVaultError::CacheMiss(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn cas_round_trips_and_verifies_digest() {
        let tmp = tempdir().unwrap();
        let cas = ContentAddressedStore::open(tmp.path()).unwrap();
        let obj = cas.put_bytes(b"hello").unwrap();
        assert_eq!(cas.get_bytes(&obj.digest).unwrap(), b"hello");
        assert!(cas.contains_verified(&obj.digest).unwrap());
    }
}
