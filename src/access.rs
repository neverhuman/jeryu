//! Owner: Access contract, repo repair, and local GitLab host detection.
//! Proof: `cargo test -p jeryu --lib access`
//! Invariants: access contract data is secret-free; local GitLab repairs stay on SSH and never scrape credential stores.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::gitlab_auth;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessContract {
    pub schema_version: String,
    pub local_api_url: String,
    pub canonical_git_remote_template: String,
    pub secret_source: String,
    pub secret_key: String,
    #[serde(default)]
    pub remote_keys: Vec<AccessRemoteKey>,
    #[serde(default)]
    pub forbidden_agent_paths: Vec<String>,
    #[serde(default)]
    pub known_repos: Vec<AccessRepoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessRemoteKey {
    pub alias: String,
    pub fingerprint: String,
    pub identity_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessRepoEntry {
    pub path: String,
    pub namespace: String,
    pub name: String,
    #[serde(default = "default_branch")]
    pub default_branch: String,
    #[serde(default = "default_protect_main")]
    pub protect_main: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessFinding {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessDoctorReport {
    pub contract_path: String,
    pub repo_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_origin_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    pub known_repo: bool,
    pub findings: Vec<AccessFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessRepairReport {
    pub contract_path: String,
    pub repo_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_after: Option<String>,
    pub written_files: Vec<String>,
    pub credential_cleanup: bool,
    pub findings: Vec<AccessFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessProjectReport {
    pub contract_path: String,
    pub requested: String,
    pub repo_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_url: Option<String>,
    pub project_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    pub findings: Vec<AccessFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessKeyStatus {
    pub alias: String,
    pub fingerprint: String,
    pub identity_path: String,
    pub exists: bool,
    pub readable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessKeysReport {
    pub contract_path: String,
    pub keys: Vec<AccessKeyStatus>,
    pub findings: Vec<AccessFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKind {
    LocalGitLabHttp,
    LocalGitLabSsh,
    GitHub,
    Other,
}

pub fn contract_path() -> PathBuf {
    crate::config::data_dir().join("access.toml")
}

pub fn load_contract() -> Result<AccessContract> {
    let path = contract_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(default_contract());
        }
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn ensure_contract_file() -> Result<AccessContract> {
    let contract = load_contract()?;
    write_contract(&contract)?;
    Ok(contract)
}

pub fn write_contract(contract: &AccessContract) -> Result<()> {
    let path = contract_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let rendered = toml::to_string_pretty(contract)?.trim_end().to_string() + "\n";
    fs::write(&path, rendered).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn default_contract() -> AccessContract {
    AccessContract {
        schema_version: "1".to_string(),
        local_api_url: format!("http://127.0.0.1:{}", crate::config::GITLAB_HTTP_PORT),
        canonical_git_remote_template: format!(
            "ssh://git@127.0.0.1:{}/root/{{repo}}.git",
            crate::config::GITLAB_SSH_PORT
        ),
        secret_source: "~/.jeryu/jeryu.env".to_string(),
        secret_key: "GITLAB_PAT".to_string(),
        remote_keys: vec![
            AccessRemoteKey {
                alias: "local-gitlab".to_string(),
                fingerprint: "SHA256:+FbqtZF+hPgrjJRh5Oq5gNUKUmtCykur2ZEUKAFRv+Y".to_string(),
                identity_path: "/home/ubuntu/.ssh/id_ed25519".to_string(),
            },
            AccessRemoteKey {
                alias: "github".to_string(),
                fingerprint: "SHA256:sgz/E2Tb2it/lw71+YABvJXmGEwf25EaEa+y4OurjZI".to_string(),
                identity_path: "/home/ubuntu/.ssh/id_ed25519_github".to_string(),
            },
        ],
        forbidden_agent_paths: vec![
            "glab".to_string(),
            "manual GitLab curl".to_string(),
            "~/.git-credentials hunting".to_string(),
            "HTTP local GitLab origins".to_string(),
        ],
        known_repos: default_known_repos(),
    }
}

pub fn default_known_repos() -> Vec<AccessRepoEntry> {
    let mut repos = Vec::new();
    for path in [
        "/home/ubuntu/jeryu",
        "/home/ubuntu/jekko",
        "/home/ubuntu/jankurai",
        "/home/ubuntu/jansu",
        "/home/ubuntu/jnoccio",
        "/home/ubuntu/redlineDB",
        "/home/ubuntu/redline-testing",
    ] {
        let name = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repo")
            .to_string();
        repos.push(AccessRepoEntry {
            path: path.to_string(),
            namespace: "root".to_string(),
            name,
            default_branch: default_branch(),
            protect_main: true,
        });
    }

    for name in [
        "veox-deploy",
        "veox-docs-meta",
        "veox-enclave",
        "veox-neverhuman-data",
        "veox-nht",
        "veox-proofs",
        "veox-shared",
        "veox-stage-catalog",
        "veox-warp",
    ] {
        repos.push(AccessRepoEntry {
            path: format!("/home/ubuntu/veox-repos/{name}"),
            namespace: "root".to_string(),
            name: name.to_string(),
            default_branch: default_branch(),
            protect_main: true,
        });
        repos.push(AccessRepoEntry {
            path: format!("/home/ubuntu/veox-split/{name}"),
            namespace: "root".to_string(),
            name: name.to_string(),
            default_branch: default_branch(),
            protect_main: true,
        });
    }

    repos
}

pub fn known_repo_entries(contract: &AccessContract) -> &[AccessRepoEntry] {
    &contract.known_repos
}

pub fn repo_entry_for_path<'a>(
    contract: &'a AccessContract,
    repo_path: &Path,
) -> Option<&'a AccessRepoEntry> {
    let canonical = repo_path.canonicalize().ok()?;
    contract
        .known_repos
        .iter()
        .find(|entry| Path::new(&entry.path).canonicalize().ok().as_ref() == Some(&canonical))
}

pub fn canonical_git_remote(contract: &AccessContract, name: &str) -> String {
    contract
        .canonical_git_remote_template
        .replace("{repo}", name.trim_end_matches(".git"))
}

pub fn classify_remote(url: &str) -> RemoteKind {
    if is_local_gitlab_http_url(url) {
        RemoteKind::LocalGitLabHttp
    } else if is_local_gitlab_ssh_remote(url) {
        RemoteKind::LocalGitLabSsh
    } else if url.contains("github.com") {
        RemoteKind::GitHub
    } else {
        RemoteKind::Other
    }
}

pub fn is_local_gitlab_http_url(url: &str) -> bool {
    crate::gitlab_auth::is_local_gitlab_url(url)
}

pub fn is_local_gitlab_ssh_remote(remote: &str) -> bool {
    let Some((host, port)) = host_port(remote) else {
        return false;
    };
    let local_host = matches!(
        host.as_str(),
        "127.0.0.1" | "localhost" | "::1" | "[::1]" | crate::config::GITLAB_HOSTNAME
    );
    local_host
        && port.is_none_or(|port| {
            port == crate::config::GITLAB_SSH_PORT || port == crate::config::GITLAB_HTTP_PORT
        })
}

pub fn repo_slug_from_remote(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches(".git");
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("git@") {
        let (_, path) = rest.split_once(':')?;
        return slug_from_path(path);
    }
    if let Some((_, path)) = trimmed.split_once("://") {
        let mut parts = path.split('/').collect::<Vec<_>>();
        if parts.len() >= 2 {
            let repo = parts.pop()?;
            let owner = parts.pop()?;
            return slug_from_path(&format!("{owner}/{repo}"));
        }
    }
    slug_from_path(trimmed)
}

pub fn repo_slug_from_path(path: &Path) -> Option<String> {
    let remote = git_remote_url(path, "origin").ok().flatten()?;
    repo_slug_from_remote(&remote)
}

pub fn repo_slug_or_name(repo_path: &Path, entry: Option<&AccessRepoEntry>) -> Option<String> {
    repo_slug_from_path(repo_path)
        .or_else(|| entry.map(|entry| format!("{}/{}", entry.namespace, entry.name)))
}

pub fn repo_project_path(repo_path: &Path, entry: Option<&AccessRepoEntry>) -> Option<String> {
    repo_slug_or_name(repo_path, entry)
}

pub fn git_repo_root(repo_path: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .with_context(|| format!("running git rev-parse in {}", repo_path.display()))?;
    if !output.status.success() {
        bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

pub fn git_remote_url(repo_path: &Path, remote: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "get-url", remote])
        .output()
        .with_context(|| format!("reading git remote {remote} in {}", repo_path.display()))?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

pub fn git_remote_exists(repo_path: &Path, remote: &str) -> Result<bool> {
    Ok(git_remote_url(repo_path, remote)?.is_some())
}

pub fn git_remote_set_url(repo_path: &Path, remote: &str, url: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "set-url", remote, url])
        .status()
        .with_context(|| format!("running git remote set-url {remote}"))?;
    if !status.success() {
        bail!("git remote set-url {remote} failed");
    }
    Ok(())
}

pub fn git_remote_add(repo_path: &Path, remote: &str, url: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "add", remote, url])
        .status()
        .with_context(|| format!("running git remote add {remote}"))?;
    if !status.success() {
        bail!("git remote add {remote} failed");
    }
    Ok(())
}

pub fn git_remote_get_or_add(repo_path: &Path, remote: &str, url: &str) -> Result<()> {
    if git_remote_exists(repo_path, remote)? {
        git_remote_set_url(repo_path, remote, url)
    } else {
        git_remote_add(repo_path, remote, url)
    }
}

pub fn git_config_set(repo_path: &Path, key: &str, value: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["config", "--local", key, value])
        .status()
        .with_context(|| format!("running git config {key}"))?;
    if !status.success() {
        bail!("git config {key} failed");
    }
    Ok(())
}

pub fn git_config_get(repo_path: &Path, key: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["config", "--local", "--get", key])
        .output()
        .with_context(|| format!("reading git config {key}"))?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

pub fn git_remote_rename(repo_path: &Path, previous_remote: &str, new: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "rename", previous_remote, new])
        .status()
        .with_context(|| format!("running git remote rename {previous_remote} {new}"))?;
    if !status.success() {
        bail!("git remote rename {previous_remote} {new} failed");
    }
    Ok(())
}

pub fn git_branch_current(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .with_context(|| format!("reading current branch in {}", repo_path.display()))?;
    if !output.status.success() {
        bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        bail!(
            "could not determine current branch in {}",
            repo_path.display()
        );
    }
    Ok(branch)
}

pub fn git_push_branch(repo_path: &Path, remote: &str, branch: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["push", "--set-upstream", remote, branch])
        .status()
        .with_context(|| format!("pushing {branch} to {remote} from {}", repo_path.display()))?;
    if !status.success() {
        bail!("git push {remote} {branch} failed");
    }
    Ok(())
}

