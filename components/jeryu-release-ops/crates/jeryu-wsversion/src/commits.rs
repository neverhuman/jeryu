//! Conventional-commit parsing over a `git log` range.
//!
//! The range is supplied by the integration point (`old_oid..new_oid` for the
//! forge `ci_bridge`, `origin/main..HEAD` for CI). Records are separated with
//! control characters so multi-line commit bodies survive intact, and the
//! engine's own release commits (carrying the `[skip-version]` sentinel) are
//! dropped so it never re-triggers on the commit it just wrote.

use std::path::Path;
use std::process::Command;

use anyhow::{Result, ensure};

/// The recursion-guard sentinel. A commit whose subject contains this marker is
/// the engine's own release commit and must never be classified.
pub const SKIP_VERSION_SENTINEL: &str = "[skip-version]";

/// A single parsed conventional commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConventionalCommit {
    /// Full commit SHA.
    pub sha: String,
    /// Conventional type, lowercased (e.g. `feat`, `fix`, `chore`).
    pub kind: String,
    /// Optional scope from `type(scope):`.
    pub scope: Option<String>,
    /// `true` if `!` is present or a `BREAKING CHANGE` footer exists.
    pub breaking: bool,
    /// The raw commit subject line.
    pub subject: String,
}

impl ConventionalCommit {
    /// Whether this commit carries the recursion-guard sentinel.
    #[must_use]
    pub fn is_skip_version(&self) -> bool {
        self.subject.contains(SKIP_VERSION_SENTINEL)
    }
}

/// Read and parse conventional commits in `range`, dropping merge commits and
/// the engine's own `[skip-version]` release commits.
///
/// # Errors
/// Returns an error if `git log` cannot be executed or exits non-zero.
pub fn commits_in_range(root: &Path, range: &str) -> Result<Vec<ConventionalCommit>> {
    let output = Command::new("git")
        .args(["log", "--no-merges", "--format=%H%x1f%s%x1f%b%x1e", range])
        .current_dir(root)
        .output()?;
    ensure!(
        output.status.success(),
        "git log {range} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_log(&text))
}

/// Parse the raw `git log` payload (record separator `\x1e`, field separator
/// `\x1f`) into commits, skipping `[skip-version]` records.
fn parse_log(text: &str) -> Vec<ConventionalCommit> {
    let mut commits = Vec::new();
    for record in text
        .split('\u{1e}')
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        let mut fields = record.splitn(3, '\u{1f}');
        let sha = fields.next().unwrap_or("").trim().to_string();
        let subject = fields.next().unwrap_or("").trim().to_string();
        let body = fields.next().unwrap_or("");
        let commit = parse_one(sha, subject, body);
        if commit.is_skip_version() {
            continue; // recursion guard: never classify our own release commit
        }
        commits.push(commit);
    }
    commits
}

/// Parse a single commit header + body into a [`ConventionalCommit`].
fn parse_one(sha: String, subject: String, body: &str) -> ConventionalCommit {
    let breaking_footer = body.lines().any(|line| {
        let l = line.trim_start();
        l.starts_with("BREAKING CHANGE:") || l.starts_with("BREAKING-CHANGE:")
    });
    // header is everything before the first ": " in the subject.
    let (head, _desc) = subject.split_once(": ").unwrap_or((subject.as_str(), ""));
    let head = head.trim();
    let bang = head.ends_with('!');
    let head = head.trim_end_matches('!');
    let (kind, scope) = match head.split_once('(') {
        Some((k, rest)) => (
            k.to_string(),
            Some(rest.trim_end_matches(')').trim().to_string()),
        ),
        None => (head.to_string(), None),
    };
    ConventionalCommit {
        sha,
        kind: kind.trim().to_lowercase(),
        scope,
        breaking: bang || breaking_footer,
        subject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(sha: &str, subject: &str, body: &str) -> String {
        format!("{sha}\u{1f}{subject}\u{1f}{body}\u{1e}")
    }

    #[test]
    fn parses_type_scope_and_breaking_bang() {
        let c = parse_one("abc".into(), "feat(api)!: drop legacy route".into(), "");
        assert_eq!(c.kind, "feat");
        assert_eq!(c.scope.as_deref(), Some("api"));
        assert!(c.breaking);
    }

    #[test]
    fn parses_plain_type() {
        let c = parse_one("abc".into(), "fix: handle empty input".into(), "");
        assert_eq!(c.kind, "fix");
        assert_eq!(c.scope, None);
        assert!(!c.breaking);
    }

    #[test]
    fn breaking_change_footer_sets_breaking() {
        let c = parse_one(
            "abc".into(),
            "refactor: rename module".into(),
            "body line\nBREAKING CHANGE: signatures changed\n",
        );
        assert!(c.breaking);
    }

    #[test]
    fn breaking_change_hyphen_footer_sets_breaking() {
        let c = parse_one(
            "abc".into(),
            "chore: cleanup".into(),
            "BREAKING-CHANGE: removed flag\n",
        );
        assert!(c.breaking);
    }

    #[test]
    fn skip_version_records_are_dropped() {
        let text = format!(
            "{}{}",
            record("a1", "feat: real feature", ""),
            record("a2", "chore(release): v4.1.0 [skip-version]", ""),
        );
        let commits = parse_log(&text);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].kind, "feat");
    }

    #[test]
    fn parses_multiple_records() {
        let text = format!(
            "{}{}",
            record("a1", "feat: one", ""),
            record("a2", "fix: two", ""),
        );
        let commits = parse_log(&text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "feat: one");
        assert_eq!(commits[1].subject, "fix: two");
    }
}
