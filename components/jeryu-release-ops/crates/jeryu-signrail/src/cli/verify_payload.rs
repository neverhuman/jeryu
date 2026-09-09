//! Provenance and JSON payload verification helpers.

use crate::error::{Result, SignRailError};
use jeryu_signing::{EdVerifier, Signature as WireSignature};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub(super) fn verify_provenance(item: &Value, pubkey_hex: &str) -> Result<()> {
    let statement = item
        .get("statement")
        .ok_or_else(|| SignRailError::Verification("provenance missing statement".to_string()))?;
    let signature = item
        .get("signature")
        .ok_or_else(|| SignRailError::Verification("provenance missing signature".to_string()))?;
    let algorithm = json_string(signature, &["algorithm"])?;
    if algorithm != "JFSIG-ED25519" {
        return Err(SignRailError::Verification(format!(
            "unsupported signature algorithm {algorithm}"
        )));
    }
    let key_id = json_string(signature, &["key_id"])?;
    let verifier = EdVerifier::from_public_key_hex(key_id, pubkey_hex)
        .map_err(|err| SignRailError::Verification(format!("public key decode failed: {err}")))?;
    let wire = WireSignature {
        key_id: key_id.to_string(),
        algo: "ed25519".to_string(),
        value: json_string(signature, &["value_hex"])?.to_string(),
    };
    if verifier.verify(&canonical_statement(statement)?, &wire) {
        Ok(())
    } else {
        Err(SignRailError::Verification(
            "provenance signature mismatch".to_string(),
        ))
    }
}

pub(super) fn json_string<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key).ok_or_else(|| {
            SignRailError::InvalidInput(format!("missing JSON field {}", path.join(".")))
        })?;
    }
    current.as_str().ok_or_else(|| {
        SignRailError::InvalidInput(format!("JSON field {} must be a string", path.join(".")))
    })
}

pub(super) fn json_u64(value: &Value, path: &[&str]) -> Result<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key).ok_or_else(|| {
            SignRailError::InvalidInput(format!("missing JSON field {}", path.join(".")))
        })?;
    }
    current.as_u64().ok_or_else(|| {
        SignRailError::InvalidInput(format!("JSON field {} must be a number", path.join(".")))
    })
}

fn canonical_statement(statement: &Value) -> Result<Vec<u8>> {
    Ok(format!(
        concat!(
            "source_repository={}\n",
            "commit_sha={}\n",
            "tree_sha={}\n",
            "jeryu_ci_ir_hash={}\n",
            "runner_class={}\n",
            "runner_rootfs_digest={}\n",
            "toolchain_digest={}\n",
            "cargo_lock_digest={}\n",
            "artifact_digest={}\n",
            "sbom_digest={}\n",
            "signer_identity={}\n",
            "oidc_subject={}\n",
            "jankurai_release_witness={}\n",
            "created_at_epoch={}\n"
        ),
        json_string(statement, &["source_repository"])?,
        json_string(statement, &["commit_sha"])?,
        json_string(statement, &["tree_sha"])?,
        json_string(statement, &["jeryu_ci_ir_hash"])?,
        json_string(statement, &["runner_class"])?,
        json_string(statement, &["runner_rootfs_digest"])?,
        json_string(statement, &["toolchain_digest"])?,
        json_string(statement, &["cargo_lock_digest"])?,
        json_string(statement, &["artifact_digest"])?,
        json_string(statement, &["sbom_digest"])?,
        json_string(statement, &["signer_identity"])?,
        json_string(statement, &["oidc_subject"])?,
        json_string(statement, &["jankurai_release_witness"])?,
        json_u64(statement, &["created_at_epoch"])?
    )
    .into_bytes())
}

pub(super) fn safe_store_name(name: &str) -> String {
    name.replace('/', "_")
}

pub(super) fn read_pubkey_hex(path: &Path) -> Result<String> {
    let contents = fs::read_to_string(path)?;
    let tokens = contents.split_whitespace().collect::<Vec<_>>();
    let Some(value) = tokens.last() else {
        return Err(SignRailError::InvalidInput(format!(
            "empty public key file {}",
            path.display()
        )));
    };
    Ok((*value).to_string())
}
