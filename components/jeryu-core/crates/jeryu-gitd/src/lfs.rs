//! Git LFS object storage and Batch API helpers.

use crate::error::{GitdError, Result};
use crate::hash::sha256_hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// GitHub's raw-Git file warning/error boundary for ordinary blobs.
pub const RAW_GIT_BLOB_LIMIT_BYTES: u64 = 100 * 1024 * 1024;

/// Default local Enterprise-style LFS object ceiling.
pub const DEFAULT_LFS_MAX_OBJECT_BYTES: u64 = 5 * 1024 * 1024 * 1024;

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
        let oid = normalize_oid(oid)?;
        Ok(self.root.join(&oid[0..2]).join(&oid[2..4]).join(&oid))
    }

    /// Store object bytes, verifying sha256 oid.
    pub fn put_bytes(&self, oid: &str, bytes: &[u8]) -> Result<()> {
        let oid = normalize_oid(oid)?;
        let actual = sha256_hex(bytes);
        if actual != oid {
            return Err(GitdError::Lfs(format!(
                "sha256 mismatch: expected {oid}, got {actual}"
            )));
        }
        let path = self.object_path(&oid)?;
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
        self.put_reader_with_limit(oid, None, u64::MAX, &mut reader)
    }

    /// Store object bytes from a reader without buffering the full payload.
    ///
    /// The object is first written to a sibling temporary file. Only after the
    /// streamed SHA-256 digest and optional size match the requested object id
    /// is the file atomically renamed into its content-addressed final path.
    pub fn put_reader_with_limit(
        &self,
        oid: &str,
        expected_size: Option<u64>,
        max_size: u64,
        mut reader: impl Read,
    ) -> Result<u64> {
        let oid = normalize_oid(oid)?;
        if expected_size.is_some_and(|size| size > max_size) {
            return Err(GitdError::Lfs(format!(
                "LFS object {oid} is larger than the configured limit of {max_size} bytes"
            )));
        }
        let path = self.object_path(&oid)?;
        let parent = path
            .parent()
            .ok_or_else(|| GitdError::Lfs("LFS object path has no parent".to_string()))?;
        fs::create_dir_all(parent)?;
        let tmp = temp_path_for(&path);
        let result = (|| {
            let mut file = fs::File::create(&tmp)?;
            let mut hasher = Sha256::new();
            let mut total = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = reader.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                total = total
                    .checked_add(read as u64)
                    .ok_or_else(|| GitdError::Lfs("LFS object size overflowed u64".to_string()))?;
                if total > max_size {
                    return Err(GitdError::Lfs(format!(
                        "LFS object {oid} is larger than the configured limit of {max_size} bytes"
                    )));
                }
                hasher.update(&buffer[..read]);
                file.write_all(&buffer[..read])?;
            }
            file.sync_all()?;
            drop(file);
            if let Some(expected) = expected_size
                && total != expected
            {
                return Err(GitdError::Lfs(format!(
                    "size mismatch for {oid}: expected {expected}, got {total}"
                )));
            }
            let actual = finalize_sha256_hex(hasher);
            if actual != oid {
                return Err(GitdError::Lfs(format!(
                    "sha256 mismatch: expected {oid}, got {actual}"
                )));
            }
            fs::rename(&tmp, &path)?;
            Ok(total)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
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

    /// Return the stored object size when the oid exists.
    pub fn size(&self, oid: &str) -> Result<Option<u64>> {
        let path = self.object_path(oid)?;
        match fs::metadata(path) {
            Ok(meta) => Ok(Some(meta.len())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(GitdError::Io(err)),
        }
    }

    /// Open a stored object and bind the exact byte length used for an HTTP
    /// `Content-Length` response without reading the object into memory.
    pub(crate) fn open_reader(&self, oid: &str) -> Result<Option<(fs::File, u64)>> {
        let path = self.object_path(oid)?;
        let link_metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(GitdError::Io(err)),
        };
        if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
            return Err(GitdError::Lfs(format!(
                "LFS object path is not a regular file: {}",
                path.display()
            )));
        }
        let file = fs::File::open(path)?;
        let opened_metadata = file.metadata()?;
        if !opened_metadata.is_file() {
            return Err(GitdError::Lfs(
                "opened LFS object is not a regular file".to_string(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if link_metadata.dev() != opened_metadata.dev()
                || link_metadata.ino() != opened_metadata.ino()
            {
                return Err(GitdError::Lfs(
                    "LFS object path changed while opening".to_string(),
                ));
            }
        }
        let size = opened_metadata.len();
        Ok(Some((file, size)))
    }

    /// Confirm that an object exists and has the expected size.
    pub fn verify(&self, oid: &str, expected_size: u64) -> Result<()> {
        let oid = normalize_oid(oid)?;
        let Some(actual_size) = self.size(&oid)? else {
            return Err(GitdError::Lfs(format!("LFS object not found: {oid}")));
        };
        if actual_size != expected_size {
            return Err(GitdError::Lfs(format!(
                "size mismatch for {oid}: expected {expected_size}, got {actual_size}"
            )));
        }
        Ok(())
    }

    /// Write object to a writer.
    pub fn write_to(&self, oid: &str, mut writer: impl Write) -> Result<()> {
        let Some((mut file, size)) = self.open_reader(oid)? else {
            return Err(GitdError::Lfs(format!("LFS object not found: {oid}")));
        };
        let mut limited = Read::take(&mut file, size);
        let copied = io::copy(&mut limited, &mut writer)?;
        if copied != size {
            return Err(GitdError::Lfs(format!(
                "LFS object changed while streaming: expected {size} bytes, copied {copied}"
            )));
        }
        Ok(())
    }

    /// Generate a Git LFS Batch API response.
    pub fn batch_response(
        &self,
        request: &str,
        objects_url: &str,
        auth_header: Option<&str>,
        max_object_bytes: u64,
    ) -> Result<Vec<u8>> {
        let request: LfsBatchRequest = serde_json::from_str(request)
            .map_err(|err| GitdError::Lfs(format!("invalid LFS batch JSON: {err}")))?;
        let response =
            self.batch_response_for(request, objects_url, auth_header, max_object_bytes)?;
        serde_json::to_vec(&response)
            .map_err(|err| GitdError::Lfs(format!("serialize LFS batch response: {err}")))
    }

    fn batch_response_for(
        &self,
        request: LfsBatchRequest,
        objects_url: &str,
        auth_header: Option<&str>,
        max_object_bytes: u64,
    ) -> Result<LfsBatchResponse> {
        if request.hash_algo.as_deref().unwrap_or("sha256") != "sha256" {
            return Err(GitdError::Lfs(
                "unsupported LFS hash algorithm; only sha256 is supported".to_string(),
            ));
        }
        if let Some(transfers) = &request.transfers
            && !transfers.is_empty()
            && !transfers.iter().any(|transfer| transfer == "basic")
        {
            return Err(GitdError::Lfs(
                "unsupported LFS transfer adapter; only basic is supported".to_string(),
            ));
        }
        let operation = match request.operation.as_str() {
            "upload" => LfsOperation::Upload,
            "download" => LfsOperation::Download,
            other => {
                return Err(GitdError::Lfs(format!(
                    "unsupported LFS batch operation: {other}"
                )));
            }
        };
        let action_header = auth_header.map(auth_action_header);
        let mut objects = Vec::with_capacity(request.objects.len());
        for object in request.objects {
            let oid = normalize_oid(&object.oid)?;
            let response = if object.size > max_object_bytes {
                LfsBatchObjectResponse::with_error(
                    oid,
                    object.size,
                    413,
                    "object exceeds the configured LFS size limit",
                )
            } else {
                match operation {
                    LfsOperation::Upload => self.upload_batch_object(
                        &oid,
                        object.size,
                        objects_url,
                        action_header.clone(),
                    )?,
                    LfsOperation::Download => self.download_batch_object(
                        &oid,
                        object.size,
                        objects_url,
                        action_header.clone(),
                    )?,
                }
            };
            objects.push(response);
        }
        Ok(LfsBatchResponse {
            transfer: "basic",
            hash_algo: "sha256",
            objects,
        })
    }

    fn upload_batch_object(
        &self,
        oid: &str,
        size: u64,
        objects_url: &str,
        header: Option<BTreeMap<String, String>>,
    ) -> Result<LfsBatchObjectResponse> {
        if let Some(actual_size) = self.size(oid)? {
            if actual_size != size {
                return Ok(LfsBatchObjectResponse::with_error(
                    oid.to_string(),
                    size,
                    422,
                    "stored object size does not match requested size",
                ));
            }
            return Ok(LfsBatchObjectResponse::new(oid.to_string(), size));
        }
        let mut response = LfsBatchObjectResponse::new(oid.to_string(), size);
        let object_url = object_href(objects_url, oid);
        response.actions.insert(
            "upload".to_string(),
            LfsAction::new(object_url.clone(), header.clone()),
        );
        response.actions.insert(
            "verify".to_string(),
            LfsAction::new(format!("{object_url}/verify"), header),
        );
        Ok(response)
    }

    fn download_batch_object(
        &self,
        oid: &str,
        size: u64,
        objects_url: &str,
        header: Option<BTreeMap<String, String>>,
    ) -> Result<LfsBatchObjectResponse> {
        let Some(actual_size) = self.size(oid)? else {
            return Ok(LfsBatchObjectResponse::with_error(
                oid.to_string(),
                size,
                404,
                "object not found",
            ));
        };
        if actual_size != size {
            return Ok(LfsBatchObjectResponse::with_error(
                oid.to_string(),
                size,
                422,
                "stored object size does not match requested size",
            ));
        }
        let mut response = LfsBatchObjectResponse::new(oid.to_string(), size);
        response.actions.insert(
            "download".to_string(),
            LfsAction::new(object_href(objects_url, oid), header),
        );
        Ok(response)
    }
}

/// Validate a Git LFS sha256 oid.
pub fn validate_oid(oid: &str) -> Result<()> {
    normalize_oid(oid).map(|_| ())
}

/// Validate and canonicalize a Git LFS sha256 oid.
pub fn normalize_oid(oid: &str) -> Result<String> {
    if oid.len() != 64 || !oid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(GitdError::Lfs(
            "LFS oid must be 64 lowercase/uppercase hex characters".to_string(),
        ));
    }
    Ok(oid.to_ascii_lowercase())
}

/// Git LFS verification request body.
#[derive(Clone, Debug, Deserialize)]
pub struct LfsVerifyRequest {
    /// Object id.
    pub oid: String,
    /// Expected object size.
    pub size: u64,
}

#[derive(Clone, Copy, Debug)]
enum LfsOperation {
    Upload,
    Download,
}

#[derive(Debug, Deserialize)]
struct LfsBatchRequest {
    operation: String,
    #[serde(default)]
    transfers: Option<Vec<String>>,
    #[serde(default)]
    hash_algo: Option<String>,
    #[serde(default)]
    objects: Vec<LfsBatchObject>,
}

#[derive(Debug, Deserialize)]
struct LfsBatchObject {
    oid: String,
    size: u64,
}

#[derive(Debug, Serialize)]
struct LfsBatchResponse {
    transfer: &'static str,
    hash_algo: &'static str,
    objects: Vec<LfsBatchObjectResponse>,
}

#[derive(Debug, Serialize)]
struct LfsBatchObjectResponse {
    oid: String,
    size: u64,
    authenticated: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    actions: BTreeMap<String, LfsAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<LfsObjectError>,
}

impl LfsBatchObjectResponse {
    fn new(oid: String, size: u64) -> Self {
        Self {
            oid,
            size,
            authenticated: true,
            actions: BTreeMap::new(),
            error: None,
        }
    }

    fn with_error(oid: String, size: u64, code: u16, message: &str) -> Self {
        Self {
            oid,
            size,
            authenticated: true,
            actions: BTreeMap::new(),
            error: Some(LfsObjectError {
                code,
                message: message.to_string(),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct LfsAction {
    href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    header: Option<BTreeMap<String, String>>,
}

impl LfsAction {
    fn new(href: String, header: Option<BTreeMap<String, String>>) -> Self {
        Self { href, header }
    }
}

#[derive(Debug, Serialize)]
struct LfsObjectError {
    code: u16,
    message: String,
}

fn auth_action_header(value: &str) -> BTreeMap<String, String> {
    let mut header = BTreeMap::new();
    header.insert("Authorization".to_string(), value.to_string());
    header
}

fn object_href(objects_url: &str, oid: &str) -> String {
    format!("{}/{}", objects_url.trim_end_matches('/'), oid)
}

fn temp_path_for(path: &Path) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("object");
    path.with_file_name(format!(".{file_name}.tmp.{}.{}", std::process::id(), now))
}

fn finalize_sha256_hex(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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

    #[test]
    fn lfs_write_to_streams_through_fragmented_writer() {
        let base = std::env::temp_dir().join(format!(
            "jeryu-lfs-stream-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let store = LfsStore::new(&base);
        let data = vec![b'x'; 2 * 1024 * 1024];
        let oid = sha256_hex(&data);
        store
            .put_bytes(&oid, &data)
            .unwrap_or_else(|err| panic!("put failed: {err}"));
        let mut writer = FragmentedWriter {
            bytes: 0,
            max_chunk: 127,
        };

        store
            .write_to(&oid, &mut writer)
            .unwrap_or_else(|err| panic!("stream failed: {err}"));

        assert_eq!(writer.bytes, data.len() as u64);
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn lfs_open_reader_rejects_symlink_object() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!(
            "jeryu-lfs-symlink-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let store = LfsStore::new(&base);
        let data = b"not an object link target";
        let oid = sha256_hex(data);
        let object_path = store.object_path(&oid).expect("object path");
        fs::create_dir_all(object_path.parent().expect("object parent"))
            .expect("create object parent");
        let target = base.join("target");
        fs::write(&target, data).expect("write link target");
        symlink(&target, &object_path).expect("create object symlink");

        let err = store
            .open_reader(&oid)
            .expect_err("symlink object must be rejected");

        assert!(err.to_string().contains("not a regular file"));
        let _ = fs::remove_dir_all(base);
    }

    struct FragmentedWriter {
        bytes: u64,
        max_chunk: usize,
    }

    impl Write for FragmentedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let count = buffer.len().min(self.max_chunk);
            self.bytes += count as u64;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
