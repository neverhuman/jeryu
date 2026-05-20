//! Owner: Release Pipeline (composite gate)
//! Proof: `cargo test -p jeryu -- release::gate`
//! Invariants: Composite gate is the single source of truth for jeryu/release-ready.
//!
//! Implements the composite `jeryu/release-ready` gate described in
//! `release.policy.toml` and `docs/release-policy.md`. The gate is itself a
//! pure data structure (`ReleaseReadyGate`) with one `Receipt` per required
//! component. Non-dry-run composition loads required receipts from the
//! repo-local evidence directory and fails closed when they are absent.
//! The CLI calls `compose_gate` then optionally posts to GitHub.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One receipt feeding the composite gate. Identifier matches
/// `release.policy.toml [gate.jeryu_release_ready] required_receipts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub id: String,
    pub status: ReceiptStatus,
    pub detail: String,
    /// Optional path to the evidence artifact (e.g. capsule.json).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Pass,
    Fail,
    Skipped,
    Pending,
}

impl ReceiptStatus {
    pub fn is_blocking(&self) -> bool {
        matches!(self, Self::Fail | Self::Pending)
    }
}

/// Composite gate composed from required receipts. Pass iff every required
/// receipt is `Pass` or `Skipped`-with-justification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReadyGate {
    pub pr: u64,
    pub overall: ReceiptStatus,
    pub receipts: Vec<Receipt>,
    pub summary: String,
}

impl ReleaseReadyGate {
    pub fn is_pass(&self) -> bool {
        self.overall == ReceiptStatus::Pass
    }
}

/// The canonical receipt ids required by the composite gate.
/// Must stay in sync with `release.policy.toml [gate.jeryu_release_ready]`.
pub const REQUIRED_RECEIPTS: &[&str] = &[
    "intake",
    "vti-plan",
    "proof-receipt",
    "risk-gate",
    "reviewer-agent",
    "rollback-plan",
    "ci-checks",
];

const DEFAULT_RECEIPT_DIR: &str = ".jeryu/release-ready/receipts";

#[derive(Debug, Default)]
struct LoadedReceipts {
    receipts: BTreeMap<String, Receipt>,
    errors: Vec<ReceiptLoadError>,
}

#[derive(Debug)]
struct ReceiptLoadError {
    id: Option<String>,
    path: PathBuf,
    detail: String,
}

impl LoadedReceipts {
    fn error_for(&self, id: &str) -> Option<&ReceiptLoadError> {
        self.errors
            .iter()
            .find(|error| error.id.as_deref() == Some(id))
    }
}

/// Compose the gate for a PR. In `dry_run` mode receipts default to
/// `Pending`; this keeps local rehearsals visibly incomplete. In non-dry-run
/// mode, every required receipt must be present in the repo-local receipt
/// directory. Missing or unreadable required receipts become explicit failing
/// receipts.
pub fn compose_gate(pr: u64, dry_run: bool) -> ReleaseReadyGate {
    if dry_run {
        return compose_dry_run_gate(pr);
    }

    compose_gate_from_receipt_dir(pr, Path::new(DEFAULT_RECEIPT_DIR))
}

fn compose_dry_run_gate(pr: u64) -> ReleaseReadyGate {
    let receipts: Vec<Receipt> = REQUIRED_RECEIPTS
        .iter()
        .map(|id| Receipt {
            id: (*id).to_string(),
            status: ReceiptStatus::Pending,
            detail: format!("{id}: awaiting CI evaluation"),
            evidence: None,
        })
        .collect();

    ReleaseReadyGate {
        pr,
        overall: ReceiptStatus::Pending,
        receipts,
        summary: format!(
            "jeryu/release-ready (PR #{pr}) — dry-run rehearsal: {} receipts pending",
            REQUIRED_RECEIPTS.len()
        ),
    }
}

fn compose_gate_from_receipt_dir(pr: u64, receipt_dir: &Path) -> ReleaseReadyGate {
    let loaded = load_receipts(receipt_dir);
    let receipts: Vec<Receipt> = REQUIRED_RECEIPTS
        .iter()
        .map(|id| {
            if let Some(error) = loaded.error_for(id) {
                return Receipt {
                    id: (*id).to_string(),
                    status: ReceiptStatus::Fail,
                    detail: format!(
                        "{id}: required receipt could not be loaded from {}: {}",
                        error.path.display(),
                        error.detail
                    ),
                    evidence: None,
                };
            }
            loaded
                .receipts
                .get(*id)
                .cloned()
                .unwrap_or_else(|| Receipt {
                    id: (*id).to_string(),
                    status: ReceiptStatus::Fail,
                    detail: format!(
                        "{id}: missing required receipt in {}",
                        receipt_dir.display()
                    ),
                    evidence: None,
                })
        })
        .collect();

    let overall = if receipts.iter().any(|r| r.status.is_blocking()) {
        ReceiptStatus::Fail
    } else {
        ReceiptStatus::Pass
    };

    let blocking = receipts.iter().filter(|r| r.status.is_blocking()).count();
    let summary = if blocking == 0 {
        format!(
            "jeryu/release-ready (PR #{pr}) — overall: {:?}; {} receipts loaded from {}",
            overall,
            receipts.len(),
            receipt_dir.display()
        )
    } else {
        format!(
            "jeryu/release-ready (PR #{pr}) — overall: {:?}; {blocking} blocking receipt(s) from {}",
            overall,
            receipt_dir.display()
        )
    };

    ReleaseReadyGate {
        pr,
        overall,
        receipts,
        summary,
    }
}