pub fn git_push_head(repo_path: &Path, remote: &str, branch: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["push", "--set-upstream", remote, &format!("HEAD:{branch}")])
        .status()
        .with_context(|| format!("pushing HEAD to {branch} from {}", repo_path.display()))?;
    if !status.success() {
        bail!("git push {remote} HEAD:{branch} failed");
    }
    Ok(())
}

pub fn git_is_repo(repo_path: &Path) -> bool {
    git_repo_root(repo_path).is_ok()
}

pub fn repo_is_local_gitlab(repo_path: &Path) -> Result<bool> {
    let origin = git_remote_url(repo_path, "origin")?;
    Ok(matches!(
        origin.as_deref().map(classify_remote),
        Some(RemoteKind::LocalGitLabHttp) | Some(RemoteKind::LocalGitLabSsh)
    ))
}

pub fn repo_origin_is_local_http(repo_path: &Path) -> Result<bool> {
    let origin = git_remote_url(repo_path, "origin")?;
    Ok(matches!(
        origin.as_deref().map(classify_remote),
        Some(RemoteKind::LocalGitLabHttp)
    ))
}

pub fn access_findings_for_repo(
    contract: &AccessContract,
    repo_path: &Path,
) -> Result<AccessDoctorReport> {
    let repo_root = git_repo_root(repo_path).unwrap_or_else(|_| repo_path.to_path_buf());
    let repo_root_text = repo_root.display().to_string();
    let entry = repo_entry_for_path(contract, &repo_root);
    let origin_url = git_remote_url(&repo_root, "origin")?;
    let expected_origin_url = entry
        .as_ref()
        .map(|entry| canonical_git_remote(contract, &entry.name));
    let repo_slug = repo_slug_or_name(&repo_root, entry);
    let mut findings = Vec::new();

    if entry.is_none() {
        findings.push(AccessFinding {
            code: "missing_known_repo_entry".to_string(),
            severity: "warning".to_string(),
            message: "repo is not listed in ~/.jeryu/access.toml".to_string(),
            repair_hint: Some("add the checkout to ~/.jeryu/access.toml".to_string()),
        });
    }

    match origin_url.as_deref().map(classify_remote) {
        Some(RemoteKind::LocalGitLabHttp) => findings.push(AccessFinding {
            code: "local_gitlab_http_origin".to_string(),
            severity: "error".to_string(),
            message: "origin still points at local GitLab over HTTP".to_string(),
            repair_hint: Some("jeryu access repair --repo . --yes".to_string()),
        }),
        Some(RemoteKind::LocalGitLabSsh) => {}
        Some(RemoteKind::GitHub) | Some(RemoteKind::Other) => findings.push(AccessFinding {
            code: "noncanonical_origin".to_string(),
            severity: "error".to_string(),
            message: "origin is not the canonical local GitLab SSH remote".to_string(),
            repair_hint: Some("jeryu access repair --repo . --yes".to_string()),
        }),
        None => findings.push(AccessFinding {
            code: "missing_origin".to_string(),
            severity: "error".to_string(),
            message: "origin remote is missing".to_string(),
            repair_hint: Some("jeryu access repair --repo . --yes".to_string()),
        }),
    }

    for needed in ["repo.toml", "policy.toml", "ci.toml"] {
        if !repo_root.join(".jeryu").join(needed).is_file() {
            findings.push(AccessFinding {
                code: format!("missing_{}", needed.replace('.', "_")),
                severity: "error".to_string(),
                message: format!(".jeryu/{needed} is missing"),
                repair_hint: Some("jeryu access repair --repo . --yes".to_string()),
            });
        }
    }

    if gitlab_auth::load_token_for_url(&contract.local_api_url)?.is_none() {
        findings.push(AccessFinding {
            code: "missing_gitlab_pat".to_string(),
            severity: "error".to_string(),
            message: "no local GitLab PAT is configured in ~/.jeryu/jeryu.env".to_string(),
            repair_hint: Some("jeryu bootstrap".to_string()),
        });
    }

    if let Some(origin) = &origin_url
        && is_local_gitlab_http_url(origin)
    {
        findings.push(AccessFinding {
            code: "forbidden_http_origin".to_string(),
            severity: "error".to_string(),
            message: "local GitLab HTTP origins are forbidden for agent-facing workspaces"
                .to_string(),
            repair_hint: Some("jeryu access repair --repo . --yes".to_string()),
        });
    }

    Ok(AccessDoctorReport {
        contract_path: contract_path().display().to_string(),
        repo_path: repo_root_text,
        repo_slug,
        origin_url,
        expected_origin_url,
        project_path: entry.map(|entry| format!("{}/{}", entry.namespace, entry.name)),
        project_id: None,
        web_url: None,
        known_repo: entry.is_some(),
        findings,
    })
}

