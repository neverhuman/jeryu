use super::*;
use clap::Parser;
use serde_json::json;

const POLICY: &str = "minimum_score = 85\n";
const GOOD: &str = r#"{"score":85,"caps_applied":[],"findings":[],"decision":{"hard_findings":0,"passed":true,"status":"advisory"}}"#;
const OWNERS: [(Owner, &str, u8); 8] = [
    (Owner::Jeryu, "jeryu", 85),
    (Owner::JeryuCore, "jeryu-core", 85),
    (Owner::JeryuDeploy, "jeryu-deploy", 85),
    (Owner::JeryuJira, "jeryu-jira", 85),
    (Owner::JeryuIntelligence, "jeryu-intelligence", 85),
    (Owner::JeryuReleaseOps, "jeryu-release-ops", 85),
    (Owner::JeryuTool, "jeryu-tool", 85),
    (Owner::JeryuWeb, "jeryu-web", 85),
];

fn rejects(report: &str) {
    assert!(
        check(Owner::Jeryu, POLICY, report).is_err(),
        "accepted {report}"
    );
}

#[test]
fn preserves_every_owner_floor_and_stricter_policy() {
    for (owner, workspace, minimum) in OWNERS {
        for floor in [minimum, 90, 100] {
            let policy = format!("workspace = {workspace:?}\nminimum_score = {floor}\n");
            for (score, accepted) in [(floor - 1, false), (floor, true), (100, true)] {
                let report = GOOD.replace("\"score\":85", &format!("\"score\":{score}"));
                assert_eq!(check(owner, &policy, &report).is_ok(), accepted);
            }
        }
        let policy = format!("minimum_score = {}", minimum - 1);
        assert!(check(owner, &policy, &GOOD.replace(":85", ":100")).is_err());
    }
}

#[test]
fn policy_requires_integer_valid_toml_and_matching_identity() {
    for policy in [
        "",
        "minimum_score = [",
        "minimum_score = true",
        "minimum_score = \"85\"",
        "minimum_score = 85.0",
        "minimum_score = -1",
        "minimum_score = 101",
        "minimum_score = 18446744073709551616",
        "minimum_score = 85\nminimum_score = 90",
        "[nested]\nminimum_score = 85",
        "minimum_score = 85\nworkspace = false",
    ] {
        assert!(
            check(Owner::Jeryu, policy, GOOD).is_err(),
            "accepted {policy}"
        );
    }
    for (owner, _, _) in OWNERS {
        let wrong = "workspace = \"not-the-owner\"\nminimum_score = 100";
        assert!(check(owner, wrong, &GOOD.replace(":85", ":100")).is_err());
    }
    let root_policy = "workspace = \"jeryu\"\nminimum_score = 85";
    assert!(check(Owner::JeryuTool, root_policy, GOOD).is_err());
    assert!(check(Owner::JeryuIntelligence, root_policy, GOOD).is_err());
    assert!(check(Owner::Jeryu, root_policy, GOOD).is_ok());
}

#[test]
fn report_requires_one_object_and_all_required_fields() {
    for report in ["", "{", "[]", "null", "true", "{}"] {
        rejects(report);
    }
    rejects("[85,[],[],[],{},0]");
    rejects(&format!("{GOOD}\n{GOOD}"));
    rejects(&format!("{GOOD} trailing"));
    for field in ["score", "caps_applied", "findings", "decision"] {
        let mut report: Value = serde_json::from_str(GOOD).unwrap();
        report.as_object_mut().unwrap().remove(field);
        rejects(&report.to_string());
    }
    assert!(check(Owner::Jeryu, POLICY, &format!(" \n{GOOD}\n ")).is_ok());
}

#[test]
fn scores_require_json_integers_in_range() {
    for value in [
        "true", "\"85\"", "85.0", "85e0", "-1", "101", "256", "null", "[]", "{}", "1e1000",
    ] {
        rejects(&GOOD.replace("\"score\":85", &format!("\"score\":{value}")));
    }
}

#[test]
fn both_cap_fields_must_be_empty_arrays() {
    for field in ["caps_applied", "caps"] {
        for value in [
            json!(["cap"]),
            json!({}),
            json!(null),
            json!(false),
            json!(0),
        ] {
            let mut report: Value = serde_json::from_str(GOOD).unwrap();
            report[field] = value;
            rejects(&report.to_string());
        }
    }
    let report = GOOD.replace("\"findings\":[]", "\"caps\":[],\"findings\":[]");
    assert!(check(Owner::Jeryu, POLICY, &report).is_ok());
}

