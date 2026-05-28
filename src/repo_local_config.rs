use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::LocalRepoConfig;

pub(super) fn load_configs() -> Result<Vec<LocalRepoConfig>> {
    let mut configs = Vec::new();
    let mut seen = HashSet::new();

    for dir in config_dirs() {
        if !dir.is_dir() {
            continue;
        }
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
            if seen.insert(config.repo.clone()) {
                configs.push(config);
            }
        }
    }
    configs.sort_by(|left, right| left.repo.cmp(&right.repo));
    Ok(configs)
}

pub(super) fn matching_configs(repo: Option<&str>) -> Result<Vec<LocalRepoConfig>> {
    let configs = load_configs()?;
    Ok(match repo {
        Some(repo) => configs
            .into_iter()
            .filter(|config| config.repo == repo)
            .collect(),
        None => configs,
    })
}

pub(super) fn config_dirs() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("JERYU_LOCAL_REPO_CONFIG_DIR") {
        return vec![PathBuf::from(path)];
    }

    let mut dirs = Vec::new();
    for path in discover_config_dirs() {
        push_unique_dir(&mut dirs, path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        push_unique_dir(&mut dirs, PathBuf::from(home).join(".jeryu/local/repos"));
    }
    if dirs.is_empty() {
        dirs.push(
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".jeryu/local/repos"),
        );
    }
    dirs
}

fn discover_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in [std::env::current_dir().ok(), std::env::current_exe().ok()]
        .into_iter()
        .flatten()
    {
        for ancestor in root.ancestors() {
            let candidate = ancestor.join(".jeryu/local/repos");
            if candidate.is_dir() {
                push_unique_dir(&mut dirs, candidate);
            }
        }
    }
    dirs
}

fn push_unique_dir(dirs: &mut Vec<PathBuf>, path: PathBuf) {
    if !dirs.iter().any(|existing| existing == &path) {
        dirs.push(path);
    }
}

pub(super) fn shadow_ref_matches(config: &LocalRepoConfig, ref_name: &str) -> bool {
    let normalized = if ref_name.starts_with("refs/") {
        ref_name.to_string()
    } else {
        format!("refs/heads/{ref_name}")
    };
    shadow_refs(config).iter().any(|r| r == &normalized)
}

pub(super) fn shadow_refs(config: &LocalRepoConfig) -> Vec<String> {
    if config.shadow_main.refs.is_empty() {
        vec![format!("refs/heads/{}", config.default_branch)]
    } else {
        config.shadow_main.refs.clone()
    }
}

pub(super) fn mirror_path(config: &LocalRepoConfig) -> PathBuf {
    crate::config::data_dir()
        .join("repo-mirrors")
        .join(format!("{}.git", safe_repo_component(&config.repo)))
}

pub(super) fn safe_repo_component(repo: &str) -> String {
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

pub(super) fn local_gitlab_ssh_url(repo: &str) -> String {
    format!(
        "ssh://git@127.0.0.1:2224/{}.git",
        repo.trim_matches('/').trim_end_matches(".git")
    )
}

pub(super) fn is_zero_sha(value: &str) -> bool {
    value.chars().all(|ch| ch == '0')
}

pub(super) fn path_with_trailing_slash(path: &Path) -> String {
    let mut value = path.display().to_string();
    if !value.ends_with('/') {
        value.push('/');
    }
    value
}

#[derive(Debug)]
pub(super) enum BackupTarget {
    Remote { host: String, path: String },
    Local(PathBuf),
}

pub(super) fn parse_backup_target(value: &str) -> Result<BackupTarget> {
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
