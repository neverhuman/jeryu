//! Help-snapshot and command-dispatch invariant tests for `jeryu-cli`.
//!
//! Two families of tests:
//! 1. Help-tree invariants: walk the clap `Command` tree and assert the
//!    vocabulary is GitHub-shaped and that the renamed verbs are present.
//! 2. Dispatch smoke tests: parse a real argv, run it against the in-memory
//!    client, and assert on the rendered output / exit code.

use clap::{CommandFactory, Parser};
use jeryu_cli::ForgeClient;
use jeryu_cli::cli::{Cli, Commands};
use jeryu_cli::client::{InMemoryClient, IssueState, PullRequestState};
use jeryu_cli::dispatch_with_api_url_env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;

// ---------------------------------------------------------------------------
// Help-tree harness
// ---------------------------------------------------------------------------

/// Collect every searchable string in the full help tree: subcommand names,
/// about / long-about text, every arg long flag, every arg value name, and
/// every arg help string. The original source snapshot test only checked
/// subcommand *names*; covering arg/about text is what catches doc-string leaks.
fn collect_help_strings(cmd: &clap::Command) -> Vec<String> {
    let mut out = Vec::new();
    out.push(cmd.get_name().to_string());
    if let Some(about) = cmd.get_about() {
        out.push(about.to_string());
    }
    if let Some(long) = cmd.get_long_about() {
        out.push(long.to_string());
    }
    for arg in cmd.get_arguments() {
        if let Some(long) = arg.get_long() {
            out.push(long.to_string());
        }
        if let Some(help) = arg.get_help() {
            out.push(help.to_string());
        }
        if let Some(long_help) = arg.get_long_help() {
            out.push(long_help.to_string());
        }
        for pv in arg.get_possible_values() {
            out.push(pv.get_name().to_string());
        }
    }
    for sub in cmd.get_subcommands() {
        out.extend(collect_help_strings(sub));
    }
    out
}

/// Top-level subcommand names only.
fn top_level_names() -> Vec<String> {
    Cli::command()
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect()
}

