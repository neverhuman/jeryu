use crate::redact::redact_text;
use crate::state::{Db, GitMirrorJob};
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use std::fs;
use std::process::{Command, Output};

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
    run_shadow_main_for_sha(db, config, trigger, None).await
}

pub(super) async fn run_shadow_main_for_sha(
    db: &Db,
    config: &LocalRepoConfig,
    trigger: &str,
    expected_sha: Option<&str>,
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

    let result = push_shadow_main(config, expected_sha).await;
    let (status, detail) = match result {
        Ok(ShadowPushOutcome::Direct) => (
            "shadow_succeeded".to_string(),
            format!("trigger={trigger} refs={}", shadow_refs(config).join(",")),
        ),
        Ok(ShadowPushOutcome::ReviewFallback {
            relay_branch,
            review,
        }) => (
            "shadow_review_opened".to_string(),
            format!("trigger={trigger} relay={relay_branch} review={review}"),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShadowPushOutcome {
    Direct,
    ReviewFallback {
        relay_branch: String,
        review: String,
    },
}

async fn push_shadow_main(
    config: &LocalRepoConfig,
    expected_sha: Option<&str>,
) -> Result<ShadowPushOutcome> {
    let mirror = refresh_bare_mirror(config)?;
    for ref_name in shadow_refs(config) {
        if let Some(expected_sha) = expected_sha {
            verify_ref_sha(&mirror, &ref_name, expected_sha)?;
        }
        let direct = run_git_checked(
            &mirror,
            &[
                "push",
                &config.shadow_main.remote_url,
                &format!("{ref_name}:{ref_name}"),
            ],
        );
        if let Err(err) = direct {
            if config.shadow_main.fallback_review {
                return push_review_fallback(config, &mirror, &ref_name, err).await;
            }
            return Err(err).with_context(|| format!("pushing {ref_name} to shadow remote"));
        }
    }
    Ok(ShadowPushOutcome::Direct)
}

fn verify_ref_sha(mirror: &std::path::Path, ref_name: &str, expected_sha: &str) -> Result<()> {
    let output = run_git_checked(mirror, &["rev-parse", &format!("{ref_name}^{{commit}}")])
        .with_context(|| format!("reading local {ref_name} before shadow push"))?;
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if actual != expected_sha {
        bail!("pipeline SHA mismatch for {ref_name}: local {actual} != pipeline {expected_sha}");
    }
    Ok(())
}

async fn push_review_fallback(
    config: &LocalRepoConfig,
    mirror: &std::path::Path,
    ref_name: &str,
    direct_error: anyhow::Error,
) -> Result<ShadowPushOutcome> {
    let sha = ref_sha(mirror, ref_name)?;
    let short = short_sha(&sha);
    let relay_branch = format!("jeryu/main-relay/{short}");
    run_git_checked(
        mirror,
        &[
            "push",
            &config.shadow_main.remote_url,
            &format!("{sha}:refs/heads/{relay_branch}"),
        ],
    )
    .with_context(|| {
        format!("direct shadow push failed ({direct_error}); pushing relay branch {relay_branch}")
    })?;
    let review = open_review(config, &relay_branch, ref_name, &sha).await?;
    Ok(ShadowPushOutcome::ReviewFallback {
        relay_branch,
        review,
    })
}

fn ref_sha(mirror: &std::path::Path, ref_name: &str) -> Result<String> {
    let output = run_git_checked(mirror, &["rev-parse", &format!("{ref_name}^{{commit}}")])
        .with_context(|| format!("reading {ref_name} for review fallback"))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(12).collect()
}

async fn open_review(
    config: &LocalRepoConfig,
    relay_branch: &str,
    ref_name: &str,
    sha: &str,
) -> Result<String> {
    let target_branch = ref_name.trim_start_matches("refs/heads/");
    if let Some(repo) = parse_github_repo(&config.shadow_main.remote_url) {
        return open_github_pr(&repo, relay_branch, target_branch, sha);
    }
    if let Some(repo) = parse_gitlab_repo(&config.shadow_main.remote_url) {
        return open_gitlab_mr(&repo, relay_branch, target_branch, sha).await;
    }
    bail!(
        "unsupported review fallback remote {}",
        redact_text(&config.shadow_main.remote_url)
    )
}

fn open_github_pr(
    repo: &str,
    relay_branch: &str,
    target_branch: &str,
    sha: &str,
) -> Result<String> {
    let title = format!("JeRyu main relay {}", short_sha(sha));
    let body = format!(
        "JeRyu could not push {sha} directly to protected {target_branch}; this relay branch carries the same commit."
    );
    let output = run_review_command(
        Command::new("gh")
            .env("GH_PROMPT_DISABLED", "1")
            .arg("pr")
            .arg("create")
            .arg("--repo")
            .arg(repo)
            .arg("--head")
            .arg(relay_branch)
            .arg("--base")
            .arg(target_branch)
            .arg("--title")
            .arg(title)
            .arg("--body")
            .arg(body),
    )
    .context("opening GitHub main-relay PR")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn open_gitlab_mr(
    repo: &str,
    relay_branch: &str,
    target_branch: &str,
    sha: &str,
) -> Result<String> {
    let auth = crate::gitlab_auth::resolve_or_repair("http://127.0.0.1:8929").await?;
    let client = crate::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));
    let project = client
        .list_projects()
        .await?
        .into_iter()
        .find(|project| project.path_with_namespace == repo)
        .ok_or_else(|| anyhow!("local GitLab project {repo} not found for review fallback"))?;
    let title = format!("JeRyu main relay {}", short_sha(sha));
    let body = format!(
        "JeRyu could not push {sha} directly to protected {target_branch}; this relay branch carries the same commit."
    );
    let mr = client
        .create_merge_request(project.id, relay_branch, target_branch, &title, &body)
        .await?;
    Ok(mr.web_url)
}

fn run_review_command(command: &mut Command) -> Result<Output> {
    run_checked(command)
}

pub(super) fn parse_github_repo(remote_url: &str) -> Option<String> {
    parse_repo_path(remote_url, "github.com")
}

pub(super) fn parse_gitlab_repo(remote_url: &str) -> Option<String> {
    if remote_url.contains("github.com") {
        return None;
    }
    parse_repo_path(remote_url, "127.0.0.1")
        .or_else(|| parse_repo_path(remote_url, "localhost"))
        .or_else(|| parse_repo_path(remote_url, "gitlab.com"))
}

fn parse_repo_path(remote_url: &str, host: &str) -> Option<String> {
    let trimmed = remote_url.trim();
    if let Some(rest) = trimmed.strip_prefix("git@") {
        let (remote_host, path) = rest.split_once(':')?;
        if remote_host == host {
            return clean_repo_path(path);
        }
    }
    let host_start = trimmed.find(host)?;
    let after_host = &trimmed[host_start + host.len()..];
    let slash = after_host.find('/')?;
    let path = &after_host[slash + 1..];
    clean_repo_path(path)
}

fn clean_repo_path(path: &str) -> Option<String> {
    let path = path
        .trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git");
    if path.contains('/') {
        Some(path.to_string())
    } else {
        None
    }
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
