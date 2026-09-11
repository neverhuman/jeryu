use super::*;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = crate::split_tree::source_git_command(repo)
        .env("GIT_AUTHOR_NAME", "Synthetic audit fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Synthetic audit fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Git fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

struct QueuedFixture {
    intake: Fixture,
    repo: PathBuf,
    commits: [String; 3],
    identity: PathBuf,
    config: PathBuf,
    policy: PathBuf,
}

impl QueuedFixture {
    fn new() -> Self {
        let mut intake = Fixture::new();
        let repo = intake.root.join("source");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "--quiet", "-b", "main"]);
        let tree = git(&repo, &["mktree"]);
        let first = git(&repo, &["commit-tree", &tree, "-m", "first"]);
        let second = git(&repo, &["commit-tree", &tree, "-p", &first, "-m", "second"]);
        let third = git(&repo, &["commit-tree", &tree, "-p", &second, "-m", "third"]);
        git(&repo, &["update-ref", "refs/heads/main", &third]);
        let policy = intake.root.join("policy.toml");
        let policy_bytes = b"workspace='jeryu'\nminimum_score=85\nhard_findings_allowed=0\nrequired_tool='jankurai'\nrequired_tool_version='1.6.11'\n";
        write(&policy, policy_bytes);
        let mut route: Value = serde_json::from_slice(&intake.route).unwrap();
        let mut executor_inputs = serde_json::Map::new();
        for name in [
            "executor_source_commit",
            "executor_executable_sha256",
            "executor_receipt_sha256",
            "governing_workflow_repository",
            "governing_workflow_path",
            "governing_workflow_commit",
            "governing_workflow_blob",
        ] {
            executor_inputs.insert(name.into(), route[name].clone());
        }
        let config_bytes = serde_json::to_vec(&json!({
            "schema_version":"jeryu.audit-ledger-execution/v1", "auditor_version":"1.6.11",
            "policy_path":"./agent/audit-policy.toml","minimum":85,"max_soft":0,
            "candidate_policy_sha256":audit_evidence::hash(policy_bytes),"executor_inputs":executor_inputs,
        })).unwrap();
        let config = intake.root.join("execution.json");
        write(&config, &config_bytes);
        route["execution_config_sha256"] = json!(audit_evidence::hash(&config_bytes));
        route["governing_policy_sha256"] = json!(audit_evidence::hash(policy_bytes));
        intake.route = serde_json::to_vec(&route).unwrap();
        let identity = intake.root.join("identity.json");
        write(&identity, &serde_json::to_vec(&json!({"repository":"neverhuman/jeryu","scope":"repository",
            "auditor_source_commit":"a".repeat(40),"auditor_executable_sha256":"b".repeat(64),
            "auditor_receipt_sha256":"c".repeat(64),"governing_policy_sha256":route["governing_policy_sha256"],
            "execution_config_sha256":route["execution_config_sha256"]})).unwrap());
        Self {
            intake,
            repo,
            commits: [first, second, third],
            identity,
            config,
            policy,
        }
    }

    fn enqueue(&mut self, received: &Value) -> Result<Value> {
        queue::run(
            &mut self.intake.connection,
            received["event_key"].as_str().unwrap(),
            received["reception_id"].as_i64().unwrap(),
            &self.repo,
            queue::Inputs::read(&self.identity, &self.config, &self.policy, &self.policy),
            2000,
        )
    }

    fn plans(&self) -> Vec<Value> {
        let mut statement = self
            .intake
            .connection
            .prepare("SELECT bytes FROM plans ORDER BY imported_at,rowid")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .unwrap()
            .map(|bytes| serde_json::from_slice(&bytes.unwrap()).unwrap())
            .collect()
    }

    fn jobs(&self) -> usize {
        self.intake
            .connection
            .query_row("SELECT count(*) FROM jobs", [], |row| row.get(0))
            .unwrap()
    }

    fn push(&mut self, before: &str, after: &str, guid: &str) -> Value {
        self.intake
            .receive(&push(before, after, "refs/heads/main"), "push", guid)
    }
}

