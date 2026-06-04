//! CHANGELOG generation: roll `## Unreleased` into a dated `## vX.Y.Z` section.
//!
//! The existing `CHANGELOG.md` uses a Keep-a-Changelog-ish structure with a
//! `## Unreleased` heading at the top and dated `## vX.Y.Z - YYYY-MM-DD` sections
//! below. This module replaces the `## Unreleased` heading with a fresh empty
//! `## Unreleased` plus a new dated section listing the range's commits grouped
//! by type. It is idempotent in the sense that it errors (rather than silently
//! corrupting) if no `## Unreleased` heading is present.

use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::commits::ConventionalCommit;
use crate::semver::Version;

/// Stable display order for grouped changelog bullets.
const KIND_ORDER: &[&str] = &["feat", "fix", "perf", "refactor", "build", "revert"];

/// Render the bullet block for a set of commits, grouped by type in
/// [`KIND_ORDER`] then any remaining types. Falls back to a maintenance line
/// when nothing notable is present.
fn render_bullets(commits: &[ConventionalCommit]) -> String {
    let mut bullets = String::new();
    let mut emitted = 0usize;
    let emit = |c: &ConventionalCommit, bullets: &mut String, emitted: &mut usize| {
        let scope = c
            .scope
            .as_deref()
            .map(|s| format!("**{s}**: "))
            .unwrap_or_default();
        let mark = if c.breaking { "[BREAKING] " } else { "" };
        bullets.push_str(&format!("- {mark}{scope}{}\n", c.subject));
        *emitted += 1;
    };

    for kind in KIND_ORDER {
        for c in commits.iter().filter(|c| c.kind == *kind) {
            emit(c, &mut bullets, &mut emitted);
        }
    }
    // Any breaking commit whose type is not in KIND_ORDER (e.g. `chore!:`) is
    // still material and must appear.
    for c in commits
        .iter()
        .filter(|c| c.breaking && !KIND_ORDER.contains(&c.kind.as_str()))
    {
        emit(c, &mut bullets, &mut emitted);
    }
    if emitted == 0 {
        bullets.push_str("- Maintenance and internal changes.\n");
    }
    bullets
}

/// Roll the `## Unreleased` heading into a dated `## vX.Y.Z - DATE` section,
/// leaving a fresh empty `## Unreleased` heading at the top.
///
/// # Errors
/// Returns an error if `CHANGELOG.md` cannot be read/written or it has no
/// `## Unreleased` heading.
pub fn roll_unreleased(
    root: &Path,
    v: Version,
    commits: &[ConventionalCommit],
    date: &str,
) -> Result<()> {
    let path = root.join("CHANGELOG.md");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let updated = roll_text(&text, v, commits, date)?;
    std::fs::write(&path, updated).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Pure text transform powering [`roll_unreleased`], factored out for testing.
///
/// # Errors
/// Returns an error if `text` contains no `## Unreleased` heading.
pub fn roll_text(
    text: &str,
    v: Version,
    commits: &[ConventionalCommit],
    date: &str,
) -> Result<String> {
    ensure!(
        text.contains("## Unreleased"),
        "no '## Unreleased' heading found in CHANGELOG.md"
    );
    let bullets = render_bullets(commits);
    let section = format!("## Unreleased\n\n## v{v} - {date}\n\n{bullets}");
    let updated = text.replacen("## Unreleased", section.trim_end(), 1);
    ensure!(updated != text, "CHANGELOG.md was not modified");
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(
        kind: &str,
        scope: Option<&str>,
        breaking: bool,
        subject: &str,
    ) -> ConventionalCommit {
        ConventionalCommit {
            sha: "deadbeef".into(),
            kind: kind.into(),
            scope: scope.map(str::to_string),
            breaking,
            subject: subject.into(),
        }
    }

    #[test]
    fn rolls_unreleased_into_dated_section() {
        let text =
            "# Changelog\n\n## Unreleased\n\n- old note\n\n## v4.0.0 - 2026-01-01\n\n- prior\n";
        let commits = [
            commit("feat", Some("api"), false, "feat(api): add endpoint"),
            commit("fix", None, false, "fix: correct off-by-one"),
        ];
        let out = roll_text(
            text,
            Version::parse("4.1.0").unwrap(),
            &commits,
            "2026-06-03",
        )
        .unwrap();
        assert!(out.contains("## v4.1.0 - 2026-06-03"));
        assert!(out.contains("- **api**: feat(api): add endpoint"));
        assert!(out.contains("- fix: correct off-by-one"));
        // A fresh empty Unreleased heading remains at the top.
        assert!(out.contains("## Unreleased\n\n## v4.1.0"));
        // The prior v4.0.0 section is untouched.
        assert!(out.contains("## v4.0.0 - 2026-01-01"));
    }

    #[test]
    fn marks_breaking_commits() {
        let text = "## Unreleased\n";
        let commits = [commit("feat", None, true, "feat!: remove legacy api")];
        let out = roll_text(
            text,
            Version::parse("5.0.0").unwrap(),
            &commits,
            "2026-06-03",
        )
        .unwrap();
        assert!(out.contains("- [BREAKING] feat!: remove legacy api"));
    }

    #[test]
    fn breaking_chore_still_listed() {
        let text = "## Unreleased\n";
        let commits = [commit("chore", None, true, "chore!: drop config key")];
        let out = roll_text(
            text,
            Version::parse("5.0.0").unwrap(),
            &commits,
            "2026-06-03",
        )
        .unwrap();
        assert!(out.contains("- [BREAKING] chore!: drop config key"));
    }

    #[test]
    fn empty_commits_get_maintenance_line() {
        let text = "## Unreleased\n";
        let out = roll_text(text, Version::parse("4.0.1").unwrap(), &[], "2026-06-03").unwrap();
        assert!(out.contains("- Maintenance and internal changes."));
    }

    #[test]
    fn errors_when_no_unreleased_heading() {
        let text = "# Changelog\n\n## v4.0.0 - 2026-01-01\n";
        assert!(roll_text(text, Version::parse("4.0.1").unwrap(), &[], "2026-06-03").is_err());
    }
}
