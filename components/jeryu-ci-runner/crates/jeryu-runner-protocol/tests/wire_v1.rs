use jeryu_ci_ir::{ArtifactPath, ArtifactWhen, CacheMode, CacheMount, RunnerClass, Step};
use jeryu_runner_protocol::wire::*;
use jeryu_runner_protocol::{Heartbeat, JobOutcome, JobRequest, JobResult, RunnerHello};
use serde_json::{Value, json};

const NOW: u64 = 1_787_900_000_000;
const RESULT_STARTED: u64 = NOW + 1_000;
const RESULT_FINISHED: u64 = NOW + 2_000;
const RESULT_SUBMITTED: u64 = RESULT_FINISHED + 1;
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECRET: &str = "planted-registration-credential";

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn hello() -> RunnerHello {
    let mut hello = RunnerHello::new(
        "xbabe3-runner-01",
        vec![
            RunnerClass::NativeRustClean,
            RunnerClass::Custom("cuda-12".to_string()),
        ],
    );
    hello.labels = vec!["linux".to_string(), "xbabe3".to_string()];
    hello.capacity = 128;
    hello
}

fn context_for(job: &WireJobRequest) -> ExecutionContext {
    ExecutionContext {
        runner_id: "xbabe3-runner-01".to_string(),
        runner_epoch: 7,
        run_id: "run-01".to_string(),
        lease_id: "lease-01".to_string(),
        job_id: "job-01".to_string(),
        repository: "jeryu/jeryu-ci-runner".to_string(),
        head_sha: HEAD.to_string(),
        required_check: "jeryu-ci-runner/required".to_string(),
        job_digest: job.execution_digest(),
        protected_policy_sha: "b".repeat(40),
        toolchain_digest: digest('d'),
        runner_class_policy_id: "native-rust-clean-v1".to_string(),
        runner_class_policy_digest: digest('e'),
        image_digest: digest('f'),
        rootfs_digest: digest('1'),
    }
}

fn context() -> ExecutionContext {
    context_for(&job())
}

fn job() -> WireJobRequest {
    let mut job = JobRequest::new(
        "pipeline-01",
        "run-01",
        "lease-01",
        "job-01",
        RunnerClass::NativeRustClean,
    );
    job.assign_runner("xbabe3-runner-01", 7);
    let mut check = Step::run("check-01", "check", "cargo test --locked");
    check.env.insert("RUST_LOG".to_string(), "info".to_string());
    check.working_directory = Some("crates/jeryu-runner-protocol".to_string());
    job.steps.push(check);
    job.steps.push(Step::uses(
        "policy-01",
        "policy",
        "jeryu/runner-policy@sha256:aaaaaaaa",
    ));
    job.cache_mounts.push(CacheMount {
        name: "cargo".to_string(),
        path: ".cache/cargo".to_string(),
        mode: CacheMode::ReadOnly,
        fingerprint: "fnv64:0123456789abcdef".to_string(),
    });
    job.artifact_paths.push(ArtifactPath {
        name: "results".to_string(),
        paths: vec!["target/**/report.json".to_string()],
        when: ArtifactWhen::Always,
        retention_days: 7,
    });
    job.env.insert("CI".to_string(), "1".to_string());
    WireJobRequest::try_from(&job).expect("valid wire job")
}

fn grant() -> LeaseGrant {
    let job = job();
    LeaseGrant::new(context_for(&job), NOW, NOW + 60_000, job).expect("valid grant")
}

fn result() -> WireJobResult {
    WireJobResult::try_from(&JobResult {
        runner_id: "xbabe3-runner-01".to_string(),
        runner_epoch: 7,
        run_id: "run-01".to_string(),
        lease_id: "lease-01".to_string(),
        job_id: "job-01".to_string(),
        outcome: JobOutcome::Success,
        exit_code: Some(0),
        started_at_millis: RESULT_STARTED,
        finished_at_millis: RESULT_FINISHED,
        artifact_digests: vec![digest('b')],
        cache_receipts: vec!["fnv64:0123456789abcdef".to_string()],
        log_digest: digest('c'),
    })
    .expect("valid result")
}