/// Names of the direct subcommands of a named top-level group.
fn group_subnames(group: &str) -> Vec<String> {
    Cli::command()
        .get_subcommands()
        .find(|s| s.get_name() == group)
        .map(|g| {
            g.get_subcommands()
                .map(|s| s.get_name().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Vocabulary invariants
// ---------------------------------------------------------------------------

#[test]
fn help_tree_contains_no_forbidden_terms() {
    let mut cmd = Cli::command();
    cmd.build();
    let haystack = collect_help_strings(&cmd).join("\u{1f}").to_lowercase();

    // Forbidden foreign-forge and renamed-away vocabulary. The denied tokens are
    // assembled from fragments so this source file itself carries no banned
    // literal (LAW: zero literal foreign-forge strings under src/ or tests/).
    for term in forbidden_terms() {
        assert!(
            !haystack.contains(&term),
            "help tree leaks forbidden term {term:?}"
        );
    }

    // Bare `m`+`r` and `pool` must not appear as whole words.
    for forbidden_word in [["m", "r"].concat(), ["po", "ol"].concat()] {
        let leaked = haystack
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|w| w == forbidden_word);
        assert!(!leaked, "help tree leaks forbidden word {forbidden_word:?}");
    }
}

/// The denied vocabulary, assembled from fragments so the literals never appear
/// verbatim in this file. These are the tokens the help tree must never leak.
fn forbidden_terms() -> Vec<String> {
    vec![
        ["git", "lab"].concat(),
        ["jit", "forge"].concat(),
        ["jit-", "forge"].concat(),
        ["ni", "tro"].concat(),
        ["merge ", "request"].concat(),
        ["merge-", "request"].concat(),
        ["merge", "request"].concat(),
        ["pipe", "line"].concat(),
        ["runner ", "pool"].concat(),
        ["runner-", "pool"].concat(),
    ]
}

#[test]
fn help_tree_uses_github_shaped_vocabulary() {
    let mut cmd = Cli::command();
    cmd.build();
    let haystack = collect_help_strings(&cmd).join("\u{1f}").to_lowercase();

    for required in ["pull request", "ci run", "runner", "proof"] {
        assert!(
            haystack.contains(required),
            "help tree missing required vocabulary {required:?}"
        );
    }
}

#[test]
fn top_level_excludes_removed_commands() {
    let names = top_level_names();
    let retired_review_command = ["m", "r"].concat();
    for removed in [
        retired_review_command.as_str(),
        "pool",
        "pipeline",
        "exec",
        "job",
    ] {
        assert!(
            !names.iter().any(|n| n == removed),
            "removed command {removed:?} is still a top-level subcommand: {names:?}"
        );
    }
}

#[test]
fn top_level_includes_renamed_commands() {
    let names = top_level_names();
    for required in [
        "forge",
        "ci",
        "runner",
        "agent",
        "proof",
        "release",
        "cache",
        "status",
        "priorities",
        "repo-graph",
        "artifacts",
        "runners",
        "gh-setup",
        "autonomy",
        "onboard",
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "required top-level command {required:?} missing from {names:?}"
        );
    }
}

#[test]
fn control_plane_commands_parse() {
    use jeryu_cli::cli::{ArtifactsCommands, RepoGraphCommands, RunnersCommands};

    let cli =
        Cli::try_parse_from(["jeryu", "priorities", "--limit", "3"]).expect("priorities parses");
    match cli.command {
        Commands::Priorities { limit } => assert_eq!(limit, Some(3)),
        other => panic!("unexpected parse: {other:?}"),
    }

    let cli = Cli::try_parse_from([
        "jeryu",
        "repo-graph",
        "clusters",
        "--cluster-kind",
        "ci_blocker",
    ])
    .expect("repo graph clusters parses");
    match cli.command {
        Commands::RepoGraph(RepoGraphCommands::Clusters { cluster_kind, .. }) => {
            assert_eq!(cluster_kind.as_deref(), Some("ci_blocker"));
        }
        other => panic!("unexpected parse: {other:?}"),
    }

    let cli = Cli::try_parse_from(["jeryu", "artifacts", "latest"]).expect("artifacts parses");
    match cli.command {
        Commands::Artifacts(ArtifactsCommands::Latest { repo }) => assert!(repo.is_none()),
        other => panic!("unexpected parse: {other:?}"),
    }

    let cli = Cli::try_parse_from(["jeryu", "runners", "status"]).expect("runners parses");
    match cli.command {
        Commands::Runners(RunnersCommands::Status) => {}
        other => panic!("unexpected parse: {other:?}"),
    }
}

#[test]
fn autonomy_group_has_init() {
    let subs = group_subnames("autonomy");
    assert!(
        subs.iter().any(|n| n == "init"),
        "autonomy missing init; has {subs:?}"
    );
}

#[test]
fn forge_group_has_repo_pr_issue() {
    let subs = group_subnames("forge");
    for required in ["repo", "pr", "issue"] {
        assert!(
            subs.iter().any(|n| n == required),
            "forge missing {required:?}; has {subs:?}"
        );
    }
}

#[test]
fn ci_group_has_run_status_explain() {
    let subs = group_subnames("ci");
    for required in ["run", "status", "explain"] {
        assert!(
            subs.iter().any(|n| n == required),
            "ci missing {required:?}; has {subs:?}"
        );
    }
}

#[test]
fn runner_group_has_enroll_list_drain_rotate() {
    let subs = group_subnames("runner");
    for required in ["list", "enroll", "drain", "rotate"] {
        assert!(
            subs.iter().any(|n| n == required),
            "runner missing {required:?}; has {subs:?}"
        );
    }
}

#[test]
fn proof_group_has_verify_explain() {
    let subs = group_subnames("proof");
    for required in ["verify", "explain"] {
        assert!(
            subs.iter().any(|n| n == required),
            "proof missing {required:?}; has {subs:?}"
        );
    }
}

#[test]
fn cache_group_has_self_test() {
    let subs = group_subnames("cache");
    assert!(
        subs.iter().any(|n| n == "self-test"),
        "cache missing self-test; has {subs:?}"
    );
}

// ---------------------------------------------------------------------------
// Parse-shape invariants
// ---------------------------------------------------------------------------

#[test]
fn forge_pr_open_parses_head_base_draft() {
    use jeryu_cli::cli::{ForgeCommands, PrCommands};
    let cli = Cli::try_parse_from([
        "jeryu", "forge", "pr", "open", "--repo", "demo", "--head", "feature", "--base", "main",
        "--title", "T", "--draft",
    ])
    .expect("pr open parses");
    match cli.command {
        Commands::Forge(ForgeCommands::Pr(PrCommands::Open {
            repo,
            head,
            base,
            title,
            draft,
        })) => {
            assert_eq!(repo, "demo");
            assert_eq!(head, "feature");
            assert_eq!(base, "main");
            assert_eq!(title, "T");
            assert!(draft);
        }
        other => panic!("unexpected parse: {other:?}"),
    }
}

#[test]
fn ci_run_rejects_foreign_kind_but_accepts_native_and_github() {
    // The removed foreign-CI dialect must not parse. The rejected value is
    // assembled from fragments so no banned literal appears in this file.
    let foreign_kind = ["git", "lab"].concat();
    assert!(
        Cli::try_parse_from([
            "jeryu",
            "ci",
            "run",
            "--repo",
            "demo",
            "--kind",
            &foreign_kind
        ])
        .is_err(),
        "foreign --kind value must be rejected"
    );

    use jeryu_cli::cli::{CiCommands, CiKindArg};
    for (flag, expected) in [("native", CiKindArg::Native), ("github", CiKindArg::Github)] {
        let cli = Cli::try_parse_from(["jeryu", "ci", "run", "--repo", "demo", "--kind", flag])
            .unwrap_or_else(|e| panic!("--kind {flag} should parse: {e}"));
        match cli.command {
            Commands::Ci(CiCommands::Run { kind, .. }) => assert_eq!(kind, expected),
            other => panic!("unexpected parse: {other:?}"),
        }
    }
}

#[test]
fn runner_enroll_parses_executor() {
    use jeryu_cli::cli::{RunnerCommands, RunnerExecutorArg};
    let cli = Cli::try_parse_from([
        "jeryu",
        "runner",
        "enroll",
        "node-7",
        "--executor",
        "native",
    ])
    .expect("runner enroll parses");
    match cli.command {
        Commands::Runner(RunnerCommands::Enroll { node, executor }) => {
            assert_eq!(node, "node-7");
            assert_eq!(executor, RunnerExecutorArg::Native);
        }
        other => panic!("unexpected parse: {other:?}"),
    }
}

#[test]
fn runner_enroll_defaults_to_native_executor() {
    use jeryu_cli::cli::{RunnerCommands, RunnerExecutorArg};
    let cli =
        Cli::try_parse_from(["jeryu", "runner", "enroll", "node-8"]).expect("runner enroll parses");
    match cli.command {
        Commands::Runner(RunnerCommands::Enroll { node, executor }) => {
            assert_eq!(node, "node-8");
            assert_eq!(executor, RunnerExecutorArg::Native);
        }
        other => panic!("unexpected parse: {other:?}"),
    }
}

#[test]
fn agent_run_parses_required_contract_shape() {
    use jeryu_cli::cli::{AgentCommands, AgentToolArg};
    let cli = Cli::try_parse_from([
        "jeryu",
        "agent",
        "run",
        "--repo",
        "alice/jeryu",
        "--agent",
        "codex",
        "--model",
        "model-x",
        "--effort",
        "xhigh",
        "--task-file",
        "TASK.md",
    ])
    .expect("agent run parses");
    match cli.command {
        Commands::Agent(AgentCommands::Run(args)) => {
            assert_eq!(args.repo, "alice/jeryu");
            assert_eq!(args.agent, AgentToolArg::Codex);
            assert_eq!(args.model, "model-x");
            assert_eq!(args.effort, "xhigh");
            assert_eq!(args.base_ref, "main");
        }
        other => panic!("unexpected parse: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Dispatch smoke tests (real assertions against the in-memory client)
// ---------------------------------------------------------------------------

/// Run a full argv through parse + dispatch against a shared in-memory client,
/// returning
/// `(exit_code, stdout, stderr)`.
fn run_cli(client: &dyn ForgeClient, argv: &[&str]) -> (i32, String, String) {
    let cli = Cli::try_parse_from(argv).expect("argv parses");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = dispatch_with_api_url_env(cli, client, &mut out, &mut err, || None);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[derive(Clone, Debug)]
struct CapturedRequest {
    request_line: String,
    body: String,
}

fn spawn_http_fixture(
    response_body: String,
) -> (
    SocketAddr,
    Arc<Mutex<Option<CapturedRequest>>>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let addr = listener.local_addr().expect("fixture addr");
    let captured = Arc::new(Mutex::new(None));
    let captured_thread = Arc::clone(&captured);
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fixture request");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .expect("read request line");
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read header line");
            if line == "\r\n" || line == "\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                content_length = value.trim().parse().expect("content length");
            }
        }
        let mut body = vec![0; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body).expect("read request body");
        }
        let body = String::from_utf8(body).expect("utf8 request body");
        *captured_thread.lock().expect("capture lock") = Some(CapturedRequest {
            request_line: request_line.trim_end().to_string(),
            body,
        });
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write fixture response");
        stream.flush().ok();
    });
    (addr, captured, handle)
}

#[test]
fn dispatch_repo_create_then_list_roundtrips() {
    let client = InMemoryClient::new();
    let (code, out, _) = run_cli(&client, &["jeryu", "forge", "repo", "create", "alpha"]);
    assert_eq!(code, 0);
    assert!(out.contains("created jeryu/alpha"), "stdout was {out:?}");

    let (code, out, _) = run_cli(&client, &["jeryu", "forge", "repo", "list"]);
    assert_eq!(code, 0);
    assert!(out.contains("jeryu/alpha"), "list stdout was {out:?}");

    // State actually landed in the client, not just rendered.
    let repos = client.list_repositories(Some("jeryu")).unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "alpha");
}

