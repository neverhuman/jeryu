//! Append-only rows, serialized lease decisions, and complete derived queue views.
use super::*;

type JobRow = (Vec<u8>, String, String, Vec<u8>, Vec<u8>, Vec<u8>);
type ObservationRow = (
    String,
    String,
    String,
    i64,
    String,
    Option<String>,
    Option<Vec<u8>>,
    String,
    String,
);

struct JobContext {
    identity: scheduler::ExecutionIdentity,
    commit: String,
    tree: String,
    config: Vec<u8>,
    governing: Vec<u8>,
    candidate: Vec<u8>,
}

fn job(connection: &Connection, key: &str) -> Result<JobContext> {
    let (identity, commit, tree, config, governing, candidate): JobRow = connection.query_row(
        "SELECT identity,source_commit,source_tree,config,governing_policy,candidate_policy FROM jobs WHERE key=?1",
        [key], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
    ).context("unknown audit job")?;
    let identity = serde_json::from_slice(&identity)?;
    ensure!(
        scheduler::source_key(&identity, &commit, &tree)? == key,
        "stored source identity mismatch"
    );
    Ok(JobContext {
        identity,
        commit,
        tree,
        config,
        governing,
        candidate,
    })
}

pub(super) fn import(
    connection: &mut Connection,
    plan: &[u8],
    config: &[u8],
    governing: &[u8],
    candidate: &[u8],
    now: i64,
) -> Result<Value> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = import_in_transaction(&transaction, plan, config, governing, candidate, now)?;
    transaction.commit()?;
    Ok(result)
}

/// The caller commits queue rows and their reception links together.
pub(crate) fn import_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    plan: &[u8],
    config: &[u8],
    governing: &[u8],
    candidate: &[u8],
    now: i64,
) -> Result<Value> {
    ensure!(
        [plan, config, governing, candidate]
            .iter()
            .all(|bytes| bytes.len() as u64 <= MAX_INPUT),
        "ledger input exceeds bound"
    );
    let plan = scheduler::import_plan(plan)?;
    validation::context(&plan.identity, config, governing, candidate)?;
    let bytes = serde_json::to_vec(&plan)?;
    let id = audit_evidence::hash(&bytes);
    let event_key = scheduler::plan_event_key(&plan)?;
    let identity = serde_json::to_vec(&plan.identity)?;
    transaction.execute(
        "INSERT OR IGNORE INTO plans(id,event_key,bytes,imported_at) VALUES(?1,?2,?3,?4)",
        params![id, event_key, bytes, now],
    )?;
    let existing: (String, Vec<u8>) = transaction.query_row(
        "SELECT id,bytes FROM plans WHERE event_key=?1",
        [&event_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        existing == (id.clone(), bytes),
        "same event identity supplied a different plan; original retained"
    );
    for planned in &plan.jobs {
        transaction.execute("INSERT OR IGNORE INTO jobs(key,identity,source_commit,source_tree,config,governing_policy,candidate_policy) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![planned.deduplication_key,identity,planned.source_commit,planned.source_tree,config,governing,candidate])?;
        let stored = job(transaction, &planned.deduplication_key)?;
        ensure!(
            stored.identity == plan.identity
                && stored.commit == planned.source_commit
                && stored.tree == planned.source_tree
                && stored.config == config
                && stored.governing == governing
                && stored.candidate == candidate,
            "immutable job identity conflicts with existing bytes"
        );
        transaction.execute(
            "INSERT OR IGNORE INTO requests(key,job_key) VALUES(?1,?2)",
            params![planned.attempt_key, planned.deduplication_key],
        )?;
        let request_job: String = transaction.query_row(
            "SELECT job_key FROM requests WHERE key=?1",
            [&planned.attempt_key],
            |row| row.get(0),
        )?;
        ensure!(
            request_job == planned.deduplication_key,
            "scheduled request identity conflict"
        );
        transaction.execute(
            "INSERT OR IGNORE INTO plan_jobs(plan_id,request_key) VALUES(?1,?2)",
            params![id, planned.attempt_key],
        )?;
    }
    Ok(
        json!({"plan_id":id,"jobs":plan.jobs.len(),"plan_authenticated":false,"publication_qualified":false}),
    )
}

pub(super) fn start(
    connection: &mut Connection,
    request_key: &str,
    lease_seconds: u64,
    now: i64,
) -> Result<Value> {
    ensure!(
        scheduler::hex(request_key, 64) && (1..=86_400).contains(&lease_seconds),
        "invalid request key or lease duration"
    );
    let deadline = now
        .checked_add(i64::try_from(lease_seconds)?)
        .context("lease deadline overflow")?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let key: String = transaction
        .query_row(
            "SELECT job_key FROM requests WHERE key=?1",
            [request_key],
            |row| row.get(0),
        )
        .context("unknown scheduled request")?;
    let live: i64 = transaction.query_row("SELECT count(*) FROM attempts a LEFT JOIN closures c ON c.attempt_id=a.id WHERE a.job_key=?1 AND c.attempt_id IS NULL", [&key], |row| row.get(0))?;
    ensure!(
        live == 0,
        "execution lease still held; timeout or report completion does not acknowledge process closure"
    );
    let (previous, previous_time): (i64, i64) = transaction.query_row("SELECT coalesce(max(ordinal),0),coalesce(max(started_at),0) FROM attempts WHERE job_key=?1", [&key], |row| Ok((row.get(0)?,row.get(1)?)))?;
    ensure!(
        now >= previous_time,
        "ledger clock moved before prior attempt"
    );
    let ordinal = previous
        .checked_add(1)
        .context("attempt ordinal overflow")?;
    let id = scheduler::digest(&("jeryu.audit-ledger-attempt/v1", &key, ordinal))?;
    transaction.execute("INSERT INTO attempts(id,job_key,request_key,ordinal,started_at,deadline) VALUES(?1,?2,?3,?4,?5,?6)", params![id,key,request_key,ordinal,now,deadline])?;
    transaction.commit()?;
    Ok(
        json!({"attempt_id":id,"deduplication_key":key,"ordinal":ordinal,"deadline":deadline,
        "lease_held":true,"execution_verified":false,"publication_qualified":false}),
    )
}

#[path = "audit_ledger_history.rs"]
mod history;
#[path = "audit_ledger_outcomes.rs"]
mod outcomes;
pub(super) use history::status;
pub(super) use outcomes::{close, finish, reconcile};
