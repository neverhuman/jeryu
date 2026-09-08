//! Minimal `X.Y.Z` version parser and bump applier.
//!
//! Workspace versions are plain `major.minor.patch` triples, so a purpose-built
//! parser avoids pulling in the full `semver` crate. Bumping follows the usual
//! SemVer reset rules: a major bump zeroes minor and patch, a minor bump zeroes
//! patch, and a patch bump increments patch only.

use anyhow::{Context, Result, ensure};

use crate::classify::Bump;

/// A parsed `major.minor.patch` version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Major component (breaking-change axis).
    pub major: u64,
    /// Minor component (feature axis).
    pub minor: u64,
    /// Patch component (fix axis).
    pub patch: u64,
}

impl Version {
    /// Parse a plain `X.Y.Z` version string, rejecting any other shape.
    ///
    /// # Errors
    /// Returns an error if `s` is not exactly three dot-separated unsigned
    /// integers.
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<_> = s.trim().split('.').collect();
        ensure!(parts.len() == 3, "non X.Y.Z version: {s}");
        Ok(Self {
            major: parts[0]
                .parse()
                .with_context(|| format!("invalid major in version {s}"))?,
            minor: parts[1]
                .parse()
                .with_context(|| format!("invalid minor in version {s}"))?,
            patch: parts[2]
                .parse()
                .with_context(|| format!("invalid patch in version {s}"))?,
        })
    }

    /// Apply a [`Bump`], returning the next version with SemVer reset rules.
    ///
    /// `Bump::None` is treated as `Bump::Patch` so every merge advances at least
    /// the patch component; callers floor `None` to `Patch` upstream as well.
    #[must_use]
    pub fn bumped(self, b: Bump) -> Self {
        match b {
            Bump::Major => Self {
                major: self.major + 1,
                minor: 0,
                patch: 0,
            },
            Bump::Minor => Self {
                major: self.major,
                minor: self.minor + 1,
                patch: 0,
            },
            Bump::Patch | Bump::None => Self {
                patch: self.patch + 1,
                ..self
            },
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_triple() {
        let v = Version::parse(" 4.0.0 ").unwrap();
        assert_eq!(
            v,
            Version {
                major: 4,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn rejects_non_triple() {
        assert!(Version::parse("4.0").is_err());
        assert!(Version::parse("4.0.0-rc1").is_err());
        assert!(Version::parse("v4.0.0").is_err());
    }

    #[test]
    fn patch_bump_increments_patch_only() {
        let v = Version::parse("4.0.0").unwrap().bumped(Bump::Patch);
        assert_eq!(v.to_string(), "4.0.1");
    }

    #[test]
    fn minor_bump_resets_patch() {
        let v = Version::parse("4.2.7").unwrap().bumped(Bump::Minor);
        assert_eq!(v.to_string(), "4.3.0");
    }

    #[test]
    fn major_bump_resets_minor_and_patch() {
        let v = Version::parse("4.2.7").unwrap().bumped(Bump::Major);
        assert_eq!(v.to_string(), "5.0.0");
    }

    #[test]
    fn none_bump_floors_to_patch() {
        let v = Version::parse("4.0.0").unwrap().bumped(Bump::None);
        assert_eq!(v.to_string(), "4.0.1");
    }
}
