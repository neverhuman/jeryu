//! Top-level `decide()` / `apply()` — combine all signals into a final bump.
//!
//! `decide()` is pure (no writes) so it can be run as a dry-run in PR/CI.
//! `apply()` performs the two file rewrites (root `Cargo.toml` +
//! `CHANGELOG.md`). Neither commits or pushes — the caller (the merge-queue /
//! forge bridge single-writer) owns the commit, so the single-writer invariant
//! lives in exactly one place.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::Serialize;

use crate::api_surface::{self, ApiSurfaceReport};
use crate::cargo_edit::{read_workspace_version, write_workspace_version};
use crate::changelog::roll_unreleased;
use crate::classify::{Bump, classify_range};
use crate::commits::{ConventionalCommit, SKIP_VERSION_SENTINEL, commits_in_range};
use crate::semver::Version;

/// The computed versioning decision for a range. Serializes to the JSON the CLI
/// and integration points consume (`from`, `to`, `bump`, `reason`, `commits`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Decision {
    /// Current workspace version.
    pub from: String,
    /// Next workspace version after applying the bump.
    pub to: String,
    /// Bump level label (`none`/`patch`/`minor`/`major`).
    pub bump: String,
    /// Human-readable reason for the decision.
    pub reason: String,
    /// Number of classified commits in the range.
    pub commits: usize,
    /// `true` when the recursion guard suppressed any bump.
    pub skipped: bool,
}

/// Derive the base ref for blast-radius computation from a range string.
///
/// Accepts `A..B`, `A...B`, or a bare ref; returns the left side (the base) so
/// `jeryu_repogate::build_affected_plan` can diff `base...HEAD`.
fn range_base(range: &str) -> String {
    if let Some((base, _)) = range.split_once("...") {
        base.to_string()
    } else if let Some((base, _)) = range.split_once("..") {
        base.to_string()
    } else {
        range.to_string()
    }
}

/// Whether the single most-recent commit in `range` carries the
/// `[skip-version]` sentinel. When it does, `decide` returns a no-bump decision
/// so the engine never re-triggers on its own release commit.
fn latest_commit_is_skip(root: &Path, range: &str) -> bool {
    let spec = range_head(range);
    Command::new("git")
        .args(["log", "-1", "--format=%s", &spec])
        .current_dir(root)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).contains(SKIP_VERSION_SENTINEL))
        .unwrap_or(false)
}

/// Extract the head ref of a range (right side of `..`/`...`, else the bare ref,
/// else `HEAD`).
fn range_head(range: &str) -> String {
    let head = if let Some((_, head)) = range.split_once("...") {
        head
    } else if let Some((_, head)) = range.split_once("..") {
        head
    } else {
        range
    };
    let head = head.trim();
    if head.is_empty() {
        "HEAD".to_string()
    } else {
        head.to_string()
    }
}

/// Compute the [`Decision`] for `range` without writing anything.
///
/// Combines: conventional-commit classification, the blast-radius package set
/// (reused from `jeryu_repogate::build_affected_plan`), and the public-API
/// breaking signal (reused from `jeryu_rustjet`'s detector + `cargo
/// semver-checks`). Honors the `[skip-version]` recursion guard on the latest
/// commit.
///
/// # Errors
/// Returns an error if the workspace version cannot be read, the git range
/// cannot be listed, or the blast-radius plan cannot be built.
pub fn decide(root: &Path, range: &str) -> Result<Decision> {
    let current = read_workspace_version(root)?;

    if latest_commit_is_skip(root, range) {
        return Ok(Decision {
            from: current.to_string(),
            to: current.to_string(),
            bump: Bump::None.label().to_string(),
            reason: format!("latest commit carries {SKIP_VERSION_SENTINEL}; no bump"),
            commits: 0,
            skipped: true,
        });
    }

    let commits = commits_in_range(root, range)?;
    let plan = jeryu_repogate::build_affected_plan(root, &range_base(range), 40)?;
    let api = api_surface::api_breaking(root, &plan.changed_files, &plan.packages)?;
    let bump = classify_range(&commits, api.breaking);
    let to = current.bumped(bump);

    Ok(Decision {
        from: current.to_string(),
        to: to.to_string(),
        bump: bump.label().to_string(),
        reason: decision_reason(bump, &commits, &api),
        commits: commits.len(),
        skipped: false,
    })
}

/// Build the human-readable reason line for a decision.
fn decision_reason(bump: Bump, commits: &[ConventionalCommit], api: &ApiSurfaceReport) -> String {
    if api.breaking {
        return format!("public-API breaking -> major ({})", api.detail);
    }
    let n = commits.len();
    let plural = if n == 1 { "commit" } else { "commits" };
    format!("{n} {plural} -> {} (floor=patch)", bump.label())
}

/// Apply a decision: rewrite the workspace version and roll the CHANGELOG. A
/// skipped (no-bump) decision is a no-op.
///
/// # Errors
/// Returns an error if `d.to` is not a valid version, or either file rewrite
/// fails.
pub fn apply(root: &Path, d: &Decision, commits: &[ConventionalCommit]) -> Result<()> {
    if d.skipped || d.to == d.from {
        return Ok(());
    }
    let to = Version::parse(&d.to)?;
    write_workspace_version(root, to)?;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    roll_unreleased(root, to, commits, &date)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_base_handles_two_and_three_dots() {
        assert_eq!(range_base("origin/main..HEAD"), "origin/main");
        assert_eq!(range_base("a...b"), "a");
        assert_eq!(range_base("origin/main"), "origin/main");
    }

    #[test]
    fn range_head_handles_forms() {
        assert_eq!(range_head("origin/main..HEAD"), "HEAD");
        assert_eq!(range_head("a...b"), "b");
        assert_eq!(range_head("origin/main"), "origin/main");
        assert_eq!(range_head("a.."), "HEAD");
    }

    #[test]
    fn apply_is_noop_when_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let d = Decision {
            from: "4.0.0".into(),
            to: "4.0.0".into(),
            bump: "none".into(),
            reason: "skip".into(),
            commits: 0,
            skipped: true,
        };
        // No files exist; a skipped apply must not touch anything and must not error.
        apply(dir.path(), &d, &[]).unwrap();
        assert!(!dir.path().join("Cargo.toml").exists());
    }
}
