//! Complete read-only ledger snapshots; no accepted audit result is inferred.
use super::*;

pub(crate) fn status(connection: &Connection) -> Result<Value> {
    let transaction = connection.unchecked_transaction()?;
    let keys: Vec<String> = {
        let mut statement = transaction.prepare("SELECT key FROM jobs ORDER BY key")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut jobs = Vec::new();
    for key in keys {
        let job = job(&transaction, &key)?;
        let requests: Vec<String> = {
            let mut statement =
                transaction.prepare("SELECT key FROM requests WHERE job_key=?1 ORDER BY key")?;
            statement
                .query_map([&key], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        type AttemptRow = (String, String, i64, i64, i64, Option<i64>, Option<String>);
        let attempts: Vec<AttemptRow> = {
            let mut statement = transaction.prepare("SELECT a.id,a.request_key,a.ordinal,a.started_at,a.deadline,c.acknowledged_at,c.acknowledgement_sha256 FROM attempts a LEFT JOIN closures c ON c.attempt_id=a.id WHERE a.job_key=?1 ORDER BY a.ordinal")?;
            statement
                .query_map([&key], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                })?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut history = Vec::new();
        let mut pending_retry = false;
        let mut state = "pending";
        for (id, request, ordinal, started, deadline, closed_at, closure_hash) in attempts {
            let closed = closed_at.is_some();
            let observations: Vec<ObservationRow> = {
                let mut statement = transaction.prepare("SELECT id,kind,outcome,recorded_at,receipt_sha256,report_sha256,summary,diagnostic,reason FROM observations WHERE attempt_id=?1 ORDER BY rowid")?;
                statement
                    .query_map([&id], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<_>>()?
            };
            pending_retry = observations
                .iter()
                .any(|row| row.2 != "completed_unqualified");
            state = if !closed {
                if observations.is_empty() {
                    "executing"
                } else {
                    "closure_pending"
                }
            } else if pending_retry {
                "pending_retry"
            } else {
                "awaiting_admission"
            };
            let observations: Vec<Value> = observations.into_iter().map(|(evidence,kind,outcome,recorded,receipt,report,summary,diagnostic,reason)| {
                Ok(json!({"evidence_id":evidence,"kind":kind,"outcome":outcome,"recorded_at":recorded,
                    "receipt_sha256":receipt,"report_sha256":report,"summary":summary.map(|bytes| serde_json::from_slice::<Value>(&bytes)).transpose()?,
                    "diagnostic":diagnostic,"reason":reason,"execution_verified":false}))
            }).collect::<Result<_>>()?;
            history.push(json!({"attempt_id":id,"request_key":request,"ordinal":ordinal,"started_at":started,"deadline":deadline,
                "lease_held":!closed,"closure_acknowledged_at":closed_at,"closure_acknowledgement_sha256":closure_hash,
                "closure_authenticated":false,"observations":observations}));
        }
        jobs.push(json!({"deduplication_key":key,"identity":job.identity,"source_commit":job.commit,"source_tree":job.tree,
            "state":state,"pending_retry":pending_retry,"required_audit_satisfied":false,
            "scheduled_requests":requests,"attempts":history}));
    }
    let plans: Vec<Value> = {
        let mut statement = transaction
            .prepare("SELECT id,bytes,imported_at FROM plans ORDER BY imported_at,id")?;
        let rows: Vec<(String, Vec<u8>, i64)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter().map(|(id,bytes,time)| Ok(json!({"plan_id":id,"plan":serde_json::from_slice::<Value>(&bytes)?,"imported_at":time}))).collect::<Result<_>>()?
    };
    let intake = crate::audit_intake::status_snapshot(&transaction)?;
    transaction.commit()?;
    Ok(
        json!({"schema_version":"jeryu.audit-ledger-status/v1","publication_qualified":false,
        "accepted_full_audits":0,"unresolved_jobs":jobs.len(),"plans":plans,"jobs":jobs,"intake":intake}),
    )
}