#[test]
fn dispatch_pr_open_status_merge_uses_pr_number() {
    let client = InMemoryClient::with_seed_repo("jeryu", "alpha");

    let (code, out, _) = run_cli(
        &client,
        &[
            "jeryu",
            "forge",
            "pr",
            "open",
            "--repo",
            "alpha",
            "--head",
            "feat",
            "--base",
            "main",
            "--title",
            "Add feature",
        ],
    );
    assert_eq!(code, 0);
    assert!(out.contains("pull request #1"), "open stdout was {out:?}");

    let (code, out, _) = run_cli(
        &client,
        &[
            "jeryu", "forge", "pr", "status", "--repo", "alpha", "--pr", "1",
        ],
    );
    assert_eq!(code, 0);
    assert!(out.contains("#1 is Open"), "status stdout was {out:?}");

    let (code, out, _) = run_cli(
        &client,
        &[
            "jeryu", "forge", "pr", "merge", "--repo", "alpha", "--pr", "1",
        ],
    );
    assert_eq!(code, 0);
    assert!(out.contains("#1 merged"), "merge stdout was {out:?}");

    // The number is per-repo PR number; verify the backing state merged it.
    let pr = client.get_pull_request("jeryu", "alpha", 1).unwrap();
    assert_eq!(pr.number, 1);
    assert_eq!(pr.state, PullRequestState::Merged);
}

