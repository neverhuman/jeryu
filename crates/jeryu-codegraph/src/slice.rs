//! Export slice predicate.
//!
//! A `Slice` is a fail-closed allow-list of repo-relative path prefixes. It is
//! modeled on `jeryu_core::phase7::AgentScope::permits_all` (prefix-based
//! scope), but uses the CORRECTED containment predicate.
//!
//! Correctness contract (the prior authored patch had a tautology bug):
//!
//! A changed repo-relative `candidate` is IN-slice for a given `prefix` iff:
//!   1. `candidate` has NO `..` component (no path traversal), AND
//!   2. `Path::new(candidate).starts_with(prefix)` (component-wise prefix).
//!
//! It must NEVER use `prefix.join(candidate).starts_with(prefix)`, which is
//! always true and therefore admits every path (the tautology bug).
//!
//! Empty allowed-prefixes => deny all (fail-closed).

use std::path::{Component, Path};

/// An allow-list of repo-relative path prefixes that an export may touch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slice {
    /// Allowed repo-relative path prefixes. Empty => deny all.
    pub allowed_prefixes: Vec<String>,
}

/// A changed path that falls outside the slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfSlice {
    /// The repo-relative paths that are not permitted by the slice.
    pub out_of_slice_paths: Vec<String>,
}

impl Slice {
    /// Builds a slice from an iterator of allowed prefixes.
    pub fn new<I, S>(prefixes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_prefixes: prefixes.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns whether a single repo-relative `candidate` is in-slice.
    ///
    /// Fail-closed: with no allowed prefixes, nothing is permitted.
    #[must_use]
    pub fn permits(&self, candidate: &str) -> bool {
        if self.allowed_prefixes.is_empty() {
            return false;
        }
        if has_parent_dir_component(candidate) {
            return false;
        }
        let candidate_path = Path::new(candidate);
        self.allowed_prefixes.iter().any(|prefix| {
            // CORRECTED predicate: the CANDIDATE must start with the PREFIX,
            // component-wise. NEVER `prefix.join(candidate).starts_with(prefix)`.
            candidate_path.starts_with(Path::new(prefix))
        })
    }

    /// Validates every changed file against the slice.
    ///
    /// Returns `Ok(())` when all files are in-slice, otherwise an `OutOfSlice`
    /// listing every offending path (deterministic input order, deduped).
    pub fn slice_permits(&self, changed_files: &[String]) -> Result<(), OutOfSlice> {
        let mut out_of_slice_paths = Vec::new();
        for file in changed_files {
            if !self.permits(file) && !out_of_slice_paths.contains(file) {
                out_of_slice_paths.push(file.clone());
            }
        }
        if out_of_slice_paths.is_empty() {
            Ok(())
        } else {
            Err(OutOfSlice { out_of_slice_paths })
        }
    }

    /// Returns the first changed file that is out of slice, if any.
    #[must_use]
    pub fn first_out_of_slice(&self, changed_files: &[String]) -> Option<String> {
        changed_files
            .iter()
            .find(|file| !self.permits(file))
            .cloned()
    }
}

/// Returns true if the path contains a `..` (parent dir) component.
fn has_parent_dir_component(candidate: &str) -> bool {
    Path::new(candidate)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_allowed_denies_all() {
        let slice = Slice::default();
        assert!(!slice.permits("crates/jeryu-codegraph/src/lib.rs"));
        assert!(slice.slice_permits(&["anything".into()]).is_err());
    }

    #[test]
    fn in_prefix_is_permitted() {
        let slice = Slice::new(["crates/jeryu-codegraph"]);
        assert!(slice.permits("crates/jeryu-codegraph/src/lib.rs"));
        assert!(
            slice
                .slice_permits(&["crates/jeryu-codegraph/src/lib.rs".into()])
                .is_ok()
        );
    }

    #[test]
    fn out_of_prefix_is_denied() {
        let slice = Slice::new(["crates/jeryu-codegraph"]);
        assert!(!slice.permits("crates/jeryu-core/x.rs"));
    }

    #[test]
    fn parent_dir_traversal_is_denied() {
        let slice = Slice::new(["crates/jeryu-codegraph"]);
        assert!(!slice.permits("crates/jeryu-codegraph/../jeryu-core/x.rs"));
    }

    #[test]
    fn tautology_regression_core_not_permitted_by_api_prefix() {
        // The classic tautology bug: `prefix.join(candidate).starts_with(prefix)`
        // would (wrongly) accept this. The corrected predicate rejects it.
        let slice = Slice::new(["crates/jeryu-api"]);
        assert!(!slice.permits("crates/jeryu-core/x.rs"));
    }

    #[test]
    fn partial_component_is_not_a_prefix() {
        // "crates/jeryu-core-extra" must not be treated as inside
        // "crates/jeryu-core" just because the string starts with it.
        let slice = Slice::new(["crates/jeryu-core"]);
        assert!(!slice.permits("crates/jeryu-core-extra/x.rs"));
        assert!(slice.permits("crates/jeryu-core/x.rs"));
    }
}
