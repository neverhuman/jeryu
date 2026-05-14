//! Owner: Release Pipeline (composite gate)
//! Proof: `cargo test -p jeryu -- release::gate`
//! Invariants: Composite gate is the single source of truth for jeryu/release-ready.
//!
//! Implements the composite `jeryu/release-ready` gate described in
//! `release.policy.toml` and `docs/release-policy.md`. The gate is itself a
//! pure data structure (`ReleaseReadyGate`) with one `Receipt` per required
//! component. The CLI calls `compose_gate` then optionally posts to GitHub.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

/// Compose the gate for a PR. In `dry_run` mode all receipts default to
/// `Pending` with explanatory detail; this keeps local rehearsals safe even
/// when no PR exists. Production gating happens in CI via `--emit-status`.
pub fn compose_gate(pr: u64, dry_run: bool) -> ReleaseReadyGate {
    // Each receipt starts as Pending; CI lane scripts write Pass/Fail evidence
    // before calling `post_check_run`. See release.policy.toml for the full
    // receipt schema.
    let receipts: Vec<Receipt> = REQUIRED_RECEIPTS
        .iter()
        .map(|id| Receipt {
            id: (*id).to_string(),
            status: ReceiptStatus::Pending,
            detail: format!("{}: awaiting CI evaluation", id),
            evidence: None,
        })
        .collect();

    let overall = if receipts.iter().any(|r| r.status.is_blocking()) {
        if dry_run {
            ReceiptStatus::Pending
        } else {
            ReceiptStatus::Fail
        }
    } else {
        ReceiptStatus::Pass
    };

    let summary = if dry_run {
        format!(
            "jeryu/release-ready (PR #{pr}) — dry-run rehearsal: {} receipts pending",
            receipts.len()
        )
    } else {
        format!("jeryu/release-ready (PR #{pr}) — overall: {:?}", overall)
    };

    ReleaseReadyGate {
        pr,
        overall,
        receipts,
        summary,
    }
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
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("gh did not expose stdin"))?;
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
}