pub fn repair_repo_layout(
    contract: &AccessContract,
    repo_path: &Path,
) -> Result<AccessRepairReport> {
    let repo_root = git_repo_root(repo_path)
        .with_context(|| format!("{} is not a git checkout", repo_path.display()))?;
    let entry = repo_entry_for_path(contract, &repo_root)
        .cloned()
        .unwrap_or_else(|| AccessRepoEntry {
            path: repo_root.display().to_string(),
            namespace: "root".to_string(),
            name: repo_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repo")
                .to_string(),
            default_branch: default_branch(),
            protect_main: true,
        });
    let mut findings = Vec::new();
    let origin_before = git_remote_url(&repo_root, "origin")?;
    let canonical_origin = canonical_git_remote(contract, &entry.name);
    let mut origin_after = origin_before.clone();
    let backup_remote_name = match origin_before.as_deref().map(classify_remote) {
        Some(RemoteKind::GitHub) => Some("github"),
        Some(RemoteKind::Other) => Some("mirror"),
        _ => None,
    };

    if let Some(existing) = &origin_before
        && !matches!(classify_remote(existing), RemoteKind::LocalGitLabSsh)
    {
        if let Some(name) = backup_remote_name {
            if git_remote_exists(&repo_root, name)? {
                git_remote_set_url(&repo_root, name, existing)?;
            } else {
                git_remote_add(&repo_root, name, existing)?;
            }
        }
        findings.push(AccessFinding {
            code: "origin_repaired".to_string(),
            severity: "warning".to_string(),
            message: format!("origin updated from {existing} to canonical local GitLab SSH"),
            repair_hint: Some("jeryu access doctor --repo . --json".to_string()),
        });
    }

    if origin_before.as_deref() != Some(canonical_origin.as_str()) {
        if git_remote_exists(&repo_root, "origin")? {
            git_remote_set_url(&repo_root, "origin", &canonical_origin)?;
        } else {
            git_remote_add(&repo_root, "origin", &canonical_origin)?;
        }
        origin_after = Some(canonical_origin.clone());
    }

    let written_files = write_repo_metadata(&repo_root, &entry)?;
    if entry.protect_main {
        findings.push(AccessFinding {
            code: "hooks_enforced".to_string(),
            severity: "info".to_string(),
            message: "protected-main pre-push hook configured".to_string(),
            repair_hint: None,
        });
    }

    Ok(AccessRepairReport {
        contract_path: contract_path().display().to_string(),
        repo_path: repo_root.display().to_string(),
        repo_slug: Some(format!("{}/{}", entry.namespace, entry.name)),
        origin_before,
        origin_after,
        written_files,
        credential_cleanup: false,
        findings,
    })
}

