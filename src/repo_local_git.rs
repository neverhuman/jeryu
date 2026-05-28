use crate::git::system::SystemGit;
use crate::redact::redact_text;
use crate::state::{Db, GitMirrorJob};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
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
        Err(err) if config.shadow_main.fallback_review => {
            match open_shadow_review_fallback(config).await {
                Ok(urls) => (
                    "shadow_review_opened".to_string(),
                    format!(
                        "trigger={trigger} refs={} prs={}",
                        shadow_refs(config).join(","),
                        urls.join(",")
                    ),
                ),
                Err(fallback_err) => (
                    "shadow_failed".to_string(),
                    redact_text(&format!(
                        "direct push failed: {err}; fallback review failed: {fallback_err}"
                    )),
                ),
            }
        }
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
        let refspec = format!("{ref_name}:{ref_name}");
        run_shadow_push(&mirror, &config.shadow_main.remote_url, &refspec)
            .with_context(|| format!("pushing {ref_name} to GitHub shadow"))?;
    }
    Ok(())
}

async fn open_shadow_review_fallback(config: &LocalRepoConfig) -> Result<Vec<String>> {
    let github = parse_github_remote(&config.shadow_main.remote_url)
        .context("shadow_main.remote_url must point at github.com for fallback_review")?;
    let token = std::env::var("GITHUB_TOKEN")
        .context("GITHUB_TOKEN is required for shadow_main.fallback_review")?;
    let mirror = refresh_bare_mirror(config)?;
    let client = reqwest::Client::builder()
        .user_agent("jeryu-repo-sidecar/0.1")
        .build()
        .context("building GitHub fallback HTTP client")?;

    let mut urls = Vec::new();
    for ref_name in shadow_refs(config) {
        let sha = git_rev_parse(&mirror, &ref_name)
            .with_context(|| format!("reading {ref_name} for review fallback"))?;
        let relay_branch = relay_branch_for_sha(&sha);
        let relay_ref = format!("refs/heads/{relay_branch}");
        let refspec = format!("{sha}:{relay_ref}");
        run_shadow_push(&mirror, &config.shadow_main.remote_url, &refspec)
            .with_context(|| format!("pushing relay branch {relay_branch}"))?;
        let url = open_or_find_github_pr(
            &client,
            &token,
            &github,
            &relay_branch,
            &config.default_branch,
            &sha,
            config,
        )
        .await?;
        urls.push(url);
    }
    Ok(urls)
}

fn git_rev_parse(mirror: &std::path::Path, ref_name: &str) -> Result<String> {
    let output = run_git_checked(mirror, &["rev-parse", ref_name])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn relay_branch_for_sha(sha: &str) -> String {
    let short = sha.chars().take(12).collect::<String>();
    format!("jeryu/main-relay/{short}")
}

async fn open_or_find_github_pr(
    client: &reqwest::Client,
    token: &str,
    github: &GithubRemote,
    head: &str,
    base: &str,
    sha: &str,
    config: &LocalRepoConfig,
) -> Result<String> {
    if let Some(existing) = find_open_github_pr(client, token, github, head, base).await? {
        return Ok(existing);
    }

    let body = serde_json::json!({
        "title": format!("Jeryu shadow sync: {} {}", config.repo, &sha[..sha.len().min(12)]),
        "head": head,
        "base": base,
        "body": format!(
            "Jeryu could not push `{}` directly to GitHub `{}`. This PR preserves local `{}` at `{}` for review and merge.",
            config.repo,
            github.slug(),
            base,
            sha
        ),
        "maintainer_can_modify": true,
    });
    let response = github_request(
        client,
        token,
        reqwest::Method::POST,
        &format!("/repos/{}/{}/pulls", github.owner, github.name),
    )
    .json(&body)
    .send()
    .await
    .context("opening GitHub main-relay PR")?;
    let status = response.status();
    if status.is_success() {
        let pr: GithubPullResponse = response
            .json()
            .await
            .context("parsing GitHub pull response")?;
        return Ok(pr.html_url);
    }

    let text = response.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
        && let Some(existing) = find_open_github_pr(client, token, github, head, base).await?
    {
        return Ok(existing);
    }
    anyhow::bail!(
        "opening GitHub main-relay PR failed: status={} body={}",
        status,
        redact_text(&text)
    );
}

async fn find_open_github_pr(
    client: &reqwest::Client,
    token: &str,
    github: &GithubRemote,
    head: &str,
    base: &str,
) -> Result<Option<String>> {
    let path = format!(
        "/repos/{}/{}/pulls?state=open&head={}:{}&base={}&per_page=1",
        github.owner,
        github.name,
        github.owner,
        urlencoding::encode(head),
        urlencoding::encode(base)
    );
    let response = github_request(client, token, reqwest::Method::GET, &path)
        .send()
        .await
        .context("finding existing GitHub main-relay PR")?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "finding existing GitHub main-relay PR failed: status={} body={}",
            status,
            redact_text(&text)
        );
    }
    let prs: Vec<GithubPullResponse> = response
        .json()
        .await
        .context("parsing existing GitHub pull response")?;
    Ok(prs.into_iter().next().map(|pr| pr.html_url))
}

