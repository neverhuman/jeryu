use super::*;
use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

const GUID: &str = "01234567-89ab-cdef-0123-456789abcdef";
const SECRET: &[u8] = b"synthetic-private-webhook-secret-never-publish";

struct Fixture {
    root: PathBuf,
    database: PathBuf,
    route: Vec<u8>,
    connection: Connection,
}

fn write(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("jeryu-intake-test-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
            .keep();
        eprintln!("retained synthetic intake fixture: {}", root.display());
        write(&root.join("secret"), SECRET);
        let route=serde_json::to_vec(&json!({"schema_version":"jeryu.audit-intake-route/v1",
            "route_id":"jeryu-public","repository_id":123,"repository":"neverhuman/jeryu","secret_file":root.join("secret"),
            "secret_version":"fixture-v1","receiver_source_commit":"a".repeat(40),"receiver_executable_sha256":"b".repeat(64),
            "governing_workflow_repository":"neverhuman/jeryu","governing_workflow_path":".github/workflows/audit.yml",
            "governing_workflow_commit":"c".repeat(40),"governing_workflow_blob":"d".repeat(40),
            "executor_source_commit":"e".repeat(40),"executor_executable_sha256":"f".repeat(64),"executor_receipt_sha256":"1".repeat(64),
            "governing_policy_sha256":"2".repeat(64),"execution_config_sha256":"3".repeat(64)})).unwrap();
        let database = root.join("ledger.sqlite");
        let connection = audit_ledger::open(&database, false).unwrap();
        Self {
            root,
            database,
            route,
            connection,
        }
    }
    fn capture(&mut self, body: &[u8], headers: Vec<u8>) -> i64 {
        capture(
            &mut self.connection,
            Ok(self.route.clone()),
            Ok(headers),
            Ok(body.to_vec()),
            1234,
        )
        .unwrap()
    }
    fn receive(&mut self, body: &[u8], event: &str, guid: &str) -> Value {
        let id = self.capture(body, headers(body, event, guid));
        store::classify(&mut self.connection, id).unwrap()
    }
    fn status(&self) -> Value {
        status_snapshot(&self.connection).unwrap()
    }
}

fn signature(bytes: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET).unwrap();
    mac.update(bytes);
    format!("sha256={:x}", mac.finalize().into_bytes())
}
fn headers(body: &[u8], event: &str, guid: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"delivery":guid,"event":event,"signature_256":signature(body)}))
        .unwrap()
}
fn push(before: &str, after: &str, reference: &str) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"repository":{"id":123,"full_name":"neverhuman/jeryu"},
        "ref":reference,"before":before,"after":after,"created":before=="0".repeat(40),
        "deleted":after=="0".repeat(40),"forced":false,
        "head_commit":{"message":"private-payload-sentinel-never-publish"},"commits":[]}),
    )
    .unwrap()
}

