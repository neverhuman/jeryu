use super::*;
use identity::Scope;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
};

struct Fixture {
    temporary: PathBuf,
    source: PathBuf,
    output: PathBuf,
    expected: ExpectedObservation,
    policy: Vec<u8>,
    report: Vec<u8>,
    lock: Vec<u8>,
    config: Vec<u8>,
    auditor: Vec<u8>,
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
        // Retain every small synthetic fixture. This test never claims custody
        // over unrelated processes or performs speculative success cleanup.
        let temporary = tempfile::Builder::new()
            .prefix("jeryu-audit-package-test-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
            .keep();
        eprintln!("retained audit-package fixture: {}", temporary.display());
        let source = temporary.join("source");
        let output = temporary.join("output");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(source.join("components/jeryu-tool/generated"))
            .unwrap();
        fs::DirBuilder::new().mode(0o700).create(&output).unwrap();
        write(
            &source.join("components/jeryu-tool/generated/jankurai-pin.env"),
            format!(
                "JANKURAI_REV=\"{}\"\nJANKURAI_BINARY_SHA256=\"{}\"\nJANKURAI_SEMVER=\"1.6.11\"\n",
                "b".repeat(40),
                "c".repeat(64)
            )
            .as_bytes(),
        );
        let policy=b"workspace='jeryu'\nminimum_score=85\nhard_findings_allowed=0\nsoft_findings_allowed=0\nrequired_tool='jankurai'\nrequired_tool_version='1.6.11'\n".to_vec();
        let report=serde_json::to_vec(&json!({
            "standard":"jankurai","schema_version":"1.9.0","auditor_version":"1.6.11","repo":".",
            "score":95,"dirty_worktree":false,"scope":{"mode":"full","paths":[]},
            "git":{"head":"aaaaaaaa","mode":"full","dirty_worktree":false},
            "policy":{"minimum_score":85,"mode":"standard","path":"./agent/audit-policy.toml","fail_on":["critical","high"]},
            "decision":{"status":"pass","passed":true,"minimum_score":85,"hard_findings":0,"soft_findings":0,"ratchet":{"passed":true}},
            "conformance_decision":"pass","conformance_blockers":[],"findings":[],"caps_applied":[]
        })).unwrap();
        let lock = b"locked dependency inputs\n".to_vec();
        let config = b"{\"scope\":\"full\"}\n".to_vec();
        let auditor = b"{\"unqualified_fixture_receipt\":true}\n".to_vec();
        let expected = ExpectedObservation {
            identity: Identity {
                repository: "neverhuman/jeryu".into(),
                source_repository: "neverhuman/jeryu".into(),
                scope: Scope::Repository,
                source_commit: "a".repeat(40),
                source_tree: "d".repeat(40),
                auditor_source_commit: "b".repeat(40),
                auditor_executable_sha256: "c".repeat(64),
                auditor_receipt_sha256: audit_evidence::hash(&auditor),
                auditor_version: "1.6.11".into(),
                candidate_policy_sha256: audit_evidence::hash(&policy),
                governing_policy_sha256: audit_evidence::hash(&policy),
                dependency_lock_sha256: audit_evidence::hash(&lock),
                execution_config_sha256: audit_evidence::hash(&config),
                minimum: 85,
                max_soft: Some(0),
                workflow_repository: "neverhuman/jeryu".into(),
                workflow_path: ".github/workflows/audit-executor.yml".into(),
                workflow_commit: "e".repeat(40),
                workflow_blob: "f".repeat(40),
                run_id: 123,
                run_attempt: 2,
                job_id: 456,
                observed_at_unix: 1_783_000_000,
            },
            command_exit: Some(0),
            report_sha256: Some(audit_evidence::hash(&report)),
            renderer: None,
        };
        Self {
            temporary,
            source,
            output,
            expected,
            policy,
            report,
            lock,
            config,
            auditor,
        }
    }

