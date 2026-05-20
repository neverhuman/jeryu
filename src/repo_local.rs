//! Owner: Repo-local sidecar worker
//! Proof: `cargo test -p jeryu --lib repo_local`
//! Invariants: Repo sidecars are config-driven, non-blocking for local GitLab merges, and secret-free in repo policy.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::git::system::SystemGit;
use crate::redact::redact_text;
use crate::state::{Db, GitMirrorJob};

#[derive(Debug, Clone, Deserialize)]
pub struct LocalRepoConfig {
    #[serde(skip)]
    pub source_path: PathBuf,
    pub repo: String,
    pub default_branch: String,
    #[serde(default)]
    pub shadow_main: ShadowMainConfig,
    #[serde(default)]
    pub backup: BackupConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShadowMainConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub remote_url: String,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BackupConfig {
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct SidecarRun {
    pub repo: String,
    pub status: String,
    pub detail: String,
}

pub async fn shadow_main_command(repo: Option<String>) -> Result<i32> {
    let db = Db::open().await?;
    let configs = matching_configs(repo.as_deref())?;
    if configs.is_empty() {
        bail!("no local repo config matched");
    }

    for config in configs {
        let run = run_shadow_main(&db, &config, "manual").await;
        println!("{} shadow_main: {} {}", run.repo, run.status, run.detail);
    }
    Ok(0)
}

pub async fn backup_command(repo: Option<String>) -> Result<i32> {
    let db = Db::open().await?;
    let configs = matching_configs(repo.as_deref())?;
    if configs.is_empty() {
        bail!("no local repo config matched");
    }

    for config in configs {
        let run = run_backup(&db, &config, "manual").await;
        println!("{} backup: {} {}", run.repo, run.status, run.detail);
    }
    Ok(0)
}

pub async fn shadow_main_for_push(db: &Db, repo: Option<&str>, ref_name: &str, after_sha: &str) {
    if is_zero_sha(after_sha) {
        return;
    }

    let Ok(configs) = matching_configs(repo) else {
        tracing::warn!("repo-local shadow config load failed");
        return;
    };

    for config in configs {
        if shadow_ref_matches(&config, ref_name) {
            let _ = run_shadow_main(db, &config, "webhook").await;
        }
    }
}

pub async fn reconcile_repo_sidecars(db: &Db) -> Result<Vec<SidecarRun>> {
    let mut runs = Vec::new();
    for config in load_configs()? {
        if config.shadow_main.enabled {
            runs.push(run_shadow_main(db, &config, "reconcile").await);
        }
        if !config.backup.target.trim().is_empty() {
            runs.push(run_backup(db, &config, "reconcile").await);
        }
    }
    Ok(runs)
}

async fn run_shadow_main(db: &Db, config: &LocalRepoConfig, trigger: &str) -> SidecarRun {
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

async fn run_backup(db: &Db, config: &LocalRepoConfig, trigger: &str) -> SidecarRun {
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

fn refresh_bare_mirror(config: &LocalRepoConfig) -> Result<PathBuf> {
    let mirror = mirror_path(config);
    let local_url = local_gitlab_ssh_url(&config.repo);
    if mirror.is_dir() {
        run_git_checked(&mirror, &["remote", "set-url", "origin", &local_url])?;
        run_git_checked(
            &mirror,
            &[
                "fetch",
                "--prune",
                "origin",
                "+refs/heads/*:refs/heads/*",
                "+refs/tags/*:refs/tags/*",
            ],
        )
        .with_context(|| format!("fetching {}", redact_text(&local_url)))?;
    } else {
        let parent = mirror
            .parent()
            .ok_or_else(|| anyhow::anyhow!("mirror path has no parent: {}", mirror.display()))?;
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        run_git_checked(
            parent,
            &[
                "clone",
                "--mirror",
                &local_url,
                mirror.to_string_lossy().as_ref(),
            ],
        )
        .with_context(|| format!("cloning local GitLab mirror for {}", config.repo))?;
    }
    Ok(mirror)
}

fn run_git_checked(cwd: &Path, args: &[&str]) -> Result<Output> {
    let git = SystemGit::resolve()?;
    run_checked(Command::new(git.path).current_dir(cwd).args(args))
        .with_context(|| format!("running git {:?}", args))
}

fn run_checked(command: &mut Command) -> Result<Output> {
    let output = command.output().context("spawning command")?;
    if output.status.success() {
        return Ok(output);
    }
    bail!(
        "command failed: {}",
        redact_text(&String::from_utf8_lossy(&output.stderr))
    )
}

fn load_configs() -> Result<Vec<LocalRepoConfig>> {
    let dir = config_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut configs = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let content =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut config: LocalRepoConfig =
            toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
        config.source_path = path;
        configs.push(config);
    }
    configs.sort_by(|left, right| left.repo.cmp(&right.repo));
    Ok(configs)
}

fn matching_configs(repo: Option<&str>) -> Result<Vec<LocalRepoConfig>> {
    let configs = load_configs()?;
    Ok(match repo {
        Some(repo) => configs
            .into_iter()
            .filter(|config| config.repo == repo)
            .collect(),
        None => configs,
    })
}

fn config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("JERYU_LOCAL_REPO_CONFIG_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = discover_config_dir() {
        return path;
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".jeryu/local/repos")
}

fn discover_config_dir() -> Option<PathBuf> {
    for root in [std::env::current_dir().ok(), std::env::current_exe().ok()]
        .into_iter()
        .flatten()
    {
        for ancestor in root.ancestors() {
            let candidate = ancestor.join(".jeryu/local/repos");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

fn shadow_ref_matches(config: &LocalRepoConfig, ref_name: &str) -> bool {
    let normalized = if ref_name.starts_with("refs/") {
        ref_name.to_string()
    } else {
        format!("refs/heads/{ref_name}")
    };
    shadow_refs(config).iter().any(|r| r == &normalized)
}

fn shadow_refs(config: &LocalRepoConfig) -> Vec<String> {
    if config.shadow_main.refs.is_empty() {
        vec![format!("refs/heads/{}", config.default_branch)]
    } else {
        config.shadow_main.refs.clone()
    }
}

fn mirror_path(config: &LocalRepoConfig) -> PathBuf {
    crate::config::data_dir()
        .join("repo-mirrors")
        .join(format!("{}.git", safe_repo_component(&config.repo)))
}

fn safe_repo_component(repo: &str) -> String {
    repo.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn local_gitlab_ssh_url(repo: &str) -> String {
    format!(
        "ssh://git@127.0.0.1:2224/{}.git",
        repo.trim_matches('/').trim_end_matches(".git")
    )
}

fn is_zero_sha(value: &str) -> bool {
    value.chars().all(|ch| ch == '0')
}

fn path_with_trailing_slash(path: &Path) -> String {
    let mut value = path.display().to_string();
    if !value.ends_with('/') {
        value.push('/');
    }
    value
}

enum BackupTarget {
    Remote { host: String, path: String },
    Local(PathBuf),
}

fn parse_backup_target(value: &str) -> Result<BackupTarget> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("backup target is empty");
    }
    if let Some((host, path)) = trimmed.split_once(':')
        && !host.contains('/')
        && !path.is_empty()
    {
        return Ok(BackupTarget::Remote {
            host: host.to_string(),
            path: path.to_string(),
        });
    }
    Ok(BackupTarget::Local(PathBuf::from(trimmed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> LocalRepoConfig {
        LocalRepoConfig {
            source_path: PathBuf::from("root-veox.toml"),
            repo: "root/veox".into(),
            default_branch: "main".into(),
            shadow_main: ShadowMainConfig {
                enabled: true,
                remote_url: "git@github.com:neverhuman/warp.git".into(),
                refs: vec!["refs/heads/main".into()],
            },
            backup: BackupConfig {
                target: "xbabe3:/home/ubuntu/jeryu-backups/veox".into(),
            },
        }
    }

    #[test]
    fn parses_local_repo_config_shape() {
        let mut parsed: LocalRepoConfig = toml::from_str(
            r#"
repo = "root/veox"
default_branch = "main"

[shadow_main]
enabled = true
remote_url = "git@github.com:neverhuman/warp.git"
refs = ["refs/heads/main"]

[backup]
target = "xbabe3:/home/ubuntu/jeryu-backups/veox"
"#,
        )
        .unwrap();
        parsed.source_path = PathBuf::from("root-veox.toml");
        assert_eq!(parsed.repo, "root/veox");
        assert!(parsed.shadow_main.enabled);
        assert_eq!(shadow_refs(&parsed), vec!["refs/heads/main"]);
        assert!(matches!(
            parse_backup_target(&parsed.backup.target).unwrap(),
            BackupTarget::Remote { .. }
        ));
    }

    #[test]
    fn shadow_ref_matching_accepts_raw_or_full_refs() {
        let config = config();
        assert!(shadow_ref_matches(&config, "main"));
        assert!(shadow_ref_matches(&config, "refs/heads/main"));
        assert!(!shadow_ref_matches(&config, "refs/heads/feature"));
    }

    #[test]
    fn repo_identifiers_are_safe_for_mirror_paths() {
        assert_eq!(safe_repo_component("root/veox"), "root-veox");
        assert_eq!(safe_repo_component("team/redlineDB"), "team-redlineDB");
    }
}
