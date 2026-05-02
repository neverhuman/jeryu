//! Owner: Shadow Remote Mirroring
//! Proof: `cargo test -p jeryu -- shadow`
//! Invariants: Mirror operations are idempotent; push failures do not block the primary pipeline; git commands are used natively.

use crate::gitlab_client::GitlabClient;
use crate::state::{Db, ShadowSyncConfig};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tracing::{error, warn};

type ShadowPushOutcome = Option<(String, Option<Result<(), String>>)>;

#[derive(Debug, Clone)]
pub struct RemoteStatus {
    pub name: String,
    pub fetch_url: Option<String>,
    pub push_url: String,
}

#[derive(Debug, Clone)]
pub struct ShadowStatus {
    pub repo_root: PathBuf,
    pub head_branch: Option<String>,
    pub target_remote: String,
    pub target_exists: bool,
    pub remotes: Vec<RemoteStatus>,
}

pub fn status(repo: Option<&Path>, target_remote: &str) -> Result<ShadowStatus> {
    let repo_root = open_repo(repo)?;
    let head_branch = get_head_branch(&repo_root).ok();
    
    let remotes = list_remotes(&repo_root).unwrap_or_default();
    let target_exists = remotes.iter().any(|remote| remote.name == target_remote);

    Ok(ShadowStatus {
        repo_root,
        head_branch,
        target_remote: target_remote.to_string(),
        target_exists,
        remotes,
    })
}

pub fn ensure_remote(repo: Option<&Path>, name: &str, url: &str) -> Result<()> {
    let repo_root = open_repo(repo)?;
    
    let remotes = list_remotes(&repo_root).unwrap_or_default();
    if remotes.iter().any(|r| r.name == name) {
        run_git(&repo_root, ["remote", "set-url", name, url])?;
    } else {
        run_git(&repo_root, ["remote", "add", name, url])?;
    }
    run_git(&repo_root, ["remote", "set-url", "--push", name, url])?;
    Ok(())
}

pub fn push_remote(
    repo: Option<&Path>,
    name: &str,
    branch: Option<&str>,
    mirror: bool,
) -> Result<()> {
    let repo_root = open_repo(repo)?;
    if mirror {
        run_git(&repo_root, ["push", "--mirror", name])?;
        return Ok(());
    }

    let branch_name = match branch {
        Some(branch) => branch.to_string(),
        None => get_head_branch(&repo_root).context("detached HEAD; pass --branch explicitly")?,
    };
    let refspec = format!("HEAD:refs/heads/{branch_name}");
    run_git(&repo_root, ["push", name, &refspec])?;
    Ok(())
}

fn open_repo(repo: Option<&Path>) -> Result<PathBuf> {
    let path = repo.unwrap_or_else(|| Path::new("."));
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .context("failed to discover git repository")?;
        
    if !output.status.success() {
        bail!("failed to discover git repository from {}", path.display());
    }
    
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(root))
}

fn get_head_branch(repo_root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()?;
        
    if !output.status.success() {
        bail!("Failed to resolve HEAD branch");
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if head == "HEAD" {
        bail!("Detached HEAD");
    }
    Ok(head)
}

fn list_remotes(repo_root: &Path) -> Result<Vec<RemoteStatus>> {
    let output = Command::new("git")
        .args(["remote", "-v"])
        .current_dir(repo_root)
        .output()?;
        
    if !output.status.success() {
        return Ok(Vec::new());
    }
    
    let out_str = String::from_utf8_lossy(&output.stdout);
    let mut names = std::collections::HashSet::new();
    let mut remotes = Vec::new();
    
    for line in out_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            names.insert(parts[0].to_string());
        }
    }
    
    for name in names {
        // Find fetch url
        let mut fetch_url = None;
        let mut push_url = None;
        for line in out_str.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == name {
                if parts[2] == "(fetch)" {
                    fetch_url = Some(parts[1].to_string());
                } else if parts[2] == "(push)" {
                    push_url = Some(parts[1].to_string());
                }
            }
        }
        
        let push_url = push_url.or_else(|| fetch_url.clone()).unwrap_or_else(|| "(none)".to_string());
        
        remotes.push(RemoteStatus {
            name,
            fetch_url,
            push_url,
        });
    }
    
    Ok(remotes)
}

