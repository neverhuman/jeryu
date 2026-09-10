//! Real HTTP transport and process-exit regression coverage for CI status.

use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    process::{Command, Output},
    time::{Duration, Instant},
};

fn check(id: usize) -> Value {
    json!({
        "id": format!("00000000-0000-4000-8000-{id:012x}"),
        "name": format!("fixture/check-{id}"),
        "head_sha": format!("{id:040x}"),
        "status": "completed", "conclusion": "success",
        "details_url": "https://example.invalid/check",
        "output": {"title":"result", "summary":"fixture", "text":"full detail"},
        "started_at":"2026-01-01T00:00:00Z", "completed_at":"2026-01-01T00:00:01Z",
        "future_extension": {"preserved":true}
    })
}

fn command(url: &str, owner: &str, repo: &str, json_output: bool) -> Output {
    let home = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let binary =
        std::env::var_os("JERYU_TEST_BINARY").unwrap_or_else(|| env!("CARGO_BIN_EXE_jeryu").into());
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("HOME", home.path())
        .current_dir(home.path())
        .env("JERYU_API_URL", url)
        .env("JERYU_TOKEN", "fixture-token")
        .args(["--owner", owner]);
    if json_output {
        command.arg("--json");
    }
    command
        .args(["ci", "status", "--repo", repo])
        .output()
        .unwrap()
}

// An actual TCP peer, with bounded accepts/reads and one connection per response.
// Unexpectedly missing requests fail instead of leaving a blocked fixture thread.
fn exchange(responses: Vec<(u16, String)>, json_output: bool) -> (Output, Vec<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/prefix", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "expected CI status HTTP request");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("fixture accept failed: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                assert!(request.len() < 16 * 1024, "unexpected request size");
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            requests.push(String::from_utf8(request).unwrap());
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    let output = command(&url, "fixture-owner", "repo.with-dashes_1", json_output);
    (output, server.join().unwrap())
}

fn report(total: usize, checks: Vec<Value>) -> (u16, String) {
    (
        200,
        json!({"total_count":total,"check_runs":checks}).to_string(),
    )
}

fn rejected(output: Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "a failed read must not print a partial report"
    );
    assert!(!output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-token"));
}

#[test]
fn authenticated_status_collects_pages_and_preserves_uuid_evidence() {
    let mut checks: Vec<_> = (0..101).map(check).collect();
    let conclusions = [
        "success",
        "failure",
        "neutral",
        "cancelled",
        "skipped",
        "action_required",
        "stale",
        "timed_out",
    ];
    for (run, conclusion) in checks.iter_mut().zip(conclusions) {
        run["conclusion"] = json!(conclusion);
    }
    checks[8]["status"] = json!("queued");
    checks[8]["conclusion"] = Value::Null;
    checks[9]["status"] = json!("in_progress");
    checks[9]["conclusion"] = Value::Null;
    // Existing Core accepts nonempty heads; transport must preserve them verbatim.
    checks[9]["head_sha"] = json!("short-head");
    let (output, requests) = exchange(
        vec![
            report(101, checks[..100].to_vec()),
            report(101, checks[100..].to_vec()),
        ],
        true,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, json!({"total_count":101,"check_runs":checks}));
    assert_eq!(requests.len(), 2);
    for (i, request) in requests.iter().enumerate() {
        assert!(request.starts_with(&format!("GET /prefix/repos/fixture-owner/repo.with-dashes_1/check-runs?per_page=100&page={} HTTP/1.1\r\n", i + 1)));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-token\r\n")
        );
    }
}

#[test]
fn human_status_keeps_distinct_outcomes_and_empty_is_not_a_pass() {
    let mut checks: Vec<_> = (0..4).map(check).collect();
    checks[0]["status"] = json!("queued");
    checks[0]["conclusion"] = Value::Null;
    checks[1]["status"] = json!("in_progress");
    checks[1]["conclusion"] = Value::Null;
    checks[2]["conclusion"] = json!("failure");
    checks[3]["conclusion"] = json!("skipped");
    checks[3]["name"] = json!("check\n\u{1b}[31m");
    let (output, _) = exchange(vec![report(4, checks)], false);
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).unwrap();
    for status in [
        "[queued; conclusion=none]",
        "[in_progress; conclusion=none]",
        "[completed; conclusion=failure]",
        "[completed; conclusion=skipped]",
    ] {
        assert!(human.contains(status), "{human}");
    }
    assert_eq!(human.lines().count(), 4);
    assert!(!human.contains('\u{1b}'));
    let (output, _) = exchange(vec![report(0, vec![])], false);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "no check runs for fixture-owner/repo.with-dashes_1\n"
    );
    let (output, _) = exchange(vec![report(0, vec![])], true);
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"total_count":0,"check_runs":[]})
    );
}

#[test]
fn malformed_or_incomplete_reports_fail_without_partial_output() {
    let mut bad_runs = Vec::new();
    for (field, value) in [
        ("id", json!(42)),
        ("id", json!("actions-42")),
        ("name", json!("")),
        ("head_sha", Value::Null),
        ("status", json!("passed")),
        ("conclusion", json!("passed")),
    ] {
        let mut run = check(1);
        run[field] = value;
        bad_runs.push(run);
    }
    let mut missing = check(1);
    missing.as_object_mut().unwrap().remove("conclusion");
    bad_runs.push(missing);
    for run in bad_runs {
        rejected(exchange(vec![report(1, vec![run])], true).0, 4);
    }
    for body in [
        "not json",
        r#"{"total_count":true,"check_runs":[]}"#,
        r#"{"total_count":0.5,"check_runs":[]}"#,
        r#"{"total_count":0,"check_runs":{}}"#,
        r#"{"total_count":10001,"check_runs":[]}"#,
    ] {
        rejected(exchange(vec![(200, body.into())], true).0, 4);
    }
    rejected(exchange(vec![report(2, vec![check(1)])], true).0, 4);
    rejected(exchange(vec![report(0, vec![check(1)])], true).0, 4);
    let first: Vec<_> = (0..100).map(check).collect();
    for second in [
        report(102, vec![check(100), check(101)]),
        report(101, vec![]),
        report(101, vec![check(0)]),
    ] {
        rejected(
            exchange(vec![report(101, first.clone()), second], true).0,
            4,
        );
    }
}

#[test]
fn http_denials_and_unavailable_server_are_errors() {
    for status in [401, 403, 404, 503] {
        let (output, _) = exchange(vec![(status, r#"{"message":"denied"}"#.into())], true);
        assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("HTTP {status}")));
        rejected(output, 3);
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    rejected(command(&url, "fixture-owner", "repo", true), 5);
}

#[test]
fn unsafe_repository_segments_fail_before_transport() {
    // Invalid segment errors must precede the otherwise invalid URL error.
    for (owner, repo) in [
        ("..", "repo"),
        ("owner", ".."),
        ("owner/other", "repo"),
        ("owner", "repo?x=1"),
        ("owner", "%2e%2e"),
        ("owner", ""),
    ] {
        let output = command("invalid-url", owner, repo, true);
        assert!(String::from_utf8_lossy(&output.stderr).contains("repository owner and name"));
        rejected(output, 4);
    }
}
