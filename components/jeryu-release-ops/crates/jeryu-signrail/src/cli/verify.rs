//! Closed release, stage-receipt, and public-key verification.

use super::support::{required, verify_release_usage};
use super::verify_payload::{
    json_string, json_u64, read_pubkey_hex, safe_store_name, verify_provenance,
};
use crate::error::{Result, SignRailError};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
struct VerifyReleaseArgs {
    release: PathBuf,
    stage: String,
    store_root: PathBuf,
    pubkey_file: PathBuf,
    json: bool,
}

pub(super) fn verify_release(raw_args: &[String]) -> Result<String> {
    let args = parse_verify_release_args(raw_args)?;
    let release_json = fs::read_to_string(&args.release)?;
    let release: Value = serde_json::from_str(&release_json)
        .map_err(|err| SignRailError::InvalidInput(format!("release JSON parse failed: {err}")))?;
    let release_id = json_string(&release, &["id"])?;
    let commit_sha = json_string(&release, &["commit_sha"])?;
    let artifacts = release
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SignRailError::InvalidInput("release artifacts must be an array".to_string())
        })?;
    let provenance = release
        .get("provenance")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SignRailError::InvalidInput("release provenance must be an array".to_string())
        })?;
    if artifacts.is_empty() {
        return Err(SignRailError::Policy(
            "release has no artifacts to verify".to_string(),
        ));
    }
    if artifacts.len() != provenance.len() {
        return Err(SignRailError::Policy(format!(
            "signature coverage is not 100%: {} provenance entries for {} artifacts",
            provenance.len(),
            artifacts.len()
        )));
    }

    let stored_release = args
        .store_root
        .join("releases")
        .join(format!("{}.json", safe_store_name(release_id)));
    if !stored_release.is_file() {
        return Err(SignRailError::Verification(format!(
            "stored release JSON missing: {}",
            stored_release.display()
        )));
    }

    let pubkey_hex = read_pubkey_hex(&args.pubkey_file)?;
    for item in provenance {
        verify_provenance(item, &pubkey_hex)?;
    }

    let receipt_path = args.store_root.join("receipts").join(format!(
        "{}-{}.json",
        safe_store_name(release_id),
        args.stage
    ));
    let receipt_json = fs::read_to_string(&receipt_path).map_err(|err| {
        SignRailError::Verification(format!(
            "stage receipt missing or unreadable {}: {err}",
            receipt_path.display()
        ))
    })?;
    let receipt: Value = serde_json::from_str(&receipt_json).map_err(|err| {
        SignRailError::InvalidInput(format!("stage receipt JSON parse failed: {err}"))
    })?;
    let payload = receipt
        .get("payload")
        .ok_or_else(|| SignRailError::Verification("stage receipt missing payload".to_string()))?;
    if json_string(payload, &["stage"])? != args.stage {
        return Err(SignRailError::Verification(format!(
            "stage receipt mismatch: expected {}, got {}",
            args.stage,
            json_string(payload, &["stage"])?
        )));
    }
    if json_string(payload, &["sha"])? != commit_sha {
        return Err(SignRailError::Verification(format!(
            "stage receipt sha mismatch: expected {commit_sha}, got {}",
            json_string(payload, &["sha"])?
        )));
    }
    if json_u64(payload, &["signature_coverage_percent"])? != 100 {
        return Err(SignRailError::Policy(
            "stage receipt signature coverage is not 100%".to_string(),
        ));
    }

    if args.json {
        Ok(format!(
            "{{{},{},{},{},{}}}",
            crate::json::field("release_id", release_id),
            crate::json::field("stage", &args.stage),
            crate::json::field("commit_sha", commit_sha),
            crate::json::number_field("artifact_count", artifacts.len() as u64),
            crate::json::number_field("signature_coverage_percent", 100)
        ))
    } else {
        Ok(format!(
            "verified release {release_id} stage {} ({} artifacts, 100% signature coverage)",
            args.stage,
            artifacts.len()
        ))
    }
}

fn parse_verify_release_args(raw_args: &[String]) -> Result<VerifyReleaseArgs> {
    let mut release = None;
    let mut stage = None;
    let mut store_root = None;
    let mut pubkey_file = None;
    let mut json = false;

    let mut index = 0;
    while index < raw_args.len() {
        let arg = &raw_args[index];
        let value = |index: &mut usize| -> Result<String> {
            *index += 1;
            raw_args.get(*index).cloned().ok_or_else(|| {
                SignRailError::InvalidInput(format!(
                    "missing value for {arg}\n{}",
                    verify_release_usage()
                ))
            })
        };
        match arg.as_str() {
            "--release" => release = Some(PathBuf::from(value(&mut index)?)),
            "--stage" => stage = Some(value(&mut index)?),
            "--store-root" => store_root = Some(PathBuf::from(value(&mut index)?)),
            "--pubkey-file" => pubkey_file = Some(PathBuf::from(value(&mut index)?)),
            "--json" => json = true,
            "--help" => return Err(SignRailError::InvalidInput(verify_release_usage())),
            _ => {
                return Err(SignRailError::InvalidInput(format!(
                    "unknown verify-release option {arg}\n{}",
                    verify_release_usage()
                )));
            }
        }
        index += 1;
    }

    let stage = required(stage, "--stage")?;
    if !matches!(stage.as_str(), "local" | "dev-canary" | "prod") {
        return Err(SignRailError::InvalidInput(format!(
            "--stage must be local, dev-canary, or prod (got {stage})"
        )));
    }

    Ok(VerifyReleaseArgs {
        release: release.ok_or_else(|| SignRailError::InvalidInput(verify_release_usage()))?,
        stage,
        store_root: store_root
            .ok_or_else(|| SignRailError::InvalidInput(verify_release_usage()))?,
        pubkey_file: pubkey_file
            .ok_or_else(|| SignRailError::InvalidInput(verify_release_usage()))?,
        json,
    })
}