#[test]
fn multi_commit_push_and_retries_bind_one_queue_across_restart() {
    let mut fixture = QueuedFixture::new();
    let [first, second, third] = fixture.commits.clone();
    // The raw payload intentionally contains an empty commits array.
    let received = fixture.push(&first, &third, GUID);
    let result = fixture.enqueue(&received).unwrap();
    assert_eq!(result["queue_imported"], true);
    assert_eq!(result["received_endpoints_enumerated"], true);
    assert_eq!(result["plans"][0]["jobs"], 2);
    let plan = &fixture.plans()[0];
    assert_eq!(plan["jobs"][0]["source_commit"], second);
    assert_eq!(plan["jobs"][1]["source_commit"], third);
    for field in [
        "headers_authenticated",
        "enrollment_complete",
        "execution_verified",
        "governing_policy_authenticated",
        "publication_qualified",
    ] {
        assert_eq!(result[field], false);
    }
    assert_eq!(fixture.enqueue(&received).unwrap(), result);
    let original = fixture.intake.status();
    drop(fixture.intake.connection);
    fixture.intake.connection = audit_ledger::open(&fixture.intake.database, false).unwrap();
    assert_eq!(fixture.intake.status(), original);
    let retry = fixture.push(&first, &third, "11234567-89ab-cdef-0123-456789abcdef");
    fixture.enqueue(&retry).unwrap();
    assert_eq!(fixture.plans().len(), 1);
    assert_eq!(fixture.jobs(), 2);
    let state = fixture.intake.status();
    assert_eq!(state["receptions"].as_array().unwrap().len(), 2);
    assert_eq!(
        state["events"][0]["queued_plans"][0]["reception_id"],
        received["reception_id"]
    );
    assert_eq!(state["events"][0]["queue_imported"], true);
    assert_eq!(state["events"][0]["planning_complete"], false);
    assert!(!state.to_string().contains("private-payload-sentinel"));
    assert!(
        !state
            .to_string()
            .contains(std::str::from_utf8(SECRET).unwrap())
    );
}

#[test]
fn creation_rewrite_and_deletion_preserve_prior_jobs_and_withdrawals() {
    let mut fixture = QueuedFixture::new();
    let [first, second, third] = fixture.commits.clone();
    let created = fixture.push(&"0".repeat(40), &third, GUID);
    fixture.enqueue(&created).unwrap();
    assert_eq!(fixture.jobs(), 3);
    let tree = git(&fixture.repo, &["rev-parse", "HEAD^{tree}"]);
    let fork = git(
        &fixture.repo,
        &["commit-tree", &tree, "-p", &first, "-m", "rewrite"],
    );
    let rewritten = fixture.push(&third, &fork, "11234567-89ab-cdef-0123-456789abcdef");
    fixture.enqueue(&rewritten).unwrap();
    let plans = fixture.plans();
    assert_eq!(plans[1]["disposition"], "rewritten");
    assert_eq!(plans[1]["withdrawn_commits"], json!([second, third]));
    assert_eq!(fixture.jobs(), 4);
    let deleted = fixture.push(
        &fork,
        &"0".repeat(40),
        "21234567-89ab-cdef-0123-456789abcdef",
    );
    fixture.enqueue(&deleted).unwrap();
    assert_eq!(fixture.plans()[2]["disposition"], "deleted");
    assert_eq!(fixture.plans()[2]["jobs"], json!([]));
    assert_eq!(fixture.jobs(), 4);
    assert!(
        fixture
            .intake
            .connection
            .execute("DELETE FROM intake_plan_links", [])
            .is_err()
    );
    assert!(
        fixture
            .intake
            .connection
            .execute("UPDATE intake_plan_links SET plan_id='changed'", [])
            .is_err()
    );
}

