use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

use super::{REQUIRED_RECEIPTS, Receipt, ReceiptStatus, ReleaseReadyGate};

#[derive(Debug, Default)]
pub(super) struct LoadedReceipts {
    pub(super) receipts: BTreeMap<String, Receipt>,
    pub(super) errors: Vec<ReceiptLoadError>,
}

#[derive(Debug)]
pub(super) struct ReceiptLoadError {
    pub(super) id: Option<String>,
    pub(super) path: PathBuf,
    pub(super) detail: String,
}

impl LoadedReceipts {
    pub(super) fn error_for(&self, id: &str) -> Option<&ReceiptLoadError> {
        self.errors
            .iter()
            .find(|error| error.id.as_deref() == Some(id))
    }
}

pub(super) fn load_receipts(receipt_dir: &Path) -> LoadedReceipts {
    let mut loaded = LoadedReceipts::default();
    let required: HashSet<&str> = REQUIRED_RECEIPTS.iter().copied().collect();

    let entries = match fs::read_dir(receipt_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return loaded,
        Err(err) => {
            loaded.errors.push(ReceiptLoadError {
                id: None,
                path: receipt_dir.to_path_buf(),
                detail: err.to_string(),
            });
            return loaded;
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                    paths.push(path);
                }
            }
            Err(err) => loaded.errors.push(ReceiptLoadError {
                id: None,
                path: receipt_dir.to_path_buf(),
                detail: err.to_string(),
            }),
        }
    }
    paths.sort();

    for path in paths {
        let receipt = match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Receipt>(&raw) {
                Ok(receipt) => receipt,
                Err(err) => {
                    loaded.errors.push(ReceiptLoadError {
                        id: required_id_from_path(&path),
                        path,
                        detail: format!("invalid receipt JSON: {err}"),
                    });
                    continue;
                }
            },
            Err(err) => {
                loaded.errors.push(ReceiptLoadError {
                    id: required_id_from_path(&path),
                    path,
                    detail: err.to_string(),
                });
                continue;
            }
        };

        if !required.contains(receipt.id.as_str()) {
            continue;
        }

        if loaded
            .receipts
            .insert(receipt.id.clone(), receipt.clone())
            .is_some()
        {
            loaded.errors.push(ReceiptLoadError {
                id: Some(receipt.id),
                path,
                detail: "duplicate required receipt id".to_string(),
            });
        }
    }

    loaded
}

fn required_id_from_path(path: &Path) -> Option<String> {
    let id = path.file_stem()?.to_str()?;
    REQUIRED_RECEIPTS.contains(&id).then(|| id.to_string())
}

pub fn post_check_run(gate: &ReleaseReadyGate, repo_slug: &str, head_sha: &str) -> Result<String> {
    let conclusion = match gate.overall {
        ReceiptStatus::Pass => "success",
        ReceiptStatus::Fail => "failure",
        ReceiptStatus::Pending => "neutral",
        ReceiptStatus::Skipped => "neutral",
    };
    let body = super::render_gate_text(gate);

    let payload = serde_json::json!({
        "name": "jeryu/release-ready",
        "head_sha": head_sha,
        "status": "completed",
        "conclusion": conclusion,
        "output": {
            "title": "jeryu/release-ready",
            "summary": gate.summary,
            "text": body,
        }
    });

    let payload_str = serde_json::to_string(&payload)?;
    let endpoint = format!("repos/{repo_slug}/check-runs");
    let mut child = Command::new("gh")
        .args([
            "api",
            "--method",
            "POST",
            "-H",
            "Accept: application/vnd.github+json",
            &endpoint,
            "--input",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn gh: {e} (is `gh` installed?)"))?;

    {
        let stdin = match child.stdin.as_mut() {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("gh did not expose stdin")),
        };
        stdin.write_all(payload_str.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "gh api failed (exit={:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
