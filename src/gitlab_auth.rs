//! GitLab auth resolution for the local JeRyu-managed GitLab.
//!
//! The canonical local secret store is `~/.jeryu/jeryu.env`. Process
//! environment variables are still accepted for compatibility and for explicit
//! non-local GitLab URLs, but local GitLab credentials are normalized back into
//! `GITLAB_PAT` in the canonical env file.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::future::Future;

use crate::config;
use crate::env_file;
use crate::gitlab_client::GitlabClient;

const TOKEN_KEYS: [&str; 3] = ["GITLAB_PAT", "GITLAB_TOKEN", "PRIVATE_TOKEN"];
const CANONICAL_TOKEN_KEY: &str = "GITLAB_PAT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGitLabAuth {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenSource {
    File,
    Process,
}

/// Resolve the default GitLab URL used by the CLI and local adapters.
pub fn default_gitlab_url() -> String {
    if let Ok(value) = std::env::var("GITLAB_URL")
        && !value.is_empty()
    {
        return value;
    }
    if let Ok(value) = std::env::var("CI_SERVER_URL")
        && !value.is_empty()
    {
        return value;
    }
    format!("http://127.0.0.1:{}", config::GITLAB_HTTP_PORT)
}

/// True when a URL targets the local JeRyu-managed GitLab instance.
pub fn is_local_gitlab_url(url: &str) -> bool {
    let Some((host, port)) = host_port(url) else {
        return false;
    };
    let local_host = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]");
    local_host && port.is_none_or(|port| port == config::GITLAB_HTTP_PORT)
}

/// Load a token without validating or repairing it.
pub fn load_token_for_url(url: &str) -> Result<Option<String>> {
    let local = is_local_gitlab_url(url);
    if local && let Some(token) = read_env_file_token()? {
        return Ok(Some(token));
    }
    Ok(read_process_token())
}

/// Read a single key from the canonical env file without mutating process env.
pub fn load_env_value(key: &str) -> Result<Option<String>> {
    let path = config::env_file();
    Ok(match env_file::read_text_file_optional(&path, "reading")? {
        Some(text) => env_file::parse_env_text(&text).get(key).cloned(),
        None => None,
    })
}

/// Resolve local GitLab auth, repairing missing or invalid local tokens.
pub async fn resolve_or_repair_default() -> Result<ResolvedGitLabAuth> {
    let url = default_gitlab_url();
    resolve_or_repair(&url).await
}

/// Resolve auth for a specific URL, repairing only local GitLab URLs.
pub async fn resolve_or_repair(url: &str) -> Result<ResolvedGitLabAuth> {
    resolve_or_repair_with(url, |url| {
        let url = url.to_string();
        async move { mint_local_root_pat(&url).await }
    })
    .await
}

/// Test hook for proving repair behavior without invoking Docker.
pub async fn resolve_or_repair_with<F, Fut>(url: &str, repair: F) -> Result<ResolvedGitLabAuth>
where
    F: FnOnce(&str) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let local = is_local_gitlab_url(url);
    let existing = load_token_with_source(url)?;

    if !local {
        if let Some((token, _source)) = existing {
            return Ok(ResolvedGitLabAuth {
                url: url.to_string(),
                token,
            });
        }
        bail!("GitLab token not found for non-local GitLab URL");
    }

    if let Some((token, source)) = existing {
        if source == TokenSource::File || source == TokenSource::Process {
            upsert_pat(&token)?;
        }
        match validate_token(url, &token).await {
            Ok(true) => {
                return Ok(ResolvedGitLabAuth {
                    url: url.to_string(),
                    token,
                });
            }
            Ok(false) => {}
            Err(_) => {
                return Ok(ResolvedGitLabAuth {
                    url: url.to_string(),
                    token,
                });
            }
        }
    }

    let token = repair(url).await.context("repairing local GitLab PAT")?;
    upsert_pat(&token)?;
    Ok(ResolvedGitLabAuth {
        url: url.to_string(),
        token,
    })
}