#[test]
fn all_request_ack_pairs_round_trip_and_bind() {
    let register = RegisterRequest::from_hello("register-01", NOW, &hello()).unwrap();
    let register_json = register.to_json().unwrap();
    assert_eq!(
        RegisterRequest::from_json(&register_json).unwrap(),
        register
    );
    assert_eq!(register.to_hello().unwrap(), hello());
    let register_ack = RegisterAck::new(&register, 7, 5_000, 30_000, NOW + 1).unwrap();
    let parsed = RegisterAck::from_json(&register_ack.to_json().unwrap()).unwrap();
    parsed.validate_for(&register).unwrap();

    let heartbeat = Heartbeat {
        runner_id: context().runner_id,
        runner_epoch: 7,
        run_id: "run-01".to_string(),
        lease_id: "lease-01".to_string(),
        job_id: "job-01".to_string(),
        monotonic_millis: 42,
        message: "building".to_string(),
    };
    let heartbeat =
        HeartbeatRequest::from_heartbeat("heartbeat-01", NOW + 2, Some(context()), &heartbeat)
            .unwrap();
    let parsed = HeartbeatRequest::from_json(&heartbeat.to_json().unwrap()).unwrap();
    assert_eq!(parsed.to_heartbeat().unwrap().lease_id, "lease-01");
    let heartbeat_ack = HeartbeatAck::new(&heartbeat, true, false, 5_000, NOW + 3).unwrap();
    HeartbeatAck::from_json(&heartbeat_ack.to_json().unwrap())
        .unwrap()
        .validate_for(&heartbeat)
        .unwrap();

    let lease_request = LeaseRequest::new("lease-poll-01", context().runner(), NOW + 4).unwrap();
    let lease_ack = LeaseAck::assigned(&lease_request, grant(), NOW + 5).unwrap();
    LeaseAck::from_json(&lease_ack.to_json().unwrap())
        .unwrap()
        .validate_for(&lease_request)
        .unwrap();
    LeaseAck::without_work(&lease_request, LeaseDecision::NoWork, NOW + 5)
        .unwrap()
        .validate_for(&lease_request)
        .unwrap();

    let result_request = ResultRequest::new(context(), result(), RESULT_SUBMITTED).unwrap();
    let parsed = ResultRequest::from_json(&result_request.to_json().unwrap()).unwrap();
    parsed.validate_for(&grant()).unwrap();
    let result_ack = ResultAck::new(
        &result_request,
        ResultDecision::Accepted,
        RESULT_SUBMITTED + 1,
    )
    .unwrap();
    ResultAck::from_json(&result_ack.to_json().unwrap())
        .unwrap()
        .validate_for(&result_request)
        .unwrap();
}

#[test]
fn schemas_reject_unknown_fields_and_wrong_versions() {
    let register = RegisterRequest::from_hello("register-01", NOW, &hello()).unwrap();
    let mut value: Value = serde_json::from_slice(&register.to_json().unwrap()).unwrap();
    value["authorization"] = Value::String(SECRET.to_string());
    let error = RegisterRequest::from_json(&serde_json::to_vec(&value).unwrap()).unwrap_err();
    assert_eq!(error.code(), WireErrorCode::InvalidJson);
    assert!(!format!("{error:?} {error}").contains(SECRET));

    let mut value: Value = serde_json::from_slice(&register.to_json().unwrap()).unwrap();
    value["protocol_version"] = Value::String(SECRET.to_string());
    let error = RegisterRequest::from_json(&serde_json::to_vec(&value).unwrap()).unwrap_err();
    assert_eq!(error.code(), WireErrorCode::InvalidProtocol);
    assert!(!format!("{error:?} {error}").contains(SECRET));
}

#[test]
fn canonical_runner_classes_round_trip_without_aliases() {
    let register = RegisterRequest::from_hello("register-01", NOW, &hello()).unwrap();
    let json = String::from_utf8(register.to_json().unwrap()).unwrap();
    assert!(json.contains("\"custom:cuda-12\""));
    assert_eq!(
        register.to_hello().unwrap().supported_classes,
        hello().supported_classes
    );

    let alias = json.replace("native-rust-clean", "docker");
    assert!(RegisterRequest::from_json(alias.as_bytes()).is_err());
    let noncanonical = json.replace("custom:cuda-12", "custom:CUDA-12");
    assert!(RegisterRequest::from_json(noncanonical.as_bytes()).is_err());
}

