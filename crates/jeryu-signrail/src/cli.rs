//! Minimal CLI for local SignRail workflows.

use crate::artifact::Artifact;
use crate::checksum::sha256_file;
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
use jeryu_signing::{EdVerifier, Signature as WireSignature};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Run the CLI using process arguments.
pub fn run_env() -> Result<String> {
    run_from_with_env(std::env::args().skip(1), |key| std::env::var(key).ok())
}

/// Run the CLI from an argument iterator.
pub fn run_from<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    run_from_with_env(args, |key| std::env::var(key).ok())
}

/// Run the CLI with an injected environment lookup.
pub fn run_from_with_env<I, S, F>(args: I, env: F) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    F: Fn(&str) -> Option<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("checksum") => {
            let path = args.get(1).ok_or_else(|| {
                SignRailError::InvalidInput("usage: jeryu_signrail checksum <path>".to_string())
            })?;
            Ok(format!("{}  {}", sha256_file(path)?, path))
        }
        Some("sbom") => {
            let version = args.get(1).ok_or_else(|| {
                SignRailError::InvalidInput(
                    "usage: jeryu_signrail sbom <version> <artifact>...".to_string(),
                )
            })?;
            if args.len() < 3 {
                return Err(SignRailError::InvalidInput(
                    "usage: jeryu_signrail sbom <version> <artifact>...".to_string(),
                ));
            }
            let artifacts = artifacts_from_paths(args.iter().skip(2))?;
            Ok(SbomDocument::from_artifacts(version, &artifacts, 0).to_json())
        }
        Some("sign-release") => sign_release(&args[1..], &env),
        Some("verify-release") => verify_release(&args[1..]),
        Some("help") | None => Ok(help()),
        Some(other) => Err(SignRailError::InvalidInput(format!(
            "unknown command {other}\n{}",
            help()
        ))),
    }
}

#[derive(Debug)]
struct VerifyReleaseArgs {
    release: PathBuf,
    stage: String,
    store_root: PathBuf,
    pubkey_file: PathBuf,
    json: bool,
}

#[derive(Debug)]
struct SignReleaseArgs {
    artifact: PathBuf,
    store_root: PathBuf,
    out_dir: PathBuf,
    repo: String,
    sha: String,
    tree_sha: String,
    version: String,
    rollback_target: String,
    test_status: String,
    stages: Vec<String>,
    key_id: Option<String>,
    created_at_epoch: u64,
    jeryu_ci_ir_hash: String,
    runner_class: String,
    runner_rootfs_digest: String,
    toolchain_digest: String,
    cargo_lock_digest: String,
}

fn sign_release<F>(raw_args: &[String], env: &F) -> Result<String>
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

fn artifacts_from_paths<'a>(paths: impl Iterator<Item = &'a String>) -> Result<Vec<Artifact>> {
    let mut artifacts = Vec::new();
    for path in paths {
        let path_buf = PathBuf::from(path);
        let name = path_buf
            .file_name()
            .and_then(|part| part.to_str())
            .ok_or_else(|| SignRailError::InvalidInput(format!("invalid artifact path: {path}")))?
            .to_string();
        artifacts.push(Artifact::from_file(
            name,
            path_buf,
            "application/octet-stream",
        )?);
    }
    Ok(artifacts)
}

