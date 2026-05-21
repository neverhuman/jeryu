//! Owner: bug-tracker db boundary
//! Proof: `cargo test -p jeryu --lib db::bugtracker_repo`
//! Invariants: all bug tracker persistence uses RedlineDB through this typed repo.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use sqlx::{AnyPool, Row};

use crate::bugtracker::{
    AttemptStatus, BugAttempt, BugAttemptInput, BugDetail, BugEvent, BugPriority, BugProject,
    BugProjectInput, BugRecord, BugSeverity, BugSort, BugStatus, CanonicalBugReport,
    generate_bug_id, ranking_key, validate_transition,
};

#[derive(Debug, Clone)]
pub struct BugTrackerRepo {
    pool: AnyPool,
}

impl BugTrackerRepo {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
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

fn base_select_with(where_clause: &str) -> String {
    format!(
        "SELECT id, title, source_project, target_project, component, status, severity, priority,
                difficulty, impact, security, owner, body_json, created_at, updated_at
         FROM bugs
         {where_clause}"
    )
}

fn decode_bug_record(row: sqlx::any::AnyRow) -> Result<BugRecord> {
    let body_json: String = row.try_get("body_json")?;
    let body: CanonicalBugReport = serde_json::from_str(&body_json).context("decode bug body")?;
    let status_s: String = row.try_get("status")?;
    let severity_s: String = row.try_get("severity")?;
    let priority_s: String = row.try_get("priority")?;
    Ok(BugRecord {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        source_project: row.try_get("source_project")?,
        target_project: row.try_get("target_project")?,
        component: row.try_get("component")?,
        status: BugStatus::parse(&status_s)?,
        severity: parse_severity(&severity_s)?,
        priority: parse_priority(&priority_s)?,
        difficulty: row.try_get::<i64, _>("difficulty")? as u8,
        impact: row.try_get("impact")?,
        security: row.try_get::<i64, _>("security")? != 0,
        owner: row.try_get("owner")?,
        body,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        attempt_count: 0,
        failed_attempt_count: 0,
    })
}

fn decode_attempt(row: sqlx::any::AnyRow) -> Result<BugAttempt> {
    let status: String = row.try_get("status")?;
    Ok(BugAttempt {
        id: row.try_get("id")?,
        bug_id: row.try_get("bug_id")?,
        agent: row.try_get("agent")?,
        status: AttemptStatus::parse(&status)?,
        sandbox_path: row.try_get("sandbox_path")?,
        branch: row.try_get("branch")?,
        base_sha: row.try_get("base_sha")?,
        head_sha: row.try_get("head_sha")?,
        pr_url: row.try_get("pr_url")?,
        ci_evidence: row.try_get("ci_evidence")?,
        notes: row.try_get("notes")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn parse_severity(input: &str) -> Result<BugSeverity> {
    match input {
        "S0" | "s0" => Ok(BugSeverity::S0),
        "S1" | "s1" => Ok(BugSeverity::S1),
        "S2" | "s2" => Ok(BugSeverity::S2),
        "S3" | "s3" => Ok(BugSeverity::S3),
        "S4" | "s4" => Ok(BugSeverity::S4),
        other => bail!("unknown severity '{other}'"),
    }
}

fn parse_priority(input: &str) -> Result<BugPriority> {
    match input {
        "P0" | "p0" => Ok(BugPriority::P0),
        "P1" | "p1" => Ok(BugPriority::P1),
        "P2" | "p2" => Ok(BugPriority::P2),
        "P3" | "p3" => Ok(BugPriority::P3),
        "P4" | "p4" => Ok(BugPriority::P4),
        other => bail!("unknown priority '{other}'"),
    }
}

fn sort_bugs(bugs: &mut [BugRecord], sort: BugSort) {
    match sort {
        BugSort::Rank => bugs.sort_by_key(ranking_key),
        BugSort::Severity => bugs.sort_by_key(|bug| bug.severity),
        BugSort::Priority => bugs.sort_by_key(|bug| bug.priority),
        BugSort::Difficulty => bugs.sort_by_key(|bug| bug.difficulty),
        BugSort::Ready => {
            bugs.sort_by_key(|bug| if bug.status == BugStatus::Ready { 0 } else { 1 })
        }
        BugSort::Updated => bugs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
        BugSort::Attempts => bugs.sort_by_key(|bug| -bug.attempt_count),
    }
}

pub fn bugtracker_schema_ddl() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS bug_projects (
        alias TEXT PRIMARY KEY,
        repo_root TEXT NOT NULL,
        repo_slug TEXT NOT NULL,
        provider_kind TEXT NOT NULL,
        provider_project_id TEXT,
        default_branch TEXT NOT NULL DEFAULT 'main',
        metadata_json TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS bug_project_edges (
        source_project TEXT NOT NULL,
        target_project TEXT NOT NULL,
        kind TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (source_project, target_project, kind)
    );
    CREATE TABLE IF NOT EXISTS bugs (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        source_project TEXT NOT NULL,
        target_project TEXT NOT NULL,
        component TEXT,
        status TEXT NOT NULL,
        severity TEXT NOT NULL,
        priority TEXT NOT NULL,
        difficulty INTEGER NOT NULL,
        repro_state TEXT NOT NULL,
        impact TEXT NOT NULL,
        security INTEGER NOT NULL DEFAULT 0,
        owner TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        body_json TEXT NOT NULL,
        idempotency_key TEXT UNIQUE
    );
    CREATE INDEX IF NOT EXISTS idx_bugs_target_status ON bugs(target_project, status, severity, priority);
    CREATE INDEX IF NOT EXISTS idx_bugs_updated ON bugs(updated_at);
    CREATE TABLE IF NOT EXISTS bug_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bug_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        actor TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_bug_events_bug ON bug_events(bug_id, id);
    CREATE TABLE IF NOT EXISTS bug_attempts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bug_id TEXT NOT NULL,
        agent TEXT,
        status TEXT NOT NULL,
        sandbox_path TEXT,
        branch TEXT,
        base_sha TEXT,
        head_sha TEXT,
        pr_url TEXT,
        ci_evidence TEXT,
        notes TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_bug_attempts_bug ON bug_attempts(bug_id, id);
    CREATE TABLE IF NOT EXISTS bug_links (
        bug_id TEXT NOT NULL,
        linked_bug_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (bug_id, linked_bug_id, kind)
    );
    CREATE TABLE IF NOT EXISTS bug_external_refs (
        bug_id TEXT NOT NULL,
        provider TEXT NOT NULL,
        external_id TEXT,
        url TEXT,
        labels_json TEXT NOT NULL DEFAULT '[]',
        sync_status TEXT NOT NULL DEFAULT 'local',
        updated_at TEXT NOT NULL,
        PRIMARY KEY (bug_id, provider)
    );
    CREATE TABLE IF NOT EXISTS bug_evidence (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bug_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        summary TEXT NOT NULL,
        path TEXT,
        url TEXT,
        digest TEXT,
        redacted INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    );
    "#
}

#[cfg(test)]
pub(crate) async fn fresh_bugtracker_pool() -> AnyPool {
    use crate::db::{AnyPoolOptions, install_default_drivers};
    install_default_drivers();
    let tmp = tempfile::NamedTempFile::new().expect("tempfile for bugtracker pool");
    let url = crate::db::config::sqlite_url(tmp.path());
    let pool = AnyPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect bugtracker sqlite");
    BugTrackerRepo::new(pool.clone())
        .install_schema()
        .await
        .unwrap();
    std::mem::forget(tmp);
    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(alias: &str) -> BugProjectInput {
        BugProjectInput {
            alias: alias.into(),
            repo_root: format!("/tmp/{alias}"),
            repo_slug: format!("neverhuman/{alias}"),
            provider_kind: "github".into(),
            provider_project_id: None,
            default_branch: "main".into(),
        }
    }

    fn report() -> CanonicalBugReport {
        CanonicalBugReport {
            target_project: "redlinedb".into(),
            source_project: "veox".into(),
            title: "adapter loses writes".into(),
            component: Some("adapter".into()),
            current_behavior: "writes disappear".into(),
            expected_behavior: "writes persist".into(),
            environment: "local".into(),
            frequency: "always".into(),
            impact: "blocks local agents".into(),
            security_privacy: "none".into(),
            no_secrets_confirmed: true,
            reproduction_steps: vec!["write row".into(), "read row".into()],
            evidence: Vec::new(),
            acceptance_criteria: Vec::new(),
            severity: BugSeverity::S1,
            priority: BugPriority::P1,
            difficulty: 2,
        }
    }

    #[tokio::test]
    async fn submit_list_show_ready_attempts() {
        let repo = BugTrackerRepo::new(fresh_bugtracker_pool().await);
        repo.add_project(&project("veox")).await.unwrap();
        repo.add_project(&project("redlinedb")).await.unwrap();
        repo.link_projects("veox", "redlinedb", "depends_on")
            .await
            .unwrap();
        let bug = repo
            .submit_bug(&report(), Some("idem-1"), "test")
            .await
            .unwrap();
        let same = repo
            .submit_bug(&report(), Some("idem-1"), "test")
            .await
            .unwrap();
        assert_eq!(bug.id, same.id);
        repo.update_bug(
            &bug.id,
            Some(BugStatus::Ready),
            None,
            None,
            None,
            None,
            "triager",
        )
        .await
        .unwrap();
        let ready = repo.ready_bugs(Some("redlinedb")).await.unwrap();
        assert_eq!(ready.len(), 1);
        repo.record_attempt(
            &bug.id,
            &BugAttemptInput {
                agent: Some("codex".into()),
                status: AttemptStatus::Failed,
                sandbox_path: None,
                branch: Some("bug/x".into()),
                base_sha: None,
                head_sha: None,
                pr_url: None,
                ci_evidence: Some("test failed".into()),
                notes: Some("learned thing".into()),
            },
            "codex",
        )
        .await
        .unwrap();
        let detail = repo.show_bug(&bug.id).await.unwrap();
        assert_eq!(detail.events.len(), 3);
        assert_eq!(detail.attempts.len(), 1);
    }
}