#[test]
fn scalar_collection_and_body_bounds_fail_closed() {
    let mut register = RegisterRequest::from_hello("register-01", NOW, &hello()).unwrap();
    register.runner_id = "x".repeat(129);
    assert_eq!(register.validate().unwrap_err().field(), "runner_id");

    let mut register = RegisterRequest::from_hello("register-01", NOW, &hello()).unwrap();
    register.labels = (0..=MAX_LABELS).map(|i| format!("label-{i}")).collect();
    assert_eq!(
        register.validate().unwrap_err().code(),
        WireErrorCode::InvalidCollection
    );
    register.labels.clear();
    register.capacity = MAX_CAPACITY + 1;
    assert_eq!(register.validate().unwrap_err().field(), "capacity");
    register.capacity = 1;
    register.sent_at_unix_millis = 0;
    assert_eq!(
        register.validate().unwrap_err().field(),
        "sent_at_unix_millis"
    );

    let mut bounded_job = job();
    bounded_job.steps = vec![bounded_job.steps[0].clone(); MAX_STEPS + 1];
    assert_eq!(bounded_job.validate().unwrap_err().field(), "steps");

    let oversized = vec![b' '; MAX_MESSAGE_BYTES + 1];
    assert_eq!(
        RegisterRequest::from_json(&oversized).unwrap_err().code(),
        WireErrorCode::MessageTooLarge
    );

    let mut bounded_result = result();
    bounded_result.artifact_digests = vec![digest('d'); MAX_RESULT_ITEMS + 1];
    assert_eq!(
        bounded_result.validate().unwrap_err().code(),
        WireErrorCode::InvalidCollection
    );
}

#[test]
fn case_sensitive_repository_and_message_time_order_are_fail_closed() {
    let assigned = LeaseRequest::new("lease-poll-01", context().runner(), NOW).unwrap();
    for invalid_server_time in [NOW - 1, NOW + 60_000] {
        assert_eq!(
            LeaseAck::assigned(&assigned, grant(), invalid_server_time)
                .unwrap_err()
                .field(),
            "server_time_unix_millis"
        );
    }
    LeaseAck::assigned(&assigned, grant(), NOW).expect("lease start is inclusive");
    LeaseAck::assigned(&assigned, grant(), NOW + 59_999).expect("lease expiry is exclusive");

    assert_eq!(
        ResultRequest::new(context(), result(), RESULT_FINISHED - 1)
            .unwrap_err()
            .field(),
        "submitted_at_unix_millis"
    );
    ResultRequest::new(context(), result(), RESULT_FINISHED)
        .expect("submission at finish is valid");

    let redline_job = job();
    let mut redline = context_for(&redline_job);
    redline.repository = "jeryu/redlineDB".to_string();
    LeaseGrant::new(redline, NOW, NOW + 60_000, redline_job)
        .expect("case-sensitive hosted repository identity");

    for invalid in [
        "Jeryu/redlineDB",
        "jeryu/redline DB",
        "jeryu/-redlineDB",
        "jeryu/redlineDB-",
        "jeryu/redline..DB",
    ] {
        let job = job();
        let mut context = context_for(&job);
        context.repository = invalid.to_string();
        assert_eq!(
            LeaseGrant::new(context, NOW, NOW + 60_000, job)
                .unwrap_err()
                .field(),
            "repository",
            "invalid repository was accepted: {invalid}"
        );
    }
}