fn verify_release(raw_args: &[String]) -> Result<String> {
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

fn verify_provenance(item: &Value, pubkey_hex: &str) -> Result<()> {
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

fn parse_sign_release_args<F>(raw_args: &[String], env: &F) -> Result<SignReleaseArgs>
where
    F: Fn(&str) -> Option<String>,
{
    let mut artifact = None;
    let mut store_root = None;
    let mut out_dir = None;
    let mut repo = None;
    let mut sha = None;
    let mut tree_sha = None;
    let mut version = None;
    let mut rollback_target = None;
    let mut test_status = None;
    let mut stages = Vec::new();
    let mut key_id = None;
    let mut created_at_epoch = None;
    let mut jeryu_ci_ir_hash = None;
    let mut runner_class = None;
    let mut runner_rootfs_digest = None;
    let mut toolchain_digest = None;
    let mut cargo_lock_digest = None;

    let mut index = 0;
    while index < raw_args.len() {
        let arg = &raw_args[index];
        if !arg.starts_with("--") && artifact.is_none() {
            artifact = Some(PathBuf::from(arg));
            index += 1;
            continue;
        }
        let value = |index: &mut usize| -> Result<String> {
            *index += 1;
            raw_args.get(*index).cloned().ok_or_else(|| {
                SignRailError::InvalidInput(format!(
                    "missing value for {arg}\n{}",
                    sign_release_usage()
                ))
            })
        };
        match arg.as_str() {
            "--artifact" => artifact = Some(PathBuf::from(value(&mut index)?)),
            "--store-root" => store_root = Some(PathBuf::from(value(&mut index)?)),
            "--out-dir" => out_dir = Some(PathBuf::from(value(&mut index)?)),
            "--repo" => repo = Some(value(&mut index)?),
            "--sha" => sha = Some(value(&mut index)?),
            "--tree-sha" => tree_sha = Some(value(&mut index)?),
            "--version" => version = Some(value(&mut index)?),
            "--rollback-target" => rollback_target = Some(value(&mut index)?),
            "--test-status" => test_status = Some(value(&mut index)?),
            "--stage" => stages.push(value(&mut index)?),
            "--key-id" => key_id = Some(value(&mut index)?),
            "--created-at-epoch" => {
                let raw = value(&mut index)?;
                created_at_epoch = Some(raw.parse::<u64>().map_err(|err| {
                    SignRailError::InvalidInput(format!("invalid --created-at-epoch: {err}"))
                })?);
            }
            "--ci-ir-hash" => jeryu_ci_ir_hash = Some(value(&mut index)?),
            "--runner-class" => runner_class = Some(value(&mut index)?),
            "--runner-rootfs-digest" => runner_rootfs_digest = Some(value(&mut index)?),
            "--toolchain-digest" => toolchain_digest = Some(value(&mut index)?),
            "--cargo-lock-digest" => cargo_lock_digest = Some(value(&mut index)?),
            "--help" => {
                return Err(SignRailError::InvalidInput(sign_release_usage()));
            }
            _ => {
                return Err(SignRailError::InvalidInput(format!(
                    "unknown sign-release option {arg}\n{}",
                    sign_release_usage()
                )));
            }
        }
        index += 1;
    }

    let artifact = artifact.ok_or_else(|| SignRailError::InvalidInput(sign_release_usage()))?;
    let repo = required(repo, "--repo")?;
    let sha = required(sha, "--sha")?;
    let version = required(version, "--version")?;
    let rollback_target = required(rollback_target, "--rollback-target")?;
    if stages.is_empty() {
        stages = vec![
            "local".to_string(),
            "dev-canary".to_string(),
            "prod".to_string(),
        ];
    }

    Ok(SignReleaseArgs {
        artifact,
        store_root: match store_root {
            Some(path) => path,
            None => default_store_root(env)?,
        },
        out_dir: out_dir.unwrap_or_else(|| PathBuf::from("target/artifact-support/signrail")),
        repo,
        tree_sha: tree_sha.unwrap_or_else(|| sha.clone()),
        sha,
        version,
        rollback_target,
        test_status: test_status.unwrap_or_else(|| "artifact-support-passed".to_string()),
        stages,
        key_id,
        created_at_epoch: created_at_epoch.unwrap_or_else(now_epoch),
        jeryu_ci_ir_hash: jeryu_ci_ir_hash.unwrap_or_else(|| "sha256:not-recorded".to_string()),
        runner_class: runner_class.unwrap_or_else(|| "release-hermetic".to_string()),
        runner_rootfs_digest: runner_rootfs_digest
            .unwrap_or_else(|| "sha256:runner-rootfs-not-recorded".to_string()),
        toolchain_digest: toolchain_digest
            .unwrap_or_else(|| "sha256:toolchain-not-recorded".to_string()),
        cargo_lock_digest: cargo_lock_digest
            .unwrap_or_else(|| "sha256:cargo-lock-not-recorded".to_string()),
    })
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

fn required(value: Option<String>, flag: &str) -> Result<String> {
    value.filter(|item| !item.trim().is_empty()).ok_or_else(|| {
        SignRailError::InvalidInput(format!("missing required {flag}\n{}", sign_release_usage()))
    })
}

fn default_store_root<F>(env: &F) -> Result<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(path) = env("SIGNRAIL_STORE_ROOT").filter(|value| !value.trim().is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = env("HOME").ok_or_else(|| {
        SignRailError::InvalidInput(
            "SIGNRAIL_STORE_ROOT or HOME is required for default store root".to_string(),
        )
    })?;
    Ok(PathBuf::from(home).join(".local/share/jeryu/signrail"))
}

fn media_type(path: &Path) -> &'static str {
    match path.extension().and_then(|part| part.to_str()) {
        Some("gz") | Some("tgz") => "application/gzip",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(1)
}

fn sign_release_usage() -> String {
    concat!(
        "usage: jeryu_signrail sign-release --artifact <bundle> --repo <owner/repo> ",
        "--sha <commit> --version <version> --rollback-target <target> ",
        "[--store-root <dir>] [--out-dir <dir>] [--stage <name>]..."
    )
    .to_string()
}

fn verify_release_usage() -> String {
    "usage: jeryu_signrail verify-release --release <file> --stage <local|dev-canary|prod> --store-root <dir> --pubkey-file <file> [--json]"
        .to_string()
}

fn help() -> String {
    format!(
        "jeryu_signrail commands:\n  checksum <path>\n  sbom <version> <artifact>...\n  {}\n  {}",
        sign_release_usage(),
        verify_release_usage()
    )
}

fn json_string<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str> {
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

fn json_u64(value: &Value, path: &[&str]) -> Result<u64> {
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

fn safe_store_name(name: &str) -> String {
    name.replace('/', "_")
}

fn read_pubkey_hex(path: &Path) -> Result<String> {
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