pub async fn resolve_project_report(
    contract: &AccessContract,
    repo_path: &Path,
    requested: Option<&str>,
    client: &crate::gitlab_client::GitlabClient,
) -> Result<AccessProjectReport> {
    let repo_root = git_repo_root(repo_path).unwrap_or_else(|_| repo_path.to_path_buf());
    let entry = repo_entry_for_path(contract, &repo_root);
    let origin_url = git_remote_url(&repo_root, "origin")?;
    let project_path = requested
        .map(str::to_string)
        .or_else(|| repo_slug_or_name(&repo_root, entry))
        .ok_or_else(|| anyhow::anyhow!("could not determine project path"))?;
    let mut findings = Vec::new();
    let mut project_id = None;
    let mut web_url = None;
    match client.get_project_by_path(&project_path).await {
        Ok(project) => {
            if project.path_with_namespace != project_path {
                findings.push(AccessFinding {
                    code: "project_path_mismatch".to_string(),
                    severity: "warning".to_string(),
                    message: format!(
                        "GitLab resolved {project_path} to {}",
                        project.path_with_namespace
                    ),
                    repair_hint: Some("repair the repo's namespace/name mapping".to_string()),
                });
            }
            project_id = Some(project.id);
            web_url = Some(project.web_url);
        }
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("502") || msg.contains("503") {
                findings.push(AccessFinding {
                    code: "gitlab_transient".to_string(),
                    severity: "warning".to_string(),
                    message: "GitLab returned a transient 502/503 while resolving the project"
                        .to_string(),
                    repair_hint: Some("retry after local GitLab recovers".to_string()),
                });
            } else {
                findings.push(AccessFinding {
                    code: "project_resolution_failed".to_string(),
                    severity: "error".to_string(),
                    message: format!("could not resolve project {project_path}: {msg}"),
                    repair_hint: Some("jeryu access repair --repo . --yes".to_string()),
                });
            }
        }
    }

    Ok(AccessProjectReport {
        contract_path: contract_path().display().to_string(),
        requested: requested.unwrap_or(&project_path).to_string(),
        repo_path: repo_root.display().to_string(),
        origin_url,
        project_path,
        project_id,
        web_url,
        findings,
    })
}

