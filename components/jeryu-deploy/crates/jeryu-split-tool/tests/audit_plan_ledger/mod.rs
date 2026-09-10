use super::*;

#[test]
fn pure_rewind_plan_roundtrips_through_durable_import_without_new_jobs() {
    use sha2::{Digest, Sha256};
    let mut f = Fixture::new();
    let ancestor = f.tip.clone();
    f.append(2);
    let before = f.tip.clone();
    let policy = b"workspace='jeryu'\nminimum_score=85\nhard_findings_allowed=0\n";
    let config = serde_json::to_vec(&json!({"schema_version":"jeryu.audit-ledger-execution/v1",
        "auditor_version":"1.6.11","policy_path":"./agent/audit-policy.toml","minimum":85,"max_soft":0,
        "candidate_policy_sha256":format!("{:x}",Sha256::digest(policy)),
        "executor_inputs":{"fixture":"unqualified planner/import roundtrip"}})).unwrap();
    let mut request = f.request(Some(&before), Some(&ancestor));
    request["identity"]["governing_policy_sha256"] = json!(format!("{:x}", Sha256::digest(policy)));
    request["identity"]["execution_config_sha256"] =
        json!(format!("{:x}", Sha256::digest(&config)));
    let plan = f.plan(&request);
    assert_eq!(plan["disposition"], "rewritten");
    assert!(plan["jobs"].as_array().unwrap().is_empty());
    assert_eq!(plan["withdrawn_commits"].as_array().unwrap().len(), 2);
    fs::write(
        f.temporary.join("plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    fs::write(f.temporary.join("config.json"), config).unwrap();
    fs::write(f.temporary.join("policy.toml"), policy).unwrap();
    let database = f.temporary.join("ledger.sqlite");
    let import = || {
        text(
            command(&f.temporary, env!("CARGO_BIN_EXE_jeryu-split"))
                .args([
                    "audit-ledger",
                    "--database",
                    database.to_str().unwrap(),
                    "import-plan",
                    "--plan",
                    "plan.json",
                    "--execution-config",
                    "config.json",
                    "--governing-policy",
                    "policy.toml",
                    "--candidate-policy",
                    "policy.toml",
                ])
                .output()
                .unwrap(),
        )
    };
    assert_eq!(import(), import());
    let output = command(&f.temporary, env!("CARGO_BIN_EXE_jeryu-split"))
        .args([
            "audit-ledger",
            "--database",
            database.to_str().unwrap(),
            "status",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(status["jobs"].as_array().unwrap().is_empty());
    assert_eq!(status["plans"].as_array().unwrap().len(), 1);
    assert_eq!(
        status["plans"][0]["plan"]["withdrawn_commits"],
        plan["withdrawn_commits"]
    );
    assert_eq!(f.git(&["rev-parse", "refs/heads/main"]), before);
}
