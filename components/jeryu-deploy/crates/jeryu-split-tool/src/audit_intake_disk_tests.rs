use super::*;
use clap::Parser;
use std::{
    ffi::OsString,
    os::unix::fs::{MetadataExt, symlink},
};

#[test]
fn private_file_and_secret_custody_refuse_links_fifo_and_unavailable_bodies() {
    let mut fixture = Fixture::new();
    let path = fixture.root.join("body");
    write(&path, b"ordinary");
    assert_eq!(input::read_private(&path, 8).unwrap(), b"ordinary");
    assert!(input::read_private(&path, 7).is_err());
    let alias = fixture.root.join("body-alias");
    symlink(&path, &alias).unwrap();
    assert!(input::read_private(&alias, 8).is_err());
    fs::hard_link(&path, fixture.root.join("body-hardlink")).unwrap();
    assert!(input::read_private(&path, 8).is_err());
    let fifo = fixture.root.join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    assert!(input::read_private(&fifo, 8).is_err());
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let id = capture(
        &mut fixture.connection,
        Ok(fixture.route.clone()),
        Ok(headers(&body, "push", GUID)),
        input::read_private(&fixture.root.join("absent"), MAX_BODY),
        1234,
    )
    .unwrap();
    assert_eq!(
        store::classify(&mut fixture.connection, id).unwrap()["reason"],
        "body_unavailable"
    );
    let oversized = fixture.root.join("oversized");
    write(&oversized, b"");
    fs::OpenOptions::new()
        .write(true)
        .open(&oversized)
        .unwrap()
        .set_len(MAX_BODY as u64 + 1)
        .unwrap();
    let id = capture(
        &mut fixture.connection,
        Ok(fixture.route.clone()),
        Ok(headers(&body, "push", GUID)),
        input::read_private(&oversized, MAX_BODY),
        1234,
    )
    .unwrap();
    assert_eq!(
        store::classify(&mut fixture.connection, id).unwrap()["reason"],
        "body_unavailable"
    );
    fs::set_permissions(
        fixture.root.join("secret"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert_eq!(
        fixture.receive(&body, "push", GUID)["reason"],
        "secret_unavailable"
    );
    assert_eq!(fixture.status()["events"].as_array().unwrap().len(), 0);
}

#[test]
fn route_and_header_flags_cannot_grant_trust_or_select_payload_owned_keys() {
    let mut fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    let mut route: Value = serde_json::from_slice(&fixture.route).unwrap();
    route["trusted"] = json!(true);
    let id = capture(
        &mut fixture.connection,
        Ok(serde_json::to_vec(&route).unwrap()),
        Ok(headers(&body, "push", GUID)),
        Ok(body.clone()),
        1234,
    )
    .unwrap();
    assert_eq!(
        store::classify(&mut fixture.connection, id).unwrap()["reason"],
        "route_unavailable_or_invalid"
    );
    let mut header: Value = serde_json::from_slice(&headers(&body, "push", GUID)).unwrap();
    header["trusted"] = json!(true);
    let id = fixture.capture(&body, serde_json::to_vec(&header).unwrap());
    assert_eq!(
        store::classify(&mut fixture.connection, id).unwrap()["state"],
        "invalid_unsigned_headers"
    );
    let mut other: Value = serde_json::from_slice(&body).unwrap();
    other["repository"]["id"] = json!(999);
    let result = fixture.receive(&serde_json::to_vec(&other).unwrap(), "push", GUID);
    assert_eq!(result["hmac_verified"], true);
    assert_eq!(result["translation"]["state"], "wrong_repository");
    assert_eq!(fixture.status()["events"].as_array().unwrap().len(), 2);
    // Exact secret bytes matter. No trailing-newline normalization is permitted.
    let mut changed = SECRET.to_vec();
    changed.push(b'\n');
    fs::write(fixture.root.join("secret"), changed).unwrap();
    assert_eq!(
        fixture.receive(&body, "push", GUID)["reason"],
        "signature_mismatch"
    );
}

#[test]
fn fork_release_and_evidence_events_remain_explicit_without_credential_or_graph_assumptions() {
    let mut fixture = Fixture::new();
    let pr = json!({"repository":{"id":123,"full_name":"neverhuman/jeryu"},"action":"synchronize","number":7,
        "pull_request":{"number":7,"base":{"repo":{"id":123,"full_name":"neverhuman/jeryu"}},
            "head":{"sha":"a".repeat(40),"repo":{"id":999,"full_name":"contributor/jeryu"}},"merged":false,"merge_commit_sha":null}});
    let result = fixture.receive(&serde_json::to_vec(&pr).unwrap(), "pull_request", GUID);
    assert_eq!(result["translation"]["facts"]["fork"], true);
    assert_eq!(
        result["translation"]["facts"]["fork_execution_admission"],
        false
    );
    assert_eq!(
        result["translation"]["facts"]["head_repository"],
        "contributor/jeryu"
    );
    let release = json!({"repository":{"id":123,"full_name":"neverhuman/jeryu"},"action":"published",
        "release":{"id":9,"tag_name":"v5.1.0","target_commitish":"main"}});
    let result = fixture.receive(
        &serde_json::to_vec(&release).unwrap(),
        "release",
        "11234567-89ab-cdef-0123-456789abcdef",
    );
    assert_eq!(
        result["translation"]["state"],
        "pending_immutable_tag_resolution"
    );
    assert!(result["translation"]["facts"]["peeled_commit"].is_null());
    let evidence = push(
        &"a".repeat(40),
        &"b".repeat(40),
        "refs/heads/audit-evidence",
    );
    let result = fixture.receive(&evidence, "push", "21234567-89ab-cdef-0123-456789abcdef");
    assert_eq!(
        result["translation"]["state"],
        "pending_evidence_branch_review"
    );
    assert_eq!(fixture.status()["events"].as_array().unwrap().len(), 3);
    assert_eq!(fixture.status()["pending_received_events"], 3);
}

#[test]
fn legacy_schema_migration_preserves_history_and_read_only_status_does_not_upgrade() {
    let fixture = Fixture::new();
    let path = fixture.root.join("legacy.sqlite");
    write(&path, b"");
    let mut legacy = Connection::open(&path).unwrap();
    let transaction = legacy.transaction().unwrap();
    audit_ledger::initialize_legacy(&transaction).unwrap();
    transaction.commit().unwrap();
    legacy.execute_batch(r"
        INSERT INTO plans(id,event_key,bytes,imported_at) VALUES('original','original',X'7b7d',1);
        INSERT INTO jobs(key,identity,source_commit,source_tree,config,governing_policy,candidate_policy)
            VALUES('job',X'7b7d','commit','tree',X'7b7d',X'6162',X'6364');
        INSERT INTO requests(key,job_key) VALUES('request','job');
        INSERT INTO plan_jobs(plan_id,request_key) VALUES('original','request');
        INSERT INTO attempts(id,job_key,request_key,ordinal,started_at,deadline)
            VALUES('attempt','job','request',1,2,3);
        INSERT INTO observations(id,attempt_id,kind,outcome,recorded_at,receipt,report,receipt_sha256,report_sha256,summary,diagnostic,reason)
            VALUES('observation','attempt','finish','tool_error',4,X'7b7d',X'ff00','receipt-hash','report-hash',X'7b7d','original failure','original reason');
        INSERT INTO closures(attempt_id,acknowledged_at,acknowledgement,acknowledgement_sha256)
            VALUES('attempt',5,X'7b7d','closure-hash');
    ").unwrap();
    // Complete old row values, including blobs and failures, must survive the schema extension.
    let tables = [
        "plans",
        "jobs",
        "requests",
        "plan_jobs",
        "attempts",
        "observations",
        "closures",
    ];
    let snapshot = |connection: &Connection| -> Vec<Vec<Vec<rusqlite::types::Value>>> {
        tables
            .iter()
            .map(|table| {
                let mut statement = connection
                    .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                    .unwrap();
                let columns = statement.column_count();
                statement
                    .query_map([], |row| {
                        (0..columns)
                            .map(|index| row.get(index))
                            .collect::<rusqlite::Result<Vec<_>>>()
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap()
            })
            .collect()
    };
    let original_rows = snapshot(&legacy);
    drop(legacy);
    let original = fs::read(&path).unwrap();
    let read_only = audit_ledger::open(&path, true).unwrap();
    assert_eq!(
        status_snapshot(&read_only).unwrap()["migration_required"],
        true
    );
    drop(read_only);
    assert_eq!(fs::read(&path).unwrap(), original);
    let upgraded = audit_ledger::open(&path, false).unwrap();
    let bytes: Vec<u8> = upgraded
        .query_row("SELECT bytes FROM plans WHERE id='original'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(bytes, b"{}");
    assert_eq!(snapshot(&upgraded), original_rows);
    assert_eq!(
        upgraded
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        status_snapshot(&upgraded).unwrap()["pending_received_events"],
        0
    );
    for table in tables {
        assert!(
            upgraded
                .execute(&format!("DELETE FROM {table}"), [])
                .is_err()
        );
    }
    assert!(upgraded.execute("INSERT INTO intake_failures(event_key,kind,recorded_at) VALUES('missing','queue_error',2)",[]).is_err());
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn owning_cli_accepts_only_durable_reception_and_reports_pending_status() {
    let fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    write(&fixture.root.join("route"), &fixture.route);
    write(&fixture.root.join("headers"), &headers(&body, "push", GUID));
    write(&fixture.root.join("body"), &body);
    let mut args = vec![
        OsString::from("jeryu-split"),
        "audit-intake".into(),
        "--database".into(),
        fixture.database.clone().into_os_string(),
        "receive".into(),
    ];
    for name in ["route", "headers", "body"] {
        args.push(format!("--{name}").into());
        args.push(fixture.root.join(name).into_os_string());
    }
    let command = crate::Cli::try_parse_from(args).unwrap();
    assert!(crate::run(command).is_ok());
    let state = fixture.status();
    assert_eq!(state["receptions"].as_array().unwrap().len(), 1);
    assert_eq!(state["pending_received_events"], 1);
    let args = vec![
        OsString::from("jeryu-split"),
        "audit-intake".into(),
        "--database".into(),
        fixture.database.clone().into_os_string(),
        "status".into(),
    ];
    assert!(crate::run(crate::Cli::try_parse_from(args).unwrap()).is_err());
    assert!(
        crate::Cli::try_parse_from(["jeryu-split", "audit-intake", "--trusted", "true"]).is_err()
    );
}

#[test]
fn owning_cli_preserves_failed_reception_and_worker_failure_without_closing_event() {
    let fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    write(&fixture.root.join("route"), &fixture.route);
    write(&fixture.root.join("headers"), &headers(&body, "push", GUID));
    let mut modified = body.clone();
    modified.push(b'\n');
    write(&fixture.root.join("body"), &modified);
    let receive_args = || {
        let mut args = vec![
            OsString::from("jeryu-split"),
            "audit-intake".into(),
            "--database".into(),
            fixture.database.clone().into_os_string(),
            "receive".into(),
        ];
        for name in ["route", "headers", "body"] {
            args.push(format!("--{name}").into());
            args.push(fixture.root.join(name).into_os_string());
        }
        args
    };
    assert!(crate::run(crate::Cli::try_parse_from(receive_args()).unwrap()).is_err());
    assert_eq!(
        fixture.status()["receptions"][0]["hmac_outcome"],
        "signature_mismatch"
    );
    assert_eq!(fixture.status()["pending_received_events"], 0);
    // The rejected raw bytes have already been committed; this changes only caller input.
    fs::write(fixture.root.join("body"), &body).unwrap();
    assert!(crate::run(crate::Cli::try_parse_from(receive_args()).unwrap()).is_ok());
    let state = fixture.status();
    assert_eq!(state["receptions"].as_array().unwrap().len(), 2);
    let event_key = state["events"][0]["event_key"].as_str().unwrap();
    write(
        &fixture.root.join("diagnostic"),
        b"private-cli-worker-diagnostic",
    );
    let args = vec![
        OsString::from("jeryu-split"),
        "audit-intake".into(),
        "--database".into(),
        fixture.database.clone().into_os_string(),
        "failure".into(),
        "--event-key".into(),
        event_key.into(),
        "--kind".into(),
        "source-unavailable".into(),
        "--command-exit".into(),
        "-1".into(),
        "--diagnostic".into(),
        fixture.root.join("diagnostic").into_os_string(),
    ];
    assert!(crate::run(crate::Cli::try_parse_from(args).unwrap()).is_ok());
    let state = fixture.status();
    assert_eq!(state["events"][0]["worker_failures"][0]["command_exit"], -1);
    assert_eq!(state["pending_received_events"], 1);
    assert_eq!(state["events"][0]["planning_complete"], false);
    assert!(
        !serde_json::to_string(&state)
            .unwrap()
            .contains("private-cli-worker-diagnostic")
    );
}