pub fn build_keys_report(contract: &AccessContract) -> AccessKeysReport {
    let keys = contract
        .remote_keys
        .iter()
        .map(|key| {
            let path = Path::new(&key.identity_path);
            AccessKeyStatus {
                alias: key.alias.clone(),
                fingerprint: key.fingerprint.clone(),
                identity_path: key.identity_path.clone(),
                exists: path.exists(),
                readable: fs::File::open(path).is_ok(),
            }
        })
        .collect::<Vec<_>>();
    let findings = keys
        .iter()
        .filter_map(|key| {
            if !key.exists {
                Some(AccessFinding {
                    code: format!("missing_key_{}", key.alias),
                    severity: "error".to_string(),
                    message: format!("identity file missing: {}", key.identity_path),
                    repair_hint: None,
                })
            } else if !key.readable {
                Some(AccessFinding {
                    code: format!("unreadable_key_{}", key.alias),
                    severity: "warning".to_string(),
                    message: format!("identity file is not readable: {}", key.identity_path),
                    repair_hint: None,
                })
            } else {
                None
            }
        })
        .collect();
    AccessKeysReport {
        contract_path: contract_path().display().to_string(),
        keys,
        findings,
    }
}

pub fn write_repo_metadata(repo_root: &Path, entry: &AccessRepoEntry) -> Result<Vec<String>> {
    let mut written = Vec::new();
    let dir = repo_root.join(".jeryu");
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let remote_url = canonical_remote_for_entry(entry);

    write_text_if_changed(
        &dir.join("repo.toml"),
        &format!(
            "schema_version = \"1\"\nmode = \"direct\"\nnamespace = \"{}\"\nname = \"{}\"\ndefault_branch = \"{}\"\nremote_url = \"{}\"\n",
            entry.namespace, entry.name, entry.default_branch, remote_url
        ),
        &mut written,
    )?;
    write_text_if_changed(
        &dir.join("policy.toml"),
        &format!(
            "schema_version = \"1\"\nprotect_main = {}\nprotected_branches = [\"{}\"]\nprotected_tags = [\"v*\"]\nhooks = \"enforce\"\n\n[main_relay]\nenabled = {}\nactor = \"jeryu\"\nprotected_branch = \"{}\"\nrequire_admission_receipt = true\n\n[offline_release_mirror]\nenabled = false\nremote = \"\"\nrefs = [\"refs/tags/v*\", \"refs/heads/release/*\"]\n",
            entry.protect_main, entry.default_branch, entry.protect_main, entry.default_branch
        ),
        &mut written,
    )?;
    write_text_if_changed(
        &dir.join("ci.toml"),
        "schema_version = \"1\"\ngithub_actions_required = false\nlocal_gitlab_required = true\n",
        &mut written,
    )?;
    if entry.protect_main {
        write_text_if_changed(
            &dir.join("hooks").join("pre-push"),
            &render_pre_push_hook(&entry.default_branch),
            &mut written,
        )?;
        git_config_set(repo_root, "core.hooksPath", ".jeryu/hooks")?;
    }
    Ok(written)
}