/// Store or replace the canonical local PAT key.
pub fn upsert_pat(value: &str) -> Result<()> {
    upsert_env_value(CANONICAL_TOKEN_KEY, value)
}

/// Store or replace an env key in the canonical env file.
pub fn upsert_env_value(key: &str, value: &str) -> Result<()> {
    let path = config::env_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let existing = env_file::read_text_file_optional(&path, "reading")?.unwrap_or_default();
    let updated = env_file::upsert_env_text(&existing, key, value);
    env_file::write_text_file_secure(&path, &updated, "opening", "writing")?;
    Ok(())
}

/// Normalize canonical env permissions when the file exists.
pub fn ensure_env_file_permissions() -> Result<()> {
    let path = config::env_file();
    if path.exists() {
        env_file::normalize_file_permissions_0600(&path)?;
    }
    Ok(())
}

pub async fn mint_local_root_pat(_url: &str) -> Result<String> {
    let pat = format!("jeryu-pat-{}", crate::bootstrap::generate_password(20));

    let rails_script = format!(
        "u = User.find_by_username('root');\
         t = u.personal_access_tokens.create!(scopes: ['api', 'create_runner', 'manage_runner', 'read_repository', 'write_repository'], name: 'jeryu-control-plane', expires_at: 365.days.from_now);\
         t.set_token('{}');\
         t.save!",
        pat
    );

    let output = tokio::process::Command::new("docker")
        .args([
            "exec",
            "jeryu-gitlab",
            "gitlab-rails",
            "runner",
            &rails_script,
        ])
        .output()
        .await
        .context("running gitlab-rails runner to create PAT")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("failed to create PAT via gitlab-rails: {}", stderr);
    }

    Ok(pat)
}

fn load_token_with_source(url: &str) -> Result<Option<(String, TokenSource)>> {
    if is_local_gitlab_url(url)
        && let Some(token) = read_env_file_token()?
    {
        return Ok(Some((token, TokenSource::File)));
    }
    Ok(read_process_token().map(|token| (token, TokenSource::Process)))
}

fn read_env_file_token() -> Result<Option<String>> {
    let path = config::env_file();
    Ok(match env_file::read_text_file_optional(&path, "reading")? {
        Some(text) => {
            let values = env_file::parse_env_text(&text);
            TOKEN_KEYS
                .iter()
                .find_map(|key| values.get(*key).filter(|v| !v.is_empty()).cloned())
        }
        None => None,
    })
}

fn read_process_token() -> Option<String> {
    TOKEN_KEYS
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
}

#[derive(Deserialize)]
struct GitLabUserProbe {
    id: serde_json::Value,
}

async fn validate_token(url: &str, token: &str) -> Result<bool> {
    let client = GitlabClient::new(url, Some(token.to_string()));
    let result: Result<GitLabUserProbe> = client.api_get_json(client.api_url("/user")).await;
    match result {
        Ok(user) => Ok(!user.id.is_null()),
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("401") || msg.contains("403") {
                Ok(false)
            } else {
                Err(err)
            }
        }
    }
}

fn host_port(url: &str) -> Option<(String, Option<u16>)> {
    let trimmed = url.trim();
    let without_scheme = trimmed.split_once("://").map_or(trimmed, |(_, rest)| rest);
    let without_user = without_scheme
        .rsplit_once('@')
        .map_or(without_scheme, |(_, rest)| rest);
    let authority = without_user.split('/').next()?.trim();
    if authority.is_empty() {
        return None;
    }
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        let host = authority[..=end].to_ascii_lowercase();
        let port = authority[end + 1..]
            .strip_prefix(':')
            .and_then(|raw| raw.parse::<u16>().ok());
        return Some((host, port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, raw_port)) if raw_port.chars().all(|ch| ch.is_ascii_digit()) => {
            (host, raw_port.parse::<u16>().ok())
        }
        _ => (authority, None),
    };
    Some((host.to_ascii_lowercase(), port))
}

#[cfg(test)]
#[path = "gitlab_auth_tests.rs"]
mod gitlab_auth_tests;
