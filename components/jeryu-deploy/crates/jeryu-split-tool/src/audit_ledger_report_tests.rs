use super::*;

#[test]
fn valid_report_is_only_unqualified_and_completion_does_not_close_lease() {
    let mut fixture = Fixture::new();
    fixture.import().unwrap();
    let start = fixture.start();
    let report = encoded(&report());
    let receipt = encoded(&fixture.receipt(&start, "report", Some(&report), Some(0)));
    let outcome = store::finish(
        &mut fixture.connection,
        &receipt,
        Some(Ok(report.clone())),
        111,
    )
    .unwrap();
    assert_eq!(outcome["outcome"], "completed_unqualified");
    assert_eq!(outcome["required_audit_satisfied"], false);
    assert_eq!(outcome["lease_held"], true);
    assert_eq!(
        store::finish(&mut fixture.connection, &receipt, Some(Ok(report)), 112).unwrap(),
        outcome
    );
    fixture.close(&start, 113).unwrap();
    let status = fixture.status();
    assert_eq!(status["accepted_full_audits"], 0);
    assert_eq!(status["unresolved_jobs"], 1);
    assert_eq!(status["jobs"][0]["state"], "awaiting_admission");
    assert_eq!(status["jobs"][0]["required_audit_satisfied"], false);
}

#[test]
fn missing_truncated_contradictory_and_failed_producer_reports_cannot_succeed() {
    let mut cases = vec![
        (None, Some(0)),
        (Some(b"{".to_vec()), Some(0)),
        (Some(encoded(&report())), Some(77)),
    ];
    for field in ["caps_applied", "findings", "git"] {
        let mut report = report();
        match field {
            "caps_applied" => report[field] = json!(["cap"]),
            "findings" => report[field] = json!([{"severity":"low","hardness":"hard"}]),
            _ => report[field]["head"] = json!("bbbbbbbb"),
        }
        cases.push((Some(encoded(&report)), Some(0)));
    }
    for (report, exit) in cases {
        let mut fixture = Fixture::new();
        fixture.import().unwrap();
        let start = fixture.start();
        let receipt = encoded(&fixture.receipt(&start, "report", report.as_deref(), exit));
        let outcome =
            store::finish(&mut fixture.connection, &receipt, report.map(Ok), 111).unwrap();
        assert_eq!(outcome["outcome"], "tool_error");
        assert_eq!(fixture.status()["jobs"][0]["pending_retry"], true);
    }
}

#[test]
fn failure_categories_and_valid_failed_policy_are_preserved() {
    for outcome in ["tool_error", "canceled", "timed_out", "source_unavailable"] {
        let mut fixture = Fixture::new();
        fixture.import().unwrap();
        let start = fixture.start();
        let receipt = encoded(&fixture.receipt(&start, outcome, None, None));
        let result = store::finish(&mut fixture.connection, &receipt, None, 111).unwrap();
        assert_eq!(result["outcome"], outcome);
        fixture.close(&start, 112).unwrap();
        assert_eq!(fixture.status()["jobs"][0]["state"], "pending_retry");
    }
    let mut fixture = Fixture::new();
    fixture.import().unwrap();
    let start = fixture.start();
    let mut report = report();
    report["score"] = json!(64);
    report["decision"]["passed"] = json!(false);
    report["decision"]["status"] = json!("fail");
    let report = encoded(&report);
    let receipt = encoded(&fixture.receipt(&start, "report", Some(&report), Some(1)));
    assert_eq!(
        store::finish(&mut fixture.connection, &receipt, Some(Ok(report)), 111).unwrap()["outcome"],
        "failed_policy"
    );
}

