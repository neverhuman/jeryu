//! Minimal CLI for local SignRail workflows.

use crate::artifact::Artifact;
use crate::checksum::sha256_file;
use crate::error::{Result, SignRailError};
use crate::identity::OidcJobIdentity;
use crate::json;
use crate::policy::{ReleasePolicy, validate_release};
use crate::receipt::Receipt;
use crate::release::Release;
use crate::rollback::RollbackMetadata;
use crate::sbom::SbomDocument;
use crate::signature::{Ed25519Signer, Signer};
use crate::store::ArtifactStore;
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
        Some("help") | None => Ok(help()),
        Some(other) => Err(SignRailError::InvalidInput(format!(
            "unknown command {other}\n{}",
            help()
        ))),
    }
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
        let receipt_json = stage_receipt_json(
            &args,
            stage,
            &artifact,
            signer.signer_id(),
            &witness.receipt_digest,
            witness.signature_coverage_percent,
        );
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

    Ok(summary_json(
        &release.id,
        &artifact.digest,
        signer.signer_id(),
        &signer.public_key_hex(),
        witness.signature_coverage_percent,
        &args,
        &stored_artifact,
        &[
            stored_release,
            stored_sbom,
            stored_provenance,
            stored_witness,
        ],
        &stage_receipt_paths,
    ))
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

fn write_json(path: impl AsRef<Path>, contents: &str) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

fn stage_receipt_json(
    args: &SignReleaseArgs,
    stage: &str,
    artifact: &Artifact,
    signer_key_id: &str,
    witness_digest: &str,
    signature_coverage_percent: u8,
) -> String {
    format!(
        "{{{},{},{},{},{},{},{},{},{},{}}}",
        json::number_field("schema_version", 1),
        json::field("stage", stage),
        json::field("sha", &args.sha),
        json::field("artifact_digest", &artifact.digest),
        json::field("rollback_target", &args.rollback_target),
        json::field("signer_key_id", signer_key_id),
        json::number_field(
            "signature_coverage_percent",
            signature_coverage_percent as u64
        ),
        json::field("test_status", &args.test_status),
        json::field("witness_digest", witness_digest),
        json::field("release_version", &args.version)
    )
}

fn summary_json(
    release_id: &str,
    artifact_digest: &str,
    signer_key_id: &str,
    signer_public_key_hex: &str,
    signature_coverage_percent: u8,
    args: &SignReleaseArgs,
    stored_artifact: &Path,
    stored_json: &[PathBuf],
    stage_receipts: &[String],
) -> String {
    let stored_json = stored_json
        .iter()
        .map(|path| json::quote(&path.display().to_string()))
        .collect::<Vec<_>>()
        .join(",");
    let stage_receipts = stage_receipts
        .iter()
        .map(|path| json::quote(path))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{{},{},{},{},{},{},{},{},\"stored_json\":[{}],\"stage_receipts\":[{}]}}",
        json::field("release_id", release_id),
        json::field("artifact_digest", artifact_digest),
        json::field("signer_key_id", signer_key_id),
        json::field("signer_public_key_hex", signer_public_key_hex),
        json::number_field(
            "signature_coverage_percent",
            signature_coverage_percent as u64
        ),
        json::field("store_root", &args.store_root.display().to_string()),
        json::field("out_dir", &args.out_dir.display().to_string()),
        json::field("stored_artifact", &stored_artifact.display().to_string()),
        stored_json,
        stage_receipts
    )
}

fn sign_release_usage() -> String {
    concat!(
        "usage: jeryu_signrail sign-release --artifact <bundle> --repo <owner/repo> ",
        "--sha <commit> --version <version> --rollback-target <target> ",
        "[--store-root <dir>] [--out-dir <dir>] [--stage <name>]..."
    )
    .to_string()
}

fn help() -> String {
    format!(
        "jeryu_signrail commands:\n  checksum <path>\n  sbom <version> <artifact>...\n  {}",
        sign_release_usage()
    )
}
