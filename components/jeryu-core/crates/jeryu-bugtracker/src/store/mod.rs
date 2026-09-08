//! The store seam: a backend-agnostic `BugStore` trait plus an in-memory impl.
//!
//! In jeryu the durable backend is `BugTrackerRepo` over RedlineDB/SQLite
//! (`sqlx::AnyPool`). To keep this crate self-contained and testable in
//! isolation, persistence lives behind [`BugStore`]; [`InMemoryBugStore`]
//! satisfies the contract for tests and the durable RedlineDB impl wires in
//! later without touching the domain or the triage logic above it.
//!
//! The trait is async (a durable backend does I/O) and object-safe via
//! `async_trait`-free hand-written futures-free signatures using
//! `impl std::future::Future`. To stay dependency-light we instead express the
//! trait with `async fn` in traits (stable on edition 2024) and require `Send`
//! bounds only where the in-memory impl naturally provides them.

mod memory;

pub use memory::InMemoryBugStore;

use anyhow::Result;

use crate::domain::{
    BugAttempt, BugAttemptInput, BugDetail, BugPriority, BugProject, BugProjectInput, BugRecord,
    BugSeverity, BugSort, BugStatus, CanonicalBugReport,
};

/// Backend-agnostic persistence + triage contract for the bug domain.
///
/// Implementors own durability and concurrency; the domain (`validate`,
/// `generate_bug_id`, `validate_transition`, `ranking_key`) is reused verbatim
/// so every backend agrees on intake gating, id minting, transition rules, and
/// triage ordering. NO remote issue tracker is implied: `provider_kind`/
/// `provider_project_id` describe a local-or-host repo, never a forge issue.
pub trait BugStore {
    /// Idempotent project upsert keyed on `alias`.
    fn add_project(
        &self,
        input: &BugProjectInput,
    ) -> impl std::future::Future<Output = Result<BugProject>> + Send;

    /// Fetch a single project by alias.
    fn project(&self, alias: &str) -> impl std::future::Future<Output = Result<BugProject>> + Send;

    /// All projects, sorted by alias.
    fn list_projects(&self) -> impl std::future::Future<Output = Result<Vec<BugProject>>> + Send;

    /// Validate and store a report; honors `idempotency_key` (returns the
    /// existing bug on a repeat key). Derives status via `report.validate()`.
    fn submit_bug(
        &self,
        report: &CanonicalBugReport,
        idempotency_key: Option<&str>,
        actor: &str,
    ) -> impl std::future::Future<Output = Result<BugRecord>> + Send;

    /// List bugs, optionally filtered by project/status, ordered by `sort`.
    fn list_bugs(
        &self,
        project: Option<&str>,
        status: Option<BugStatus>,
        sort: BugSort,
    ) -> impl std::future::Future<Output = Result<Vec<BugRecord>>> + Send;

    /// Ready, rank-ordered, actionable bugs (status == Ready, < 3 failed attempts).
    fn ready_bugs(
        &self,
        project: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<BugRecord>>> + Send;

    /// A single bug with its events and attempt history.
    fn show_bug(&self, bug_id: &str)
    -> impl std::future::Future<Output = Result<BugDetail>> + Send;

    /// Triage update; enforces `validate_transition` (terminal bugs cannot reopen).
    #[allow(clippy::too_many_arguments)] // CLI/MCP triage surface is intentionally flat.
    fn update_bug(
        &self,
        bug_id: &str,
        status: Option<BugStatus>,
        severity: Option<BugSeverity>,
        priority: Option<BugPriority>,
        component: Option<&str>,
        owner: Option<&str>,
        actor: &str,
    ) -> impl std::future::Future<Output = Result<BugRecord>> + Send;

    /// Append an attempt to a bug's history and re-derive attempt counts.
    fn record_attempt(
        &self,
        bug_id: &str,
        input: &BugAttemptInput,
        actor: &str,
    ) -> impl std::future::Future<Output = Result<BugAttempt>> + Send;
}
