//! Immutable executor observations and closure acknowledgements.
use super::*;

fn attempt(connection: &Connection, id: &str) -> Result<(String, i64)> {
    ensure!(scheduler::hex(id, 64), "invalid attempt identity");
    connection
        .query_row(
            "SELECT job_key,started_at FROM attempts WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("unknown audit attempt")
}

pub(crate) fn finish(
    connection: &mut Connection,
    receipt: &[u8],
    report: Option<Result<Vec<u8>>>,
    now: i64,
) -> Result<Value> {
    ensure!(
        receipt.len() as u64 <= MAX_INPUT,
        "executor receipt exceeds bound"
    );
    let JsonObject(submitted): JsonObject<validation::Receipt> = serde_json::from_slice(receipt)?;
    ensure!(
        submitted.schema_version == "jeryu.audit-ledger-observation/v1",
        "unsupported executor observation"
    );
    ensure!(
        !submitted.reason.trim().is_empty() && submitted.reason.len() <= 4096,
        "concrete bounded outcome reason required"
    );
    ensure!(
        submitted
            .report_sha256
            .as_ref()
            .is_none_or(|hash| scheduler::hex(hash, 64)),
        "invalid report hash"
    );
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (key, started) = attempt(&transaction, &submitted.attempt_id)?;
    ensure!(now >= started, "finish precedes attempt start");
    let job = job(&transaction, &key)?;
    ensure!(
        submitted.deduplication_key == key
            && submitted.source_commit == job.commit
            && submitted.source_tree == job.tree
            && submitted.identity.0 == job.identity,
        "executor observation names another source, auditor, policy, or configuration"
    );
    let config = validation::context(&job.identity, &job.config, &job.governing, &job.candidate)?;
    let mut outcome = submitted.outcome.label();
    let mut diagnostic = String::new();
    let report = match report {
        Some(Ok(bytes)) if bytes.len() as u64 <= MAX_INPUT => Some(bytes),
        Some(Ok(_)) => {
            diagnostic = "report exceeds admission bound".into();
            None
        }
        Some(Err(_)) => {
            diagnostic = "report file unavailable or failed custody checks".into();
            None
        }
        None => None,
    };
    let report_hash = report.as_ref().map(|bytes| audit_evidence::hash(bytes));
    let mut summary = None;
    if submitted.report_sha256 != report_hash {
        diagnostic = "submitted report hash does not match supplied bytes".into();
    }
    if submitted.outcome == validation::Outcome::Report && report.is_none() {
        diagnostic = "full report missing; no successful outcome can be recorded".into();
    }
    if diagnostic.is_empty()
        && let Some(bytes) = &report
    {
        if let Some(exit) = submitted.command_exit {
            match audit_evidence::admit(
                bytes,
                audit_evidence::Binding {
                    commit: &job.commit,
                    version: &config.auditor_version,
                    policy_path: &config.policy_path,
                    minimum: config.minimum,
                    max_soft: config.max_soft,
                },
                exit,
            ) {
                Ok(value) => {
                    if submitted.outcome == validation::Outcome::Report {
                        outcome = match exit {
                            124 | 137 => "timed_out",
                            0 | 1 if value.passed => "completed_unqualified",
                            0 | 1 => "failed_policy",
                            _ => "tool_error",
                        };
                    }
                    summary = Some(serde_json::to_vec(&value)?);
                }
                Err(error) => diagnostic = format!("full report rejected: {error}"),
            }
        } else {
            diagnostic = "report requires an observed command exit".into();
        }
    }
    if !diagnostic.is_empty() && submitted.outcome == validation::Outcome::Report {
        outcome = if matches!(submitted.command_exit, Some(124 | 137)) {
            "timed_out"
        } else {
            "tool_error"
        };
    }
    let deadline: i64 = transaction.query_row(
        "SELECT deadline FROM attempts WHERE id=?1",
        [&submitted.attempt_id],
        |row| row.get(0),
    )?;
    record_expiry(&transaction, &submitted.attempt_id, &key, deadline, now)?;
    let receipt_hash = audit_evidence::hash(receipt);
    let id = scheduler::digest(&(
        "jeryu.audit-ledger-evidence/v1",
        &submitted.attempt_id,
        "finish",
        &receipt_hash,
        &report_hash,
    ))?;
    transaction.execute("INSERT OR IGNORE INTO observations(id,attempt_id,kind,outcome,recorded_at,receipt,report,receipt_sha256,report_sha256,summary,diagnostic,reason) VALUES(?1,?2,'finish',?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![id,submitted.attempt_id,outcome,now,receipt,report,receipt_hash,report_hash,summary,diagnostic,submitted.reason])?;
    let existing: (String, String, String) = transaction.query_row(
        "SELECT id,outcome,diagnostic FROM observations WHERE attempt_id=?1 AND kind='finish'",
        [&submitted.attempt_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    ensure!(
        existing == (id.clone(), outcome.into(), diagnostic),
        "attempt finish conflicts with immutable original evidence"
    );
    let closed: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM closures WHERE attempt_id=?1)",
        [&submitted.attempt_id],
        |row| row.get(0),
    )?;
    transaction.commit()?;
    Ok(
        json!({"evidence_id":id,"outcome":outcome,"lease_held":!closed,
        "execution_verified":false,"publication_qualified":false,"required_audit_satisfied":false}),
    )
}

pub(crate) fn close(
    connection: &mut Connection,
    acknowledgement: &[u8],
    now: i64,
) -> Result<Value> {
    ensure!(
        acknowledgement.len() as u64 <= MAX_INPUT,
        "closure acknowledgement exceeds bound"
    );
    let JsonObject(closure): JsonObject<validation::Closure> =
        serde_json::from_slice(acknowledgement)?;
    ensure!(
        closure.schema_version == "jeryu.audit-ledger-closure/v1" && closure.executor_closed,
        "explicit executor closure acknowledgement required"
    );
    ensure!(
        !closure.reason.trim().is_empty() && closure.reason.len() <= 4096,
        "concrete closure reason required"
    );
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (key, started) = attempt(&transaction, &closure.attempt_id)?;
    ensure!(
        closure.deduplication_key == key && now >= started,
        "closure identity or time mismatch"
    );
    let observed: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM observations WHERE attempt_id=?1)",
        [&closure.attempt_id],
        |row| row.get(0),
    )?;
    ensure!(observed, "record an outcome before acknowledging closure");
    let hash = audit_evidence::hash(acknowledgement);
    transaction.execute("INSERT OR IGNORE INTO closures(attempt_id,acknowledged_at,acknowledgement,acknowledgement_sha256) VALUES(?1,?2,?3,?4)", params![closure.attempt_id,now,acknowledgement,hash])?;
    let original: Vec<u8> = transaction.query_row(
        "SELECT acknowledgement FROM closures WHERE attempt_id=?1",
        [&closure.attempt_id],
        |row| row.get(0),
    )?;
    ensure!(
        original == acknowledgement,
        "closure conflicts with original acknowledgement"
    );
    transaction.commit()?;
    Ok(
        json!({"attempt_id":closure.attempt_id,"lease_held":false,"closure_authenticated":false,"publication_qualified":false}),
    )
}