#[test]
fn stale_context_and_receipt_replays_are_rejected() {
    let mut stale_grant = grant();
    stale_grant.context.runner_epoch += 1;
    assert_eq!(
        stale_grant.validate().unwrap_err().code(),
        WireErrorCode::ContextMismatch
    );

    let base = ResultRequest::new(context(), result(), RESULT_SUBMITTED).unwrap();
    let replay = ResultRequest::new(context(), result(), RESULT_SUBMITTED).unwrap();
    assert_eq!(base.receipt_id, replay.receipt_id);

    for mutate in [
        |ctx: &mut ExecutionContext| ctx.runner_epoch += 1,
        |ctx: &mut ExecutionContext| ctx.lease_id = "lease-02".to_string(),
        |ctx: &mut ExecutionContext| ctx.repository = "jeryu/jeryu-core".to_string(),
        |ctx: &mut ExecutionContext| ctx.head_sha = "b".repeat(40),
        |ctx: &mut ExecutionContext| ctx.required_check = "jeryu-core/required".to_string(),
    ] {
        let mut changed = context();
        mutate(&mut changed);
        let mut changed_result = result();
        changed_result.runner_epoch = changed.runner_epoch;
        changed_result.lease_id = changed.lease_id.clone();
        let changed = ResultRequest::new(changed, changed_result, RESULT_SUBMITTED).unwrap();
        assert_ne!(base.receipt_id, changed.receipt_id);
    }

    let mut tampered = base.clone();
    tampered.context.repository = "jeryu/jeryu-core".to_string();
    assert_eq!(
        tampered.validate().unwrap_err().code(),
        WireErrorCode::ReceiptMismatch
    );
    let mut ack = ResultAck::new(&base, ResultDecision::Duplicate, RESULT_SUBMITTED + 1).unwrap();
    ack.context.head_sha = "b".repeat(40);
    assert_eq!(
        ack.validate_for(&base).unwrap_err().code(),
        WireErrorCode::ContextMismatch
    );
}

#[test]
fn every_job_field_is_bound_to_the_lease_digest() {
    let job = job();
    let context = context_for(&job);
    let value = serde_json::to_value(&job).unwrap();
    let mutations = [
        ("/request_id", json!("request-02")),
        ("/pipeline_id", json!("pipeline-02")),
        ("/run_id", json!("run-02")),
        ("/lease_id", json!("lease-02")),
        ("/job_id", json!("job-02")),
        ("/runner_id", json!("xbabe3-runner-02")),
        ("/runner_epoch", json!(8)),
        ("/runner_class", json!("native-rust-hot")),
        ("/steps/0/id", json!("check-02")),
        ("/steps/0/name", json!("changed")),
        ("/steps/0/command", json!("cargo check --locked")),
        (
            "/steps/1/uses",
            json!("jeryu/runner-policy@sha256:bbbbbbbb"),
        ),
        ("/steps/0/env/RUST_LOG", json!("debug")),
        ("/steps/0/working_directory", json!("crates/jeryu-ci-ir")),
        ("/cache_mounts/0/name", json!("registry")),
        ("/cache_mounts/0/path", json!(".cache/registry")),
        ("/cache_mounts/0/mode", json!("read-write-quarantine")),
        (
            "/cache_mounts/0/fingerprint",
            json!("fnv64:fedcba9876543210"),
        ),
        ("/artifact_paths/0/name", json!("logs")),
        ("/artifact_paths/0/paths/0", json!("target/**/output.json")),
        ("/artifact_paths/0/when", json!("on-failure")),
        ("/artifact_paths/0/retention_days", json!(8)),
        ("/env/CI", json!("0")),
        ("/timeout_seconds", json!(3601)),
    ];

    for (pointer, replacement) in mutations {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let changed: WireJobRequest = serde_json::from_value(changed).unwrap();
        changed.validate().unwrap();
        assert_ne!(context.job_digest, changed.execution_digest(), "{pointer}");
        assert_eq!(
            LeaseGrant::new(context.clone(), NOW, NOW + 60_000, changed)
                .unwrap_err()
                .code(),
            WireErrorCode::ContextMismatch,
            "{pointer}"
        );
    }
}

#[test]
fn provenance_is_validated_and_bound_to_result_receipts() {
    let base = ResultRequest::new(context(), result(), RESULT_SUBMITTED).unwrap();
    let mutations: [fn(&mut ExecutionContext); 7] = [
        |ctx| ctx.job_digest = digest('2'),
        |ctx| ctx.protected_policy_sha = "c".repeat(40),
        |ctx| ctx.toolchain_digest = digest('3'),
        |ctx| ctx.runner_class_policy_id = "native-rust-clean-v2".to_string(),
        |ctx| ctx.runner_class_policy_digest = digest('4'),
        |ctx| ctx.image_digest = digest('5'),
        |ctx| ctx.rootfs_digest = digest('6'),
    ];
    for mutate in mutations {
        let mut changed_context = context();
        mutate(&mut changed_context);
        let changed = ResultRequest::new(changed_context, result(), RESULT_SUBMITTED).unwrap();
        assert_ne!(base.receipt_id, changed.receipt_id);
    }

    let mut invalid = context();
    invalid.toolchain_digest = "sha256:not-a-digest".to_string();
    assert_eq!(
        ResultRequest::new(invalid, result(), RESULT_SUBMITTED)
            .unwrap_err()
            .field(),
        "toolchain_digest"
    );
}

