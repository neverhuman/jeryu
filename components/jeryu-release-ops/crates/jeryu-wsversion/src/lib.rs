//! `jeryu-wsversion` — the workspace auto-versioning engine.
//!
//! The system decides version numbers with zero human edits: it (1) reads the
//! conventional commits in a `base..head` range, (2) classifies them into a
//! bump level (breaking public-API change or `feat!`/`BREAKING CHANGE` ->
//! MAJOR; `feat` -> MINOR; everything else -> PATCH; floor = PATCH so every
//! merge bumps at least patch), (3) rewrites the single
//! `[workspace.package].version` line in the root `Cargo.toml`, and (4) rolls
//! the `## Unreleased` CHANGELOG section into a dated `## vX.Y.Z` entry.
//!
//! Breaking public-API detection REUSES `jeryu_rustjet`'s [`PublicApiDetector`]
//! / [`PublicApiChange`] and the same `cargo semver-checks` integration the
//! rustjet classifier names; the blast-radius package set REUSES
//! `jeryu_repogate::build_affected_plan`. The engine never reimplements API
//! diffing or the dependency-graph walk.
//!
//! Modules:
//! * [`commits`] — parse conventional-commit subjects over a git range.
//! * [`classify`] — map commits (+ API signal) to a [`Bump`] level.
//! * [`api_surface`] — breaking public-API detection (the MAJOR signal).
//! * [`cargo_edit`] — read/write `[workspace.package].version` + inherit guard.
//! * [`changelog`] — generate/prepend CHANGELOG entries.
//! * [`decide`] — combine all signals into a final [`Decision`] (+ `apply`).
//! * [`semver`] — minimal `X.Y.Z` parse/bump.
//! * [`outcome`] — shared gate-outcome type for the CLI.
//!
//! [`PublicApiDetector`]: jeryu_rustjet::public_api::PublicApiDetector
//! [`PublicApiChange`]: jeryu_rustjet::public_api::PublicApiChange

pub mod api_surface;
pub mod cargo_edit;
pub mod changelog;
pub mod classify;
pub mod commits;
pub mod decide;
pub mod outcome;
pub mod semver;

pub use api_surface::{ApiSurfaceReport, api_breaking, detect_public_api_candidates};
pub use cargo_edit::{assert_members_inherit, read_workspace_version, write_workspace_version};
pub use changelog::roll_unreleased;
pub use classify::{Bump, classify_commit, classify_range};
pub use commits::{ConventionalCommit, SKIP_VERSION_SENTINEL, commits_in_range};
pub use decide::{Decision, apply, decide};
pub use outcome::GateOutcome;
pub use semver::Version;