fn record_expiry(
    connection: &Connection,
    attempt: &str,
    key: &str,
    deadline: i64,
    now: i64,
) -> Result<()> {
    let observed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM observations WHERE attempt_id=?1)",
        [attempt],
        |row| row.get(0),
    )?;
    if now < deadline || observed {
        return Ok(());
    }
    let receipt = serde_json::to_vec(
        &json!({"schema_version":"jeryu.audit-ledger-expiry/v1","attempt_id":attempt,
        "deduplication_key":key,"deadline":deadline,"execution_closure_confirmed":false}),
    )?;
    let receipt_hash = audit_evidence::hash(&receipt);
    let id = scheduler::digest(&(
        "jeryu.audit-ledger-evidence/v1",
        attempt,
        "timeout",
        &receipt_hash,
        Option::<String>::None,
    ))?;
    connection.execute("INSERT INTO observations(id,attempt_id,kind,outcome,recorded_at,receipt,receipt_sha256,diagnostic,reason) VALUES(?1,?2,'timeout','timed_out',?3,?4,?5,'lease expired; execution closure remains unacknowledged','deadline elapsed without a recorded outcome')",
        params![id,attempt,now,receipt,receipt_hash])?;
    Ok(())
}

pub(crate) fn reconcile(connection: &mut Connection, now: i64) -> Result<Value> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // No LIMIT: every overdue live lease is reconciled in this transaction.
    let expired: Vec<(String, String, i64)> = {
        let mut statement = transaction.prepare("SELECT a.id,a.job_key,a.deadline FROM attempts a LEFT JOIN closures c ON c.attempt_id=a.id WHERE a.deadline<=?1 AND c.attempt_id IS NULL AND NOT EXISTS(SELECT 1 FROM observations o WHERE o.attempt_id=a.id) ORDER BY a.job_key,a.ordinal")?;
        statement
            .query_map([now], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (attempt, key, deadline) in &expired {
        record_expiry(&transaction, attempt, key, *deadline, now)?;
    }
    transaction.commit()?;
    Ok(json!({"timeouts_recorded":expired.len(),"leases_released":0,"publication_qualified":false}))
}
