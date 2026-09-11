//! Atomic received-event/plan/queue binding. No executor or publication authority.
use super::*;
use crate::audit_scheduler::{self as scheduler, ExecutionIdentity, Request};

pub(super) struct Inputs {
    identity: ExecutionIdentity,
    config: Vec<u8>,
    governing: Vec<u8>,
    candidate: Vec<u8>,
}

impl Inputs {
    pub(super) fn read(
        identity: &Path,
        config: &Path,
        governing: &Path,
        candidate: &Path,
    ) -> Result<Self> {
        let JsonObject(identity) =
            serde_json::from_slice(&input::read_private(identity, MAX_CONFIG)?)?;
        Ok(Self {
            identity,
            config: input::read_private(config, MAX_CONFIG)?,
            governing: input::read_private(governing, MAX_CONFIG)?,
            candidate: input::read_private(candidate, MAX_CONFIG)?,
        })
    }

    fn bind(&self, route: &Route) -> Result<()> {
        ensure!(
            self.identity.repository == route.repository
                && self.identity.governing_policy_sha256 == route.governing_policy_sha256
                && self.identity.execution_config_sha256 == route.execution_config_sha256
                && audit_evidence::hash(&self.config) == route.execution_config_sha256
                && audit_evidence::hash(&self.governing) == route.governing_policy_sha256,
            "queue input identity differs from retained route observations"
        );
        let JsonObject(config): JsonObject<Value> = serde_json::from_slice(&self.config)?;
        let inputs = &config["executor_inputs"];
        let context = route.public_context();
        for name in [
            "executor_source_commit",
            "executor_executable_sha256",
            "executor_receipt_sha256",
            "governing_workflow_repository",
            "governing_workflow_path",
            "governing_workflow_commit",
            "governing_workflow_blob",
        ] {
            ensure!(
                inputs[name] == context[name],
                "execution inputs do not bind the retained configured {name}"
            );
        }
        Ok(())
    }
}

struct Received {
    route: Route,
    facts: Value,
}