    fn receipt(&self) -> Vec<u8> {
        serde_json::to_vec(
            &json!({"schema_version":"jeryu.audit-publication-observation/v1",
            "identity":self.expected.identity,"command_exit":self.expected.command_exit,
            "report_sha256":self.expected.report_sha256}),
        )
        .unwrap()
    }
    fn inputs<'a>(&'a self, receipt: &'a [u8]) -> Inputs<'a> {
        Inputs {
            receipt,
            report: Some(&self.report),
            candidate_policy: &self.policy,
            governing_policy: &self.policy,
            dependency_lock: &self.lock,
            execution_config: &self.config,
            auditor_receipt: &self.auditor,
            renderer_output: None,
            renderer_unavailable: false,
        }
    }
    fn prepare(&self) -> PreparedBundle {
        prepare(&self.source, &self.expected, self.inputs(&self.receipt())).unwrap()
    }
    fn change_report(&mut self, change: impl FnOnce(&mut Value)) {
        let mut report: Value = serde_json::from_slice(&self.report).unwrap();
        change(&mut report);
        self.report = serde_json::to_vec(&report).unwrap();
        self.expected.report_sha256 = Some(audit_evidence::hash(&self.report));
    }
}

fn state(bundle: &PreparedBundle, expected: &str) {
    assert_eq!(bundle.metadata()["display_state"], expected);
    assert_eq!(bundle.metadata()["publication_qualified"], false);
    assert_eq!(bundle.metadata()["governing_policy_authenticated"], false);
    assert_eq!(bundle.metadata()["svg_safety_verified"], false);
    assert!(bundle.require_publication_admission().is_err());
    if expected != "FAIL" {
        assert!(bundle.metadata()["display_score"].is_null());
        assert!(bundle.metadata()["display_counts"].is_null());
    }
}

#[test]
fn local_policy_pass_is_nonnumeric_pending_and_real_failure_is_preserved() {
    let mut fixture = Fixture::new();
    let pending = fixture.prepare();
    state(&pending, "PENDING");
    assert_eq!(
        pending.metadata()["report_observation"]["summary"]["score"],
        95
    );
    assert_eq!(pending.metadata()["report_observation"]["qualified"], false);
    fixture.expected.command_exit = Some(1);
    fixture.change_report(|report| {
        report["score"] = json!(64);
        report["decision"]["status"] = json!("fail");
        report["decision"]["passed"] = json!(false);
    });
    let failed = fixture.prepare();
    state(&failed, "FAIL");
    assert_eq!(failed.metadata()["display_score"], 64);
    fixture.change_report(|report| {
        report["score"] = json!(95);
        report["findings"] = json!([{"severity":"high","hardness":"hard"}]);
        report["decision"]["hard_findings"] = json!(1);
        report["caps_applied"] = json!(["cap"]);
    });
    let high = fixture.prepare();
    state(&high, "FAIL");
    assert_eq!(
        high.metadata()["display_counts"],
        json!({"hard":1,"caps":1})
    );
}

