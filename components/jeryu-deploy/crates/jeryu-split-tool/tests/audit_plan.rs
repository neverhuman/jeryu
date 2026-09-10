use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn command(root: &Path, program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z");
    command
}

fn text(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

struct Fixture {
    temporary: PathBuf,
    root: PathBuf,
    helper: String,
    identity: String,
    tip: String,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::Builder::new()
            .prefix("jeryu-audit-plan-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
            .keep();
        eprintln!(
            "audit-plan fixture retained until guarded success: {}",
            temporary.display()
        );
        let package = Path::new(env!("CARGO_MANIFEST_DIR"));
        let helper = [2, 4]
            .into_iter()
            .find_map(|depth| {
                let candidate = package.ancestors().nth(depth)?.join("tests/scratch.sh");
                candidate.is_file().then_some(candidate)
            })
            .expect("owning scratch helper");
        let helper = fs::read_to_string(helper).unwrap();
        let record = format!(
            "{helper}\njeryu_record_test_scratch \"$1\"\nprintf '%s' \"$jeryu_test_scratch_identity\"\n"
        );
        let identity = text(
            command(&temporary, "/bin/bash")
                .args([
                    "-euo",
                    "pipefail",
                    "-c",
                    &record,
                    "audit-plan-fixture",
                    temporary.to_str().unwrap(),
                ])
                .output()
                .unwrap(),
        );
        let root = temporary.join("repository");
        fs::create_dir(&root).unwrap();
        text(
            command(&root, "/usr/bin/git")
                .args(["init", "--quiet", "--template=", "--initial-branch=main"])
                .output()
                .unwrap(),
        );
        let mut fixture = Self {
            temporary,
            root,
            helper,
            identity,
            tip: String::new(),
        };
        fixture.append(1);
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        text(
            command(&self.root, "/usr/bin/git")
                .args(args)
                .output()
                .unwrap(),
        )
    }

    fn append(&mut self, count: usize) {
        let mut input = String::new();
        for index in 0..count {
            let message = format!("graph fixture {index}");
            input.push_str(&format!("commit refs/heads/main\ncommitter Fixture <fixture@example.invalid> 946684800 +0000\ndata {}\n{message}\n", message.len()));
            if index == 0 && !self.tip.is_empty() {
                input.push_str(&format!("from {}\n", self.tip));
            }
            input.push('\n');
        }
        input.push_str("done\n");
        let mut child = command(&self.root, "/usr/bin/git")
            .args(["fast-import", "--quiet", "--date-format=raw"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        text(child.wait_with_output().unwrap());
        self.tip = self.git(&["rev-parse", "refs/heads/main"]);
    }

    fn request(&self, before: Option<&str>, after: Option<&str>) -> Value {
        json!({
            "schema_version": "jeryu.audit-plan-request/v1",
            "identity": {
                "repository": "neverhuman/jeryu",
                "scope": "repository",
                "auditor_source_commit": "a".repeat(40),
                "auditor_executable_sha256": "b".repeat(64),
                "auditor_receipt_sha256": "c".repeat(64),
                "governing_policy_sha256": "d".repeat(64),
                "execution_config_sha256": "e".repeat(64)
            },
            "event": "push",
            "source_ref": "refs/heads/main",
            "before": before,
            "after": after,
            "run_id": "1001",
            "run_attempt": 1,
            "previous_run_id": null
        })
    }

    fn invoke(&self, request: &Value) -> Output {
        let path = self.temporary.join("request.json");
        fs::write(&path, serde_json::to_vec(request).unwrap()).unwrap();
        command(&self.root, env!("CARGO_BIN_EXE_jeryu-split"))
            .args([
                "audit-plan",
                "--source-repo",
                self.root.to_str().unwrap(),
                "--request",
                path.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    }

    fn plan(&self, request: &Value) -> Value {
        serde_json::from_str(&text(self.invoke(request))).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retaining failed audit-plan fixture: {}",
                self.temporary.display()
            );
            return;
        }
        let cleanup = format!(
            "{}\njeryu_test_scratch=\"$1\"\njeryu_test_scratch_identity=\"$2\"\njeryu_remove_test_scratch\n",
            self.helper
        );
        let output = command(Path::new("/"), "/bin/bash")
            .args([
                "-euo",
                "pipefail",
                "-c",
                &cleanup,
                "audit-plan-fixture",
                self.temporary.to_str().unwrap(),
                &self.identity,
            ])
            .output()
            .expect("fixture cleanup available");
        assert!(
            output.status.success(),
            "cleanup refused; retained {}: {}",
            self.temporary.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn refused(output: Output, reason: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(reason),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn all_intermediate_commits_survive_large_push_and_duplicate_delivery() {
    let mut f = Fixture::new();
    let before = f.tip.clone();
    f.append(2051);
    let request = f.request(Some(&before), Some(&f.tip));
    let refs_before = f.git(&["show-ref"]);
    let plan = f.plan(&request);
    assert_eq!(plan, f.plan(&request));
    let jobs = plan["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 2051);
    let graph = f.git(&["rev-list", "--reverse", &format!("{before}..{}", f.tip)]);
    assert_eq!(
        jobs.iter()
            .map(|job| job["source_commit"].as_str().unwrap())
            .collect::<Vec<_>>(),
        graph.lines().collect::<Vec<_>>()
    );
    assert!(jobs.iter().all(|job| job["status"] == "pending"));
    assert_eq!(plan["disposition"], "advanced");
    assert_eq!(plan["execution_verified"], false);
    assert_eq!(plan["publication_qualified"], false);
    assert_eq!(f.git(&["show-ref"]), refs_before);
    assert!(f.git(&["status", "--porcelain"]).is_empty());
}

#[test]
fn creation_reconciliation_deletion_and_evidence_branch_are_explicit() {
    let mut f = Fixture::new();
    f.append(2);
    let mut request = f.request(Some(&"0".repeat(40)), Some(&f.tip));
    let created = f.plan(&request);
    assert_eq!(created["disposition"], "created");
    assert_eq!(created["jobs"].as_array().unwrap().len(), 3);
    request["event"] = json!("reconcile");
    assert_eq!(f.plan(&request)["disposition"], "reconciled");
    request["event"] = json!("push");
    request["source_ref"] = json!("refs/heads/audit-evidence");
    let excluded = f.plan(&request);
    assert_eq!(excluded["disposition"], "excluded_evidence_branch");
    assert!(excluded["jobs"].as_array().unwrap().is_empty());
    request["source_ref"] = json!("refs/heads/main");
    request["before"] = json!(f.tip);
    request["after"] = Value::Null;
    let deleted = f.plan(&request);
    assert_eq!(deleted["disposition"], "deleted");
    assert!(deleted["jobs"].as_array().unwrap().is_empty());
}

#[test]
fn rewritten_branch_keeps_new_side_and_records_withdrawn_history() {
    let mut f = Fixture::new();
    let common = f.tip.clone();
    f.append(2);
    let before = f.tip.clone();
    let tree = f.git(&["rev-parse", &format!("{common}^{{tree}}")]);
    let after = f.git(&[
        "-c",
        "commit.gpgsign=false",
        "commit-tree",
        &tree,
        "-p",
        &common,
        "-m",
        "independent successor",
    ]);
    let result = f.plan(&f.request(Some(&before), Some(&after)));
    assert_eq!(result["disposition"], "rewritten");
    assert_eq!(result["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(result["jobs"][0]["source_commit"], after);
    assert_eq!(result["withdrawn_commits"].as_array().unwrap().len(), 2);
    assert_eq!(f.git(&["rev-parse", "refs/heads/main"]), before);
}

#[test]
fn pull_request_and_annotated_release_select_exact_revision() {
    let mut f = Fixture::new();
    f.append(2);
    let mut request = f.request(None, Some(&f.tip));
    request["event"] = json!("pull_request");
    request["source_ref"] = json!("refs/pull/12/head");
    let pr = f.plan(&request);
    assert_eq!(pr["disposition"], "exact_revision");
    assert_eq!(pr["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(pr["jobs"][0]["source_commit"], f.tip);
    f.git(&[
        "-c",
        "tag.gpgsign=false",
        "tag",
        "-a",
        "v-test",
        "-m",
        "fixture",
        &f.tip,
    ]);
    let tag = f.git(&["rev-parse", "refs/tags/v-test"]);
    request["event"] = json!("release");
    request["source_ref"] = json!("refs/tags/v-test");
    request["after"] = json!(tag);
    let release = f.plan(&request);
    assert_eq!(release["after"], tag);
    assert_eq!(release["resolved_after_commit"], f.tip);
    assert_eq!(release["jobs"], pr["jobs"]);
}

#[test]
fn retries_preserve_source_keys_but_change_attempt_keys_and_identity_changes_reschedule() {
    let f = Fixture::new();
    let request = f.request(None, Some(&f.tip));
    let original = f.plan(&request);
    let mut retry = request.clone();
    retry["run_attempt"] = json!(2);
    retry["previous_run_id"] = json!("1001");
    let retried = f.plan(&retry);
    assert_eq!(
        retried["jobs"][0]["deduplication_key"],
        original["jobs"][0]["deduplication_key"]
    );
    assert_ne!(
        retried["jobs"][0]["attempt_key"],
        original["jobs"][0]["attempt_key"]
    );
    assert_eq!(retried["previous_run_id"], "1001");
    for (field, value) in [
        ("repository", "neverhuman/jeryu-core".to_owned()),
        ("scope", "components/jeryu-core".to_owned()),
        ("auditor_source_commit", "f".repeat(40)),
        ("auditor_executable_sha256", "f".repeat(64)),
        ("auditor_receipt_sha256", "f".repeat(64)),
        ("governing_policy_sha256", "f".repeat(64)),
        ("execution_config_sha256", "f".repeat(64)),
    ] {
        let mut changed = request.clone();
        changed["identity"][field] = json!(value);
        assert_ne!(
            f.plan(&changed)["jobs"][0]["deduplication_key"],
            original["jobs"][0]["deduplication_key"],
            "{field}"
        );
    }
}

#[test]
fn incomplete_graphs_and_malformed_events_fail_without_a_plan() {
    let f = Fixture::new();
    refused(
        f.invoke(&f.request(None, Some(&"f".repeat(40)))),
        "source graph unavailable",
    );
    let mut request = f.request(None, Some(&f.tip));
    request["after"] = json!("HEAD");
    refused(f.invoke(&request), "full SHA-1");
    request["after"] = json!(f.tip);
    request["run_attempt"] = json!(0);
    refused(f.invoke(&request), "run identity");
    request["run_attempt"] = json!(1);
    request["source_ref"] = json!("refs/heads/main..other");
    refused(f.invoke(&request), "check-ref-format");
    request["source_ref"] = json!("refs/heads/main");
    request["event"] = json!("pull_request");
    refused(f.invoke(&request), "ref does not match");
    request["event"] = json!("push");
    request["unexpected"] = json!(true);
    refused(f.invoke(&request), "invalid audit-plan request JSON");
    request.as_object_mut().unwrap().remove("unexpected");
    fs::write(f.root.join(".git/shallow"), format!("{}\n", f.tip)).unwrap();
    refused(f.invoke(&request), "shallow source graph");
}