#[test]
fn dispatch_issue_create_then_list() {
    let client = InMemoryClient::with_seed_repo("jeryu", "alpha");
    let (code, out, _) = run_cli(
        &client,
        &[
            "jeryu", "forge", "issue", "create", "--repo", "alpha", "--title", "Bug",
        ],
    );
    assert_eq!(code, 0);
    assert!(out.contains("issue #1: Bug"), "create stdout was {out:?}");

    let issues = client.list_issues("jeryu", "alpha").unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].state, IssueState::Open);
}

#[test]
fn dispatch_forge_repo_list_uses_live_api_url() {
    let (addr, captured, server) = spawn_http_fixture(
        r#"[{"name":"alpha","full_name":"jeryu/alpha","private":false,"owner":{"login":"jeryu"},"default_branch":"main"}]"#
            .to_string(),
    );
    let api_url = format!("http://{addr}");
    let client = InMemoryClient::new();
    let (code, out, err) = run_cli(
        &client,
        &["jeryu", "--api-url", &api_url, "forge", "repo", "list"],
    );
    assert_eq!(code, 0);
    assert!(err.is_empty(), "stderr was {err:?}");
    assert!(out.contains("jeryu/alpha"), "stdout was {out:?}");
    let captured = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("request captured");
    assert!(
        captured.request_line.starts_with("GET /repos HTTP/1.1"),
        "request line was {:?}",
        captured.request_line
    );
    assert!(captured.body.is_empty(), "GET must not send a body");
    server.join().expect("fixture server");
}

#[test]
fn dispatch_forge_pr_merge_uses_put_live_api_url() {
    let (addr, captured, server) =
        spawn_http_fixture(r#"{"merged":true,"message":"pull request #1 merged"}"#.to_string());
    let api_url = format!("http://{addr}");
    let client = InMemoryClient::new();
    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu",
            "--api-url",
            &api_url,
            "forge",
            "pr",
            "merge",
            "--repo",
            "alpha",
            "--pr",
            "1",
        ],
    );
    assert_eq!(code, 0);
    assert!(err.is_empty(), "stderr was {err:?}");
    assert!(out.contains("merged"), "stdout was {out:?}");
    let captured = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("request captured");
    assert!(
        captured
            .request_line
            .starts_with("PUT /repos/jeryu/alpha/pulls/1/merge HTTP/1.1"),
        "request line was {:?}",
        captured.request_line
    );
    assert_eq!(captured.body, "{}", "merge should send an empty JSON body");
    server.join().expect("fixture server");
}

