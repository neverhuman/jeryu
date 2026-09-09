//! Closed parsing for `sign-release` arguments.

use super::support::{default_store_root, now_epoch, required, sign_release_usage};
use crate::error::{Result, SignRailError};
use std::path::PathBuf;

#[derive(Debug)]
pub(super) struct SignReleaseArgs {
    pub(super) artifact: PathBuf,
    pub(super) store_root: PathBuf,
    pub(super) out_dir: PathBuf,
    pub(super) repo: String,
    pub(super) sha: String,
    pub(super) tree_sha: String,
    pub(super) version: String,
    pub(super) rollback_target: String,
    pub(super) test_status: String,
    pub(super) stages: Vec<String>,
    pub(super) key_id: Option<String>,
    pub(super) created_at_epoch: u64,
    pub(super) jeryu_ci_ir_hash: String,
    pub(super) runner_class: String,
    pub(super) runner_rootfs_digest: String,
    pub(super) toolchain_digest: String,
    pub(super) cargo_lock_digest: String,
}

pub(super) fn parse_sign_release_args<F>(raw_args: &[String], env: &F) -> Result<SignReleaseArgs>
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
            "--help" => return Err(SignRailError::InvalidInput(sign_release_usage())),
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
