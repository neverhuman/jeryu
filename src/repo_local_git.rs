use crate::redact::redact_text;
use crate::state::{Db, GitMirrorJob};
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::process::Command;

use super::{
    BackupTarget, LocalRepoConfig, SidecarRun, local_gitlab_ssh_url, mirror_path,
    parse_backup_target, path_with_trailing_slash, shadow_refs,
};

#[path = "repo_local_git_support.rs"]
mod git_support;
pub(crate) use git_support::{refresh_bare_mirror, run_checked, run_git_checked};

pub(super) async fn run_shadow_main(
    db: &Db,
    config: &LocalRepoConfig,
    trigger: &str,
) -> SidecarRun {
    if !config.shadow_main.enabled {
        return SidecarRun {
            repo: config.repo.clone(),
            status: "shadow_skipped".into(),
            detail: "disabled".into(),
        };
    }
    if config.shadow_main.remote_url.trim().is_empty() {
        let detail = "missing shadow_main.remote_url".to_string();
        record_sidecar(db, config, "github-shadow", None, "shadow_failed", &detail).await;
        return SidecarRun {
            repo: config.repo.clone(),
            status: "shadow_failed".into(),
            detail,
        };
    }

    let result = push_shadow_main(config);
    let (status, detail) = match result {
        Ok(()) => (
            "shadow_succeeded".to_string(),
            format!("trigger={trigger} refs={}", shadow_refs(config).join(",")),
        ),
        Err(err) => ("shadow_failed".to_string(), redact_text(&err.to_string())),
    };
    record_sidecar(
        db,
        config,
        "github-shadow",
        Some(&format!("refs/heads/{}", config.default_branch)),
        &status,
        &detail,
    )
    .await;
    if status == "shadow_failed" {
        tracing::warn!(repo = %config.repo, detail = %detail, "GitHub shadow push failed");
    }
    SidecarRun {
        repo: config.repo.clone(),
        status,
        detail,
    }
}

pub(super) async fn run_backup(db: &Db, config: &LocalRepoConfig, trigger: &str) -> SidecarRun {
    if config.backup.target.trim().is_empty() {
        return SidecarRun {
            repo: config.repo.clone(),
            status: "backup_skipped".into(),
            detail: "missing backup.target".into(),
        };
    }

    let result = sync_backup(config);
    let (status, detail) = match result {
        Ok(()) => (
            "backup_succeeded".to_string(),
            format!(
                "trigger={trigger} target={}",
                redact_text(&config.backup.target)
            ),
        ),
        Err(err) => ("backup_degraded".to_string(), redact_text(&err.to_string())),
    };
    record_sidecar(
        db,
        config,
        "repo-backup",
        Some(&format!("refs/heads/{}", config.default_branch)),
        &status,
        &detail,
    )
    .await;
    if status == "backup_degraded" {
        tracing::warn!(repo = %config.repo, detail = %detail, "repo backup degraded");
    }
    SidecarRun {
        repo: config.repo.clone(),
        status,
        detail,
    }
}

async fn record_sidecar(
    db: &Db,
    config: &LocalRepoConfig,
    remote_name: &str,
    branch_name: Option<&str>,
    status: &str,
    detail: &str,
) {
    let job = GitMirrorJob {
        id: 0,
        request_id: format!("repo-sidecar-{}", uuid::Uuid::new_v4()),
        remote_name: remote_name.to_string(),
        branch_name: branch_name.map(str::to_string),
        status: status.to_string(),
        detail: format!("repo={} {}", config.repo, redact_text(detail)),
        created_at: Utc::now().to_rfc3339(),
    };
    if let Err(err) = db.record_git_mirror_job(&job).await {
        tracing::warn!(error = %err, repo = %config.repo, "failed to record repo sidecar status");
    }
}

fn push_shadow_main(config: &LocalRepoConfig) -> Result<()> {
    let mirror = refresh_bare_mirror(config)?;
    for ref_name in shadow_refs(config) {
        run_git_checked(
            &mirror,
            &[
                "push",
                &config.shadow_main.remote_url,
                &format!("{ref_name}:{ref_name}"),
            ],
        )
        .with_context(|| format!("pushing {ref_name} to GitHub shadow"))?;
    }
    Ok(())
}

fn sync_backup(config: &LocalRepoConfig) -> Result<()> {
    let mirror = refresh_bare_mirror(config)?;
    run_git_checked(&mirror, &["fsck"]).context("local bare mirror failed git fsck")?;

    let mirror_src = path_with_trailing_slash(&mirror);
    match parse_backup_target(&config.backup.target)? {
        BackupTarget::Remote { host, path } => {
            run_checked(
                Command::new("ssh")
                    .arg(&host)
                    .arg("mkdir")
                    .arg("-p")
                    .arg(&path),
            )
            .with_context(|| format!("creating remote backup target {host}:{path}"))?;
            run_checked(
                Command::new("rsync")
                    .arg("-a")
                    .arg("--delete")
                    .arg(&mirror_src)
                    .arg(format!("{host}:{}/mirror.git/", path.trim_end_matches('/'))),
            )
            .context("rsyncing bare mirror backup")?;
            run_checked(
                Command::new("rsync")
                    .arg("-a")
                    .arg(&config.source_path)
                    .arg(format!("{host}:{}/repo.toml", path.trim_end_matches('/'))),
            )
            .context("rsyncing repo sidecar config")?;
            run_checked(
                Command::new("ssh")
                    .arg(&host)
                    .arg("git")
                    .arg("-C")
                    .arg(format!("{}/mirror.git", path.trim_end_matches('/')))
                    .arg("fsck"),
            )
            .context("remote backup mirror failed git fsck")?;
        }
        BackupTarget::Local(path) => {
            fs::create_dir_all(&path)
                .with_context(|| format!("creating local backup target {}", path.display()))?;
            let mirror_dst = path.join("mirror.git");
            fs::create_dir_all(&mirror_dst)
                .with_context(|| format!("creating {}", mirror_dst.display()))?;
            run_checked(
                Command::new("rsync")
                    .arg("-a")
                    .arg("--delete")
                    .arg(&mirror_src)
                    .arg(path_with_trailing_slash(&mirror_dst)),
            )
            .context("rsyncing local bare mirror backup")?;
            fs::copy(&config.source_path, path.join("repo.toml")).with_context(|| {
                format!(
                    "copying repo sidecar config {}",
                    config.source_path.display()
                )
            })?;
            run_git_checked(&mirror_dst, &["fsck"])
                .context("local backup mirror failed git fsck")?;
        }
    }
    Ok(())
}