fn run_git<const N: usize>(repo_root: &Path, args: [&str; N]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .status()
        .with_context(|| format!("failed to run git in {}", repo_root.display()))?;
    if !status.success() {
        bail!("git command failed in {}", repo_root.display());
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct ShadowSyncSummary {
    pub enabled_count: usize,
    pub syncing_count: usize,
    pub error_count: usize,
    pub display_text: String,
    pub upstream_url: Option<String>,
    pub upstream_status: String,
    pub upstream_gap: Option<usize>,
}

pub async fn run_shadow_loop(db: Db, client: GitlabClient) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        let configs = match db.list_shadow_sync_configs().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to list shadow sync configs: {}", e);
                continue;
            }
        };

        for mut config in configs {
            if !config.enabled {
                continue;
            }

            // Honour immediate sync request — skip backoff entirely.
            let forced = config.status == "sync_requested";

            // Exponential backoff (skipped when forced)
            if !forced && config.consecutive_failures > 0 {
                let backoff_secs = 2_u64.pow(config.consecutive_failures.min(4) as u32);
                let last_attempt = config
                    .last_attempt_at
                    .as_deref()
                    .unwrap_or("1970-01-01T00:00:00Z");
                if let Ok(last) = chrono::DateTime::parse_from_rfc3339(last_attempt) {
                    let now = chrono::Utc::now();
                    if now.signed_duration_since(last).num_seconds() < backoff_secs as i64 {
                        continue;
                    }
                }
            }

            config.last_attempt_at = Some(chrono::Utc::now().to_rfc3339());
            let source_dir = config.source_dir.clone();

            match sync_once(&db, &client, &mut config).await {
                Ok(true) => {
                    config.status = "idle".to_string();
                    config.error_msg = None;
                    config.consecutive_failures = 0;
                    config.last_success_at = Some(chrono::Utc::now().to_rfc3339());
                }
                Ok(false) => {
                    // No new commits, stay ok
                }
                Err(e) => {
                    warn!("Shadow sync failed for {}: {:#}", source_dir, e);
                    config.status = "error".to_string();
                    config.error_msg = Some(e.to_string());
                    config.consecutive_failures += 1;
                }
            }

            let _ = db.upsert_shadow_sync_config(&config).await;
        }
    }
}