#[test]
fn raw_hmac_matches_known_vector_and_signature_changes_never_claim_authentication() {
    let signature = "sha256=b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7";
    assert!(signature_matches(&[0x0b; 20], b"Hi There", signature));
    assert!(!signature_matches(&[0x0b; 20], b"Hi There\n", signature));
    assert!(!signature_matches(&[0x0c; 20], b"Hi There", signature));
    for wrong in ["sha1=abcd", "sha256=", "sha256=zzzz", ""] {
        assert!(!signature_matches(SECRET, b"body", wrong));
    }
    let mut fixture = Fixture::new();
    let original = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let mut reserialized = original.clone();
    reserialized.push(b'\n');
    let id = fixture.capture(&reserialized, headers(&original, "push", GUID));
    let result = store::classify(&mut fixture.connection, id).unwrap();
    assert_eq!(result["reception_accepted"], false);
    assert_eq!(result["hmac_verified"], false);
    assert_eq!(result["reason"], "signature_mismatch");
    assert_eq!(fixture.status()["events"].as_array().unwrap().len(), 0);
    let stored: Vec<u8> = fixture
        .connection
        .query_row(
            "SELECT body_bytes FROM intake_receptions WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, reserialized);
}

#[test]
fn invalid_signature_cannot_poison_delivery_and_replays_keep_one_source_event() {
    let mut fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let bad = fixture.capture(&body, headers(b"different bytes", "push", GUID));
    assert_eq!(
        store::classify(&mut fixture.connection, bad).unwrap()["reception_accepted"],
        false
    );
    let first = fixture.receive(&body, "push", GUID);
    assert_eq!(first["state"], "received");
    let duplicate = fixture.receive(&body, "push", GUID);
    assert_eq!(duplicate["state"], "duplicate_delivery");
    let replay = fixture.receive(&body, "push", "11234567-89ab-cdef-0123-456789abcdef");
    assert_eq!(replay["state"], "duplicate_event");
    assert_eq!(first["event_key"], duplicate["event_key"]);
    assert_eq!(first["event_key"], replay["event_key"]);
    let state = fixture.status();
    assert_eq!(state["receptions"].as_array().unwrap().len(), 4);
    assert_eq!(state["events"].as_array().unwrap().len(), 1);
    assert_eq!(state["deliveries"].as_array().unwrap().len(), 2);
    assert_eq!(state["pending_received_events"], 1);
    assert_eq!(state["publication_qualified"], false);
    assert_eq!(
        state["events"][0]["metadata"]["context"]["governing_policy_authenticated"],
        false
    );
}

#[test]
fn conflicting_delivery_retains_both_bodies_without_replacing_original_obligation() {
    let mut fixture = Fixture::new();
    let first_body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let second_body = push(&"b".repeat(40), &"c".repeat(40), "refs/heads/main");
    let first = fixture.receive(&first_body, "push", GUID);
    let conflict = fixture.receive(&second_body, "push", GUID);
    assert_eq!(conflict["reception_accepted"], false);
    assert_eq!(conflict["state"], "conflicting_delivery");
    assert_eq!(conflict["original_event_key"], first["event_key"]);
    let state = fixture.status();
    assert_eq!(state["events"].as_array().unwrap().len(), 2);
    assert_eq!(state["pending_received_events"], 2);
    assert_eq!(
        state["events"][0]["metadata"]["body_sha256"],
        audit_evidence::hash(&first_body)
    );
    let bodies: Vec<Vec<u8>> = {
        let mut statement = fixture
            .connection
            .prepare("SELECT body_bytes FROM intake_receptions ORDER BY id")
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(bodies, vec![first_body, second_body]);
}

#[test]
fn committed_reception_survives_interruption_and_parse_failures_are_visible() {
    let mut fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let id = fixture.capture(&body, headers(&body, "push", GUID));
    assert_eq!(fixture.status()["pending_classifications"], json!([id]));
    drop(fixture.connection);
    fixture.connection = audit_ledger::open(&fixture.database, false).unwrap();
    assert_eq!(
        store::reconcile(&mut fixture.connection).unwrap()["classification_errors"],
        0
    );
    assert!(
        fixture.status()["pending_classifications"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for malformed in [
        b"{".as_slice(),
        b"[]".as_slice(),
        br#"{"repository":{"id":123,"id":456,"full_name":"neverhuman/jeryu"}}"#.as_slice(),
    ] {
        let result = fixture.receive(malformed, "push", GUID);
        assert_eq!(result["hmac_verified"], true);
        assert_eq!(result["reception_accepted"], false);
        assert_eq!(result["translation"]["state"], "malformed_payload");
    }
    assert_eq!(fixture.status()["receptions"].as_array().unwrap().len(), 4);
}

#[test]
fn source_transitions_keep_exact_endpoints_and_incomplete_payloads_remain_pending() {
    let mut fixture = Fixture::new();
    for (index, before, after, transition) in [
        (1, "0".repeat(40), "a".repeat(40), "creation"),
        (2, "a".repeat(40), "0".repeat(40), "deletion"),
        (
            3,
            "b".repeat(40),
            "a".repeat(40),
            "graph_classification_required",
        ),
    ] {
        let body = push(&before, &after, "refs/heads/main");
        let result = fixture.receive(
            &body,
            "push",
            &format!("{index:08}-89ab-cdef-0123-456789abcdef"),
        );
        let facts = &result["translation"]["facts"];
        assert_eq!(facts["before"], before);
        assert_eq!(facts["after"], after);
        assert_eq!(facts["transition"], transition);
        assert_eq!(facts["payload_commit_array_used_for_coverage"], false);
        assert_eq!(result["translation"]["planning_complete"], false);
    }
    let mut payload: Value =
        serde_json::from_slice(&push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main")).unwrap();
    payload.as_object_mut().unwrap().remove("before");
    let result = fixture.receive(&serde_json::to_vec(&payload).unwrap(), "push", GUID);
    assert_eq!(result["translation"]["state"], "incomplete_identity");
    let deleted = json!({"repository":{"id":123,"full_name":"neverhuman/jeryu"},"ref":"main","ref_type":"branch"});
    let result = fixture.receive(
        &serde_json::to_vec(&deleted).unwrap(),
        "delete",
        "21234567-89ab-cdef-0123-456789abcdef",
    );
    assert_eq!(result["translation"]["state"], "incomplete_identity");
    assert_eq!(
        result["translation"]["facts"]["source_ref"],
        "refs/heads/main"
    );
}

#[test]
fn raw_payload_and_diagnostics_never_enter_metadata_views_and_failures_do_not_close_work() {
    let mut fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let result = fixture.receive(&body, "push", GUID);
    let key = result["event_key"].as_str().unwrap();
    for (index, kind) in [
        FailureKind::SourceUnavailable,
        FailureKind::PlannerError,
        FailureKind::QueueError,
    ]
    .into_iter()
    .enumerate()
    {
        store::failure(
            &mut fixture.connection,
            key,
            kind,
            Some(42),
            Some(b"private-diagnostic-sentinel-never-publish".to_vec()),
            2000 + index as i64,
        )
        .unwrap();
    }
    let state = fixture.status();
    let encoded = serde_json::to_string(&state).unwrap();
    assert!(!encoded.contains("private-payload-sentinel"));
    assert!(!encoded.contains("private-diagnostic-sentinel"));
    assert!(!encoded.contains(std::str::from_utf8(SECRET).unwrap()));
    assert!(!encoded.contains(&signature(&body)));
    assert_eq!(
        state["events"][0]["worker_failures"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(state["events"][0]["planning_complete"], false);
    assert_eq!(state["pending_received_events"], 1);
    for table in [
        "intake_receptions",
        "intake_events",
        "intake_deliveries",
        "intake_classifications",
        "intake_failures",
    ] {
        assert!(
            fixture
                .connection
                .execute(&format!("DELETE FROM {table}"), [])
                .is_err()
        );
    }
}

#[path = "audit_intake_disk_tests.rs"]
mod disk;

#[test]
fn first_received_delivery_survives_reversed_classification_and_unsigned_header_changes() {
    let mut fixture = Fixture::new();
    let first = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let second = push(&"b".repeat(40), &"c".repeat(40), "refs/heads/main");
    let first_id = fixture.capture(&first, headers(&first, "push", GUID));
    let second_id = fixture.capture(&second, headers(&second, "push", GUID));
    assert_eq!(fixture.status()["pending_received_events"], 2);
    let later = store::classify(&mut fixture.connection, second_id).unwrap();
    assert_eq!(later["state"], "conflicting_delivery");
    let original = store::classify(&mut fixture.connection, first_id).unwrap();
    assert_eq!(original["state"], "received");
    assert_eq!(later["original_event_key"], original["event_key"]);
    let changed_header = fixture.receive(&first, "release", GUID);
    assert_eq!(changed_header["hmac_verified"], true);
    assert_eq!(changed_header["headers_authenticated"], false);
    assert_eq!(changed_header["state"], "conflicting_delivery");
    assert_eq!(fixture.status()["pending_received_events"], 2);
    assert_eq!(
        fixture.status()["deliveries"][0]["first_reception_id"],
        first_id
    );
}

#[test]
fn classifier_failure_is_durable_and_does_not_prevent_other_receptions() {
    let mut fixture = Fixture::new();
    let first = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let second = push(&"b".repeat(40), &"c".repeat(40), "refs/heads/main");
    let first_id = fixture.capture(&first, headers(&first, "push", GUID));
    let second_id = fixture.capture(
        &second,
        headers(&second, "push", "11234567-89ab-cdef-0123-456789abcdef"),
    );
    fixture.connection.execute_batch(&format!("CREATE TRIGGER fixture_classification_failure BEFORE INSERT ON intake_classifications WHEN NEW.reception_id={first_id} BEGIN SELECT RAISE(ABORT,'fixture-private-processing-diagnostic'); END;")).unwrap();
    let result = store::reconcile(&mut fixture.connection).unwrap();
    assert_eq!(result["classification_errors"], 1);
    assert_eq!(result["receptions"][1]["reception_id"], second_id);
    assert_eq!(result["receptions"][1]["state"], "received");
    let state = fixture.status();
    assert_eq!(state["pending_classifications"], json!([first_id]));
    assert_eq!(state["processing_errors"].as_array().unwrap().len(), 1);
    assert!(
        !serde_json::to_string(&state)
            .unwrap()
            .contains("fixture-private-processing-diagnostic")
    );
    fixture
        .connection
        .execute_batch("DROP TRIGGER fixture_classification_failure")
        .unwrap();
    assert_eq!(
        store::reconcile(&mut fixture.connection).unwrap()["classification_errors"],
        0
    );
    let state = fixture.status();
    assert!(
        state["pending_classifications"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(state["processing_errors"].as_array().unwrap().len(), 1);
    assert_eq!(state["pending_received_events"], 2);
    assert!(
        fixture
            .connection
            .execute("DELETE FROM intake_processing_errors", [])
            .is_err()
    );
}

#[test]
fn unsigned_metadata_cannot_suppress_a_signed_body_or_reserve_invalid_deliveries() {
    let mut fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let original: Value = serde_json::from_slice(&headers(&body, "push", GUID)).unwrap();
    let mut mutations = Vec::new();
    for (key, value) in [
        ("delivery", json!("bad-guid")),
        ("delivery", json!(false)),
        ("event", json!("bad\nevent")),
        ("event", json!({"trusted":true})),
    ] {
        let mut header = original.clone();
        header[key] = value;
        mutations.push(header);
    }
    for key in ["delivery", "event"] {
        let mut header = original.clone();
        header.as_object_mut().unwrap().remove(key);
        mutations.push(header);
    }
    for header in mutations {
        let id = fixture.capture(&body, serde_json::to_vec(&header).unwrap());
        assert_eq!(fixture.status()["pending_received_events"], 1);
        let result = store::classify(&mut fixture.connection, id).unwrap();
        assert_eq!(result["hmac_verified"], true);
        assert_eq!(result["reception_accepted"], false);
        assert_eq!(result["state"], "invalid_unsigned_headers");
        assert!(
            fixture.status()["deliveries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    let accepted = fixture.receive(&body, "push", GUID);
    assert_eq!(accepted["state"], "duplicate_event");
    assert_eq!(accepted["reception_accepted"], true);
    assert_eq!(fixture.status()["deliveries"].as_array().unwrap().len(), 1);
    let other = push(&"b".repeat(40), &"c".repeat(40), "refs/heads/main");
    let original = headers(&other, "push", "11234567-89ab-cdef-0123-456789abcdef");
    let mut duplicate = b"{\"signature_256\":".to_vec();
    duplicate.extend(serde_json::to_vec(&signature(&other)).unwrap());
    duplicate.push(b',');
    duplicate.extend_from_slice(&original[1..]);
    let id = fixture.capture(&other, duplicate);
    let result = store::classify(&mut fixture.connection, id).unwrap();
    assert_eq!(result["hmac_verified"], false);
    assert_eq!(result["reason"], "headers_unavailable_or_invalid");
    assert_eq!(fixture.status()["pending_received_events"], 1);
}
