use super::*;
use std::os::unix::fs::PermissionsExt;

struct Fixture {
    connection: Connection,
    plan: Value,
    config: Vec<u8>,
    policy: Vec<u8>,
}

fn encoded(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

impl Fixture {
    fn new() -> Self {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        initialize(&mut connection).unwrap();
        let policy = b"workspace='jeryu'\nminimum_score=85\nhard_findings_allowed=0\nrequired_tool='jankurai'\nrequired_tool_version='1.6.11'\n".to_vec();
        let config = encoded(&json!({"schema_version":"jeryu.audit-ledger-execution/v1",
            "auditor_version":"1.6.11","policy_path":"./agent/audit-policy.toml","minimum":85,"max_soft":0,
            "candidate_policy_sha256":audit_evidence::hash(&policy),
            "executor_inputs":{"fixture":true,"qualification":"synthetic-unqualified"}}));
        let mut plan = json!({"schema_version":"jeryu.audit-plan/v1","identity":{
            "repository":"neverhuman/jeryu","scope":"repository","auditor_source_commit":"e".repeat(40),
            "auditor_executable_sha256":"f".repeat(64),"auditor_receipt_sha256":"1".repeat(64),
            "governing_policy_sha256":audit_evidence::hash(&policy),"execution_config_sha256":audit_evidence::hash(&config)},
            "event":"push","source_ref":"refs/heads/main","before":null,"after":"a".repeat(40),
            "resolved_after_commit":"a".repeat(40),"disposition":"created","run_id":"fixture-1","run_attempt":1,
            "previous_run_id":null,"withdrawn_commits":[],"jobs":[{"source_commit":"a".repeat(40),
                "source_tree":"b".repeat(40),"deduplication_key":"","attempt_key":"","status":"pending"}],
            "execution_verified":false,"publication_qualified":false});
        Self::keys(&mut plan);
        Self {
            connection,
            plan,
            config,
            policy,
        }
    }

    fn keys(plan: &mut Value) {
        let identity: scheduler::ExecutionIdentity =
            serde_json::from_value(plan["identity"].clone()).unwrap();
        let run = plan["run_id"].as_str().unwrap().to_owned();
        let attempt = plan["run_attempt"].as_u64().unwrap() as u32;
        for job in plan["jobs"].as_array_mut().unwrap() {
            let key = scheduler::source_key(
                &identity,
                job["source_commit"].as_str().unwrap(),
                job["source_tree"].as_str().unwrap(),
            )
            .unwrap();
            job["attempt_key"] = json!(scheduler::attempt_key(&key, &run, attempt).unwrap());
            job["deduplication_key"] = json!(key);
        }
    }

    fn import(&mut self) -> Result<Value> {
        store::import(
            &mut self.connection,
            &encoded(&self.plan),
            &self.config,
            &self.policy,
            &self.policy,
            100,
        )
    }

    fn start(&mut self) -> Value {
        let request = self.plan["jobs"][0]["attempt_key"].as_str().unwrap();
        store::start(&mut self.connection, request, 10, 110).unwrap()
    }

    fn receipt(
        &self,
        start: &Value,
        outcome: &str,
        report: Option<&[u8]>,
        exit: Option<i32>,
    ) -> Value {
        json!({"schema_version":"jeryu.audit-ledger-observation/v1","attempt_id":start["attempt_id"],
            "deduplication_key":start["deduplication_key"],"source_commit":self.plan["jobs"][0]["source_commit"],
            "source_tree":self.plan["jobs"][0]["source_tree"],"identity":self.plan["identity"],
            "command_exit":exit,"outcome":outcome,"reason":"synthetic fixture outcome; no execution qualification",
            "report_sha256":report.map(audit_evidence::hash)})
    }

    fn close(&mut self, start: &Value, time: i64) -> Result<Value> {
        store::close(
            &mut self.connection,
            &encoded(&json!({"schema_version":"jeryu.audit-ledger-closure/v1",
            "attempt_id":start["attempt_id"],"deduplication_key":start["deduplication_key"],"executor_closed":true,
            "reason":"fixture executor closure acknowledgement; not authenticated"})),
            time,
        )
    }

    fn status(&self) -> Value {
        store::status(&self.connection).unwrap()
    }
}

fn report() -> Value {
    json!({"standard":"jankurai","schema_version":"1.9.0","auditor_version":"1.6.11","repo":".","score":95,
        "dirty_worktree":false,"scope":{"mode":"full","paths":[]},"git":{"head":"aaaaaaaa","mode":"full","dirty_worktree":false},
        "policy":{"minimum_score":85,"mode":"standard","path":"./agent/audit-policy.toml","fail_on":["critical","high"]},
        "decision":{"status":"pass","passed":true,"minimum_score":85,"hard_findings":0,"soft_findings":0,"ratchet":{"passed":true}},
        "conformance_decision":"pass","conformance_blockers":[],"findings":[],"caps_applied":[]})
}

#[test]
fn duplicate_events_share_jobs_but_preserve_each_scheduled_request() {
    let mut fixture = Fixture::new();
    let first = fixture.import().unwrap();
    assert_eq!(fixture.import().unwrap(), first);
    assert_eq!(fixture.status()["plans"].as_array().unwrap().len(), 1);
    fixture.plan["run_id"] = json!("fixture-retry");
    fixture.plan["previous_run_id"] = json!("fixture-1");
    Fixture::keys(&mut fixture.plan);
    fixture.import().unwrap();
    let status = fixture.status();
    assert_eq!(status["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(
        status["jobs"][0]["scheduled_requests"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(status["plans"].as_array().unwrap().len(), 2);
    fixture.plan["identity"]["auditor_executable_sha256"] = json!("2".repeat(64));
    Fixture::keys(&mut fixture.plan);
    fixture.import().unwrap();
    assert_eq!(fixture.status()["jobs"].as_array().unwrap().len(), 2);
}

#[test]
fn multi_commit_import_and_reconciliation_never_truncate() {
    let mut fixture = Fixture::new();
    let template = fixture.plan["jobs"][0].clone();
    fixture.plan["jobs"] = json!(
        (1..=2050)
            .map(|number| {
                let mut job = template.clone();
                job["source_commit"] = json!(format!("{number:040x}"));
                job
            })
            .collect::<Vec<_>>()
    );
    Fixture::keys(&mut fixture.plan);
    fixture.import().unwrap();
    let requests: Vec<_> = fixture.plan["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job["attempt_key"].as_str().unwrap().to_owned())
        .collect();
    for request in &requests {
        store::start(&mut fixture.connection, request, 10, 110).unwrap();
    }
    assert_eq!(
        store::reconcile(&mut fixture.connection, 120).unwrap()["timeouts_recorded"],
        2050
    );
    let status = fixture.status();
    assert_eq!(status["jobs"].as_array().unwrap().len(), 2050);
    assert!(
        status["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|job| job["state"] == "closure_pending" && job["pending_retry"] == true)
    );
}

#[test]
fn altered_event_content_rolls_back_and_forged_keys_cannot_enter() {
    let mut fixture = Fixture::new();
    fixture.import().unwrap();
    let before = fixture.status();
    fixture.plan["jobs"][0]["source_tree"] = json!("c".repeat(40));
    Fixture::keys(&mut fixture.plan);
    assert!(fixture.import().is_err());
    assert_eq!(fixture.status(), before);
    fixture.plan["run_id"] = json!("new-event");
    fixture.plan["jobs"][0]["deduplication_key"] = json!("9".repeat(64));
    assert!(fixture.import().is_err());
    Fixture::keys(&mut fixture.plan);
    fixture.plan["publication_qualified"] = json!(true);
    assert!(fixture.import().is_err());
    assert_eq!(fixture.status(), before);
}

#[test]
fn timeout_retains_lease_until_separate_closure_then_retry_preserves_failure() {
    let mut fixture = Fixture::new();
    fixture.import().unwrap();
    let first = fixture.start();
    assert!(fixture.close(&first, 111).is_err());
    assert_eq!(
        store::reconcile(&mut fixture.connection, 119).unwrap()["timeouts_recorded"],
        0
    );
    assert_eq!(
        store::reconcile(&mut fixture.connection, 120).unwrap()["timeouts_recorded"],
        1
    );
    assert_eq!(
        store::reconcile(&mut fixture.connection, 121).unwrap()["timeouts_recorded"],
        0
    );
    let request = fixture.plan["jobs"][0]["attempt_key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(store::start(&mut fixture.connection, &request, 10, 122).is_err());
    assert_eq!(fixture.status()["jobs"][0]["state"], "closure_pending");
    assert_eq!(fixture.status()["jobs"][0]["pending_retry"], true);
    fixture.close(&first, 123).unwrap();
    assert_eq!(fixture.status()["jobs"][0]["state"], "pending_retry");
    let second = store::start(&mut fixture.connection, &request, 10, 124).unwrap();
    assert_ne!(first["attempt_id"], second["attempt_id"]);
    let history = &fixture.status()["jobs"][0]["attempts"];
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[0]["observations"][0]["outcome"], "timed_out");
}

#[path = "audit_ledger_disk_tests.rs"]
mod disk;
#[path = "audit_ledger_report_tests.rs"]
mod reports;