pub fn cleanup_local_http_gitlab_credentials() -> Result<bool> {
    let credential_path = dirs::home_dir()
        .map(|home| home.join(".git-credentials"))
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    let text = match fs::read_to_string(&credential_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(err).with_context(|| format!("reading {}", credential_path.display()));
        }
    };
    let filtered = text
        .lines()
        .filter(|line| {
            !(line.contains("127.0.0.1:8929")
                || line.contains("localhost:8929")
                || line.contains("gitlab.local:8929"))
        })
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    if filtered == text {
        return Ok(false);
    }
    fs::write(&credential_path, filtered)
        .with_context(|| format!("writing {}", credential_path.display()))?;
    Ok(true)
}

pub fn current_worktree_remote(repo_root: &Path) -> Result<Option<String>> {
    git_remote_url(repo_root, "origin")
}

fn canonical_remote_for_entry(entry: &AccessRepoEntry) -> String {
    format!(
        "ssh://git@127.0.0.1:{}/{}/{}.git",
        crate::config::GITLAB_SSH_PORT,
        entry.namespace.trim_matches('/'),
        entry.name.trim_end_matches(".git")
    )
}

fn write_text_if_changed(path: &Path, content: &str, written: &mut Vec<String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(());
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    set_executable_if_needed(path, path.ends_with("pre-push"))?;
    written.push(path.display().to_string());
    Ok(())
}