#[test]
fn wrong_receipt_binding_and_conflicting_finish_cannot_replace_original() {
    for pointer in [
        "/source_commit",
        "/source_tree",
        "/identity/repository",
        "/identity/governing_policy_sha256",
        "/identity/auditor_receipt_sha256",
        "/identity/execution_config_sha256",
    ] {
        let mut fixture = Fixture::new();
        fixture.import().unwrap();
        let start = fixture.start();
        let mut receipt = fixture.receipt(&start, "canceled", None, None);
        *receipt.pointer_mut(pointer).unwrap() = json!("different");
        assert!(store::finish(&mut fixture.connection, &encoded(&receipt), None, 111).is_err());
        assert!(
            fixture.status()["jobs"][0]["attempts"][0]["observations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    let mut fixture = Fixture::new();
    fixture.import().unwrap();
    let start = fixture.start();
    let receipt = encoded(&fixture.receipt(&start, "canceled", None, None));
    store::finish(&mut fixture.connection, &receipt, None, 111).unwrap();
    let original = fixture.status();
    let other = encoded(&fixture.receipt(&start, "tool_error", None, None));
    assert!(store::finish(&mut fixture.connection, &other, None, 112).is_err());
    assert_eq!(fixture.status(), original);
}

#[test]
fn late_report_does_not_erase_timeout_regardless_of_reconcile_order() {
    for reconcile_first in [true, false] {
        let mut fixture = Fixture::new();
        fixture.import().unwrap();
        let start = fixture.start();
        if reconcile_first {
            store::reconcile(&mut fixture.connection, 120).unwrap();
        }
        let report = encoded(&report());
        let receipt = encoded(&fixture.receipt(&start, "report", Some(&report), Some(0)));
        store::finish(&mut fixture.connection, &receipt, Some(Ok(report)), 121).unwrap();
        assert_eq!(
            store::reconcile(&mut fixture.connection, 122).unwrap()["timeouts_recorded"],
            0
        );
        fixture.close(&start, 123).unwrap();
        let status = fixture.status();
        let observations = &status["jobs"][0]["attempts"][0]["observations"];
        assert_eq!(observations.as_array().unwrap().len(), 2);
        assert_eq!(observations[0]["outcome"], "timed_out");
        assert_eq!(observations[1]["outcome"], "completed_unqualified");
        assert_eq!(status["jobs"][0]["pending_retry"], true);
    }
}

#[test]
fn policy_floor_owner_version_and_soft_limits_are_bound() {
    let fixture = Fixture::new();
    let identity: scheduler::ExecutionIdentity =
        serde_json::from_value(fixture.plan["identity"].clone()).unwrap();
    assert!(validation::context(&identity, &fixture.config, &fixture.policy, b"wrong").is_err());
    for (field, value) in [
        ("minimum", json!(84)),
        ("max_soft", json!(1)),
        ("auditor_version", json!("9.0.0")),
    ] {
        let mut config: Value = serde_json::from_slice(&fixture.config).unwrap();
        config[field] = value;
        let bytes = encoded(&config);
        let mut identity = identity.clone();
        identity.execution_config_sha256 = audit_evidence::hash(&bytes);
        assert!(validation::context(&identity, &bytes, &fixture.policy, &fixture.policy).is_err());
    }
    for owner in ["jeryu-cache", "jeryu-jira", "jeryu-ci-runner"] {
        let policy = format!("workspace='{owner}'\nminimum_score=85\nhard_findings_allowed=0\n")
            .into_bytes();
        let mut identity = identity.clone();
        identity.repository = format!("neverhuman/{owner}");
        identity.governing_policy_sha256 = audit_evidence::hash(&policy);
        let mut config: Value = serde_json::from_slice(&fixture.config).unwrap();
        config["candidate_policy_sha256"] = json!(audit_evidence::hash(&policy));
        for minimum in [85, 91] {
            config["minimum"] = json!(minimum);
            let bytes = encoded(&config);
            identity.execution_config_sha256 = audit_evidence::hash(&bytes);
            assert_eq!(
                validation::context(&identity, &bytes, &policy, &policy).is_ok(),
                minimum == 91
            );
        }
    }
}
