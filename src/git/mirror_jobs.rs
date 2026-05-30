//! Owner: Post-merge external-mirror enqueue plane
//! Proof: `cargo test -p jeryu --lib git::mirror_jobs`
//! Invariants:
//!   - Enqueue is best-effort: caller treats `Err` as a log line and never lets
//!     it abort the merge result. The hook in `git_host::gitlab_merge::gl_merge_mr`
//!     logs and continues on failure.
//!   - The hook records merge-intent metadata only; the background consumer
//!     (engine_background_remote_mirror, follow-up MR) resolves the local repo
//!     clone, reads `.jeryu/policy.toml` for [offline_release_mirror] config,
//!     and decides whether to push refs to the configured external remote.
//!   - Append-only JSONL log at `~/.jeryu/mirror_intents.jsonl`. The consumer
//!     claims rows by file offset and updates a separate status file; this
//!     module never rewrites prior lines.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Append-only log of merge-mirror intents. The consumer is wired in a
/// follow-up MR (see ~/jeryu/MASTER_HARDENING_PLAN.md phase H).
pub fn mirror_intents_log_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".jeryu/mirror_intents.jsonl")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirrorIntent {
    pub schema_version: u32,
    pub enqueued_at: DateTime<Utc>,
    pub repo_owner: String,
    pub repo_name: String,
    pub merged_sha: Option<String>,
    pub merge_url: Option<String>,
}

/// Append a mirror intent for a just-merged MR to the standard log location.
/// Best-effort: caller treats the `Err` as a log line and does not propagate
/// it into the merge result.
pub fn enqueue_merge_mirror_intent(
    repo_owner: &str,
    repo_name: &str,
    merged_sha: Option<&str>,
    merge_url: Option<&str>,
) -> Result<MirrorIntent> {
    enqueue_merge_mirror_intent_to(
        &mirror_intents_log_path(),
        repo_owner,
        repo_name,
        merged_sha,
        merge_url,
    )
}

/// Explicit-path variant of `enqueue_merge_mirror_intent`. Used by tests to
/// avoid `$HOME` global state and by future consumers that target a non-default
/// log location. Creates the parent directory if missing.
pub fn enqueue_merge_mirror_intent_to(
    path: &Path,
    repo_owner: &str,
    repo_name: &str,
    merged_sha: Option<&str>,
    merge_url: Option<&str>,
) -> Result<MirrorIntent> {
    let intent = MirrorIntent {
        schema_version: 1,
        enqueued_at: Utc::now(),
        repo_owner: repo_owner.to_string(),
        repo_name: repo_name.to_string(),
        merged_sha: merged_sha.map(|s| s.to_string()),
        merge_url: merge_url.map(|s| s.to_string()),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let line = serde_json::to_string(&intent).context("serialize mirror intent")?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("append {}", path.display()))?;
    Ok(intent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_writes_one_jsonl_line() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mirror_intents.jsonl");
        let intent = enqueue_merge_mirror_intent_to(
            &path,
            "acme",
            "widget",
            Some("deadbeef"),
            Some("http://gl/acme/widget/-/merge_requests/1"),
        )
        .unwrap();
        assert_eq!(intent.repo_owner, "acme");
        assert_eq!(intent.repo_name, "widget");
        assert_eq!(intent.merged_sha.as_deref(), Some("deadbeef"));
        let log = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: MirrorIntent = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed, intent);
    }

    #[test]
    fn enqueue_appends_multiple_intents_in_order() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mirror_intents.jsonl");
        enqueue_merge_mirror_intent_to(&path, "a", "b", None, None).unwrap();
        enqueue_merge_mirror_intent_to(&path, "c", "d", Some("ff"), None).unwrap();
        enqueue_merge_mirror_intent_to(
            &path,
            "e",
            "f",
            Some("12"),
            Some("http://gl/e/f/-/merge_requests/9"),
        )
        .unwrap();
        let log = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 3);
        let parsed: Vec<MirrorIntent> = lines
            .iter()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(parsed[0].repo_owner, "a");
        assert_eq!(parsed[1].repo_owner, "c");
        assert_eq!(parsed[2].repo_owner, "e");
        assert_eq!(parsed[1].merged_sha.as_deref(), Some("ff"));
        assert_eq!(
            parsed[2].merge_url.as_deref(),
            Some("http://gl/e/f/-/merge_requests/9")
        );
    }

    #[test]
    fn enqueue_creates_parent_dir_on_first_use() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nested/deeper/mirror_intents.jsonl");
        let parent = path.parent().unwrap().to_path_buf();
        assert!(!parent.exists());
        enqueue_merge_mirror_intent_to(&path, "a", "b", None, None).unwrap();
        assert!(parent.exists());
        assert!(path.exists());
    }

    #[test]
    fn intent_serde_round_trip_includes_timestamp() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mirror_intents.jsonl");
        let intent = enqueue_merge_mirror_intent_to(&path, "x", "y", Some("abc"), None).unwrap();
        let json = serde_json::to_string(&intent).unwrap();
        let back: MirrorIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, intent);
        assert_eq!(back.schema_version, 1);
    }

    #[test]
    fn default_log_path_lives_under_home_dot_jeryu() {
        let path = mirror_intents_log_path();
        let s = path.to_string_lossy();
        assert!(s.ends_with(".jeryu/mirror_intents.jsonl"), "got: {s}");
    }
}
