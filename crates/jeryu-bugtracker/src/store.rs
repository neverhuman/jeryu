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

use std::collections::BTreeMap;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::domain::{
    BugAttempt, BugAttemptInput, BugDetail, BugEvent, BugPriority, BugProject, BugProjectInput,
    BugRecord, BugSeverity, BugSort, BugStatus, CanonicalBugReport, generate_bug_id, sort_bugs,
    validate_transition,
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

// ---------------------------------------------------------------------------
// In-memory reference backend (tests + the standalone-crate default).
// ---------------------------------------------------------------------------

#[derive(Default)]
struct State {
    projects: BTreeMap<String, BugProject>,
    bugs: BTreeMap<String, BugRecord>,
    events: Vec<BugEvent>,
    attempts: Vec<BugAttempt>,
    idempotency: BTreeMap<String, String>, // key -> bug_id
    next_event_id: i64,
    next_attempt_id: i64,
}

/// In-memory `BugStore` mirroring `BugTrackerRepo` semantics without a database.
///
/// Suitable for tests and the standalone crate; the durable RedlineDB/SQLite
/// backend implements the same `BugStore` contract later.
#[derive(Default)]
pub struct InMemoryBugStore {
    state: Mutex<State>,
}

impl InMemoryBugStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn append_event_locked(
        state: &mut State,
        bug_id: &str,
        event_type: &str,
        actor: &str,
        payload: serde_json::Value,
    ) {
        state.next_event_id += 1;
        let id = state.next_event_id;
        state.events.push(BugEvent {
            id,
            bug_id: bug_id.to_string(),
            event_type: event_type.to_string(),
            actor: actor.to_string(),
            payload,
            created_at: Utc::now().to_rfc3339(),
        });
    }

    fn attach_attempt_counts_locked(state: &State, bug: &mut BugRecord) {
        let attempts: Vec<&BugAttempt> = state
            .attempts
            .iter()
            .filter(|a| a.bug_id == bug.id)
            .collect();
        bug.attempt_count = attempts.len() as i64;
        bug.failed_attempt_count = attempts
            .iter()
            .filter(|a| a.status.as_str() == "failed")
            .count() as i64;
    }
}

impl BugStore for InMemoryBugStore {
    async fn add_project(&self, input: &BugProjectInput) -> Result<BugProject> {
        let now = Utc::now().to_rfc3339();
        let mut state = self.state.lock().unwrap();
        let created_at = state
            .projects
            .get(&input.alias)
            .map(|p| p.created_at.clone())
            .unwrap_or_else(|| now.clone());
        let project = BugProject {
            alias: input.alias.clone(),
            repo_root: input.repo_root.clone(),
            repo_slug: input.repo_slug.clone(),
            provider_kind: input.provider_kind.clone(),
            provider_project_id: input.provider_project_id.clone(),
            default_branch: input.default_branch.clone(),
            created_at,
            updated_at: now,
        };
        state.projects.insert(input.alias.clone(), project.clone());
        Ok(project)
    }

    async fn project(&self, alias: &str) -> Result<BugProject> {
        let state = self.state.lock().unwrap();
        state
            .projects
            .get(alias)
            .cloned()
            .with_context(|| format!("load bug project {alias}"))
    }

    async fn list_projects(&self) -> Result<Vec<BugProject>> {
        let state = self.state.lock().unwrap();
        Ok(state.projects.values().cloned().collect())
    }

