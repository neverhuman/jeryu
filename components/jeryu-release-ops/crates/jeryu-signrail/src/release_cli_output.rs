use crate::error::Result;
use crate::json;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct StageReceiptInput<'a> {
    pub(crate) stage: &'a str,
    pub(crate) sha: &'a str,
    pub(crate) artifact_digest: &'a str,
    pub(crate) rollback_target: &'a str,
    pub(crate) signer_key_id: &'a str,
    pub(crate) witness_digest: &'a str,
    pub(crate) signature_coverage_percent: u8,
    pub(crate) test_status: &'a str,
    pub(crate) release_version: &'a str,
}

pub(crate) struct SummaryJsonInput<'a> {
    pub(crate) release_id: &'a str,
    pub(crate) artifact_digest: &'a str,
    pub(crate) signer_key_id: &'a str,
    pub(crate) signer_public_key_hex: &'a str,
    pub(crate) signature_coverage_percent: u8,
    pub(crate) store_root: &'a Path,
    pub(crate) out_dir: &'a Path,
    pub(crate) stored_artifact: &'a Path,
    pub(crate) stored_json: &'a [PathBuf],
    pub(crate) stage_receipts: &'a [String],
}

pub(crate) fn write_json(path: impl AsRef<Path>, contents: &str) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

pub(crate) fn stage_receipt_json(input: &StageReceiptInput<'_>) -> String {
    format!(
        "{{{},{},{},{},{},{},{},{},{},{}}}",
        json::number_field("schema_version", 1),
        json::field("stage", input.stage),
        json::field("sha", input.sha),
        json::field("artifact_digest", input.artifact_digest),
        json::field("rollback_target", input.rollback_target),
        json::field("signer_key_id", input.signer_key_id),
        json::number_field(
            "signature_coverage_percent",
            input.signature_coverage_percent as u64
        ),
        json::field("test_status", input.test_status),
        json::field("witness_digest", input.witness_digest),
        json::field("release_version", input.release_version)
    )
}

pub(crate) fn summary_json(input: &SummaryJsonInput<'_>) -> String {
    let stored_json = input
        .stored_json
        .iter()
        .map(|path| json::quote(&path.display().to_string()))
        .collect::<Vec<_>>()
        .join(",");
    let stage_receipts = input
        .stage_receipts
        .iter()
        .map(|path| json::quote(path))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{{},{},{},{},{},{},{},{},\"stored_json\":[{}],\"stage_receipts\":[{}]}}",
        json::field("release_id", input.release_id),
        json::field("artifact_digest", input.artifact_digest),
        json::field("signer_key_id", input.signer_key_id),
        json::field("signer_public_key_hex", input.signer_public_key_hex),
        json::number_field(
            "signature_coverage_percent",
            input.signature_coverage_percent as u64
        ),
        json::field("store_root", &input.store_root.display().to_string()),
        json::field("out_dir", &input.out_dir.display().to_string()),
        json::field(
            "stored_artifact",
            &input.stored_artifact.display().to_string()
        ),
        stored_json,
        stage_receipts
    )
}
