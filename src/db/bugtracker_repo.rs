//! Owner: bug-tracker db boundary
//! Proof: `cargo test -p jeryu --lib db::bugtracker_repo`
//! Invariants: all bug tracker persistence uses RedlineDB through this typed repo.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{AnyPool, Row};

use crate::bugtracker::{
    BugAttempt, BugAttemptInput, BugDetail, BugEvent, BugPriority, BugProject, BugProjectInput,
    BugRecord, BugSeverity, BugSort, BugStatus, CanonicalBugReport, generate_bug_id,
    validate_transition,
};

#[path = "bugtracker_repo_decode.rs"]
mod bugtracker_repo_decode;
use bugtracker_repo_decode::{base_select_with, decode_attempt, decode_bug_record, sort_bugs};

#[path = "bugtracker_repo_schema.rs"]
mod bugtracker_repo_schema;
pub use bugtracker_repo_schema::bugtracker_schema_ddl;

#[cfg(test)]
pub(crate) use bugtracker_repo_schema::fresh_bugtracker_pool;

#[cfg(test)]
#[path = "bugtracker_repo_tests.rs"]
mod tests;

#[derive(Debug, Clone)]
pub struct BugTrackerRepo {
    pool: AnyPool,
}

impl BugTrackerRepo {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub async fn open_default() -> Result<Self> {
        let db = crate::state::Db::open().await?;
        Ok(Self::new(db.pool()))
    }