fn load_receipts(receipt_dir: &Path) -> LoadedReceipts {
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

/// Render a human-readable summary suitable for stdout or a GitHub Check Run.
pub fn render_gate_text(gate: &ReleaseReadyGate) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n", gate.summary));
    out.push_str("\nReceipts:\n");
    for r in &gate.receipts {
        let glyph = match r.status {
            ReceiptStatus::Pass => "✓",
            ReceiptStatus::Fail => "✗",
            ReceiptStatus::Skipped => "·",
            ReceiptStatus::Pending => "…",
        };
        out.push_str(&format!("  {glyph} {:<16} {}\n", r.id, r.detail));
    }
    out
}

/// Post the gate as a GitHub Check Run via the `gh` CLI. Returns the API
/// response body on success. Requires `gh` to be on PATH and a valid
/// `GITHUB_TOKEN` in the environment.
pub fn post_check_run(gate: &ReleaseReadyGate, repo_slug: &str, head_sha: &str) -> Result<String> {
    let conclusion = match gate.overall {
        ReceiptStatus::Pass => "success",
        ReceiptStatus::Fail => "failure",
        ReceiptStatus::Pending => "neutral",
        ReceiptStatus::Skipped => "neutral",
    };
    let body = render_gate_text(gate);

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
        use std::io::Write;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn write_receipt(dir: &Path, id: &str, status: ReceiptStatus) {
        std::fs::create_dir_all(dir).unwrap();
        let receipt = Receipt {
            id: id.to_string(),
            status,
            detail: format!("{id}: test receipt"),
            evidence: Some(PathBuf::from(format!("evidence/{id}.json"))),
        };
        let raw = serde_json::to_string_pretty(&receipt).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), raw).unwrap();
    }

    fn write_all_required(dir: &Path, status: ReceiptStatus) {
        for id in REQUIRED_RECEIPTS {
            write_receipt(dir, id, status);
        }
    }

    #[test]
    fn dry_run_gate_has_all_required_receipts() {
        let gate = compose_gate(0, true);
        assert_eq!(gate.receipts.len(), REQUIRED_RECEIPTS.len());
        for required in REQUIRED_RECEIPTS {
            assert!(gate.receipts.iter().any(|r| r.id == *required));
        }
    }

    #[test]
    fn dry_run_gate_is_pending_overall() {
        let gate = compose_gate(42, true);
        assert_eq!(gate.overall, ReceiptStatus::Pending);
        assert!(!gate.is_pass());
    }

    #[test]
    fn render_includes_all_receipts() {
        let gate = compose_gate(7, true);
        let text = render_gate_text(&gate);
        for r in &gate.receipts {
            assert!(
                text.contains(&r.id),
                "rendered text missing receipt {}",
                r.id
            );
        }
    }

    #[test]
    fn receipt_status_blocking() {
        assert!(ReceiptStatus::Fail.is_blocking());
        assert!(ReceiptStatus::Pending.is_blocking());
        assert!(!ReceiptStatus::Pass.is_blocking());
        assert!(!ReceiptStatus::Skipped.is_blocking());
    }

    #[test]
    fn non_dry_run_with_no_receipts_fails() {
        let dir = TempDir::new().unwrap();
        let gate = compose_gate_from_receipt_dir(99, dir.path());

        assert_eq!(gate.overall, ReceiptStatus::Fail);
        assert!(!gate.is_pass());
        assert!(
            gate.receipts
                .iter()
                .all(|r| r.status == ReceiptStatus::Fail)
        );
    }

    #[test]
    fn all_required_pass_receipts_pass() {
        let dir = TempDir::new().unwrap();
        write_all_required(dir.path(), ReceiptStatus::Pass);

        let gate = compose_gate_from_receipt_dir(100, dir.path());

        assert_eq!(gate.overall, ReceiptStatus::Pass);
        assert!(gate.is_pass());
        assert!(
            gate.receipts
                .iter()
                .all(|r| r.status == ReceiptStatus::Pass)
        );
    }

    #[test]
    fn missing_receipt_fails() {
        let dir = TempDir::new().unwrap();
        for id in REQUIRED_RECEIPTS
            .iter()
            .copied()
            .filter(|id| *id != "ci-checks")
        {
            write_receipt(dir.path(), id, ReceiptStatus::Pass);
        }

        let gate = compose_gate_from_receipt_dir(101, dir.path());

        assert_eq!(gate.overall, ReceiptStatus::Fail);
        assert!(!gate.is_pass());
        let missing = gate
            .receipts
            .iter()
            .find(|receipt| receipt.id == "ci-checks")
            .unwrap();
        assert_eq!(missing.status, ReceiptStatus::Fail);
        assert!(missing.detail.contains("missing required receipt"));
    }

    #[test]
    fn explicit_fail_receipt_fails() {
        let dir = TempDir::new().unwrap();
        write_all_required(dir.path(), ReceiptStatus::Pass);
        write_receipt(dir.path(), "risk-gate", ReceiptStatus::Fail);

        let gate = compose_gate_from_receipt_dir(102, dir.path());

        assert_eq!(gate.overall, ReceiptStatus::Fail);
        assert!(!gate.is_pass());
        let failed = gate
            .receipts
            .iter()
            .find(|receipt| receipt.id == "risk-gate")
            .unwrap();
        assert_eq!(failed.status, ReceiptStatus::Fail);
    }
}
