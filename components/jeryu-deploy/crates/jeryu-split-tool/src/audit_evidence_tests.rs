use super::*;
use serde_json::json;

fn report() -> Value {
    json!({
        "standard":"jankurai", "schema_version":"1.9.0", "auditor_version":"1.6.11", "repo":".",
        "score":95, "dirty_worktree":false, "scope":{"mode":"full","paths":[]},
        "git":{"head":"aaaaaaaa", "mode":"full", "dirty_worktree":false},
        "policy":{"minimum_score":85,"mode":"standard","path":"/source/agent/audit-policy.toml","fail_on":["critical","high"]},
        "decision":{"status":"pass","passed":true,"minimum_score":85,"hard_findings":0,"soft_findings":0,"ratchet":{"passed":true}},
        "conformance_decision":"pass","conformance_blockers":[], "findings":[], "caps_applied":[]
    })
}

fn binding() -> Binding<'static> {
    Binding {
        commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        version: "1.6.11",
        policy_path: "/source/agent/audit-policy.toml",
        minimum: 85,
        max_soft: Some(0),
    }
}

fn evaluate(report: &Value, exit: i32) -> Result<Summary> {
    admit(&serde_json::to_vec(report)?, binding(), exit)
}

#[test]
fn complete_pass_and_real_failure_are_preserved() {
    assert!(evaluate(&report(), 0).unwrap().passed);
    let mut failed = report();
    failed["score"] = json!(64);
    failed["decision"]["status"] = json!("fail");
    failed["decision"]["passed"] = json!(false);
    let summary = evaluate(&failed, 1).unwrap();
    assert_eq!(summary.score, 64);
    assert!(!summary.passed);
}

#[test]
fn high_scores_cannot_hide_hard_findings_caps_or_ratchet_failure() {
    for pointer in [
        "/caps_applied",
        "/caps",
        "/decision/ratchet/passed",
        "/findings",
    ] {
        let mut forged = report();
        forged["score"] = json!(100);
        match pointer {
            "/caps_applied" => forged["caps_applied"] = json!(["cap"]),
            "/caps" => forged["caps"] = json!(["cap"]),
            "/decision/ratchet/passed" => forged["decision"]["ratchet"]["passed"] = json!(false),
            _ => {
                forged["findings"] = json!([{"severity":"low","hardness":"hard"}]);
                forged["decision"]["hard_findings"] = json!(1);
            }
        }
        assert!(evaluate(&forged, 0).is_err(), "{pointer}");
    }
    let mut high = report();
    high["findings"] = json!([{"severity":"high","hardness":"soft"}]);
    assert!(evaluate(&high, 0).is_err());
}

#[test]
fn producer_failure_after_success_output_is_fatal() {
    for exit in [1, 2, 42, 77, 124, 137] {
        assert!(evaluate(&report(), exit).is_err());
    }
}

#[test]
fn wrong_identity_partial_or_contradictory_evidence_is_rejected() {
    for (pointer, replacement) in [
        ("/standard", json!("other")),
        ("/schema_version", json!("future")),
        ("/auditor_version", json!("9.0.0")),
        ("/repo", json!("other")),
        ("/git/head", json!("bbbbbbbb")),
        ("/git/head", json!("a")),
        ("/dirty_worktree", json!(true)),
        ("/git/dirty_worktree", json!(true)),
        ("/scope/mode", json!("changed")),
        ("/git/mode", json!("changed")),
        ("/scope/paths", json!(["one.rs"])),
        ("/policy/minimum_score", json!(75)),
        ("/policy/path", json!("different-policy.toml")),
        ("/policy/mode", json!("advisory")),
        ("/decision/minimum_score", json!(65)),
        ("/decision/soft_findings", json!(1)),
        ("/decision/hard_findings", json!(1)),
        ("/decision/status", json!("fail")),
        ("/score", json!(101)),
        ("/score", json!(85.5)),
        ("/score", Value::Null),
    ] {
        let mut wrong = report();
        *wrong.pointer_mut(pointer).unwrap() = replacement;
        assert!(evaluate(&wrong, 0).is_err(), "{pointer}");
    }
    let mut null = report();
    null["hard_findings"] = Value::Null;
    assert!(evaluate(&null, 0).is_err());
}

#[test]
fn missing_truncated_duplicate_and_array_encoded_reports_fail() {
    let original = serde_json::to_string(&report()).unwrap();
    for bytes in [
        "",
        "{",
        "[]",
        "null",
        &format!("{original}{original}"),
        &original[..original.len() - 1],
        &original.replacen("\"score\":95", "\"score\":95,\"score\":95", 1),
        &original.replacen("\"passed\":true", "\"passed\":true,\"passed\":true", 1),
    ] {
        assert!(
            admit(bytes.as_bytes(), binding(), 0).is_err(),
            "invalid JSON must fail"
        );
    }
    for field in [
        "scope",
        "git",
        "decision",
        "findings",
        "caps_applied",
        "conformance_decision",
    ] {
        let mut missing = report();
        missing.as_object_mut().unwrap().remove(field);
        assert!(evaluate(&missing, 0).is_err(), "missing {field}");
    }
}

#[test]
fn stronger_floors_soft_limits_and_conformance_are_retained() {
    for floor in [65, 75, 80, 82, 85, 91, 95] {
        let text = format!("workspace='jeryu'\nminimum_score={floor}\nhard_findings_allowed=0\n");
        assert_eq!(policy(&text, "jeryu", 85).unwrap(), floor.max(85));
        assert_eq!(policy(&text, "jeryu", 91).unwrap(), floor.max(91));
        assert!(policy(&text, "other", 85).is_err());
    }
    let mut soft = report();
    soft["findings"] = json!([{"severity":"low","hardness":"soft"}]);
    soft["decision"]["soft_findings"] = json!(1);
    assert!(!evaluate(&soft, 0).unwrap().passed);
    let mut nonconforming = report();
    nonconforming["conformance_decision"] = json!("fail");
    assert!(!evaluate(&nonconforming, 0).unwrap().passed);
    nonconforming["conformance_decision"] = json!("pass");
    nonconforming["conformance_blockers"] = json!(["missing proof"]);
    assert!(!evaluate(&nonconforming, 0).unwrap().passed);
}
