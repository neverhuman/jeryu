use std::path::Path;

use super::{REQUIRED_RECEIPTS, Receipt, ReceiptStatus, ReleaseReadyGate};

#[path = "gate_logic_io.rs"]
mod io;

pub use io::post_check_run;

/// Compose the gate for a PR. In `dry_run` mode receipts default to
/// `Pending`; this keeps local rehearsals visibly incomplete. In non-dry-run
/// mode, every required receipt must be present in the repo-local receipt
/// directory. Missing or unreadable required receipts become explicit failing
/// receipts.
pub fn compose_gate(pr: u64, dry_run: bool) -> ReleaseReadyGate {
    if dry_run {
        return compose_dry_run_gate(pr);
    }

    compose_gate_from_receipt_dir(pr, Path::new(super::DEFAULT_RECEIPT_DIR))
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

pub(crate) fn compose_gate_from_receipt_dir(pr: u64, receipt_dir: &Path) -> ReleaseReadyGate {
    let loaded = io::load_receipts(receipt_dir);
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
