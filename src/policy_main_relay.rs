//! Owner: Post-merge external relay policy reader
//! Proof: `cargo test -p jeryu --lib policy_main_relay`
//! Invariants:
//!   - Pure parser over `.jeryu/policy.toml`. No I/O beyond an explicit file
//!     read in `for_repo`. Every field is `#[serde(default)]` so legacy policy
//!     files (written before the `[main_relay.github]` sub-table existed) parse
//!     into safe disabled defaults rather than erroring.
//!   - This module only READS policy; it never writes it. `repo_direct.rs`
//!     remains the sole writer of `.jeryu/policy.toml`.
//!   - The post-merge GitHub relay consumer (Phase H
//!     `engine_background_remote_mirror`) uses `github_relay_target` to decide
//!     whether — and where — to mirror `main` after a local MR merges. Push is
//!     attempted only post-merge; on a protected branch it falls back to a PR.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// The subset of `.jeryu/policy.toml` the post-merge relay consumer needs.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct MainRelayPolicy {
    #[serde(default)]
    pub main_relay: MainRelay,
    #[serde(default)]
    pub offline_release_mirror: OfflineReleaseMirror,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct MainRelay {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub protected_branch: String,
    /// New sub-table `[main_relay.github]`. Absent in legacy policy files →
    /// `None` → no GitHub relay (safe default).
    #[serde(default)]
    pub github: Option<MainRelayGithub>,
}

/// `[main_relay.github]` — the post-merge push-to-GitHub config.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MainRelayGithub {
    #[serde(default)]
    pub enabled: bool,
    /// Git remote name (configured in the local repo) pointing at GitHub.
    #[serde(default)]
    pub remote: String,
    /// GitHub branch to fast-forward push to (defaults to `main`).
    #[serde(default = "default_branch")]
    pub branch: String,
    /// If the GitHub branch is protected (push rejected), open a PR instead.
    #[serde(default = "default_true")]
    pub fallback_to_pr: bool,
}

impl Default for MainRelayGithub {
    fn default() -> Self {
        Self {
            enabled: false,
            remote: String::new(),
            branch: default_branch(),
            fallback_to_pr: default_true(),
        }
    }
}

/// `[offline_release_mirror]` — release tag/branch mirror config (already
/// written by `repo_direct.rs`; consumed here so the relay can also mirror
/// release refs, not just `main`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct OfflineReleaseMirror {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub remote: String,
    #[serde(default)]
    pub refs: Vec<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_true() -> bool {
    true
}

/// A resolved, ready-to-act GitHub relay target. `None` from
/// [`MainRelayPolicy::github_relay_target`] means "do not relay to GitHub."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRelayTarget {
    pub remote: String,
    pub branch: String,
    pub fallback_to_pr: bool,
}

impl MainRelayPolicy {
    /// Parse from a TOML string. Tolerant of missing sections.
    pub fn from_toml_str(s: &str) -> Result<Self> {
        toml::from_str::<Self>(s).context("parse .jeryu/policy.toml")
    }