#[test]
fn missing_source_and_policy_mismatch_remain_pending_with_private_failures() {
    let mut fixture = QueuedFixture::new();
    let first = fixture.commits[0].clone();
    let received = fixture.push(&first, &"f".repeat(40), GUID);
    assert!(fixture.enqueue(&received).is_err());
    assert_eq!(fixture.jobs(), 0);
    assert!(fixture.plans().is_empty());
    assert_eq!(
        fixture.intake.status()["events"][0]["queue_imported"],
        false
    );
    assert_eq!(
        fixture.intake.status()["events"][0]["worker_failures"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let third = fixture.commits[2].clone();
    let valid = fixture.push(&first, &third, "11234567-89ab-cdef-0123-456789abcdef");
    let original = fs::read(&fixture.identity).unwrap();
    let mut identity: Value = serde_json::from_slice(&original).unwrap();
    identity["governing_policy_sha256"] = json!("4".repeat(64));
    fs::write(&fixture.identity, serde_json::to_vec(&identity).unwrap()).unwrap();
    assert!(fixture.enqueue(&valid).is_err());
    assert_eq!(fixture.jobs(), 0);
    fs::write(&fixture.identity, original).unwrap();
    fixture.enqueue(&valid).unwrap();
    assert_eq!(fixture.jobs(), 2);
    assert_eq!(
        fixture.intake.status()["events"][1]["worker_failures"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn queue_write_failure_rolls_back_all_plans_jobs_and_links_then_retries() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    let received = fixture.push(&first, &third, GUID);
    fixture.intake.connection.execute_batch("CREATE TRIGGER fixture_queue_failure BEFORE INSERT ON intake_plan_links BEGIN SELECT RAISE(ABORT,'private-queue-write-diagnostic'); END;").unwrap();
    assert!(fixture.enqueue(&received).is_err());
    assert_eq!(fixture.jobs(), 0);
    assert!(fixture.plans().is_empty());
    let status = fixture.intake.status();
    assert_eq!(status["events"][0]["queued_plans"], json!([]));
    assert!(
        !status
            .to_string()
            .contains("private-queue-write-diagnostic")
    );
    drop(fixture.intake.connection);
    fixture.intake.connection = audit_ledger::open(&fixture.intake.database, false).unwrap();
    fixture
        .intake
        .connection
        .execute_batch("DROP TRIGGER fixture_queue_failure")
        .unwrap();
    fixture.enqueue(&received).unwrap();
    assert_eq!(fixture.jobs(), 2);
    assert_eq!(
        fixture.intake.status()["events"][0]["worker_failures"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

fn merged_pull(head: &str, merged: &str) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"repository":{"id":123,"full_name":"neverhuman/jeryu"},
        "action":"closed","number":7,"pull_request":{"number":7,
        "base":{"repo":{"id":123,"full_name":"neverhuman/jeryu"}},
        "head":{"sha":head,"repo":{"id":999,"full_name":"contributor/jeryu"}},
        "merged":true,"merge_commit_sha":merged}}),
    )
    .unwrap()
}

#[test]
fn merged_fork_keeps_both_parent_histories_without_granting_execution() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    let tree = git(&fixture.repo, &["rev-parse", "HEAD^{tree}"]);
    let head = git(
        &fixture.repo,
        &["commit-tree", &tree, "-p", &first, "-m", "fork"],
    );
    let merge = git(
        &fixture.repo,
        &[
            "commit-tree",
            &tree,
            "-p",
            &third,
            "-p",
            &head,
            "-m",
            "merge",
        ],
    );
    let received = fixture
        .intake
        .receive(&merged_pull(&head, &merge), "pull_request", GUID);
    assert_eq!(
        received["translation"]["facts"]["fork_execution_admission"],
        false
    );
    let result = fixture.enqueue(&received).unwrap();
    assert_eq!(result["plans"].as_array().unwrap().len(), 2);
    let plans = fixture.plans();
    assert_eq!(plans[0]["source_ref"], "refs/pull/7/head");
    assert_eq!(plans[0]["jobs"].as_array().unwrap().len(), 2);
    assert_eq!(plans[1]["source_ref"], "refs/pull/7/merge");
    assert_eq!(plans[1]["jobs"].as_array().unwrap().len(), 5);
    assert_eq!(fixture.jobs(), 5);
    assert_eq!(result["execution_verified"], false);
    fixture.enqueue(&received).unwrap();
    assert_eq!(fixture.jobs(), 5);
}

#[test]
fn missing_merge_or_second_link_failure_cannot_partially_queue_pr_head() {
    for missing in [true, false] {
        let mut fixture = QueuedFixture::new();
        let [first, _, third] = fixture.commits.clone();
        let merged = if missing { "f".repeat(40) } else { third };
        let received = fixture
            .intake
            .receive(&merged_pull(&first, &merged), "pull_request", GUID);
        if !missing {
            fixture.intake.connection.execute_batch("CREATE TRIGGER fixture_second_link_failure BEFORE INSERT ON intake_plan_links WHEN NEW.source_ref='refs/pull/7/merge' BEGIN SELECT RAISE(ABORT,'second-link-failure'); END;").unwrap();
        }
        assert!(fixture.enqueue(&received).is_err());
        assert_eq!(fixture.jobs(), 0);
        assert!(fixture.plans().is_empty());
        assert_eq!(
            fixture.intake.status()["events"][0]["queue_imported"],
            false
        );
    }
}

#[test]
fn conflicting_or_mismatched_receptions_and_unresolved_events_cannot_queue() {
    let mut fixture = QueuedFixture::new();
    let [first, second, third] = fixture.commits.clone();
    let accepted = fixture.push(&first, &second, GUID);
    let conflicting = fixture.push(&second, &third, GUID);
    assert!(fixture.enqueue(&conflicting).is_err());
    let mut mismatched = conflicting.clone();
    mismatched["reception_id"] = accepted["reception_id"].clone();
    assert!(fixture.enqueue(&mismatched).is_err());
    let release = serde_json::to_vec(
        &json!({"repository":{"id":123,"full_name":"neverhuman/jeryu"},
        "action":"published","release":{"id":1,"tag_name":"v1","target_commitish":"main"}}),
    )
    .unwrap();
    let unresolved =
        fixture
            .intake
            .receive(&release, "release", "11234567-89ab-cdef-0123-456789abcdef");
    assert!(fixture.enqueue(&unresolved).is_err());
    assert_eq!(fixture.jobs(), 0);
    fixture.enqueue(&accepted).unwrap();
    assert_eq!(fixture.jobs(), 1);
}

#[test]
fn changed_auditor_identity_creates_distinct_jobs_and_cannot_replace_original_links() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    let received = fixture.push(&first, &third, GUID);
    let original = fixture.enqueue(&received).unwrap();
    let mut identity: Value =
        serde_json::from_slice(&fs::read(&fixture.identity).unwrap()).unwrap();
    identity["auditor_executable_sha256"] = json!("d".repeat(64));
    fs::write(&fixture.identity, serde_json::to_vec(&identity).unwrap()).unwrap();
    let changed = fixture.enqueue(&received).unwrap();
    assert_ne!(changed["identity_sha256"], original["identity_sha256"]);
    assert_eq!(fixture.jobs(), 4);
    assert_eq!(fixture.plans().len(), 2);
    assert_eq!(
        fixture.intake.status()["events"][0]["queued_plans"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn v2_read_only_receiving_preserves_bytes_and_upgrade_keeps_raw_history() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    let received = fixture.push(&first, &third, GUID);
    // Reconstruct the previous schema using a synthetic, empty v3 link table.
    fixture
        .intake
        .connection
        .execute_batch("DROP TABLE intake_plan_links; PRAGMA user_version=2;")
        .unwrap();
    let before = fixture.intake.status();
    assert_eq!(before["migration_required"], true);
    drop(fixture.intake.connection);
    let original_bytes = fs::read(&fixture.intake.database).unwrap();
    fixture.intake.connection = audit_ledger::open(&fixture.intake.database, true).unwrap();
    assert_eq!(fixture.intake.status(), before);
    assert_eq!(fs::read(&fixture.intake.database).unwrap(), original_bytes);
    drop(fixture.intake.connection);
    fixture.intake.connection = audit_ledger::open(&fixture.intake.database, false).unwrap();
    let after = fixture.intake.status();
    assert_eq!(after["migration_required"], false);
    assert_eq!(after["receptions"], before["receptions"]);
    assert_eq!(after["events"], before["events"]);
    assert_eq!(after["deliveries"], before["deliveries"]);
    fixture.enqueue(&received).unwrap();
    assert_eq!(fixture.jobs(), 2);
}

#[test]
fn queue_cli_requires_exact_reception_and_closed_input_files() {
    use clap::Parser;
    let args = [
        "jeryu-split",
        "audit-intake",
        "--database",
        "/private/ledger.sqlite",
        "plan",
        "--event-key",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--reception-id",
        "1",
        "--source-repo",
        "/private/source",
        "--identity",
        "/private/identity.json",
        "--execution-config",
        "/private/execution.json",
        "--governing-policy",
        "/private/policy.toml",
        "--candidate-policy",
        "/private/candidate.toml",
    ];
    assert!(crate::Cli::try_parse_from(args).is_ok());
    let mut unauthorized = args.to_vec();
    unauthorized.extend(["--trusted", "true"]);
    assert!(crate::Cli::try_parse_from(unauthorized).is_err());
    for flag in [
        "--event-key",
        "--reception-id",
        "--source-repo",
        "--identity",
        "--execution-config",
        "--governing-policy",
        "--candidate-policy",
    ] {
        let mut missing = args.to_vec();
        let position = missing.iter().position(|arg| *arg == flag).unwrap();
        missing.drain(position..position + 2);
        assert!(crate::Cli::try_parse_from(missing).is_err());
    }
}
