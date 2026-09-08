//! Signed release creation, storage, and stage-receipt emission.

use super::sign_args::parse_sign_release_args;
use super::support::media_type;
use crate::artifact::Artifact;
use crate::error::{Result, SignRailError};
use crate::identity::OidcJobIdentity;
use crate::policy::{ReleasePolicy, validate_release};
use crate::receipt::Receipt;
use crate::release::Release;
use crate::release_cli_output::{
    StageReceiptInput, SummaryJsonInput, stage_receipt_json, summary_json, write_json,
};
use crate::rollback::RollbackMetadata;
use crate::sbom::SbomDocument;
use crate::signature::{Ed25519Signer, Signer};
use crate::store::ArtifactStore;
use std::fs;

pub(super) fn sign_release<F>(raw_args: &[String], env: &F) -> Result<String>
where
    F: Fn(&str) -> Option<String>,
{
    let args = parse_sign_release_args(raw_args, env)?;
    let github_actions = env("GITHUB_ACTIONS").as_deref() == Some("true");
    let seed_var = if github_actions {
        "SIGNRAIL_ED25519_SEED"
    } else {
        "JERYU_SIGNRAIL_ED25519_SEED"
    };
    let seed = env(seed_var)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            SignRailError::SigningUnavailable(format!(
                "{seed_var} is required for SignRail release signing"
            ))
        })?;
    let signer = Ed25519Signer::from_seed_hex(args.key_id.clone(), &seed)?;

    let artifact_name = args
        .artifact
        .file_name()
        .and_then(|part| part.to_str())
        .ok_or_else(|| {
            SignRailError::InvalidInput(format!(
                "invalid artifact path: {}",
                args.artifact.display()
            ))
        })?
        .to_string();
    let artifact = Artifact::from_file(artifact_name, &args.artifact, media_type(&args.artifact))?;

    let oidc = OidcJobIdentity::new(
        "https://jeryu.local/signrail",
        "jeryu_signrail",
        format!("repo:{}:sha:{}", args.repo, args.sha),
        args.repo.clone(),
        env("GITHUB_WORKFLOW_REF").unwrap_or_else(|| format!("artifact-support@{}", args.sha)),
        env("GITHUB_RUN_ID")
            .or_else(|| env("GITHUB_JOB"))
            .unwrap_or_else(|| format!("local-{}", &args.sha[..args.sha.len().min(12)])),
        env("RUNNER_NAME")
            .or_else(|| env("HOSTNAME"))
            .unwrap_or_else(|| "local-runner".to_string()),
        args.created_at_epoch + 3600,
    );

    let mut release = Release::new(
        format!("{}@{}", args.repo, args.sha),
        format!("{} artifact-support {}", args.repo, args.version),
        args.version.clone(),
        args.repo.clone(),
        args.sha.clone(),
        args.tree_sha.clone(),
        args.jeryu_ci_ir_hash.clone(),
        args.runner_class.clone(),
        args.runner_rootfs_digest.clone(),
        args.toolchain_digest.clone(),
        args.cargo_lock_digest.clone(),
        oidc,
    );
    release.add_artifact(artifact.clone());
    release.attach_sbom(SbomDocument::from_artifacts(
        &args.version,
        &release.artifacts,
        args.created_at_epoch,
    ));
    release.attach_rollback(RollbackMetadata::new(
        args.rollback_target.clone(),
        format!("restore signed artifact {}", args.rollback_target),
        args.jeryu_ci_ir_hash.clone(),
        "no migration declared by artifact-support",
        args.created_at_epoch,
    ));
    release.sign_with(&signer, args.created_at_epoch)?;

    let policy = ReleasePolicy::strict(
        args.repo.clone(),
        "https://jeryu.local/signrail",
        "jeryu_signrail",
        args.created_at_epoch,
    );
    let witness = validate_release(&release, &policy, &signer)?;
    if witness.signature_coverage_percent != 100 {
        return Err(SignRailError::Policy(format!(
            "signature coverage is not 100%: {}",
            witness.signature_coverage_percent
        )));
    }

    let release_json = release.to_json();
    let sbom_json = release
        .sbom
        .as_ref()
        .ok_or_else(|| SignRailError::Policy("missing SBOM after signing".to_string()))?
        .to_json();
    let provenance_json = format!(
        "[{}]",
        release
            .provenance
            .iter()
            .map(|provenance| provenance.to_json())
            .collect::<Vec<_>>()
            .join(",")
    );
    let witness_json = witness.to_json();

    let store = ArtifactStore::open(&args.store_root)?;
    let stored_artifact = store.put_artifact(&artifact)?;
    let stored_release = store.put_json("releases", &release.id, &release_json)?;
    let stored_sbom = store.put_json("sboms", &release.id, &sbom_json)?;
    let stored_provenance = store.put_json("provenance", &release.id, &provenance_json)?;
    let stored_witness = store.put_json("witnesses", &release.id, &witness_json)?;

    fs::create_dir_all(args.out_dir.join("stage-receipts"))?;
    write_json(args.out_dir.join("release.json"), &release_json)?;
    write_json(args.out_dir.join("sbom.json"), &sbom_json)?;
    write_json(args.out_dir.join("provenance.json"), &provenance_json)?;
    write_json(args.out_dir.join("witness.json"), &witness_json)?;

    let mut stage_receipt_paths = Vec::new();
    for stage in &args.stages {
        let receipt_json = stage_receipt_json(&StageReceiptInput {
            stage,
            sha: &args.sha,
            artifact_digest: &artifact.digest,
            rollback_target: &args.rollback_target,
            signer_key_id: signer.signer_id(),
            witness_digest: &witness.receipt_digest,
            signature_coverage_percent: witness.signature_coverage_percent,
            test_status: &args.test_status,
            release_version: &args.version,
        });
        let receipt = Receipt::new(
            "signrail-stage",
            format!("{}:{stage}", release.id),
            receipt_json,
        );
        let path = args
            .out_dir
            .join("stage-receipts")
            .join(format!("{stage}.json"));
        write_json(&path, &receipt.to_json())?;
        store.put_json(
            "receipts",
            &format!("{}-{stage}", release.id),
            &receipt.to_json(),
        )?;
        stage_receipt_paths.push(path.display().to_string());
    }

    Ok(summary_json(&SummaryJsonInput {
        release_id: &release.id,
        artifact_digest: &artifact.digest,
        signer_key_id: signer.signer_id(),
        signer_public_key_hex: &signer.public_key_hex(),
        signature_coverage_percent: witness.signature_coverage_percent,
        store_root: &args.store_root,
        out_dir: &args.out_dir,
        stored_artifact: &stored_artifact,
        stored_json: &[
            stored_release,
            stored_sbom,
            stored_provenance,
            stored_witness,
        ],
        stage_receipts: &stage_receipt_paths,
    }))
}