fn set_executable_if_needed(path: &Path, executable: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if executable {
            let mut permissions = fs::metadata(path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions)?;
        }
    }
    Ok(())
}

fn render_pre_push_hook(protected_branch: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

while read -r _local_ref _local_sha remote_ref _remote_sha; do
  if [ "$remote_ref" = "refs/heads/{protected_branch}" ]; then
    echo "jeryu: direct push to {protected_branch} is blocked; use merge request plus local JeRyu/GitLab gate" >&2
    exit 1
  fi
done

if [ -x .jeryu/ci/required.sh ]; then
  bash .jeryu/ci/required.sh
fi
"#
    )
}

fn slug_from_path(path: &str) -> Option<String> {
    let trimmed = path.trim().trim_end_matches(".git");
    let mut parts = trimmed
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let repo = parts.pop()?.to_string();
    let owner = parts.pop()?.to_string();
    Some(format!("{owner}/{repo}"))
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

fn default_branch() -> String {
    "main".to_string()
}

fn default_protect_main() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{LazyLock, Mutex};
    use std::thread;

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    use std::path::Path;

    struct EnvGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvGuard {
        fn new(home: &Path) -> Self {
            let keys = [
                "HOME",
                "GITLAB_PAT",
                "GITLAB_TOKEN",
                "PRIVATE_TOKEN",
                "GITLAB_URL",
                "CI_SERVER_URL",
            ];
            let saved = keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect::<Vec<_>>();
            // SAFETY: the test guard serializes env mutation with ENV_LOCK,
            // and the mutation is confined to process-local variables.
            unsafe {
                std::env::set_var("HOME", home);
                for key in [
                    "GITLAB_PAT",
                    "GITLAB_TOKEN",
                    "PRIVATE_TOKEN",
                    "GITLAB_URL",
                    "CI_SERVER_URL",
                ] {
                    std::env::remove_var(key);
                }
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: EnvGuard only restores state that it captured while
            // ENV_LOCK was held, so this cannot race with another test.
            unsafe {
                for (key, value) in self.saved.drain(..) {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn init_git_repo(repo_path: &Path, origin: &str) {
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo_path)
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .args(["remote", "add", "origin", origin])
            .current_dir(repo_path)
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn mock_gitlab_server(status: u16, body: &'static str) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0_u8; 4096];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    http_status_text(status),
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{port}"), handle)
    }

    fn http_status_text(status: u16) -> &'static str {
        match status {
            200 => "OK",
            404 => "Not Found",
            503 => "Service Unavailable",
            _ => "OK",
        }
    }

    #[test]
    fn contract_round_trip_is_secret_free() {
        let contract = default_contract();
        let rendered = toml::to_string_pretty(&contract).unwrap();
        assert!(rendered.contains("local_api_url"));
        assert!(rendered.contains("GITLAB_PAT"));
        assert!(!rendered.contains("PRIVATE-TOKEN"));
    }

    #[test]
    fn local_gitlab_classification_includes_gitlab_local() {
        assert!(is_local_gitlab_http_url("http://gitlab.local:8929"));
        assert!(is_local_gitlab_http_url("http://localhost:8929"));
        assert!(is_local_gitlab_ssh_remote(
            "ssh://git@127.0.0.1:2224/root/jekko.git"
        ));
    }

    #[test]
    fn access_contract_round_trip_from_disk_is_secret_free() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::new(temp.path());
        let contract = default_contract();
        write_contract(&contract).unwrap();
        let loaded = load_contract().unwrap();
        assert_eq!(loaded, contract);
        let rendered = std::fs::read_to_string(contract_path()).unwrap();
        assert!(rendered.contains("local_api_url"));
        assert!(!rendered.contains("PRIVATE-TOKEN"));
    }

    #[test]
    fn default_repos_include_known_checkout_paths() {
        let contract = default_contract();
        assert!(
            contract
                .known_repos
                .iter()
                .any(|repo| repo.path.ends_with("/jeryu"))
        );
        assert!(
            contract
                .known_repos
                .iter()
                .any(|repo| repo.path.ends_with("/veox-repos/veox-deploy"))
        );
        assert!(
            contract
                .known_repos
                .iter()
                .any(|repo| repo.path.ends_with("/veox-split/veox-deploy"))
        );
    }

    #[test]
    fn doctor_reports_http_origin_missing_metadata_and_missing_token() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::new(temp.path());
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_git_repo(&repo, "http://localhost:8929/root/example.git");

        let report = access_findings_for_repo(&default_contract(), &repo).unwrap();
        let codes = report
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<Vec<_>>();

        assert!(!report.known_repo);
        assert!(codes.contains(&"missing_known_repo_entry"));
        assert!(codes.contains(&"local_gitlab_http_origin"));
        assert!(codes.contains(&"forbidden_http_origin"));
        assert!(codes.contains(&"missing_repo_toml"));
        assert!(codes.contains(&"missing_policy_toml"));
        assert!(codes.contains(&"missing_ci_toml"));
        assert!(codes.contains(&"missing_gitlab_pat"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn project_resolution_reports_bad_path_and_transient_errors() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::new(temp.path());
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_git_repo(&repo, "ssh://git@127.0.0.1:2224/root/example.git");

        let contract = default_contract();
        let (bad_url, bad_handle) =
            mock_gitlab_server(404, r#"{"message":"404 Project Not Found"}"#);
        let bad_client =
            crate::gitlab_client::GitlabClient::new(&bad_url, Some("token".to_string()));
        let bad_report =
            resolve_project_report(&contract, &repo, Some("root/example"), &bad_client)
                .await
                .unwrap();
        bad_handle.join().unwrap();
        assert!(
            bad_report
                .findings
                .iter()
                .any(|finding| finding.code == "project_resolution_failed")
        );

        let (transient_url, transient_handle) = mock_gitlab_server(503, r#"{"message":"busy"}"#);
        let transient_client =
            crate::gitlab_client::GitlabClient::new(&transient_url, Some("token".to_string()));
        let transient_report =
            resolve_project_report(&contract, &repo, Some("root/example"), &transient_client)
                .await
                .unwrap();
        transient_handle.join().unwrap();
        assert!(
            transient_report
                .findings
                .iter()
                .any(|finding| finding.code == "gitlab_transient")
        );
    }
}