    async fn submit_bug(
        &self,
        report: &CanonicalBugReport,
        idempotency_key: Option<&str>,
        actor: &str,
    ) -> Result<BugRecord> {
        let status = report.validate()?;
        let now = Utc::now();
        let mut state = self.state.lock().unwrap();

        if let Some(key) = idempotency_key
            && let Some(existing_id) = state.idempotency.get(key).cloned()
            && let Some(existing) = state.bugs.get(&existing_id)
        {
            let mut bug = existing.clone();
            Self::attach_attempt_counts_locked(&state, &mut bug);
            return Ok(bug);
        }

        let id = generate_bug_id(report, now);
        let record = BugRecord {
            id: id.clone(),
            title: report.title.clone(),
            source_project: report.source_project.clone(),
            target_project: report.target_project.clone(),
            component: report.component.clone(),
            status,
            severity: report.severity,
            priority: report.priority,
            difficulty: report.difficulty,
            impact: report.impact.clone(),
            security: report.is_security_sensitive(),
            owner: None,
            body: report.clone(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
            attempt_count: 0,
            failed_attempt_count: 0,
        };
        state.bugs.insert(id.clone(), record);
        if let Some(key) = idempotency_key {
            state.idempotency.insert(key.to_string(), id.clone());
        }
        Self::append_event_locked(
            &mut state,
            &id,
            "submitted",
            actor,
            serde_json::json!({ "status": status.as_str() }),
        );
        let mut bug = state.bugs.get(&id).cloned().expect("just inserted");
        Self::attach_attempt_counts_locked(&state, &mut bug);
        Ok(bug)
    }

    async fn list_bugs(
        &self,
        project: Option<&str>,
        status: Option<BugStatus>,
        sort: BugSort,
    ) -> Result<Vec<BugRecord>> {
        let state = self.state.lock().unwrap();
        let mut bugs: Vec<BugRecord> = state
            .bugs
            .values()
            .filter(|bug| match project {
                Some(p) if p != "all" => bug.target_project == p,
                _ => true,
            })
            .filter(|bug| match status {
                Some(s) => bug.status == s,
                None => true,
            })
            .cloned()
            .collect();
        for bug in &mut bugs {
            Self::attach_attempt_counts_locked(&state, bug);
        }
        sort_bugs(&mut bugs, sort);
        Ok(bugs)
    }

    async fn ready_bugs(&self, project: Option<&str>) -> Result<Vec<BugRecord>> {
        let mut ready = self
            .list_bugs(project, Some(BugStatus::Ready), BugSort::Rank)
            .await?;
        ready.retain(|bug| bug.failed_attempt_count < 3);
        Ok(ready)
    }

    async fn show_bug(&self, bug_id: &str) -> Result<BugDetail> {
        let state = self.state.lock().unwrap();
        let mut bug = state
            .bugs
            .get(bug_id)
            .cloned()
            .with_context(|| format!("load bug {bug_id}"))?;
        Self::attach_attempt_counts_locked(&state, &mut bug);
        let mut events: Vec<BugEvent> = state
            .events
            .iter()
            .filter(|e| e.bug_id == bug_id)
            .cloned()
            .collect();
        events.sort_by_key(|e| e.id);
        let mut attempts: Vec<BugAttempt> = state
            .attempts
            .iter()
            .filter(|a| a.bug_id == bug_id)
            .cloned()
            .collect();
        attempts.sort_by_key(|a| a.id);
        Ok(BugDetail {
            bug,
            events,
            attempts,
        })
    }

    async fn update_bug(
        &self,
        bug_id: &str,
        status: Option<BugStatus>,
        severity: Option<BugSeverity>,
        priority: Option<BugPriority>,
        component: Option<&str>,
        owner: Option<&str>,
        actor: &str,
    ) -> Result<BugRecord> {
        let mut state = self.state.lock().unwrap();
        let before = state
            .bugs
            .get(bug_id)
            .cloned()
            .with_context(|| format!("load bug {bug_id}"))?;
        let next_status = status.unwrap_or(before.status);
        validate_transition(before.status, next_status)?;
        let next_severity = severity.unwrap_or(before.severity);
        let next_priority = priority.unwrap_or(before.priority);
        let next_component = component
            .map(ToString::to_string)
            .or(before.component.clone());
        let next_owner = owner.map(ToString::to_string).or(before.owner.clone());
        let now = Utc::now().to_rfc3339();
        {
            let bug = state.bugs.get_mut(bug_id).expect("checked above");
            bug.status = next_status;
            bug.severity = next_severity;
            bug.priority = next_priority;
            bug.component = next_component.clone();
            bug.owner = next_owner.clone();
            bug.updated_at = now;
        }
        Self::append_event_locked(
            &mut state,
            bug_id,
            "triaged",
            actor,
            serde_json::json!({
                "status": next_status.as_str(),
                "severity": next_severity.label(),
                "priority": next_priority.label(),
                "component": next_component,
                "owner": next_owner,
            }),
        );
        let mut bug = state.bugs.get(bug_id).cloned().expect("checked above");
        Self::attach_attempt_counts_locked(&state, &mut bug);
        Ok(bug)
    }

    async fn record_attempt(
        &self,
        bug_id: &str,
        input: &BugAttemptInput,
        actor: &str,
    ) -> Result<BugAttempt> {
        let now = Utc::now().to_rfc3339();
        let mut state = self.state.lock().unwrap();
        if !state.bugs.contains_key(bug_id) {
            bail!("load bug {bug_id}");
        }
        state.next_attempt_id += 1;
        let id = state.next_attempt_id;
        let attempt = BugAttempt {
            id,
            bug_id: bug_id.to_string(),
            agent: input.agent.clone(),
            status: input.status,
            sandbox_path: input.sandbox_path.clone(),
            branch: input.branch.clone(),
            base_sha: input.base_sha.clone(),
            head_sha: input.head_sha.clone(),
            pr_url: input.pr_url.clone(),
            ci_evidence: input.ci_evidence.clone(),
            notes: input.notes.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        state.attempts.push(attempt.clone());
        Self::append_event_locked(
            &mut state,
            bug_id,
            "attempt_recorded",
            actor,
            serde_json::json!({ "status": input.status.as_str(), "attempt_id": id }),
        );
        Ok(attempt)
    }
}
