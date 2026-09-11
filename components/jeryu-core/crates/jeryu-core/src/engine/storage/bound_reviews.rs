//! Transaction composition for authenticated review history and its compatibility
//! State projection. Snapshot saves do not own or reconcile these two tables.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use super::{SqliteStore, State, insert_audit, persist_snapshot, storage_error};
use crate::core::bound_reviews::{BoundReviewEvent, ReviewChallenge};
use crate::core::ref_operations::{canonical_json, sha256};
use crate::{AuditEntry, ForgeError, Result};

impl SqliteStore {
    pub(in crate::core) fn insert_review_challenge(
        &self,
        challenge: &ReviewChallenge,
    ) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        tx.execute(
            "INSERT INTO forge_review_challenges (id, repo_id, pull_id, pull_number, expires_at, challenge_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![challenge.id.to_string(), challenge.snapshot.repository_id.to_string(),
                challenge.snapshot.pull_request_id.to_string(), challenge.snapshot.pull_number,
                challenge.expires_at.to_rfc3339(), canonical_json(challenge)?],
        ).map_err(storage_error)?;
        insert_audit(
            &tx,
            &AuditEntry {
                id: Uuid::new_v4().to_string(),
                occurred_at: challenge.created_at.to_rfc3339(),
                actor: challenge.snapshot.reviewer.login.clone(),
                action: "review.challenge".into(),
                subject: challenge.snapshot.repository_id.to_string(),
                phase: "requested".into(),
                detail: serde_json::json!({"challenge_id": challenge.id,
                "snapshot_sha256": challenge.snapshot_sha256, "actor": challenge.snapshot.reviewer}),
            },
        )?;
        tx.commit().map_err(storage_error)
    }

    pub(in crate::core) fn load_review_challenge(
        &self,
        id: Uuid,
    ) -> Result<(ReviewChallenge, Option<BoundReviewEvent>)> {
        load_challenge(&self.connect()?, id)
    }

    pub(in crate::core) fn commit_bound_review(
        &self,
        challenge: &ReviewChallenge,
        event: &BoundReviewEvent,
        proposed: &State,
    ) -> Result<()> {
        let mut conn = self.connect()?;
        conn.execute_batch("PRAGMA temp_store = MEMORY;")
            .map_err(storage_error)?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let (current, accepted) = load_challenge(&tx, challenge.id)?;
        if current != *challenge || accepted.is_some() {
            return Err(ForgeError::Conflict(
                "review challenge changed before commit".into(),
            ));
        }
        persist_snapshot(&tx, proposed)?;
        tx.execute(
            "INSERT INTO forge_bound_review_events (id, repo_id, pull_id, pull_number, sequence, challenge_id, event_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![event.id.to_string(), event.repository_id.to_string(), event.pull_request_id.to_string(),
                event.pull_number, event.sequence, event.challenge_id.to_string(), canonical_json(event)?],
        ).map_err(storage_error)?;
        let consumed = tx.execute("UPDATE forge_review_challenges SET accepted_event_id = ?1 WHERE id = ?2 AND accepted_event_id IS NULL",
            params![event.id.to_string(), challenge.id.to_string()]).map_err(storage_error)?;
        if consumed != 1 {
            return Err(ForgeError::Conflict(
                "review challenge was already consumed".into(),
            ));
        }
        insert_audit(
            &tx,
            &AuditEntry {
                id: event.audit_id.to_string(),
                occurred_at: event.review.submitted_at.to_rfc3339(),
                actor: event.actor.login.clone(),
                action: "review.bound_event".into(),
                subject: event.repository_id.to_string(),
                phase: "completed".into(),
                detail: serde_json::json!({"review_id": event.id, "sequence": event.sequence,
                "challenge_id": event.challenge_id, "actor": event.actor,
                "snapshot_sha256": event.snapshot_sha256, "request_sha256": event.request_sha256}),
            },
        )?;
        tx.commit().map_err(storage_error)
    }
}

