//! GitHub export entry points: read a provider export file or JSON value and
//! assemble it into a deterministic typed [`Archive`].

use std::fs;
use std::path::Path;

use chrono::Utc;
use serde_json::Value;

use crate::errors::{JeryuMirrorError, Result};
use crate::model::*;

use super::parsers::{
    parse_app, parse_artifact, parse_issue, parse_label, parse_milestone, parse_pr,
    parse_protected_branch, parse_release, parse_webhook,
};
use super::value::array;

pub fn load_github_export(path: impl AsRef<Path>) -> Result<Archive> {
    let text = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    archive_from_github_value(value)
}

pub fn archive_from_github_value(value: Value) -> Result<Archive> {
    let repos = value
        .get("repositories")
        .or_else(|| value.get("repos"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            JeryuMirrorError::UnsupportedSource(
                "expected GitHub export with repositories[] or repos[]".to_string(),
            )
        })?;

    let mut archive = Archive::new(
        SourceKind::GitHub,
        value
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("github-export"),
    );
    for repo_value in repos {
        archive.repositories.push(parse_repo(repo_value));
    }
    archive.sort_for_determinism();
    Ok(archive)
}

fn parse_repo(value: &Value) -> RepositoryArchive {
    let owner = value
        .pointer("/owner/login")
        .or_else(|| value.pointer("/organization/login"))
        .or_else(|| value.get("owner"))
        .and_then(Value::as_str)
        .unwrap_or("unknown-owner");
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("unknown-repo");
    let mut repo = RepositoryArchive::new(owner, name);
    repo.description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    repo.default_branch = value
        .get("default_branch")
        .and_then(Value::as_str)
        .unwrap_or("main")
        .to_string();
    repo.archived = value
        .get("archived")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    repo.disabled = value
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    repo.visibility = if value
        .get("private")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        Visibility::Private
    } else {
        Visibility::Public
    };
    if let Some(url) = value
        .get("clone_url")
        .or_else(|| value.get("ssh_url"))
        .or_else(|| value.get("mirror_url"))
        .and_then(Value::as_str)
    {
        repo.git = Some(GitRemoteSnapshot {
            remote_url: url.to_string(),
            head_sha: value
                .get("head_sha")
                .and_then(Value::as_str)
                .map(str::to_string),
            mirror_ref: value
                .get("mirror_ref")
                .and_then(Value::as_str)
                .map(str::to_string),
            object_format: value
                .get("object_format")
                .and_then(Value::as_str)
                .unwrap_or("sha1")
                .to_string(),
            mirrored_at: Utc::now(),
        });
    }
    repo.labels = array(value, "labels")
        .into_iter()
        .map(parse_label)
        .collect();
    repo.milestones = array(value, "milestones")
        .into_iter()
        .map(parse_milestone)
        .collect();
    repo.issues = array(value, "issues")
        .into_iter()
        .filter(|item| item.get("pull_request").is_none())
        .map(parse_issue)
        .collect();
    repo.pull_requests = array(value, "pull_requests")
        .into_iter()
        .chain(array(value, "pulls"))
        .map(parse_pr)
        .collect();
    repo.releases = array(value, "releases")
        .into_iter()
        .map(parse_release)
        .collect();
    repo.artifacts = array(value, "artifacts")
        .into_iter()
        .map(parse_artifact)
        .collect();
    repo.webhooks = array(value, "hooks")
        .into_iter()
        .chain(array(value, "webhooks"))
        .map(parse_webhook)
        .collect();
    repo.app_installations = array(value, "app_installations")
        .into_iter()
        .map(parse_app)
        .collect();
    repo.protected_branches = array(value, "protected_branches")
        .into_iter()
        .chain(array(value, "branch_protections"))
        .map(parse_protected_branch)
        .collect();
    repo.raw_source = Some(value.clone());
    repo
}
