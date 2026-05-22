use super::gate_logic::compose_gate_from_receipt_dir;
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