    /// Read `<repo_root>/.jeryu/policy.toml`. A missing file yields the disabled
    /// default (no relay) rather than an error, so unconfigured repos are inert.
    pub fn for_repo(repo_root: &Path) -> Result<Self> {
        let path = repo_root.join(".jeryu/policy.toml");
        match std::fs::read_to_string(&path) {
            Ok(body) => Self::from_toml_str(&body),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("read {}", path.display())),
        }
    }

    /// The actionable GitHub relay target, or `None` when relay is off. Relay is
    /// active only when BOTH `main_relay.enabled` AND `main_relay.github.enabled`
    /// are true and a non-empty remote is configured — a conservative AND so a
    /// half-configured policy never pushes anywhere unexpected.
    pub fn github_relay_target(&self) -> Option<GithubRelayTarget> {
        if !self.main_relay.enabled {
            return None;
        }
        let gh = self.main_relay.github.as_ref()?;
        if !gh.enabled || gh.remote.trim().is_empty() {
            return None;
        }
        Some(GithubRelayTarget {
            remote: gh.remote.clone(),
            branch: if gh.branch.trim().is_empty() {
                default_branch()
            } else {
                gh.branch.clone()
            },
            fallback_to_pr: gh.fallback_to_pr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact shape repo_direct.rs writes today (no github sub-table).
    const LEGACY: &str = r#"
schema_version = "1"
protect_main = true
protected_branches = ["main"]
protected_tags = ["v*"]
hooks = "advisory"

[main_relay]
enabled = true
actor = "jeryu"
protected_branch = "main"
require_admission_receipt = true

[offline_release_mirror]
enabled = false
remote = ""
refs = ["refs/tags/v*", "refs/heads/release/*"]
"#;

    const WITH_GITHUB: &str = r#"
[main_relay]
enabled = true
actor = "jeryu"
protected_branch = "main"

[main_relay.github]
enabled = true
remote = "github"
branch = "main"
fallback_to_pr = true

[offline_release_mirror]
enabled = true
remote = "github"
refs = ["refs/tags/v*"]
"#;

    #[test]
    fn legacy_policy_parses_with_no_github_relay() {
        let p = MainRelayPolicy::from_toml_str(LEGACY).unwrap();
        assert!(p.main_relay.enabled);
        assert!(p.main_relay.github.is_none());
        assert!(!p.offline_release_mirror.enabled);
        assert_eq!(p.github_relay_target(), None, "legacy → no GitHub relay");
    }

    #[test]
    fn github_subtable_resolves_target() {
        let p = MainRelayPolicy::from_toml_str(WITH_GITHUB).unwrap();
        let t = p.github_relay_target().expect("relay target");
        assert_eq!(t.remote, "github");
        assert_eq!(t.branch, "main");
        assert!(t.fallback_to_pr);
        assert!(p.offline_release_mirror.enabled);
        assert_eq!(p.offline_release_mirror.refs, vec!["refs/tags/v*"]);
    }

    #[test]
    fn empty_policy_is_inert() {
        let p = MainRelayPolicy::from_toml_str("").unwrap();
        assert_eq!(p, MainRelayPolicy::default());
        assert_eq!(p.github_relay_target(), None);
    }

    #[test]
    fn relay_off_when_main_relay_disabled_even_if_github_enabled() {
        let s = r#"
[main_relay]
enabled = false
[main_relay.github]
enabled = true
remote = "github"
"#;
        let p = MainRelayPolicy::from_toml_str(s).unwrap();
        assert_eq!(p.github_relay_target(), None, "main_relay gate is required");
    }

    #[test]
    fn relay_off_when_remote_empty() {
        let s = r#"
[main_relay]
enabled = true
[main_relay.github]
enabled = true
remote = ""
"#;
        let p = MainRelayPolicy::from_toml_str(s).unwrap();
        assert_eq!(p.github_relay_target(), None, "empty remote → no relay");
    }

    #[test]
    fn github_branch_defaults_to_main_when_unset() {
        let s = r#"
[main_relay]
enabled = true
[main_relay.github]
enabled = true
remote = "github"
"#;
        let p = MainRelayPolicy::from_toml_str(s).unwrap();
        let t = p.github_relay_target().unwrap();
        assert_eq!(t.branch, "main");
        assert!(t.fallback_to_pr, "fallback_to_pr defaults true");
    }

    #[test]
    fn for_repo_missing_file_is_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let p = MainRelayPolicy::for_repo(tmp.path()).unwrap();
        assert_eq!(p, MainRelayPolicy::default());
    }

    #[test]
    fn for_repo_reads_written_policy() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".jeryu")).unwrap();
        std::fs::write(tmp.path().join(".jeryu/policy.toml"), WITH_GITHUB).unwrap();
        let p = MainRelayPolicy::for_repo(tmp.path()).unwrap();
        assert!(p.github_relay_target().is_some());
    }
}
