//! MirrorVault is JitForge Nitro's Phase 9 import, backup, restore, and replay kernel.
//!
//! The crate is deliberately transport-agnostic: it normalizes provider exports
//! into a typed archive, writes deterministic offline bundles, verifies bundle
//! integrity, creates restore plans, detects mirror drift, and constructs safe
//! Git mirror commands. Network fetchers can be layered above this crate; product
//! truth stays in these typed Rust models.

pub mod bundle;
pub mod drift;
pub mod errors;
pub mod github;
pub mod mirror;
pub mod model;
pub mod restore;
pub mod store;

pub use crate::bundle::{read_bundle, verify_bundle, write_bundle, BundleVerification};
pub use crate::drift::{compare_archives, MirrorDriftReport, RepositoryDrift};
pub use crate::errors::{MirrorVaultError, Result};
pub use crate::github::{archive_from_github_value, load_github_export};
pub use crate::mirror::{plan_git_mirror, MirrorMode, MirrorSpec};
pub use crate::model::*;
pub use crate::restore::{plan_restore, InMemoryRestoreTarget, RestoreOptions, RestoreTarget};
pub use crate::store::BundleCatalog;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secret_metadata_never_contains_values() {
        let value = json!({
          "repositories": [{
            "owner": {"login": "acme"},
            "name": "rocket",
            "hooks": [{"id": 1, "name": "web", "config": {"url": "https://ci.invalid/hook", "secret": "super-secret"}}]
          }]
        });
        let archive = archive_from_github_value(value).expect("github archive");
        let hook = &archive.repositories[0].webhooks[0];
        assert!(hook.secret_name.is_some());
        let as_json = serde_json::to_string(hook).expect("serialize hook");
        assert!(!as_json.contains("super-secret"));
    }

    #[test]
    fn archive_counts_are_aggregated() {
        let mut archive = Archive::new(SourceKind::GitHub, "unit-test");
        let mut repo = RepositoryArchive::new("acme", "rocket");
        repo.issues
            .push(NormalizedIssue::new(1, "bug", ObjectState::Open));
        repo.pull_requests
            .push(NormalizedPullRequest::new(2, "fix", ObjectState::Closed));
        repo.releases.push(ReleaseArchive::new("v1.0.0"));
        archive.repositories.push(repo);
        let counts = archive.counts();
        assert_eq!(counts.repositories, 1);
        assert_eq!(counts.issues, 1);
        assert_eq!(counts.pull_requests, 1);
        assert_eq!(counts.releases, 1);
    }
}