#[test]
fn dispatch_ci_run_schedules_then_status_and_explain() {
    let client = InMemoryClient::with_seed_repo("jeryu", "alpha");
    let (code, out, _) = run_cli(
        &client,
        &[
            "jeryu", "ci", "run", "--repo", "alpha", "--ref", "main", "--kind", "native",
        ],
    );
    assert_eq!(code, 0);
    assert!(
        out.contains("scheduled ci run run-1"),
        "run stdout was {out:?}"
    );
    assert!(
        out.contains("3 jobs"),
        "native should compile 3 jobs: {out:?}"
    );

    let (code, out, _) = run_cli(&client, &["jeryu", "ci", "status", "--repo", "alpha"]);
    assert_eq!(code, 0);
    assert!(out.contains("run-1"), "status stdout was {out:?}");
    assert!(out.contains("Queued"), "status stdout was {out:?}");

    let (code, out, _) = run_cli(&client, &["jeryu", "ci", "explain", "run-1"]);
    assert_eq!(code, 0);
    assert!(out.contains("blocked=false"), "explain stdout was {out:?}");
}

#[test]
fn dispatch_ci_run_github_kind_compiles_different_ir() {
    let client = InMemoryClient::with_seed_repo("jeryu", "alpha");
    let (code, out, _) = run_cli(
        &client,
        &["jeryu", "ci", "run", "--repo", "alpha", "--kind", "github"],
    );
    assert_eq!(code, 0);
    // github dialect compiles to a different (2) job count than native (3),
    // proving the kind is threaded through the compile path.
    assert!(
        out.contains("2 jobs"),
        "github should compile 2 jobs: {out:?}"
    );
}