async fn sync_once(_db: &Db, client: &GitlabClient, config: &mut ShadowSyncConfig) -> Result<bool> {
    let source_dir = config.source_dir.clone();
    let target_branch = config.target_branch.clone();
    let target_project_id = config.target_project_id;
    let pat_opt = client.pat_value_for_clone();
    let project = client.get_project(target_project_id).await?;
    let remote_url =
        format!("{}.git", project.web_url).replace(crate::config::GITLAB_HOSTNAME, "localhost");

    let last_pushed_sha = config.last_pushed_sha.clone();
    let status = config.status.clone();

    tokio::task::spawn_blocking(move || -> Result<ShadowPushOutcome> {
        let repo_root = PathBuf::from(&source_dir);
        
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_root)
            .output()
            .context("failed to run git rev-parse")?;
            
        if !output.status.success() {
            bail!("failed to resolve HEAD in {}", source_dir);
        }
        
        let head_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if let Some(ref pushed_sha) = last_pushed_sha
            && pushed_sha == &head_sha
            && status != "error"
        {
            // No changes
            return Ok(None);
        }

        // We need to push to shadow server
        let push_url = if let Some(pat) = &pat_opt {
            if remote_url.starts_with("http://") {
                remote_url.replace("http://", &format!("http://oauth2:{}@", pat))
            } else if remote_url.starts_with("https://") {
                remote_url.replace("https://", &format!("https://oauth2:{}@", pat))
            } else {
                remote_url.clone()
            }
        } else {
            remote_url.clone()
        };

        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&repo_root)
            .output()?;
        let head_branch_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let head_name = if head_branch_name == "HEAD" { "HEAD" } else { &head_branch_name };

        let refspec = format!("+{}:refs/heads/{}", head_name, target_branch);

        let push_status = std::process::Command::new("git")
            .args(["push", &push_url, &refspec])
            .current_dir(&repo_root)
            .status()
            .context("failed to execute push command")?;
            
        if !push_status.success() {
            bail!("failed to push to shadow remote");
        }

        // Also push upstream if configured!
        let mut upstream_res = None;
        if let Some(upstream_url) = crate::settings::get().shadow.upstream_url.clone() {
            let out = std::process::Command::new("git")
                .args(["push", &upstream_url, &refspec])
                .current_dir(&source_dir)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    upstream_res = Some(Ok(()));
                }
                Ok(o) => {
                    upstream_res = Some(Err(String::from_utf8_lossy(&o.stderr).into_owned()));
                }
                Err(e) => {
                    upstream_res = Some(Err(e.to_string()));
                }
            }
        }

        Ok(Some((head_sha, upstream_res)))
    })
    .await?
    .map(|opt_res| {
        if let Some((sha, up_res)) = opt_res {
            config.last_seen_head_sha = Some(sha.clone());
            config.last_pushed_sha = Some(sha.clone());

            // Handle upstream outcome
            if let Some(res) = up_res {
                match res {
                    Ok(_) => {
                        config.upstream_status = "ok".into();
                        config.upstream_last_pushed_sha = Some(sha);
                        config.upstream_error_msg = None;
                    }
                    Err(e) => {
                        config.upstream_status = "error".into();
                        config.upstream_error_msg = Some(e);
                    }
                }
            }
            return true;
        }
        false
    })
}

pub async fn compute_summary(db: &Db) -> Result<Option<ShadowSyncSummary>> {
    let configs = db.list_shadow_sync_configs().await?;
    let mut summary = ShadowSyncSummary::default();

    if configs.is_empty() {
        return Ok(None);
    }

    for c in &configs {
        if c.enabled {
            summary.enabled_count += 1;
        }
        if c.status == "syncing" {
            summary.syncing_count += 1;
        }
        if c.status == "error" {
            summary.error_count += 1;
        }
    }

    if configs.len() == 1 {
        let c = &configs[0];
        let sha = c.last_pushed_sha.as_deref().unwrap_or("empty");
        summary.display_text = format!(
            "[SHADOW {} @ {}]",
            c.target_branch,
            sha.chars().take(7).collect::<String>()
        );
        summary.upstream_status = c.upstream_status.clone();
        if let Some(url) = crate::settings::get().shadow.upstream_url.clone() {
            summary.upstream_url = Some(url);

            if let (Some(head), Some(up)) = (&c.last_pushed_sha, &c.upstream_last_pushed_sha) {
                if head == up {
                    summary.upstream_gap = Some(0);
                } else {
                    let repo_root = std::path::Path::new(&c.source_dir);
                    if let Ok(output) = std::process::Command::new("git")
                        .args(["rev-list", "--count", &format!("{}...{}", up, head)])
                        .current_dir(repo_root)
                        .output()
                    {
                        let s = String::from_utf8_lossy(&output.stdout);
                        if let Ok(gap) = s.trim().parse::<usize>() {
                            summary.upstream_gap = Some(gap);
                        } else {
                            summary.upstream_gap = Some(999);
                        }
                    } else {
                        summary.upstream_gap = Some(999);
                    }
                }
            } else if c.last_pushed_sha.is_some() {
                summary.upstream_gap = Some(999);
            }
        }
    } else {
        summary.display_text = format!(
            "[SHADOW {} enabled | {} syncing | {} error]",
            summary.enabled_count, summary.syncing_count, summary.error_count
        );
        summary.upstream_status = "unconfigured".into();
    }

    Ok(Some(summary))
}