fn github_request(
    client: &reqwest::Client,
    token: &str,
    method: reqwest::Method,
    path: &str,
) -> reqwest::RequestBuilder {
    client
        .request(method, format!("https://api.github.com{path}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
}

fn run_shadow_push(mirror: &Path, remote_url: &str, refspec: &str) -> Result<()> {
    let Some(github) = parse_github_remote(remote_url) else {
        run_git_checked(mirror, &["push", remote_url, refspec])?;
        return Ok(());
    };
    let Ok(token) = std::env::var("GITHUB_TOKEN") else {
        run_git_checked(mirror, &["push", remote_url, refspec])?;
        return Ok(());
    };
    if token.trim().is_empty() {
        run_git_checked(mirror, &["push", remote_url, refspec])?;
        return Ok(());
    }

    let askpass = github_askpass_script()?;
    let push_url = format!("https://github.com/{}/{}.git", github.owner, github.name);
    let git = SystemGit::resolve()?;
    run_checked(
        Command::new(git.path)
            .current_dir(mirror)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", askpass.path())
            .env("JERYU_GITHUB_TOKEN", token)
            .args(["push", push_url.as_str(), refspec]),
    )
    .with_context(|| format!("running authenticated GitHub push to {}", github.slug()))?;
    Ok(())
}

fn github_askpass_script() -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new().context("creating GitHub askpass helper")?;
    file.write_all(
        br#"#!/bin/sh
case "$1" in
  *Username*) printf '%s\n' x-access-token ;;
  *) printf '%s\n' "$JERYU_GITHUB_TOKEN" ;;
esac
"#,
    )
    .context("writing GitHub askpass helper")?;
    file.as_file_mut()
        .sync_all()
        .context("syncing GitHub askpass helper")?;
    #[cfg(unix)]
    {
        let mut permissions = file
            .as_file()
            .metadata()
            .context("reading GitHub askpass helper metadata")?
            .permissions();
        permissions.set_mode(0o700);
        file.as_file()
            .set_permissions(permissions)
            .context("marking GitHub askpass helper executable")?;
    }
    Ok(file)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubRemote {
    owner: String,
    name: String,
}

impl GithubRemote {
    fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

fn parse_github_remote(remote_url: &str) -> Option<GithubRemote> {
    let trimmed = remote_url.trim().trim_end_matches(".git");
    let path = trimmed
        .strip_prefix("git@github.com:")
        .or_else(|| trimmed.strip_prefix("https://github.com/"))
        .or_else(|| trimmed.strip_prefix("http://github.com/"))?;
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    Some(GithubRemote {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct GithubPullResponse {
    html_url: String,
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

#[cfg(test)]
mod git_tests {
    use super::*;

    #[test]
    fn parses_github_shadow_remote_urls() {
        assert_eq!(
            parse_github_remote("git@github.com:neverhuman/redline-testing.git"),
            Some(GithubRemote {
                owner: "neverhuman".into(),
                name: "redline-testing".into()
            })
        );
        assert_eq!(
            parse_github_remote("https://github.com/neverhuman/jeryu"),
            Some(GithubRemote {
                owner: "neverhuman".into(),
                name: "jeryu".into()
            })
        );
        assert_eq!(
            parse_github_remote("ssh://git@127.0.0.1/root/jeryu.git"),
            None
        );
    }

    #[test]
    fn relay_branch_is_sha_bound() {
        assert_eq!(
            relay_branch_for_sha("0123456789abcdef"),
            "jeryu/main-relay/0123456789ab"
        );
    }
}
