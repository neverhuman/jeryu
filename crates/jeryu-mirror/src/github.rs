use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::errors::{JeryuMirrorError, Result};
use crate::model::*;

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

fn array<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|v| v.iter().collect())
        .unwrap_or_default()
}

fn parse_label(value: &Value) -> LabelArchive {
    LabelArchive {
        name: string(value, "name", "unnamed"),
        color: value
            .get("color")
            .and_then(Value::as_str)
            .map(str::to_string),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn parse_milestone(value: &Value) -> MilestoneArchive {
    MilestoneArchive {
        title: string(value, "title", "untitled"),
        state: parse_state(value.get("state").and_then(Value::as_str)),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        due_on: parse_time(value.get("due_on").and_then(Value::as_str)),
    }
}

fn parse_issue(value: &Value) -> NormalizedIssue {
    let mut issue = NormalizedIssue::new(
        number(value, "number"),
        string(value, "title", "untitled issue"),
        parse_state(value.get("state").and_then(Value::as_str)),
    );
    issue.body = value
        .get("body")
        .and_then(Value::as_str)
        .map(str::to_string);
    issue.author = value
        .pointer("/user/login")
        .or_else(|| value.get("author"))
        .and_then(Value::as_str)
        .map(str::to_string);
    issue.labels = array(value, "labels")
        .into_iter()
        .map(|label| {
            label
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string()
        })
        .collect();
    issue.comments = array(value, "comments")
        .into_iter()
        .map(parse_comment)
        .collect();
    issue.created_at = parse_time(value.get("created_at").and_then(Value::as_str));
    issue.updated_at = parse_time(value.get("updated_at").and_then(Value::as_str));
    issue.closed_at = parse_time(value.get("closed_at").and_then(Value::as_str));
    issue.raw_source = Some(value.clone());
    issue
}

fn parse_pr(value: &Value) -> NormalizedPullRequest {
    let merged = value.get("merged_at").and_then(Value::as_str).is_some();
    let state = if merged {
        ObjectState::Merged
    } else {
        parse_state(value.get("state").and_then(Value::as_str))
    };
    let mut pr = NormalizedPullRequest::new(
        number(value, "number"),
        string(value, "title", "untitled pull request"),
        state,
    );
    pr.body = value
        .get("body")
        .and_then(Value::as_str)
        .map(str::to_string);
    pr.author = value
        .pointer("/user/login")
        .or_else(|| value.get("author"))
        .and_then(Value::as_str)
        .map(str::to_string);
    pr.head_ref = value
        .pointer("/head/ref")
        .or_else(|| value.get("head_ref"))
        .and_then(Value::as_str)
        .map(str::to_string);
    pr.base_ref = value
        .pointer("/base/ref")
        .or_else(|| value.get("base_ref"))
        .and_then(Value::as_str)
        .map(str::to_string);
    pr.merge_commit_sha = value
        .get("merge_commit_sha")
        .and_then(Value::as_str)
        .map(str::to_string);
    pr.reviews = array(value, "reviews")
        .into_iter()
        .map(parse_review)
        .collect();
    pr.comments = array(value, "comments")
        .into_iter()
        .map(parse_comment)
        .collect();
    pr.created_at = parse_time(value.get("created_at").and_then(Value::as_str));
    pr.updated_at = parse_time(value.get("updated_at").and_then(Value::as_str));
    pr.closed_at = parse_time(value.get("closed_at").and_then(Value::as_str));
    pr.merged_at = parse_time(value.get("merged_at").and_then(Value::as_str));
    pr.raw_source = Some(value.clone());
    pr
}

fn parse_review(value: &Value) -> ReviewArchive {
    ReviewArchive {
        id: value
            .get("id")
            .map(Value::to_string)
            .unwrap_or_else(|| "unknown".to_string()),
        author: value
            .pointer("/user/login")
            .or_else(|| value.get("author"))
            .and_then(Value::as_str)
            .map(str::to_string),
        state: string(value, "state", "COMMENTED"),
        body: value
            .get("body")
            .and_then(Value::as_str)
            .map(str::to_string),
        submitted_at: parse_time(value.get("submitted_at").and_then(Value::as_str)),
    }
}

fn parse_release(value: &Value) -> ReleaseArchive {
    let mut release = ReleaseArchive::new(string(value, "tag_name", "untagged"));
    release.name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string);
    release.body = value
        .get("body")
        .and_then(Value::as_str)
        .map(str::to_string);
    release.draft = value.get("draft").and_then(Value::as_bool).unwrap_or(false);
    release.prerelease = value
        .get("prerelease")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    release.assets = array(value, "assets")
        .into_iter()
        .map(parse_release_asset)
        .collect();
    release.created_at = parse_time(value.get("created_at").and_then(Value::as_str));
    release.published_at = parse_time(value.get("published_at").and_then(Value::as_str));
    release.raw_source = Some(value.clone());
    release
}

fn parse_release_asset(value: &Value) -> ReleaseAssetArchive {
    ReleaseAssetArchive {
        name: string(value, "name", "asset"),
        size: value.get("size").and_then(Value::as_u64),
        digest: value
            .get("digest")
            .or_else(|| value.get("sha256"))
            .and_then(Value::as_str)
            .map(str::to_string),
        content_type: value
            .get("content_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        download_url: value
            .get("browser_download_url")
            .or_else(|| value.get("download_url"))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn parse_artifact(value: &Value) -> ArtifactMetadata {
    ArtifactMetadata {
        name: string(value, "name", "artifact"),
        kind: value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("github-actions-artifact")
            .to_string(),
        size: value
            .get("size_in_bytes")
            .or_else(|| value.get("size"))
            .and_then(Value::as_u64),
        digest: value
            .get("digest")
            .or_else(|| value.get("sha256"))
            .and_then(Value::as_str)
            .map(str::to_string),
        source_url: value
            .get("archive_download_url")
            .or_else(|| value.get("url"))
            .and_then(Value::as_str)
            .map(str::to_string),
        expires_at: parse_time(value.get("expires_at").and_then(Value::as_str)),
        retained: !value
            .get("expired")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_webhook(value: &Value) -> WebhookMigration {
    let config = value.get("config").unwrap_or(value);
    let has_secret = config
        .get("secret")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let id = value
        .get("id")
        .map(Value::to_string)
        .unwrap_or_else(|| "unknown".to_string());
    WebhookMigration {
        id: id.clone(),
        url: config
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("https://example.invalid/webhook")
            .to_string(),
        events: value
            .get("events")
            .and_then(Value::as_array)
            .map(|events| {
                events
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|| vec!["push".to_string()]),
        active: value.get("active").and_then(Value::as_bool).unwrap_or(true),
        content_type: config
            .get("content_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        secret_name: has_secret.then(|| format!("jeryu_mirror/webhook/{id}")),
        notes: vec![
            "Secret value intentionally omitted; rehydrate in target secret store.".to_string(),
        ],
    }
}

fn parse_app(value: &Value) -> AppInstallationMigration {
    let id = value
        .get("id")
        .map(Value::to_string)
        .unwrap_or_else(|| "unknown".to_string());
    AppInstallationMigration {
        id: id.clone(),
        slug: string(value, "slug", "github-app"),
        permissions: value
            .get("permissions")
            .and_then(Value::as_object)
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default(),
        events: value
            .get("events")
            .and_then(Value::as_array)
            .map(|events| {
                events
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        token_secret_name: Some(format!("jeryu_mirror/app/{id}/token")),
        notes: vec![
            "Installation token is not exportable; recreate or rotate after restore.".to_string(),
        ],
    }
}

fn parse_protected_branch(value: &Value) -> ProtectedBranchArchive {
    ProtectedBranchArchive {
        pattern: value
            .get("pattern")
            .or_else(|| value.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string(),
        required_status_checks: value
            .pointer("/required_status_checks/contexts")
            .and_then(Value::as_array)
            .map(|contexts| {
                contexts
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        required_reviews: value
            .pointer("/required_pull_request_reviews/required_approving_review_count")
            .or_else(|| value.get("required_reviews"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        linear_history: value
            .get("required_linear_history")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allow_force_pushes: value
            .get("allow_force_pushes")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allow_deletions: value
            .get("allow_deletions")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_comment(value: &Value) -> NormalizedComment {
    NormalizedComment {
        id: value
            .get("id")
            .map(Value::to_string)
            .unwrap_or_else(|| "unknown".to_string()),
        author: value
            .pointer("/user/login")
            .or_else(|| value.get("author"))
            .and_then(Value::as_str)
            .map(str::to_string),
        body: string(value, "body", ""),
        created_at: parse_time(value.get("created_at").and_then(Value::as_str)),
        raw_source: Some(value.clone()),
    }
}

fn string(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn number(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .or_else(|| value.get("iid"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn parse_state(value: Option<&str>) -> ObjectState {
    match value.unwrap_or("unknown").to_ascii_lowercase().as_str() {
        "open" | "opened" => ObjectState::Open,
        "closed" => ObjectState::Closed,
        "merged" => ObjectState::Merged,
        "draft" => ObjectState::Draft,
        "archived" => ObjectState::Archived,
        _ => ObjectState::Unknown,
    }
}

fn parse_time(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.with_timezone(&Utc))
}