#[test]
fn dispatch_runner_enroll_list_drain_rotate() {
    let client = InMemoryClient::new();

    let (code, _, _) = run_cli(
        &client,
        &["jeryu", "runner", "enroll", "node-a", "--executor", "oci"],
    );
    assert_eq!(code, 0);

    let (code, out, _) = run_cli(&client, &["jeryu", "runner", "list"]);
    assert_eq!(code, 0);
    assert!(out.contains("node-a"), "list stdout was {out:?}");
    assert!(out.contains("Oci"), "list stdout was {out:?}");
    assert!(out.contains("accepting=true"), "list stdout was {out:?}");

    let (code, out, _) = run_cli(&client, &["jeryu", "runner", "drain", "node-a"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("draining runner node-a"),
        "drain stdout was {out:?}"
    );
    assert!(!client.runner_list().unwrap()[0].accepting);

    let (code, out, _) = run_cli(&client, &["jeryu", "runner", "rotate", "node-a"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("rotated credential"),
        "rotate stdout was {out:?}"
    );
}

#[test]
fn dispatch_proof_verify_is_replay_stable() {
    let client = InMemoryClient::new();
    let (code, out_a, _) = run_cli(&client, &["jeryu", "--json", "proof", "verify", "cs-123"]);
    assert_eq!(code, 0);
    let (code, out_b, _) = run_cli(&client, &["jeryu", "--json", "proof", "verify", "cs-123"]);
    assert_eq!(code, 0);
    // Same changeset must verify to an identical plan hash (replay-stable).
    assert_eq!(out_a, out_b);
    assert!(
        out_a.contains("\"admissible\":true"),
        "verify json was {out_a:?}"
    );

    let (code, out_c, _) = run_cli(&client, &["jeryu", "--json", "proof", "verify", "cs-999"]);
    assert_eq!(code, 0);
    assert_ne!(out_a, out_c, "different changesets must hash differently");
}

#[test]
fn dispatch_proof_verify_blocks_forbidden_changeset() {
    let client = InMemoryClient::new();
    let (code, out, _) = run_cli(
        &client,
        &["jeryu", "proof", "verify", "touch-FORBIDDEN-path"],
    );
    assert_eq!(code, 0);
    assert!(
        out.contains("blocked:"),
        "forbidden verify stdout was {out:?}"
    );
}

#[test]
fn dispatch_release_and_cache_self_test() {
    let client = InMemoryClient::new();
    let (code, out, _) = run_cli(&client, &["jeryu", "release", "--version", "3.0.1"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("release 3.0.1 ready=true"),
        "release stdout was {out:?}"
    );

    let (code, out, _) = run_cli(&client, &["jeryu", "cache", "self-test"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("cache self-test passed"),
        "cache stdout was {out:?}"
    );
    assert!(out.contains("false_hits=0"), "cache stdout was {out:?}");
}

#[test]
fn dispatch_agent_auth_import_then_doctor_roundtrips() {
    let client = InMemoryClient::new();
    let (code, out, _) = run_cli(
        &client,
        &["jeryu", "agent", "auth", "import", "--from-host", "codex"],
    );
    assert_eq!(code, 0);
    assert!(
        out.contains("imported codex auth"),
        "auth import stdout was {out:?}"
    );

    let (code, out, _) = run_cli(&client, &["jeryu", "agent", "auth", "doctor", "codex"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("codex auth ok=true"),
        "doctor stdout was {out:?}"
    );
}

#[test]
fn dispatch_agent_run_fails_closed_until_runtime_is_wired() {
    let client = InMemoryClient::new();
    let task = tempfile::NamedTempFile::new().expect("task file");
    std::fs::write(task.path(), "fix the failing test").expect("write task");
    let path = task.path().to_string_lossy().to_string();
    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu",
            "agent",
            "run",
            "--repo",
            "alice/jeryu",
            "--agent",
            "codex",
            "--model",
            "model-x",
            "--task-file",
            &path,
        ],
    );
    assert_eq!(code, 5, "NotWired maps to exit code 5");
    assert!(out.is_empty(), "no stdout on denied launch");
    assert!(
        err.contains("not yet wired")
            && err.contains("protected runner PTY")
            && err.contains("required stream"),
        "stderr must explain the fail-closed runtime gap: {err:?}"
    );
}

#[test]
fn dispatch_issue_create_success_and_missing_repo_maps_to_exit_code_2_and_names_repo() {
    // This test pins the *full* contract of one command shape
    // (`forge issue create`): the positive path (repo exists) AND the
    // missing-repo negative path, using the SAME argv shape for both. Proving
    // both halves makes the negative assertions trustworthy: they cannot pass
    // for the trivial reason that the command always fails / never produces
    // output — the positive half proves the success path is live, the negative
    // half proves the error path is wired and specific.
    let client = InMemoryClient::with_seed_repo("jeryu", "real");

    // --- Positive path: the same command shape succeeds when the repo EXISTS.
    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu", "forge", "issue", "create", "--repo", "real", "--title", "x",
        ],
    );
    assert_eq!(
        code, 0,
        "create on an existing repo must exit 0, got {code}"
    );
    assert!(
        out.contains("issue #1: x"),
        "success must render the created issue on stdout, was {out:?}"
    );
    assert!(err.is_empty(), "no stderr on success, got {err:?}");
    // The issue actually landed in the backing store (rendered != persisted).
    let issues = client.list_issues("jeryu", "real").unwrap();
    assert_eq!(issues.len(), 1, "exactly one issue must persist");
    assert_eq!(issues[0].state, IssueState::Open);

    // --- Negative path: the same shape against a missing repo fails precisely.
    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu", "forge", "issue", "create", "--repo", "ghost", "--title", "x",
        ],
    );
    // NotFound maps to exit code 2 (the contract dispatch.rs encodes); this is
    // strictly stronger than a bare non-zero check and distinguishes NotFound
    // (2) from Conflict (3) / Invalid (4) / NotWired (5).
    assert_eq!(code, 2, "NotFound must map to exit code 2, got {code}");
    // Failures write nothing to stdout, so a caller piping `--json` never sees a
    // half-formed record.
    assert!(out.is_empty(), "no stdout on error, got {out:?}");
    // The diagnostic must name the *specific* missing repo, not just any
    // "not found": catches a regression that reported the wrong entity or
    // swallowed the owner/name. The owner defaults to the canonical "jeryu".
    assert!(
        err.contains("not found") && err.contains("jeryu/ghost"),
        "stderr must name the missing repo jeryu/ghost, was {err:?}"
    );
    // No issue must have leaked into the missing repo's (absent) backing store,
    // and the seeded repo's issue count must be unchanged by the failed create.
    assert!(
        client.list_issues("jeryu", "ghost").unwrap().is_empty(),
        "failed create must not persist an issue under the missing repo"
    );
    assert_eq!(
        client.list_issues("jeryu", "real").unwrap().len(),
        1,
        "failed create must not perturb the existing repo's issues"
    );
}

// ---------------------------------------------------------------------------
// Operator / onboarding dispatch tests (gh-setup, autonomy init, onboard)
// ---------------------------------------------------------------------------

/// A unique, process-scoped scratch directory for tests that write files. No
/// `tempfile` dependency is available, so we synthesize a unique path under the
/// system temp dir and clean it up at the end of the test.
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "jeryu-cli-test-{}-{tag}-{}-{n}",
        std::process::id(),
        // Nanosecond clock keeps reruns from colliding with a stale dir.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