fn received(connection: &Connection, event_key: &str, id: i64) -> Result<Received> {
    ensure!(
        digest(event_key) && id > 0,
        "invalid retained event/reception identity"
    );
    let (route, headers, body, classification): (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) = connection
        .query_row(
            "SELECT r.route_bytes,r.header_bytes,r.body_bytes,c.metadata
             FROM intake_receptions r JOIN intake_classifications c ON c.reception_id=r.id
             JOIN intake_events e ON e.key=c.event_key
             WHERE r.id=?1 AND e.key=?2 AND r.authentication='matched'
               AND r.body_sha256=e.body_sha256",
            params![id, event_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .context("classified authenticated reception is unavailable")?;
    let hashes: (String, String, String) = connection.query_row(
        "SELECT route_sha256,header_sha256,body_sha256 FROM intake_receptions WHERE id=?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    ensure!(
        hashes
            == (
                audit_evidence::hash(&route),
                audit_evidence::hash(&headers),
                audit_evidence::hash(&body)
            ),
        "retained reception hashes disagree"
    );
    let route = Route::parse(&route)?;
    ensure!(
        scheduler::digest(&("jeryu.audit-received-body/v1", &route.route_id, &hashes.2))?
            == event_key,
        "retained signed body differs from requested event"
    );
    let headers = Headers::parse(&headers)?;
    let translation = translate::translate(&route, &headers, &body);
    let JsonObject(classification): JsonObject<Value> = serde_json::from_slice(&classification)?;
    ensure!(
        classification["reception_id"] == id
            && classification["event_key"] == event_key
            && classification["reception_accepted"] == true
            && classification["hmac_verified"] == true
            && classification["headers_authenticated"] == false
            && classification["context"] == route.public_context()
            && classification["body_sha256"] == hashes.2
            && classification["translation"] == translation,
        "retained reception classification is rejected or inconsistent"
    );
    ensure!(
        translation["state"] == "pending_plan",
        "received source identity remains unresolved"
    );
    Ok(Received {
        route,
        facts: translation["facts"].clone(),
    })
}

fn requests(event_key: &str, inputs: &Inputs, facts: &Value) -> Result<Vec<Request>> {
    let field = |name: &str| -> Result<String> {
        Ok(facts[name]
            .as_str()
            .context("missing received source field")?
            .to_owned())
    };
    let request = |event, source_ref, before, after| Request {
        schema_version: "jeryu.audit-plan-request/v1".into(),
        identity: inputs.identity.clone(),
        event,
        source_ref,
        before,
        after,
        // This is the local planning request, never a claimed GitHub run identity.
        run_id: format!("intake-{event_key}"),
        run_attempt: 1,
        previous_run_id: None,
    };
    match facts["event"].as_str() {
        Some("push") => Ok(vec![request(
            scheduler::Event::Push,
            field("source_ref")?,
            Some(field("before")?),
            Some(field("after")?),
        )]),
        Some("pull_request") => {
            let reference = field("source_ref")?;
            let mut result = vec![request(
                scheduler::Event::PullRequest,
                reference.clone(),
                None,
                Some(field("head_commit")?),
            )];
            if let Some(merged) = facts["merged_commit"].as_str() {
                let prefix = reference
                    .strip_suffix("/head")
                    .context("invalid received PR ref")?;
                result.push(request(
                    scheduler::Event::PullRequest,
                    format!("{prefix}/merge"),
                    None,
                    Some(merged.to_owned()),
                ));
            }
            Ok(result)
        }
        _ => bail!("received event requires source reconciliation before planning"),
    }
}

fn queue_once(
    connection: &mut Connection,
    event_key: &str,
    reception_id: i64,
    source_repo: &Path,
    inputs: &Inputs,
    now: i64,
) -> Result<Value> {
    let received = received(connection, event_key, reception_id)?;
    inputs.bind(&received.route)?;
    // Finish every required endpoint before beginning the atomic queue write.
    // A missing merge object must not leave a queued head claiming complete reception.
    let plans = requests(event_key, inputs, &received.facts)?
        .into_iter()
        .map(|request| scheduler::plan(source_repo, request))
        .collect::<Result<Vec<_>>>()?;
    let identity_hash = scheduler::digest(&inputs.identity)?;
    let facts_hash = scheduler::digest(&received.facts)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Keep input observation and queue insertion in the same snapshot. No JSON import
    // can manufacture this link; source plans above came from the owning Git planner.
    let confirmed = self::received(&transaction, event_key, reception_id)?;
    inputs.bind(&confirmed.route)?;
    ensure!(
        confirmed.facts == received.facts,
        "received facts changed during planning"
    );
    let mut results = Vec::new();
    for plan in plans {
        let result = audit_ledger::import_in_transaction(
            &transaction,
            &serde_json::to_vec(&plan)?,
            &inputs.config,
            &inputs.governing,
            &inputs.candidate,
            now,
        )?;
        let plan_id = result["plan_id"]
            .as_str()
            .context("missing queued plan identity")?;
        transaction.execute(
            "INSERT OR IGNORE INTO intake_plan_links(event_key,identity_sha256,source_ref,plan_id,reception_id,facts_sha256,linked_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![event_key,identity_hash,plan.source_ref,plan_id,reception_id,facts_hash,now],
        )?;
        let original: (String, String) = transaction.query_row(
            "SELECT plan_id,facts_sha256 FROM intake_plan_links WHERE event_key=?1 AND identity_sha256=?2 AND source_ref=?3",
            params![event_key,identity_hash,plan.source_ref], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(
            original == (plan_id.to_owned(), facts_hash.clone()),
            "received event plan conflicts with original queue binding"
        );
        results.push(json!({"plan_id":plan_id,"source_ref":plan.source_ref,
            "jobs":plan.jobs.len(),"disposition":plan.disposition}));
    }
    transaction.commit()?;
    Ok(json!({"event_key":event_key,"reception_id":reception_id,
        "identity_sha256":identity_hash,"facts_sha256":facts_hash,"plans":results,
        "queue_imported":true,"received_endpoints_enumerated":true,
        "headers_authenticated":false,"enrollment_complete":false,"execution_verified":false,
        "governing_policy_authenticated":false,"publication_qualified":false}))
}

pub(super) fn run(
    connection: &mut Connection,
    event_key: &str,
    reception_id: i64,
    source_repo: &Path,
    inputs: Result<Inputs>,
    now: i64,
) -> Result<Value> {
    let result = inputs.and_then(|inputs| {
        queue_once(
            connection,
            event_key,
            reception_id,
            source_repo,
            &inputs,
            now,
        )
    });
    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            let diagnostic = format!("{error:#}").chars().take(4096).collect::<String>();
            store::failure(
                connection,
                event_key,
                FailureKind::PlannerError,
                None,
                Some(diagnostic.into_bytes()),
                now,
            )
            .context("queue failed and failure persistence was also unavailable")?;
            bail!(
                "received-event planning or queue import failed; private diagnostic and reception retained"
            );
        }
    }
}

pub(super) fn links(connection: &Connection, event_key: &str) -> Result<Vec<Value>> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 3 {
        return Ok(Vec::new());
    }
    let mut statement = connection.prepare(
        "SELECT identity_sha256,source_ref,plan_id,reception_id,facts_sha256,linked_at
         FROM intake_plan_links WHERE event_key=?1 ORDER BY identity_sha256,source_ref",
    )?;
    Ok(statement
        .query_map([event_key], |row| {
            Ok(json!({
        "identity_sha256":row.get::<_,String>(0)?,"source_ref":row.get::<_,String>(1)?,
        "plan_id":row.get::<_,String>(2)?,"reception_id":row.get::<_,i64>(3)?,
        "facts_sha256":row.get::<_,String>(4)?,"linked_at":row.get::<_,i64>(5)?,
        "execution_verified":false,"publication_qualified":false}))
        })?
        .collect::<rusqlite::Result<_>>()?)
}
