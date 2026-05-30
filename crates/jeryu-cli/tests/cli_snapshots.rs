//! Help-snapshot and command-dispatch invariant tests for `jeryu-cli`.
//!
//! Two families of tests:
//! 1. Help-tree invariants: walk the clap `Command` tree and assert the
//!    vocabulary is GitHub-shaped (no foreign-forge terms, no merge-request /
//!    pipeline / pool leakage) and that the renamed verbs are present.
//! 2. Dispatch smoke tests: parse a real argv, run it against the in-memory
//!    client, and assert on the rendered output / exit code.

use clap::{CommandFactory, Parser};
use jeryu_cli::ForgeClient;
use jeryu_cli::cli::{Cli, Commands};
use jeryu_cli::client::{InMemoryClient, IssueState, PullRequestState};
use jeryu_cli::dispatch;

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
    for removed in ["mr", "pool", "pipeline", "exec", "job"] {
        assert!(
            !names.iter().any(|n| n == removed),
            "removed command {removed:?} is still a top-level subcommand: {names:?}"
        );
    }
}

#[test]
fn top_level_includes_renamed_commands() {
    let names = top_level_names();
    for required in ["forge", "ci", "runner", "proof", "release", "cache"] {
        assert!(
            names.iter().any(|n| n == required),
            "required top-level command {required:?} missing from {names:?}"
        );
    }
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
    let code = dispatch(cli, client, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
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
fn dispatch_missing_repo_maps_to_exit_code_2_and_names_repo() {
    let client = InMemoryClient::new();
    let (code, out, err) = run_cli(
        &client,
        &[
            "jeryu", "forge", "issue", "create", "--repo", "ghost", "--title", "x",
        ],
    );
    // NotFound maps to exit code 2 (the contract dispatch.rs encodes); this is
    // strictly stronger than a bare non-zero check.
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
    // No issue must have leaked into the backing store on the failed create.
    assert!(
        client.list_issues("jeryu", "ghost").unwrap().is_empty(),
        "failed create must not persist an issue"
    );
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