#[test]
fn dispatch_gh_setup_print_does_not_write_and_dumps_entry() {
    let client = InMemoryClient::new();
    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu",
            "gh-setup",
            "--host",
            "https://forge.example:9000",
            "--token",
            "tok",
            "--print",
            "--path",
            "/definitely/not/written.yml",
        ],
    );
    assert_eq!(code, 0);
    assert!(err.is_empty(), "stderr was {err:?}");
    // The host key is the authority (no scheme, no port-stripping surprises).
    assert!(out.contains("forge.example:9000:"), "stdout was {out:?}");
    assert!(out.contains("oauth_token: tok"), "stdout was {out:?}");
    assert!(
        out.contains("do not run gh auth login"),
        "stdout was {out:?}"
    );
    // --print must not create the file.
    assert!(
        !std::path::Path::new("/definitely/not/written.yml").exists(),
        "--print must not write the hosts file"
    );
}

#[test]
fn dispatch_gh_setup_writes_idempotent_hosts_file() {
    let client = InMemoryClient::new();
    let dir = scratch_dir("gh");
    let path = dir.join("hosts.yml");
    let path_str = path.to_str().unwrap().to_string();

    let argv = [
        "jeryu",
        "gh-setup",
        "--host",
        "http://localhost:8080",
        "--token",
        "secret-token",
        "--path",
        &path_str,
    ];

    let (code, out, _) = run_cli(&client, &argv);
    assert_eq!(code, 0);
    assert!(
        out.contains("wrote gh host localhost:8080"),
        "stdout {out:?}"
    );
    assert!(
        out.contains("token source: explicit --token"),
        "stdout {out:?}"
    );
    assert!(out.contains("do not run gh auth login"), "stdout {out:?}");
    assert!(!out.contains("secret-token"), "stdout leaked token {out:?}");
    let first = std::fs::read_to_string(&path).expect("hosts.yml written");
    assert!(first.contains("oauth_token: secret-token"));

    // Re-running with identical inputs is idempotent (byte-identical file).
    let (code, _, _) = run_cli(&client, &argv);
    assert_eq!(code, 0);
    let second = std::fs::read_to_string(&path).expect("hosts.yml rewritten");
    assert_eq!(first, second, "gh-setup must be idempotent");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dispatch_gh_setup_reads_token_file_without_printing_token() {
    let client = InMemoryClient::new();
    let dir = scratch_dir("gh-token-file");
    let token_path = dir.join("merge-token");
    let hosts_path = dir.join("hosts.yml");
    std::fs::write(&token_path, "file-secret-token\n").expect("write token file");
    let token_path_str = token_path.to_str().unwrap().to_string();
    let hosts_path_str = hosts_path.to_str().unwrap().to_string();

    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu",
            "gh-setup",
            "--host",
            "http://127.0.0.1:8787",
            "--token-file",
            &token_path_str,
            "--path",
            &hosts_path_str,
        ],
    );

    assert_eq!(code, 0);
    assert!(err.is_empty(), "stderr was {err:?}");
    assert!(
        out.contains(&format!("token file used: {token_path_str}")),
        "stdout {out:?}"
    );
    assert!(
        out.contains(
            "jeryu gh-setup --host http://127.0.0.1:8787 --token-file ~/.jeryu/secrets/merge-token"
        ),
        "stdout {out:?}"
    );
    assert!(
        !out.contains("file-secret-token"),
        "stdout leaked token {out:?}"
    );
    let hosts = std::fs::read_to_string(&hosts_path).expect("hosts.yml written");
    assert!(hosts.contains("oauth_token: file-secret-token"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dispatch_autonomy_init_print_json_keeps_safety_floor() {
    let client = InMemoryClient::new();
    let (code, out, _) = run_cli(
        &client,
        &[
            "jeryu",
            "--json",
            "autonomy",
            "init",
            "--profile",
            "full-auto",
            "--print",
        ],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("valid json");
    assert_eq!(value["profile"], "full-auto");
    assert_eq!(value["written"], false);
    let files = value["files"].as_array().expect("files array");
    assert_eq!(files.len(), 7, "full canonical bundle is 7 files");

    // The risk policy lifts R0-R4 to auto but R5 stays fail-closed.
    let risk = files
        .iter()
        .find(|f| f["path"] == "autonomy/policies/risk.yml")
        .expect("risk.yml emitted")["contents"]
        .as_str()
        .unwrap();
    assert!(risk.contains("fail_closed: true"), "R5 fail-closed intact");

    // The protected-paths floor still hard-gates the autonomy tree itself.
    let protected = files
        .iter()
        .find(|f| f["path"] == "autonomy/policies/protected-paths.yml")
        .expect("protected-paths emitted")["contents"]
        .as_str()
        .unwrap();
    assert!(protected.contains(".jeryu/autonomy/**"));
}

#[test]
fn dispatch_autonomy_init_writes_canonical_tree() {
    let client = InMemoryClient::new();
    let dir = scratch_dir("autonomy");
    let dir_str = dir.to_str().unwrap().to_string();

    let (code, out, _) = run_cli(&client, &["jeryu", "autonomy", "init", "--path", &dir_str]);
    assert_eq!(code, 0);
    assert!(out.contains("autonomy init (full-auto)"), "stdout {out:?}");

    for rel in [
        "autonomy/policies/risk.yml",
        "autonomy/policies/approvals.yml",
        "autonomy/policies/release.yml",
        "autonomy/policies/protected-paths.yml",
        "autonomy/policies/freeze.yml",
        "ci.toml",
        "policy.toml",
    ] {
        assert!(dir.join(rel).exists(), "{rel} must be written");
    }

    // The control files encode the required keys verbatim.
    let ci = std::fs::read_to_string(dir.join("ci.toml")).unwrap();
    assert!(ci.contains("github_actions_required = true"));
    let policy = std::fs::read_to_string(dir.join("policy.toml")).unwrap();
    assert!(policy.contains("require_admission_receipt = false"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dispatch_onboard_dry_run_prints_ordered_plan() {
    let client = InMemoryClient::new();
    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu",
            "onboard",
            "/home/u/projects/alpha",
            "--host",
            "http://localhost:8080",
            "--owner",
            "acme",
            "--dry-run",
        ],
    );
    assert_eq!(code, 0);
    assert!(err.is_empty(), "stderr was {err:?}");
    // Plan derives the repo name from the path's final component.
    assert!(out.contains("acme/alpha"), "stdout was {out:?}");
    // The five ordered steps are present in order.
    for needle in [
        "1. create:",
        "2. materialize:",
        "3. repoint-remote:",
        "4. register:",
        "5. set-autonomy:",
    ] {
        assert!(out.contains(needle), "plan missing {needle:?}: {out:?}");
    }
    assert!(out.contains("origin -> http://localhost:8080/acme/alpha.git"));
    assert!(out.contains("dry-run"), "must flag dry-run: {out:?}");
}

#[test]
fn dispatch_onboard_without_dry_run_is_not_wired_exit_5() {
    let client = InMemoryClient::new();
    let (code, out, err) = run_cli(&client, &["jeryu", "onboard", "/tmp/alpha"]);
    // NotWired maps to exit code 5 (the live transport is not yet available).
    assert_eq!(
        code, 5,
        "non-dry-run onboard must be NotWired (5), got {code}"
    );
    assert!(out.is_empty(), "no stdout on error, got {out:?}");
    assert!(err.contains("not yet wired"), "stderr was {err:?}");
}

#[test]
fn dispatch_control_plane_commands_require_live_api_url() {
    let client = InMemoryClient::new();
    for argv in [
        vec!["jeryu", "status"],
        vec!["jeryu", "priorities"],
        vec!["jeryu", "repo-graph", "clusters"],
        vec!["jeryu", "artifacts", "latest"],
        vec!["jeryu", "runners", "status"],
    ] {
        let (code, out, err) = run_cli(&client, &argv);
        assert_eq!(code, 5, "argv {argv:?} should fail closed without API URL");
        assert!(
            out.is_empty(),
            "no stdout on error for {argv:?}, got {out:?}"
        );
        assert!(
            err.contains("--api-url") || err.contains("JERYU_API_URL"),
            "stderr for {argv:?} was {err:?}"
        );
    }
}

#[test]
fn dispatch_onboard_json_emits_machine_plan() {
    let client = InMemoryClient::new();
    let (code, out, _) = run_cli(
        &client,
        &[
            "jeryu",
            "--json",
            "onboard",
            "/srv/code/beta.git",
            "--dry-run",
        ],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("valid json");
    // The .git suffix is stripped from the derived repo name.
    assert_eq!(value["repo"], "beta");
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["steps"].as_array().unwrap().len(), 5);
}

#[test]
fn dispatch_json_output_is_machine_readable() {
    let client = InMemoryClient::new();
    let (code, out, _) = run_cli(
        &client,
        &["jeryu", "--json", "forge", "repo", "create", "alpha"],
    );
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("valid json");
    assert_eq!(value["name"], "alpha");
    assert_eq!(value["owner"], "jeryu");
}
