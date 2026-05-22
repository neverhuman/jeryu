use anyhow::{Context, Result, bail};
use sqlx::Row;

use crate::bugtracker::{
    AttemptStatus, BugAttempt, BugPriority, BugRecord, BugSeverity, BugSort, BugStatus,
    CanonicalBugReport, ranking_key,
};

pub(super) fn base_select_with(where_clause: &str) -> String {
    format!(
        "SELECT id, title, source_project, target_project, component, status, severity, priority,
                difficulty, impact, security, owner, body_json, created_at, updated_at
         FROM bugs
         {where_clause}"
    )
}

pub(super) fn decode_bug_record(row: sqlx::any::AnyRow) -> Result<BugRecord> {
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

pub(super) fn decode_attempt(row: sqlx::any::AnyRow) -> Result<BugAttempt> {
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

pub(super) fn sort_bugs(bugs: &mut [BugRecord], sort: BugSort) {
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
