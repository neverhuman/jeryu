//! # jeryu-bugtracker
//!
//! The canonical jeryu bug domain — `CanonicalBugReport` intake, the
//! severity/priority/status/sort/attempt enums, persisted records, attempt
//! history, and pure triage ops (`generate_bug_id`, `branch_name`,
//! `validate_transition`, `ranking_key`) — plus the CRUD + triage surface
//! (`submit`/`list`/`show`/`ready`/`update`/`record_attempt`) behind a
//! backend-agnostic [`store::BugStore`] seam.
//!
//! Persistence is intentionally pluggable. jeryu's durable backend is
//! RedlineDB/SQLite (`sqlx::AnyPool`); this crate keeps that out of the domain
//! so it compiles and tests in isolation. [`store::InMemoryBugStore`] is the
//! reference backend used by tests; the durable RedlineDB impl satisfies the
//! same `BugStore` contract later, with zero changes to the domain or triage
//! logic.
//!
//! There is NO remote issue tracker here: bug intake is the canonical
//! `CanonicalBugReport`, attempt URLs are PR-named (`pr_url`), and
//! `provider_kind`/`provider_project_id` describe a local-or-host repo.

pub mod domain;
pub mod render;
pub mod store;

pub use domain::{
    AttemptStatus, BugAttempt, BugAttemptInput, BugDetail, BugEvent, BugEvidenceInput, BugPriority,
    BugProject, BugProjectInput, BugRecord, BugSeverity, BugSort, BugStatus, CanonicalBugReport,
    branch_name, generate_bug_id, parse_report_json, ranking_key, sort_bugs, validate_transition,
};
pub use render::canonical_markdown;
pub use store::{BugStore, InMemoryBugStore};