#[test]
fn actual_findings_override_advisory_pass_and_zero_summaries() {
    for finding in [
        json!({"severity":"critical"}),
        json!({"severity":"high"}),
        json!({"severity":"low","hardness":"hard"}),
    ] {
        let mut report: Value = serde_json::from_str(GOOD).unwrap();
        report["hard_findings"] = json!(0);
        report["findings"] = json!([finding]);
        rejects(&report.to_string());
    }
    let mut report: Value = serde_json::from_str(GOOD).unwrap();
    report["findings"] =
        json!([{"severity":"medium","hardness":"soft"},{"severity":"low"},{"severity":"info"}]);
    report["decision"] = json!({});
    assert!(check(Owner::Jeryu, POLICY, &report.to_string()).is_ok());
}

#[test]
fn malformed_findings_and_explicit_null_hardness_fail() {
    for findings in [
        json!({}),
        json!(null),
        json!([null]),
        json!([["low", "soft"]]),
        json!([{}]),
        json!([{"severity":"unknown"}]),
        json!([{"severity":true}]),
        json!([{"severity":{"low":null}}]),
        json!([{"severity":"low","hardness":{"soft":null}}]),
        json!([{"severity":"low","hardness":null}]),
        json!([{"severity":"low","hardness":false}]),
        json!([{"severity":"low","hardness":"unknown"}]),
    ] {
        let mut report: Value = serde_json::from_str(GOOD).unwrap();
        report["findings"] = findings;
        rejects(&report.to_string());
    }
}

#[test]
fn advisory_counts_cannot_conceal_each_other_or_use_null_defaults() {
    for value in [
        "1",
        "-1",
        "true",
        "false",
        "null",
        "\"0\"",
        "[]",
        "{}",
        "0.0",
        "18446744073709551616",
    ] {
        rejects(&GOOD.replace("\"hard_findings\":0", &format!("\"hard_findings\":{value}")));
        rejects(&GOOD.replacen('{', &format!("{{\"hard_findings\":{value},"), 1));
    }
    for decision in [json!(null), json!([]), json!([0]), json!(false)] {
        let mut report: Value = serde_json::from_str(GOOD).unwrap();
        report["decision"] = decision;
        rejects(&report.to_string());
    }
}

#[test]
fn duplicate_gate_fields_fail_instead_of_using_the_last_value() {
    for duplicate in [
        "\"score\":100,",
        "\"caps_applied\":[\"cap\"],",
        "\"findings\":[{\"severity\":\"high\"}],",
        "\"decision\":{\"hard_findings\":1},",
    ] {
        rejects(&GOOD.replacen('{', &format!("{{{duplicate}"), 1));
    }
    for duplicate in [
        "\"caps\":[\"cap\"],\"caps\":[],",
        "\"hard_findings\":1,\"hard_findings\":0,",
    ] {
        rejects(&GOOD.replacen('{', &format!("{{{duplicate}"), 1));
    }
    rejects(&GOOD.replace(
        "\"hard_findings\":0",
        "\"hard_findings\":1,\"hard_findings\":0",
    ));
    for finding in [
        r#"{"severity":"high","severity":"low"}"#,
        r#"{"severity":"low","hardness":"hard","hardness":"soft"}"#,
    ] {
        rejects(&GOOD.replace("\"findings\":[]", &format!("\"findings\":[{finding}]")));
    }
}

#[test]
fn cli_dispatch_reads_explicit_inputs_and_does_not_rewrite_them() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("policy.toml");
    let report = directory.path().join("report.json");
    fs::write(&policy, POLICY).unwrap();
    fs::write(&report, GOOD).unwrap();
    let args = [
        "jeryu-split",
        "audit-score-check",
        "--owner",
        "jeryu",
        "--policy",
        policy.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
    ];
    crate::run(crate::Cli::try_parse_from(args).unwrap()).unwrap();
    assert_eq!(fs::read_to_string(&policy).unwrap(), POLICY);
    assert_eq!(fs::read_to_string(&report).unwrap(), GOOD);
    fs::write(&policy, "minimum_score = 90").unwrap();
    assert!(crate::run(crate::Cli::try_parse_from(args).unwrap()).is_err());
}

