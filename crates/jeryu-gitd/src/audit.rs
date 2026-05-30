//! Append-only audit receipts for Phase 1 operations.

use crate::error::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Append an audit line to a repository-local receipt log.
pub fn append_receipt(repo_path: &Path, event: &str, detail: &str) -> Result<()> {
    let dir = repo_path.join("jeryu");
    std::fs::create_dir_all(&dir)?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("receipts.log"))?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(f, "{ts}\t{event}\t{}", detail.replace('\n', "\\n"))?;
    Ok(())
}
