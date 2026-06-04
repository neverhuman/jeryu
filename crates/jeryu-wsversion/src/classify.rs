//! Bump-level classification: commit text + public-API signal -> [`Bump`].
//!
//! Rules (per the WS-VERSION design):
//! * breaking public-API change OR `feat!`/`BREAKING CHANGE` -> MAJOR
//! * `feat` -> MINOR
//! * everything else -> PATCH
//! * floor = PATCH: every merge to main bumps at least the patch component.

use crate::commits::ConventionalCommit;

/// A SemVer bump level. Ordering is `None < Patch < Minor < Major`, so the
/// bump for a range is the maximum bump across its commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    /// No bump implied by the commit.
    None,
    /// Patch-level bump (fixes, chores, docs, everything non-feature).
    Patch,
    /// Minor-level bump (new features).
    Minor,
    /// Major-level bump (breaking changes).
    Major,
}

impl Bump {
    /// Lowercase label used in the [`crate::decide::Decision`] JSON.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Bump::None => "none",
            Bump::Patch => "patch",
            Bump::Minor => "minor",
            Bump::Major => "major",
        }
    }
}

/// Classify a single commit into its implied bump level.
#[must_use]
pub fn classify_commit(c: &ConventionalCommit) -> Bump {
    if c.breaking {
        return Bump::Major; // `!` suffix or BREAKING CHANGE footer
    }
    match c.kind.as_str() {
        "feat" => Bump::Minor,
        // fix/perf/refactor/build/revert and all other types (docs, chore, ci,
        // test, style, ...) still bump patch — every merge advances the patch.
        _ => Bump::Patch,
    }
}

/// Classify a whole range: the maximum per-commit bump, overridden to MAJOR by a
/// breaking public-API signal, then floored to PATCH (auto-patch every merge).
#[must_use]
pub fn classify_range(commits: &[ConventionalCommit], api_breaking: bool) -> Bump {
    let mut bump = commits
        .iter()
        .map(classify_commit)
        .max()
        .unwrap_or(Bump::Patch);
    if api_breaking {
        bump = Bump::Major; // public-API breakage overrides commit text
    }
    if bump == Bump::None {
        bump = Bump::Patch; // RULE: every merge to main is at least a patch
    }
    bump
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(kind: &str, breaking: bool) -> ConventionalCommit {
        ConventionalCommit {
            sha: "deadbeef".into(),
            kind: kind.into(),
            scope: None,
            breaking,
            subject: format!("{kind}: subject"),
        }
    }

    #[test]
    fn feat_is_minor() {
        assert_eq!(classify_commit(&commit("feat", false)), Bump::Minor);
    }

    #[test]
    fn fix_is_patch() {
        assert_eq!(classify_commit(&commit("fix", false)), Bump::Patch);
    }

    #[test]
    fn chore_is_patch() {
        assert_eq!(classify_commit(&commit("chore", false)), Bump::Patch);
    }

    #[test]
    fn breaking_bang_is_major() {
        assert_eq!(classify_commit(&commit("feat", true)), Bump::Major);
        assert_eq!(classify_commit(&commit("fix", true)), Bump::Major);
    }

    #[test]
    fn range_takes_max_bump() {
        let commits = [
            commit("fix", false),
            commit("feat", false),
            commit("docs", false),
        ];
        assert_eq!(classify_range(&commits, false), Bump::Minor);
    }

    #[test]
    fn empty_range_floors_to_patch() {
        assert_eq!(classify_range(&[], false), Bump::Patch);
    }

    #[test]
    fn chore_only_range_floors_to_patch() {
        let commits = [commit("chore", false), commit("docs", false)];
        assert_eq!(classify_range(&commits, false), Bump::Patch);
    }

    #[test]
    fn api_breaking_overrides_to_major_even_for_fix() {
        let commits = [commit("fix", false)];
        assert_eq!(classify_range(&commits, true), Bump::Major);
    }

    #[test]
    fn api_breaking_overrides_empty_range() {
        assert_eq!(classify_range(&[], true), Bump::Major);
    }
}