#[test]
fn cli_has_no_numeric_floor_override_or_unreviewed_owners() {
    for (_, name, floor) in OWNERS {
        let parsed = crate::Cli::try_parse_from([
            "jeryu-split",
            "audit-score-check",
            "--owner",
            name,
            "--policy",
            "p",
            "--report",
            "r",
        ])
        .unwrap();
        let crate::Command::AuditScoreCheck { owner, .. } = parsed.command else {
            panic!("wrong command dispatch");
        };
        assert_eq!(owner.policy(), (name, floor));
    }
    for owner in [
        "unknown",
        "jeryu-cache",
        "jeryu-ci-runner",
        "jeryu-tool-finder",
    ] {
        assert!(
            crate::Cli::try_parse_from([
                "jeryu-split",
                "audit-score-check",
                "--owner",
                owner,
                "--policy",
                "p",
                "--report",
                "r"
            ])
            .is_err()
        );
    }
    assert!(
        crate::Cli::try_parse_from([
            "jeryu-split",
            "audit-score-check",
            "--owner",
            "jeryu",
            "--policy",
            "p",
            "--report",
            "r",
            "--minimum-score",
            "0"
        ])
        .is_err()
    );
}

// The 32 cases per owner retain the earlier policy suite's inputs/expectations.
// They now exercise the common Rust gate, not the unported Python bodies.
#[test]
fn legacy_policy_case_matrix() {
    let mut cases = vec![
        ("minimum_score = 85\n".to_owned(), json!({"score":85}), true),
        (
            "minimum_score = 85\n".to_owned(),
            json!({"score":84}),
            false,
        ),
        (
            "minimum_score = 90\n".to_owned(),
            json!({"score":89}),
            false,
        ),
        ("minimum_score = 90\n".to_owned(), json!({"score":90}), true),
        (
            "minimum_score = 100\n".to_owned(),
            json!({"score":100}),
            true,
        ),
        (
            "minimum_score = 85\n".to_owned(),
            json!({"score":100,"caps_applied":["cap"]}),
            false,
        ),
        (
            "minimum_score = 85\n".to_owned(),
            json!({"score":100,"caps":["cap"]}),
            false,
        ),
        (
            "minimum_score = 85\n".to_owned(),
            json!({"score":100,"hard_findings":1}),
            false,
        ),
        (
            "minimum_score = 85\n".to_owned(),
            json!({"score":100,"decision":{"hard_findings":1}}),
            false,
        ),
    ];
    for report in [
        json!({"score":100,"decision":{"hard_findings":0},"hard_findings":1}),
        json!({"score":true}),
        json!({"score":"100"}),
        json!({"score":100.0}),
        json!({"score":101}),
        json!({"score":100,"caps_applied":{}}),
        json!({"score":100,"caps_applied":[],"caps":["concealed-cap"]}),
        json!({"score":100,"findings":{}}),
        json!({"score":100,"findings":[{"severity":"high"}]}),
        json!({"score":100,"findings":[{"severity":"critical"}]}),
        json!({"score":100,"findings":[{"severity":"low","hardness":"hard"}]}),
        json!({"score":100,"findings":[{"severity":"unknown"}]}),
        json!({"score":100,"decision":{"hard_findings":-1}}),
        json!({"score":100,"decision":{"hard_findings":true}}),
    ] {
        cases.push((POLICY.to_owned(), report, false));
    }
    for policy in [
        "",
        "minimum_score = [",
        "minimum_score = true",
        "minimum_score = \"85\"",
        "minimum_score = 85.0",
        "minimum_score = 101",
    ] {
        cases.push((policy.to_owned(), json!({"score":100}), false));
    }
    assert_eq!(cases.len(), 29);
    let mut exercised = 0;
    for (owner, name, minimum) in OWNERS {
        let owner_cases = [
            (
                format!("minimum_score = {}", minimum - 1),
                json!({"score":100}),
                false,
            ),
            (
                format!("minimum_score = {minimum}"),
                json!({"score":minimum}),
                true,
            ),
            (
                format!("minimum_score = {minimum}"),
                json!({"score":minimum - 1}),
                false,
            ),
        ];
        for (policy, fields, accepted) in cases.iter().chain(owner_cases.iter()) {
            let mut report = json!({"caps_applied":[],"findings":[],"decision":{}});
            report
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            let report = report.to_string();
            assert_eq!(
                check(owner, policy, &report).is_ok(),
                *accepted,
                "owner={name} policy={policy:?} report={report}"
            );
            exercised += 1;
        }
    }
    assert_eq!(exercised, 256);
    println!("\n256 score policy/report cases passed");
}