#[test]
fn workspace_paths_reject_unsafe_lexical_forms() {
    let unsafe_paths = [
        "",
        ".",
        "..",
        "./child",
        "child/./file",
        "../child",
        "child/../file",
        "/absolute",
        "child\\file",
        "C:/absolute",
        "child//file",
        "child/",
        "child\0file",
    ];
    for path in unsafe_paths {
        let mut changed = job();
        changed.steps[0].working_directory = Some(path.to_string());
        assert_eq!(
            changed.validate().unwrap_err().field(),
            "step.working_directory"
        );

        let mut changed = job();
        changed.cache_mounts[0].path = path.to_string();
        assert_eq!(changed.validate().unwrap_err().field(), "cache.path");

        let mut changed = job();
        changed.artifact_paths[0].paths[0] = path.to_string();
        assert_eq!(changed.validate().unwrap_err().field(), "artifact.paths");
    }
}

#[test]
fn diagnostics_redact_opaque_values_and_adapter_controls_are_reserved() {
    let mut secret_job = job();
    secret_job.steps[0].command = Some(format!("echo {SECRET}"));
    secret_job
        .env
        .insert("SAFE_VALUE".to_string(), SECRET.to_string());
    secret_job.validate().unwrap();
    assert!(serde_json::to_string(&secret_job).unwrap().contains(SECRET));
    assert!(!format!("{secret_job:?}").contains(SECRET));
    assert!(!format!("{:?}", secret_job.steps[0]).contains(SECRET));

    let secret_context = context_for(&secret_job);
    let secret_grant = LeaseGrant::new(secret_context, NOW, NOW + 1_000, secret_job).unwrap();
    let lease_request = LeaseRequest::new("lease-poll-01", context().runner(), NOW).unwrap();
    let lease_ack = LeaseAck::assigned(&lease_request, secret_grant, NOW + 1).unwrap();
    assert!(!format!("{lease_ack:?}").contains(SECRET));

    let heartbeat = Heartbeat {
        runner_id: context().runner_id,
        runner_epoch: 7,
        run_id: String::new(),
        lease_id: String::new(),
        job_id: String::new(),
        monotonic_millis: 1,
        message: SECRET.to_string(),
    };
    let heartbeat =
        HeartbeatRequest::from_heartbeat("heartbeat-01", NOW, None, &heartbeat).unwrap();
    assert!(!format!("{heartbeat:?}").contains(SECRET));

    for reserved in [
        "aUtH_tOkEn",
        "GITHUB_TOKEN",
        "JERYU_TOKEN",
        "AWS_SECRET_ACCESS_KEY",
        "SSH_AUTH_SOCK",
        "GIT_ASKPASS",
        "JERYU_ACTOR",
        "SERVICE_ACCESS_TOKEN",
        "TOKEN_BOOTSTRAP",
        "BUILD_CREDENTIALS",
        "JERYU_RUNNER_ENDPOINT",
        "GIT_CONFIG_COUNT",
        "REMOTE_ACTOR_ID",
    ] {
        let mut auth_job = job();
        auth_job
            .env
            .insert(reserved.to_string(), SECRET.to_string());
        let error = auth_job.validate().unwrap_err();
        assert_eq!(error.field(), "env", "{reserved}");
        assert!(!format!("{error:?} {error}").contains(SECRET));

        let mut auth_step = job();
        auth_step.steps[0]
            .env
            .insert(reserved.to_ascii_lowercase(), SECRET.to_string());
        assert_eq!(auth_step.validate().unwrap_err().field(), "step.env");
    }
}