#[test]
fn mismatched_identity_and_invented_trust_flags_cannot_admit_reports() {
    let fixture = Fixture::new();
    for (pointer, value) in [
        ("/identity/repository", json!("neverhuman/other")),
        ("/identity/source_repository", json!("neverhuman/other")),
        ("/identity/source_commit", json!("b".repeat(40))),
        ("/identity/source_tree", json!("b".repeat(40))),
        ("/identity/scope", json!({"kind":"dependency"})),
        ("/identity/auditor_source_commit", json!("f".repeat(40))),
        ("/identity/auditor_executable_sha256", json!("a".repeat(64))),
        ("/identity/auditor_receipt_sha256", json!("a".repeat(64))),
        ("/identity/candidate_policy_sha256", json!("a".repeat(64))),
        ("/identity/governing_policy_sha256", json!("a".repeat(64))),
        ("/identity/dependency_lock_sha256", json!("a".repeat(64))),
        ("/identity/execution_config_sha256", json!("a".repeat(64))),
        ("/identity/workflow_commit", json!("a".repeat(40))),
        ("/identity/workflow_blob", json!("a".repeat(40))),
        ("/identity/workflow_repository", json!("neverhuman/other")),
        (
            "/identity/workflow_path",
            json!(".github/workflows/other.yml"),
        ),
        ("/identity/run_id", json!(124)),
        ("/identity/run_attempt", json!(3)),
        ("/identity/job_id", json!(789)),
        ("/command_exit", json!(42)),
        ("/report_sha256", json!("a".repeat(64))),
    ] {
        let mut receipt: Value = serde_json::from_slice(&fixture.receipt()).unwrap();
        *receipt.pointer_mut(pointer).unwrap() = value;
        let bytes = serde_json::to_vec(&receipt).unwrap();
        let rejected = prepare(&fixture.source, &fixture.expected, fixture.inputs(&bytes)).unwrap();
        state(&rejected, "ERROR");
        assert!(
            rejected.metadata()["report_observation"]["rejection"]
                .as_str()
                .unwrap()
                .contains("observation disagrees")
        );
    }
    let mut receipt: Value = serde_json::from_slice(&fixture.receipt()).unwrap();
    receipt["trusted"] = json!(true);
    state(
        &prepare(
            &fixture.source,
            &fixture.expected,
            fixture.inputs(&serde_json::to_vec(&receipt).unwrap()),
        )
        .unwrap(),
        "ERROR",
    );
}

#[test]
fn missing_truncated_wrong_source_and_failed_commands_are_errors() {
    let mut fixture = Fixture::new();
    let receipt = fixture.receipt();
    let mut inputs = fixture.inputs(&receipt);
    inputs.report = None;
    state(
        &prepare(&fixture.source, &fixture.expected, inputs).unwrap(),
        "ERROR",
    );
    for report in [b"{".as_slice(), b"[]".as_slice(), b"".as_slice()] {
        fixture.report = report.to_vec();
        fixture.expected.report_sha256 = Some(audit_evidence::hash(report));
        state(&fixture.prepare(), "ERROR");
    }
    let mut fixture = Fixture::new();
    for exit in [1, 2, 42, 124, 137] {
        fixture.expected.command_exit = Some(exit);
        state(&fixture.prepare(), "ERROR");
    }
    fixture.expected.command_exit = Some(0);
    fixture.change_report(|report| report["git"]["head"] = json!("bbbbbbbb"));
    state(&fixture.prepare(), "ERROR");
}

#[test]
fn source_pin_and_effective_policy_floors_remain_required() {
    let mut fixture = Fixture::new();
    fixture.expected.identity.auditor_source_commit = "f".repeat(40);
    state(&fixture.prepare(), "ERROR");
    fixture.expected.identity.auditor_source_commit = "b".repeat(40);
    fixture.expected.identity.max_soft = None;
    state(&fixture.prepare(), "ERROR");
    fixture.expected.identity.max_soft = Some(0);
    for (owner, minimum) in [
        ("jeryu-ci-runner", 85),
        ("jeryu-ci-runner", 91),
        ("jeryu-ci-runner", 100),
    ] {
        fixture.expected.identity.repository = format!("neverhuman/{owner}");
        fixture.expected.identity.source_repository = fixture.expected.identity.repository.clone();
        fixture.expected.identity.minimum = minimum;
        fixture.policy =
            format!("workspace='{owner}'\nminimum_score={minimum}\nhard_findings_allowed=0\n")
                .into_bytes();
        fixture.expected.identity.candidate_policy_sha256 = audit_evidence::hash(&fixture.policy);
        fixture.expected.identity.governing_policy_sha256 = audit_evidence::hash(&fixture.policy);
        fixture.change_report(|report| {
            report["score"] = json!(100);
            report["policy"]["minimum_score"] = json!(minimum);
            report["decision"]["minimum_score"] = json!(minimum);
        });
        state(
            &fixture.prepare(),
            if minimum == 85 { "ERROR" } else { "PENDING" },
        );
    }
}