    pub async fn install_schema(&self) -> Result<()> {
        for statement in bugtracker_schema_ddl().split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement)
                    .execute(&self.pool)
                    .await
                    .with_context(|| format!("install bug tracker schema: {statement}"))?;
            }
        }
        Ok(())
    }

    pub async fn add_project(&self, input: &BugProjectInput) -> Result<BugProject> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO bug_projects
                (alias, repo_root, repo_slug, provider_kind, provider_project_id, default_branch, metadata_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, '{}', ?, ?)
             ON CONFLICT(alias) DO UPDATE SET
                repo_root = excluded.repo_root,
                repo_slug = excluded.repo_slug,
                provider_kind = excluded.provider_kind,
                provider_project_id = excluded.provider_project_id,
                default_branch = excluded.default_branch,
                updated_at = excluded.updated_at",
        )
        .bind(&input.alias)
        .bind(&input.repo_root)
        .bind(&input.repo_slug)
        .bind(&input.provider_kind)
        .bind(&input.provider_project_id)
        .bind(&input.default_branch)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("upsert bug project")?;
        self.project(&input.alias).await
    }

    pub async fn project(&self, alias: &str) -> Result<BugProject> {
        let row = sqlx::query(
            "SELECT alias, repo_root, repo_slug, provider_kind, provider_project_id, default_branch, created_at, updated_at
             FROM bug_projects WHERE alias = ?",
        )
        .bind(alias)
        .fetch_one(&self.pool)
        .await
        .with_context(|| format!("load bug project {alias}"))?;
        Ok(BugProject {
            alias: row.try_get("alias")?,
            repo_root: row.try_get("repo_root")?,
            repo_slug: row.try_get("repo_slug")?,
            provider_kind: row.try_get("provider_kind")?,
            provider_project_id: row.try_get("provider_project_id")?,
            default_branch: row.try_get("default_branch")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    pub async fn list_projects(&self) -> Result<Vec<BugProject>> {
        let rows = sqlx::query(
            "SELECT alias, repo_root, repo_slug, provider_kind, provider_project_id, default_branch, created_at, updated_at
             FROM bug_projects ORDER BY alias ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("list bug projects")?;
        rows.into_iter()
            .map(|row| {
                Ok(BugProject {
                    alias: row.try_get("alias")?,
                    repo_root: row.try_get("repo_root")?,
                    repo_slug: row.try_get("repo_slug")?,
                    provider_kind: row.try_get("provider_kind")?,
                    provider_project_id: row.try_get("provider_project_id")?,
                    default_branch: row.try_get("default_branch")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    pub async fn link_projects(&self, source: &str, target: &str, kind: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO bug_project_edges (source_project, target_project, kind, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(source)
        .bind(target)
        .bind(kind)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("insert project edge")?;
        Ok(())
    }

    pub async fn submit_bug(
        &self,
        report: &CanonicalBugReport,
        idempotency_key: Option<&str>,
        actor: &str,
    ) -> Result<BugRecord> {
        let status = report.validate()?;
        if let Some(key) = idempotency_key
            && let Some(existing) = self.by_idempotency_key(key).await?
        {
            return Ok(existing);
        }
        let now = Utc::now();
        let id = generate_bug_id(report, now);
        let security = !matches!(
            report.security_privacy.trim().to_ascii_lowercase().as_str(),
            "no" | "none" | "no security impact" | "no privacy impact"
        );
        let body_json = serde_json::to_string(report).context("serialize canonical bug body")?;
        sqlx::query(
            "INSERT INTO bugs
                (id, title, source_project, target_project, component, status, severity, priority, difficulty,
                 repro_state, impact, security, owner, created_at, updated_at, body_json, idempotency_key)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&report.title)
        .bind(&report.source_project)
        .bind(&report.target_project)
        .bind(&report.component)
        .bind(status.as_str())
        .bind(report.severity.label())
        .bind(report.priority.label())
        .bind(i64::from(report.difficulty))
        .bind(if status == BugStatus::NeedsInfo { "missing_repro_or_evidence" } else { "provided" })
        .bind(&report.impact)
        .bind(if security { 1_i64 } else { 0_i64 })
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(body_json)
        .bind(idempotency_key)
        .execute(&self.pool)
        .await
        .context("insert bug")?;
        self.append_event(
            &id,
            "submitted",
            actor,
            serde_json::json!({"status": status.as_str()}),
        )
        .await?;
        self.show_bug(&id).await.map(|detail| detail.bug)
    }

    pub async fn list_bugs(
        &self,
        project: Option<&str>,
        status: Option<BugStatus>,
        sort: BugSort,
    ) -> Result<Vec<BugRecord>> {
        let rows = match (project, status) {
            (Some(project), Some(status)) if project != "all" => {
                let sql = base_select_with("WHERE target_project = ? AND status = ?");
                sqlx::query(&sql)
                    .bind(project)
                    .bind(status.as_str())
                    .fetch_all(&self.pool)
                    .await?
            }
            (Some(project), None) if project != "all" => {
                let sql = base_select_with("WHERE target_project = ?");
                sqlx::query(&sql)
                    .bind(project)
                    .fetch_all(&self.pool)
                    .await?
            }
            (_, Some(status)) => {
                let sql = base_select_with("WHERE status = ?");
                sqlx::query(&sql)
                    .bind(status.as_str())
                    .fetch_all(&self.pool)
                    .await?
            }
            _ => {
                let sql = base_select_with("");
                sqlx::query(&sql).fetch_all(&self.pool).await?
            }
        };
        let mut bugs = rows
            .into_iter()
            .map(decode_bug_record)
            .collect::<Result<Vec<_>>>()?;
        self.attach_attempt_counts(&mut bugs).await?;
        sort_bugs(&mut bugs, sort);
        Ok(bugs)
    }

    pub async fn ready_bugs(&self, project: Option<&str>) -> Result<Vec<BugRecord>> {
        let mut ready = self
            .list_bugs(project, Some(BugStatus::Ready), BugSort::Rank)
            .await?;
        ready.retain(|bug| bug.failed_attempt_count < 3);
        Ok(ready)
    }

    pub async fn show_bug(&self, bug_id: &str) -> Result<BugDetail> {
        let sql = base_select_with("WHERE id = ?");
        let mut bug = sqlx::query(&sql)
            .bind(bug_id)
            .fetch_one(&self.pool)
            .await
            .with_context(|| format!("load bug {bug_id}"))
            .and_then(decode_bug_record)?;
        self.attach_attempt_counts(std::slice::from_mut(&mut bug))
            .await?;
        let events = self.events(bug_id).await?;
        let attempts = self.attempts(bug_id).await?;
        Ok(BugDetail {
            bug,
            events,
            attempts,
        })
    }

    #[allow(clippy::too_many_arguments)] // CLI/MCP triage surface is intentionally flat.
    pub async fn update_bug(
        &self,
        bug_id: &str,
        status: Option<BugStatus>,
        severity: Option<BugSeverity>,
        priority: Option<BugPriority>,
        component: Option<&str>,
        owner: Option<&str>,
        actor: &str,
    ) -> Result<BugRecord> {
        let before = self.show_bug(bug_id).await?.bug;
        let next_status = status.unwrap_or(before.status);
        validate_transition(before.status, next_status)?;
        let next_severity = severity.unwrap_or(before.severity);
        let next_priority = priority.unwrap_or(before.priority);
        let next_component = component.map(ToString::to_string).or(before.component);
        let next_owner = owner.map(ToString::to_string).or(before.owner);
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE bugs
             SET status = ?, severity = ?, priority = ?, component = ?, owner = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(next_status.as_str())
        .bind(next_severity.label())
        .bind(next_priority.label())
        .bind(&next_component)
        .bind(&next_owner)
        .bind(&now)
        .bind(bug_id)
        .execute(&self.pool)
        .await
        .context("update bug")?;
        self.append_event(
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
        )
        .await?;
        self.show_bug(bug_id).await.map(|detail| detail.bug)
    }

    pub async fn link_bugs(
        &self,
        bug_id: &str,
        other_id: &str,
        kind: &str,
        actor: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO bug_links (bug_id, linked_bug_id, kind, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(bug_id)
        .bind(other_id)
        .bind(kind)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("insert bug link")?;
        self.append_event(
            bug_id,
            "linked",
            actor,
            serde_json::json!({"kind": kind, "other": other_id}),
        )
        .await
    }

    pub async fn record_attempt(
        &self,
        bug_id: &str,
        input: &BugAttemptInput,
        actor: &str,
    ) -> Result<BugAttempt> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO bug_attempts
                (bug_id, agent, status, sandbox_path, branch, base_sha, head_sha, pr_url, ci_evidence, notes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(bug_id)
        .bind(&input.agent)
        .bind(input.status.as_str())
        .bind(&input.sandbox_path)
        .bind(&input.branch)
        .bind(&input.base_sha)
        .bind(&input.head_sha)
        .bind(&input.pr_url)
        .bind(&input.ci_evidence)
        .bind(&input.notes)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .context("insert bug attempt")?;
        let id: i64 = sqlx::query_scalar(
            "SELECT id FROM bug_attempts WHERE bug_id = ? ORDER BY id DESC LIMIT 1",
        )
        .bind(bug_id)
        .fetch_one(&self.pool)
        .await
        .context("load inserted bug attempt id")?;
        self.append_event(
            bug_id,
            "attempt_recorded",
            actor,
            serde_json::json!({"status": input.status.as_str(), "attempt_id": id}),
        )
        .await?;
        self.attempt(id).await
    }

    async fn by_idempotency_key(&self, key: &str) -> Result<Option<BugRecord>> {
        let sql = base_select_with("WHERE idempotency_key = ?");
        let row = sqlx::query(&sql)
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .context("lookup bug idempotency key")?;
        let mut bug = row.map(decode_bug_record).transpose()?;
        if let Some(bug) = bug.as_mut() {
            self.attach_attempt_counts(std::slice::from_mut(bug))
                .await?;
        }
        Ok(bug)
    }

    async fn attach_attempt_counts(&self, bugs: &mut [BugRecord]) -> Result<()> {
        for bug in bugs {
            let rows = sqlx::query("SELECT status FROM bug_attempts WHERE bug_id = ?")
                .bind(&bug.id)
                .fetch_all(&self.pool)
                .await
                .context("count bug attempts")?;
            bug.attempt_count = rows.len() as i64;
            bug.failed_attempt_count = rows
                .iter()
                .filter_map(|row| row.try_get::<String, _>("status").ok())
                .filter(|status| status == "failed")
                .count() as i64;
        }
        Ok(())
    }

    async fn append_event(
        &self,
        bug_id: &str,
        event_type: &str,
        actor: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO bug_events (bug_id, event_type, actor, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(bug_id)
        .bind(event_type)
        .bind(actor)
        .bind(serde_json::to_string(&payload)?)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .context("append bug event")?;
        Ok(())
    }

    async fn events(&self, bug_id: &str) -> Result<Vec<BugEvent>> {
        let rows = sqlx::query(
            "SELECT id, bug_id, event_type, actor, payload_json, created_at
             FROM bug_events WHERE bug_id = ? ORDER BY id ASC",
        )
        .bind(bug_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let payload_json: String = row.try_get("payload_json")?;
                Ok(BugEvent {
                    id: row.try_get("id")?,
                    bug_id: row.try_get("bug_id")?,
                    event_type: row.try_get("event_type")?,
                    actor: row.try_get("actor")?,
                    payload: serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null),
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn attempts(&self, bug_id: &str) -> Result<Vec<BugAttempt>> {
        let rows = sqlx::query(
            "SELECT id, bug_id, agent, status, sandbox_path, branch, base_sha, head_sha, pr_url, ci_evidence, notes, created_at, updated_at
             FROM bug_attempts WHERE bug_id = ? ORDER BY id ASC",
        )
        .bind(bug_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(decode_attempt).collect()
    }

    async fn attempt(&self, id: i64) -> Result<BugAttempt> {
        let row = sqlx::query(
            "SELECT id, bug_id, agent, status, sandbox_path, branch, base_sha, head_sha, pr_url, ci_evidence, notes, created_at, updated_at
             FROM bug_attempts WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        decode_attempt(row)
    }
}
