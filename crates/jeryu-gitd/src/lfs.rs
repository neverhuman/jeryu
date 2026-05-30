//! Basic Git LFS object storage.

use crate::error::{GitdError, Result};
use crate::hash::sha256_hex;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Repository-local LFS store.
#[derive(Clone, Debug)]
pub struct LfsStore {
    root: PathBuf,
}

impl LfsStore {
    /// Create an LFS store at an explicit root.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Default store location for a bare repository.
    #[must_use]
    pub fn for_repo(repo_path: &Path) -> Self {
        Self::new(repo_path.join("lfs").join("objects"))
    }

    /// Path for an object id.
    pub fn object_path(&self, oid: &str) -> Result<PathBuf> {
        validate_oid(oid)?;
        Ok(self.root.join(&oid[0..2]).join(&oid[2..4]).join(oid))
    }

    /// Store object bytes, verifying sha256 oid.
    pub fn put_bytes(&self, oid: &str, bytes: &[u8]) -> Result<()> {
        validate_oid(oid)?;
        let actual = sha256_hex(bytes);
        if actual != oid {
            return Err(GitdError::Lfs(format!(
                "sha256 mismatch: expected {oid}, got {actual}"
            )));
        }
        let path = self.object_path(oid)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    /// Store object bytes from a reader.
    pub fn put_reader(&self, oid: &str, mut reader: impl Read) -> Result<u64> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        let len = bytes.len() as u64;
        self.put_bytes(oid, &bytes)?;
        Ok(len)
    }

    /// Read object bytes.
    pub fn get(&self, oid: &str) -> Result<Vec<u8>> {
        let path = self.object_path(oid)?;
        Ok(fs::read(path)?)
    }

    /// Whether an oid exists.
    pub fn exists(&self, oid: &str) -> bool {
        self.object_path(oid).map(|p| p.is_file()).unwrap_or(false)
    }

    /// Write object to a writer.
    pub fn write_to(&self, oid: &str, mut writer: impl Write) -> Result<()> {
        let bytes = self.get(oid)?;
        writer.write_all(&bytes)?;
        Ok(())
    }

    /// Generate a minimal LFS batch response from a simple JSON-ish request.
    ///
    /// This Phase 1 function extracts any 64-character hex strings and reports
    /// object presence plus upload/download endpoints that the HTTP edge can wire
    /// later without changing storage semantics.
    #[must_use]
    pub fn batch_response_from_jsonish(&self, request: &str) -> String {
        let mut objects = Vec::new();
        for token in request.split(|c: char| !c.is_ascii_hexdigit()) {
            if token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) {
                objects.push(token.to_ascii_lowercase());
            }
        }
        objects.sort();
        objects.dedup();
        let mut body = String::from("{\"transfer\":\"basic\",\"objects\":[");
        for (idx, oid) in objects.iter().enumerate() {
            if idx > 0 {
                body.push(',');
            }
            let exists = self.exists(oid);
            body.push_str(&format!(
                "{{\"oid\":\"{}\",\"size\":0,\"authenticated\":true,\"actions\":{{\"download\":{{\"href\":\"/lfs/objects/{}\"}},\"upload\":{{\"href\":\"/lfs/objects/{}\"}}}},\"exists\":{}}}",
                oid, oid, oid, exists
            ));
        }
        body.push_str("]}");
        body
    }
}

/// Validate a Git LFS sha256 oid.
pub fn validate_oid(oid: &str) -> Result<()> {
    if oid.len() != 64 || !oid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(GitdError::Lfs(
            "LFS oid must be 64 lowercase/uppercase hex characters".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn lfs_roundtrip() {
        let base = std::env::temp_dir().join(format!(
            "jeryu-lfs-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let store = LfsStore::new(&base);
        let data = b"lfs object";
        let oid = sha256_hex(data);
        store
            .put_bytes(&oid, data)
            .unwrap_or_else(|err| panic!("put failed: {err}"));
        assert_eq!(
            store
                .get(&oid)
                .unwrap_or_else(|err| panic!("get failed: {err}")),
            data
        );
        let _ = fs::remove_dir_all(base);
    }
}