#[test]
fn renderer_bytes_remain_private_and_failure_cannot_produce_a_card() {
    let mut fixture = Fixture::new();
    let display = fixture.prepare().files["renderer-input.json"].clone();
    let unsafe_svg =
        b"<svg onload='alert(1)'><script>do_not_execute()</script><text>&amp;&lt;</text></svg>";
    fixture.expected.renderer = Some(RendererObservation {
        source_commit: fixture.expected.identity.auditor_source_commit.clone(),
        executable_sha256: fixture.expected.identity.auditor_executable_sha256.clone(),
        receipt_sha256: fixture.expected.identity.auditor_receipt_sha256.clone(),
        display_input_sha256: audit_evidence::hash(&display),
        svg_sha256: audit_evidence::hash(unsafe_svg),
        command_exit: 0,
    });
    let receipt = fixture.receipt();
    let mut inputs = fixture.inputs(&receipt);
    inputs.renderer_output = Some(unsafe_svg);
    let opaque = prepare(&fixture.source, &fixture.expected, inputs).unwrap();
    state(&opaque, "PENDING");
    assert_eq!(opaque.metadata()["renderer_identity_matched"], true);
    assert_eq!(
        opaque.files["renderer-output.bin"].as_slice(),
        unsafe_svg.as_slice()
    );
    assert!(!opaque.files.keys().any(|name| name.ends_with(".svg")));
    for failure in ["exit", "hash", "input", "missing"] {
        let mut expected = fixture.expected.clone();
        let renderer = expected.renderer.as_mut().unwrap();
        match failure {
            "exit" => renderer.command_exit = 42,
            "hash" => renderer.svg_sha256 = "a".repeat(64),
            "input" => renderer.display_input_sha256 = "a".repeat(64),
            _ => (),
        }
        let mut inputs = fixture.inputs(&receipt);
        if failure != "missing" {
            inputs.renderer_output = Some(unsafe_svg);
        }
        state(
            &prepare(&fixture.source, &expected, inputs).unwrap(),
            "ERROR",
        );
    }
    fixture.expected.renderer = None;
    let mut inputs = fixture.inputs(&receipt);
    inputs.renderer_unavailable = true;
    let missing = prepare(&fixture.source, &fixture.expected, inputs).unwrap();
    state(&missing, "ERROR");
    assert_eq!(missing.metadata()["renderer_file_unavailable"], true);
}

#[test]
fn bound_inputs_cannot_be_replaced_or_candidate_policy_comparison_relaxed() {
    let fixture = Fixture::new();
    let receipt = fixture.receipt();
    for field in ["candidate", "governing", "lock", "config", "auditor"] {
        let mut inputs = fixture.inputs(&receipt);
        match field {
            "candidate" => inputs.candidate_policy = b"changed",
            "governing" => inputs.governing_policy = b"changed",
            "lock" => inputs.dependency_lock = b"changed",
            "config" => inputs.execution_config = b"changed",
            _ => inputs.auditor_receipt = b"changed",
        }
        state(
            &prepare(&fixture.source, &fixture.expected, inputs).unwrap(),
            "ERROR",
        );
    }
    let mut expected = fixture.expected.clone();
    let mut governing = fixture.policy.clone();
    governing.extend_from_slice(b"# different protected bytes\n");
    expected.identity.governing_policy_sha256 = audit_evidence::hash(&governing);
    let receipt=serde_json::to_vec(&json!({"schema_version":"jeryu.audit-publication-observation/v1",
        "identity":expected.identity,"command_exit":expected.command_exit,"report_sha256":expected.report_sha256})).unwrap();
    let mut inputs = fixture.inputs(&receipt);
    inputs.governing_policy = &governing;
    state(
        &prepare(&fixture.source, &expected, inputs).unwrap(),
        "ERROR",
    );
    for malicious in [
        "neverhuman/../../outside",
        "neverhuman/<svg>",
        "neverhuman/name\nscript",
    ] {
        expected.identity.repository = malicious.into();
        assert!(prepare(&fixture.source, &expected, fixture.inputs(&receipt)).is_err());
    }
}

#[path = "audit_publication_storage_tests.rs"]
mod disk;

#[path = "audit_publication_identity_tests.rs"]
mod forms;
