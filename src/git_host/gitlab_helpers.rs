//! Owner: GitLab host adapter — pure conversion helpers
//! Proof: `cargo test -p jeryu --lib git_host::gitlab`
//! Invariants: helpers are pure functions over `GitLabMergeRequest` /
//! `GitLabChange` / `CheckStatus` / `anyhow::Error`. No I/O, no globals.

use super::gitlab_types::*;
use crate::git_host::{CheckStatus, HostError, PrSummary};

pub(super) fn pr_summary_from_mr(mr: GitLabMergeRequest) -> PrSummary {
    let head_sha = head_sha_from_mr(&mr);
    PrSummary {
        mr_iid: mr.iid.to_string(),
        head_sha,
        target_branch: mr.target_branch,
        author: match mr.author {
            Some(author) if !author.username.is_empty() => author.username,
            _ => "unknown".into(),
        },
        title: mr.title,
        draft: mr.draft || mr.work_in_progress,
        labels: mr.labels,
    }
}

pub(super) fn target_branch_sha_from_mr(mr: &GitLabMergeRequest) -> String {
    let Some(refs) = mr.diff_refs.as_ref() else {
        return String::new();
    };
    if let Some(base) = non_empty_clone(refs.base_sha.as_ref()) {
        return base;
    }
    if let Some(start) = non_empty_clone(refs.start_sha.as_ref()) {
        return start;
    }
    String::new()
}

pub(super) fn head_sha_from_mr(mr: &GitLabMergeRequest) -> String {
    if let Some(sha) = non_empty_clone(mr.sha.as_ref()) {
        return sha;
    }
    let Some(refs) = mr.diff_refs.as_ref() else {
        return String::new();
    };
    match non_empty_clone(refs.head_sha.as_ref()) {
        Some(sha) => sha,
        None => String::new(),
    }
}

pub(super) fn change_path(change: &GitLabChange) -> String {
    if let Some(path) = non_empty_clone(change.new_path.as_ref()) {
        return path;
    }
    match non_empty_clone(change.old_path.as_ref()) {
        Some(path) => path,
        None => String::new(),
    }
}

fn non_empty_clone(value: Option<&String>) -> Option<String> {
    let value = value?;
    if value.is_empty() {
        None
    } else {
        Some(value.clone())
    }
}

pub(super) fn gitlab_status(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Queued => "pending",
        CheckStatus::InProgress => "running",
        CheckStatus::Success => "success",
        CheckStatus::Failure | CheckStatus::ActionRequired => "failed",
        CheckStatus::Neutral => "success",
    }
}

pub(super) fn count_diff_lines(diff: &str) -> (u32, u32) {
    let mut added = 0;
    let mut removed = 0;
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    (added, removed)
}

pub(super) fn map_error(err: anyhow::Error) -> HostError {
    let msg = err.to_string();
    if msg.contains("401") || msg.contains("403") || msg.contains("no PAT configured") {
        HostError::Auth
    } else if msg.contains("429") {
        HostError::RateLimited {
            retry_after_ms: 30_000,
        }
    } else if msg.contains("409") {
        HostError::Conflict(msg)
    } else if msg.contains("502") || msg.contains("503") || msg.contains("504") {
        HostError::Transient(msg)
    } else {
        HostError::Permanent(msg)
    }
}

pub(super) fn map_reqwest(err: reqwest::Error) -> HostError {
    if err.status() == Some(reqwest::StatusCode::UNAUTHORIZED)
        || err.status() == Some(reqwest::StatusCode::FORBIDDEN)
    {
        HostError::Auth
    } else if err.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
        HostError::RateLimited {
            retry_after_ms: 30_000,
        }
    } else if err.status() == Some(reqwest::StatusCode::CONFLICT) {
        HostError::Conflict(err.to_string())
    } else if err.is_timeout() || err.is_connect() {
        HostError::Transient(err.to_string())
    } else {
        HostError::Permanent(err.to_string())
    }
}