fn load_challenge(
    conn: &Connection,
    id: Uuid,
) -> Result<(ReviewChallenge, Option<BoundReviewEvent>)> {
    let row = conn.query_row(
        "SELECT repo_id, pull_id, pull_number, expires_at, challenge_json, accepted_event_id FROM forge_review_challenges WHERE id = ?1",
        params![id.to_string()], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, u64>(2)?,
            row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, Option<String>>(5)?)),
    ).optional().map_err(storage_error)?.ok_or_else(|| ForgeError::NotFound(format!("review challenge {id}")))?;
    let challenge: ReviewChallenge = serde_json::from_str(&row.4).map_err(storage_error)?;
    if challenge.id != id
        || challenge.snapshot.repository_id.to_string() != row.0
        || challenge.snapshot.pull_request_id.to_string() != row.1
        || challenge.snapshot.pull_number != row.2
        || challenge.expires_at.to_rfc3339() != row.3
        || sha256(&canonical_json(&challenge.snapshot)?) != challenge.snapshot_sha256
        || challenge.merge_qualified
    {
        return Err(ForgeError::Storage(
            "inconsistent persisted review challenge".into(),
        ));
    }
    let accepted = row.5.map(|event_id| {
        let json = conn.query_row("SELECT event_json FROM forge_bound_review_events WHERE id = ?1 AND challenge_id = ?2",
            params![event_id, id.to_string()], |row| row.get::<_, String>(0)).map_err(storage_error)?;
        let event = decode_event(&json)?;
        if event.id.to_string() != event_id || event.challenge_id != id || event.snapshot != challenge.snapshot {
            return Err(ForgeError::Storage("inconsistent consumed review challenge".into()));
        }
        Ok(event)
    }).transpose()?;
    Ok((challenge, accepted))
}

pub(super) fn load_bound_reviews(conn: &Connection) -> Result<Vec<BoundReviewEvent>> {
    let mut statement = conn.prepare("SELECT id, repo_id, pull_id, pull_number, sequence, challenge_id, event_json FROM forge_bound_review_events ORDER BY repo_id, pull_id, sequence").map_err(storage_error)?;
    let mut rows = statement.query([]).map_err(storage_error)?;
    let mut events = Vec::new();
    while let Some(row) = rows.next().map_err(storage_error)? {
        let event = decode_event(&row.get::<_, String>(6).map_err(storage_error)?)?;
        if event.id.to_string() != row.get::<_, String>(0).map_err(storage_error)?
            || event.repository_id.to_string() != row.get::<_, String>(1).map_err(storage_error)?
            || event.pull_request_id.to_string()
                != row.get::<_, String>(2).map_err(storage_error)?
            || event.pull_number != row.get::<_, u64>(3).map_err(storage_error)?
            || event.sequence != row.get::<_, u64>(4).map_err(storage_error)?
            || event.challenge_id.to_string() != row.get::<_, String>(5).map_err(storage_error)?
        {
            return Err(ForgeError::Storage(
                "inconsistent bound review index".into(),
            ));
        }
        events.push(event);
    }
    Ok(events)
}

fn decode_event(json: &str) -> Result<BoundReviewEvent> {
    let event: BoundReviewEvent = serde_json::from_str(json).map_err(storage_error)?;
    if sha256(&canonical_json(&event.snapshot)?) != event.snapshot_sha256
        || event.snapshot.reviewer != event.actor
        || event.id != event.review.id
        || event.snapshot.repository_id != event.repository_id
        || event.snapshot.pull_request_id != event.pull_request_id
        || event.snapshot.pull_number != event.pull_number
        || event.sequence == 0
        || event.review.author != event.actor.login
        || event.review.head_sha.as_deref() != Some(event.snapshot.git.source.commit_sha.as_str())
        || event
            .comments
            .iter()
            .any(|comment| comment.review_id != event.id || comment.author != event.actor.login)
    {
        return Err(ForgeError::Storage(
            "inconsistent persisted bound review event".into(),
        ));
    }
    Ok(event)
}
