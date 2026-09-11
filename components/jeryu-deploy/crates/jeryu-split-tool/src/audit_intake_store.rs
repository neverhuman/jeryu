//! Append-only reception capture, idempotent delivery binding and failure history.
use super::*;

pub(super) fn capture(
    connection: &mut Connection,
    route: Option<&[u8]>,
    headers: Option<&[u8]>,
    body: Option<&[u8]>,
    authentication: &str,
    received_at: i64,
) -> Result<i64> {
    ensure!(
        route.is_none_or(|bytes| bytes.len() <= MAX_CONFIG)
            && headers.is_none_or(|bytes| bytes.len() <= MAX_HEADERS)
            && body.is_none_or(|bytes| bytes.len() <= MAX_BODY),
        "raw reception exceeds retention bounds"
    );
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute("INSERT INTO intake_receptions(received_at,route_bytes,route_sha256,header_bytes,header_sha256,body_bytes,body_sha256,authentication) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![received_at,route,route.map(audit_evidence::hash),headers,headers.map(audit_evidence::hash),body,body.map(audit_evidence::hash),authentication])?;
    let id = transaction.last_insert_rowid();
    // Reserve authenticated bytes and the first delivery before interpreting the body.
    // A later classifier must not win an earlier reception's delivery identity.
    if authentication == "matched" {
        let route = Route::parse(route.context("matched reception route missing")?)?;
        let hash = audit_evidence::hash(body.context("matched reception body missing")?);
        let event_key = crate::audit_scheduler::digest(&(
            "jeryu.audit-received-body/v1",
            &route.route_id,
            &hash,
        ))?;
        let metadata = json!({"context":route.public_context(),"body_sha256":hash,"first_reception_id":id,
            "identity_kind":"received_signed_bytes","source_verified":false,"publication_qualified":false});
        transaction.execute("INSERT OR IGNORE INTO intake_events(key,route_id,body_sha256,first_reception_id,metadata) VALUES(?1,?2,?3,?4,?5)",
            params![event_key,route.route_id,hash,id,serde_json::to_vec(&metadata)?])?;
        let original: (String, String) = transaction.query_row(
            "SELECT route_id,body_sha256 FROM intake_events WHERE key=?1",
            [&event_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(
            original == (route.route_id.clone(), hash.clone()),
            "stored event key conflicts with its received byte identity"
        );
        if let Ok(headers) = Headers::parse(headers.context("matched reception headers missing")?) {
            let delivery_key = crate::audit_scheduler::digest(&(
                "jeryu.audit-delivery/v1",
                &route.route_id,
                headers.delivery.to_ascii_lowercase(),
            ))?;
            transaction.execute("INSERT OR IGNORE INTO intake_deliveries(key,route_id,delivery_guid,body_sha256,event_header,event_key,first_reception_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![delivery_key,route.route_id,headers.delivery.to_ascii_lowercase(),hash,headers.event,event_key,id])?;
        }
    }
    transaction.commit()?;
    Ok(id)
}

type Reception = (
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

fn observed(
    transaction: &Connection,
    id: i64,
    event_key: Option<&str>,
    result: &Value,
) -> Result<Value> {
    transaction.execute(
        "INSERT INTO intake_classifications(reception_id,event_key,metadata) VALUES(?1,?2,?3)",
        params![id, event_key, serde_json::to_vec(result)?],
    )?;
    Ok(result.clone())
}

pub(super) fn classify(connection: &mut Connection, id: i64) -> Result<Value> {
    match classify_once(connection, id) {
        Ok(result) => Ok(result),
        Err(error) => {
            let diagnostic = format!("{error:#}").chars().take(4096).collect::<String>();
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute("INSERT INTO intake_processing_errors(reception_id,diagnostic,diagnostic_sha256,recorded_at) VALUES(?1,?2,?3,?4)",
                params![id,diagnostic.as_bytes(),audit_evidence::hash(diagnostic.as_bytes()),audit_ledger::now()?])?;
            transaction.commit()?;
            bail!("classification failed; private diagnostic and raw reception retained");
        }
    }
}

fn classify_once(connection: &mut Connection, id: i64) -> Result<Value> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(bytes) = transaction
        .query_row(
            "SELECT metadata FROM intake_classifications WHERE reception_id=?1",
            [id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
    {
        return Ok(serde_json::from_slice(&bytes)?);
    }
    let (route,headers,body,route_hash,header_hash,body_hash,authentication):Reception=transaction.query_row(
        "SELECT route_bytes,header_bytes,body_bytes,route_sha256,header_sha256,body_sha256,authentication FROM intake_receptions WHERE id=?1",[id],
        |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)))?;
    ensure!(
        route.as_deref().map(audit_evidence::hash) == route_hash
            && headers.as_deref().map(audit_evidence::hash) == header_hash
            && body.as_deref().map(audit_evidence::hash) == body_hash,
        "stored reception input hashes disagree"
    );
    let result = if authentication != "matched" {
        observed(
            &transaction,
            id,
            None,
            &json!({"reception_id":id,"reception_accepted":false,"state":"authentication_failed",
            "reason":authentication,"hmac_verified":false,"publication_qualified":false}),
        )?
    } else {
        let route = Route::parse(route.as_deref().context("stored route missing")?)?;
        let body = body.context("stored signed body missing")?;
        let hash = body_hash.context("stored body digest missing")?;
        ensure!(
            audit_evidence::hash(&body) == hash,
            "stored raw body differs from receipt"
        );
        let event_key = crate::audit_scheduler::digest(&(
            "jeryu.audit-received-body/v1",
            &route.route_id,
            &hash,
        ))?;
        let Ok(headers) = Headers::parse(headers.as_deref().context("stored headers missing")?)
        else {
            let result = observed(
                &transaction,
                id,
                Some(&event_key),
                &json!({"reception_id":id,"event_key":event_key,
                "reception_accepted":false,"state":"invalid_unsigned_headers","reason":"delivery_or_event_metadata_invalid",
                "hmac_verified":true,"headers_authenticated":false,"context":route.public_context(),"body_sha256":hash,
                "planning_complete":false,"publication_qualified":false}),
            )?;
            transaction.commit()?;
            return Ok(result);
        };
        let translation = translate::translate(&route, &headers, &body);
        let eligible = !matches!(
            translation["state"].as_str(),
            Some("malformed_payload" | "wrong_repository")
        );
        let mut result = json!({"reception_id":id,"reception_accepted":eligible,"hmac_verified":true,
            "headers_authenticated":false,"context":route.public_context(),"translation":translation,
            "body_sha256":hash,"publication_qualified":false});
        let delivery_key = crate::audit_scheduler::digest(&(
            "jeryu.audit-delivery/v1",
            &route.route_id,
            headers.delivery.to_ascii_lowercase(),
        ))?;
        let (prior_hash,prior_header,prior_event,first_delivery):(String,String,String,i64)=transaction.query_row(
            "SELECT body_sha256,event_header,event_key,first_reception_id FROM intake_deliveries WHERE key=?1",
            [&delivery_key],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
        let (event_route, event_hash, first_event): (String, String, i64) = transaction.query_row(
            "SELECT route_id,body_sha256,first_reception_id FROM intake_events WHERE key=?1",
            [&event_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        ensure!(
            event_route == route.route_id && event_hash == hash,
            "stored event differs from received byte identity"
        );
        result["event_key"] = json!(event_key);
        if prior_hash != hash || prior_header != headers.event {
            result["reception_accepted"] = json!(false);
            result["state"] = json!("conflicting_delivery");
            result["original_event_key"] = json!(prior_event);
        } else {
            ensure!(
                prior_event == event_key,
                "stored delivery event identity disagrees with body"
            );
            result["state"] = json!(if first_delivery != id {
                "duplicate_delivery"
            } else if first_event != id {
                "duplicate_event"
            } else {
                "received"
            });
        }
        // The signed body stays pending even when parsing or unsigned headers conflict.
        observed(&transaction, id, Some(&event_key), &result)?
    };
    transaction.commit()?;
    Ok(result)
}

pub(super) fn reconcile(connection: &mut Connection) -> Result<Value> {
    let pending: Vec<i64> = {
        let mut statement=connection.prepare("SELECT r.id FROM intake_receptions r LEFT JOIN intake_classifications c ON c.reception_id=r.id WHERE c.reception_id IS NULL ORDER BY r.id")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut results = Vec::new();
    let mut errors = 0;
    for id in pending {
        match classify(connection, id) {
            Ok(result) => results.push(result),
            Err(_) => {
                errors += 1;
                results.push(json!({"reception_id":id,"state":"classification_error","original_reception_retained":true}));
            }
        }
    }
    Ok(
        json!({"classification_errors":errors,"receptions":results,"planning_complete":false,"publication_qualified":false}),
    )
}

pub(super) fn failure(
    connection: &mut Connection,
    event_key: &str,
    kind: FailureKind,
    command_exit: Option<i32>,
    diagnostic: Option<Vec<u8>>,
    now: i64,
) -> Result<Value> {
    ensure!(
        digest(event_key)
            && diagnostic
                .as_ref()
                .is_none_or(|bytes| bytes.len() <= MAX_CONFIG),
        "invalid failure observation input"
    );
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: i64 = transaction.query_row(
        "SELECT count(*) FROM intake_events WHERE key=?1",
        [event_key],
        |row| row.get(0),
    )?;
    ensure!(exists == 1, "unknown received source event");
    let kind = serde_json::to_value(kind)?
        .as_str()
        .context("failure kind")?
        .to_owned();
    let hash = diagnostic.as_deref().map(audit_evidence::hash);
    transaction.execute("INSERT INTO intake_failures(event_key,kind,command_exit,diagnostic,diagnostic_sha256,recorded_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![event_key,kind,command_exit,diagnostic,hash,now])?;
    let id = transaction.last_insert_rowid();
    transaction.commit()?;
    Ok(
        json!({"failure_id":id,"event_key":event_key,"kind":kind,"diagnostic_sha256":hash,
        "observation_authenticated":false,"planning_complete":false,"publication_qualified":false}),
    )
}

#[path = "audit_intake_status.rs"]
mod history;
pub(super) use history::status;
