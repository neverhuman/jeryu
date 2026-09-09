//! Shared CLI parsing, path, and usage helpers.

use crate::artifact::Artifact;
use crate::error::{Result, SignRailError};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn artifacts_from_paths<'a>(
    paths: impl Iterator<Item = &'a String>,
) -> Result<Vec<Artifact>> {
    let mut artifacts = Vec::new();
    for path in paths {
        let path_buf = PathBuf::from(path);
        let name = path_buf
            .file_name()
            .and_then(|part| part.to_str())
            .ok_or_else(|| SignRailError::InvalidInput(format!("invalid artifact path: {path}")))?
            .to_string();
        artifacts.push(Artifact::from_file(
            name,
            path_buf,
            "application/octet-stream",
        )?);
    }
    Ok(artifacts)
}

pub(super) fn required(value: Option<String>, flag: &str) -> Result<String> {
    value.filter(|item| !item.trim().is_empty()).ok_or_else(|| {
        SignRailError::InvalidInput(format!("missing required {flag}\n{}", sign_release_usage()))
    })
}

pub(super) fn default_store_root<F>(env: &F) -> Result<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(path) = env("SIGNRAIL_STORE_ROOT").filter(|value| !value.trim().is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = env("HOME").ok_or_else(|| {
        SignRailError::InvalidInput(
            "SIGNRAIL_STORE_ROOT or HOME is required for default store root".to_string(),
        )
    })?;
    Ok(PathBuf::from(home).join(".local/share/jeryu/signrail"))
}

pub(super) fn media_type(path: &Path) -> &'static str {
    match path.extension().and_then(|part| part.to_str()) {
        Some("gz") | Some("tgz") => "application/gzip",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

pub(super) fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(1)
}

pub(super) fn sign_release_usage() -> String {
    concat!(
        "usage: jeryu_signrail sign-release --artifact <bundle> --repo <owner/repo> ",
        "--sha <commit> --version <version> --rollback-target <target> ",
        "[--store-root <dir>] [--out-dir <dir>] [--stage <name>]..."
    )
    .to_string()
}

pub(super) fn verify_release_usage() -> String {
    "usage: jeryu_signrail verify-release --release <file> --stage <local|dev-canary|prod> --store-root <dir> --pubkey-file <file> [--json]"
        .to_string()
}

pub(super) fn help() -> String {
    format!(
        "jeryu_signrail commands:\n  checksum <path>\n  sbom <version> <artifact>...\n  {}\n  {}",
        sign_release_usage(),
        verify_release_usage()
    )
}
