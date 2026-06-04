//! Breaking public-API detection — the major-bump signal.
//!
//! Two layered signals are combined; either firing forces a MAJOR bump:
//!
//! * SIGNAL A (zero deps, always available): the conventional-commit `!` /
//!   `BREAKING CHANGE` marker, already carried on
//!   [`crate::commits::ConventionalCommit::breaking`] and handled in
//!   [`crate::classify`].
//! * SIGNAL B (real public-API diff): this module. It REUSES
//!   `jeryu_rustjet`'s [`PublicApiDetector`]/[`PublicApiChange`] over the
//!   workspace manifest to find changed public surfaces, then SHELLS OUT to
//!   `cargo semver-checks check-release` — the exact tool the rustjet
//!   classifier's `semver` lane already names (`classifier/derive.rs`). The
//!   engine never reimplements API diffing.
//!
//! When `cargo-semver-checks` is not installed the result is recorded honestly
//! (no faking) and the engine falls back to SIGNAL A only.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use jeryu_rustjet::manifest::WorkspaceManifest;
use jeryu_rustjet::public_api::{PublicApiChange, PublicApiDetector};

/// Outcome of the public-API breaking-change probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiSurfaceReport {
    /// `true` if a breaking public-API change was detected (forces MAJOR).
    pub breaking: bool,
    /// Human-readable explanation recorded in the gate outcome.
    pub detail: String,
    /// Candidate public-surface changes found by the rustjet detector.
    pub candidates: Vec<PublicApiChange>,
    /// Whether `cargo-semver-checks` was available and run.
    pub tool_ran: bool,
}

/// Detect candidate public-API surface changes by replaying `changed_files`
/// through `jeryu_rustjet`'s [`PublicApiDetector`] against the workspace
/// manifest. This is the zero-dependency surface signal; it does not by itself
/// decide breakage (a changed public file may be additive), but it scopes which
/// packages are worth running `cargo semver-checks` against.
///
/// # Errors
/// Returns an error if the workspace manifest cannot be loaded.
pub fn detect_public_api_candidates(
    root: &Path,
    changed_files: &[String],
) -> Result<Vec<PublicApiChange>> {
    let manifest = WorkspaceManifest::load(root)
        .map_err(|e| anyhow::anyhow!("load workspace manifest: {e}"))?;
    let detector = PublicApiDetector::new();
    let mut changes = Vec::new();
    for file in changed_files {
        for package in manifest.packages.values() {
            if let Some(inside) = package.path_inside_package(file)
                && let Some(change) = detector.detect(package, inside)
            {
                changes.push(change);
            }
        }
    }
    changes.sort_by(|a, b| {
        (a.package.clone(), a.path.clone()).cmp(&(b.package.clone(), b.path.clone()))
    });
    changes.dedup();
    Ok(changes)
}

/// Probe whether the change set constitutes a breaking public-API change.
///
/// `changed_pkgs` is the blast-radius package set (from
/// `jeryu_repogate::build_affected_plan(...).packages`). When
/// `cargo-semver-checks` is present, it is invoked as the rustjet `semver` lane
/// names it and a non-zero exit is treated as breaking. When it is absent, the
/// report records that honestly and `breaking` stays `false` (SIGNAL A still
/// applies independently in [`crate::classify`]).
///
/// # Errors
/// Returns an error only if the manifest cannot be loaded; tool absence or a
/// tool failure to spawn is recorded in the report, not surfaced as an error.
pub fn api_breaking(
    root: &Path,
    changed_files: &[String],
    changed_pkgs: &[String],
) -> Result<ApiSurfaceReport> {
    let candidates = detect_public_api_candidates(root, changed_files)?;

    if changed_pkgs.is_empty() {
        return Ok(ApiSurfaceReport {
            breaking: false,
            detail: "no changed packages in blast radius; no public-API check needed".into(),
            candidates,
            tool_ran: false,
        });
    }

    if !semver_checks_available(root) {
        return Ok(ApiSurfaceReport {
            breaking: false,
            detail: "cargo-semver-checks not installed; relied on commit `!`/footer signal only"
                .into(),
            candidates,
            tool_ran: false,
        });
    }

    // Mirror the rustjet classifier's `semver` lane: `cargo semver-checks
    // check-release` scoped to the changed packages.
    let mut args = vec!["semver-checks".to_string(), "check-release".to_string()];
    for pkg in changed_pkgs {
        args.push("-p".to_string());
        args.push(pkg.clone());
    }
    let status = Command::new("cargo").args(&args).current_dir(root).status();
    match status {
        Ok(st) => {
            let breaking = !st.success();
            Ok(ApiSurfaceReport {
                detail: if breaking {
                    "cargo semver-checks reported a major-requiring change".into()
                } else {
                    "cargo semver-checks found no breaking change".into()
                },
                breaking,
                candidates,
                tool_ran: true,
            })
        }
        Err(e) => Ok(ApiSurfaceReport {
            breaking: false,
            detail: format!(
                "cargo semver-checks failed to spawn ({e}); fell back to commit signal"
            ),
            candidates,
            tool_ran: false,
        }),
    }
}

/// Probe for `cargo-semver-checks` by asking cargo for its version. A clean exit
/// means the subcommand is installed.
fn semver_checks_available(root: &Path) -> bool {
    Command::new("cargo")
        .args(["semver-checks", "--version"])
        .current_dir(root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn init_workspace(root: &Path) {
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/demo\"]\n",
        );
        write(
            root,
            "crates/demo/Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        write(root, "crates/demo/src/lib.rs", "pub fn api() {}\n");
    }

    #[test]
    fn detects_public_surface_in_changed_lib() {
        let dir = tempdir().unwrap();
        init_workspace(dir.path());
        let candidates =
            detect_public_api_candidates(dir.path(), &["crates/demo/src/lib.rs".into()]).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].package, "demo");
    }

    #[test]
    fn non_rust_change_yields_no_candidate() {
        let dir = tempdir().unwrap();
        init_workspace(dir.path());
        let candidates =
            detect_public_api_candidates(dir.path(), &["crates/demo/README.md".into()]).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn empty_packages_means_not_breaking() {
        let dir = tempdir().unwrap();
        init_workspace(dir.path());
        let report = api_breaking(dir.path(), &[], &[]).unwrap();
        assert!(!report.breaking);
        assert!(!report.tool_ran);
    }
}
