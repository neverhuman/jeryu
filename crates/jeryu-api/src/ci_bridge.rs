//! Push -> CI bridge.
//!
//! When a push lands a new commit on a branch, read its GitHub-Actions
//! workflows from the bare repo, compile them, **execute** each job's steps in
//! the real sandboxed runner, and record a check-run with the actual result so
//! the autonomy gate has live CI state for the pushed commit. Execution runs
//! synchronously on the blocking pool (the caller holds the receive-pack
//! response until it finishes), so a `git push` produces real green/red CI.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jeryu_ci_compiler::{CiKind, CompileContext, Compiler};
use jeryu_core::{CheckConclusion, CheckRunStatus, CreateCheckRunRequest, ForgeCore};
use jeryu_gitd::RepoManager;
use jeryu_gitd::refs::GitRef;
use jeryu_runner_core::JobRequest as CoreJobRequest;
use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::receipt::ReceiptStatus;
use jeryu_runner_core::trust::{RunnerClass, TrustTier};
use jeryu_runnerd::submit as submit_runner_job;

/// All-zero oid: a ref delete, which carries no commit to build.
const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// A branch ref whose tip a push moved to a new commit.
pub(crate) struct RefUpdate {
    pub new_oid: String,
}

/// Branch refs whose tip changed between two ref snapshots (new branches are
/// treated as updates; deletes and tags are ignored).
pub(crate) fn ref_updates(before: &[GitRef], after: &[GitRef]) -> Vec<RefUpdate> {
    after
        .iter()
        .filter(|r| r.name.starts_with("refs/heads/") && r.oid != ZERO_OID)
        .filter(|r| {
            before
                .iter()
                .find(|b| b.name == r.name)
                .map(|b| b.oid != r.oid)
                .unwrap_or(true)
        })
        .map(|r| RefUpdate {
            new_oid: r.oid.clone(),
        })
        .collect()
}

/// For each updated commit, compile its workflows, run each job in the sandbox,
/// and record a completed check-run with the real conclusion.
pub(crate) fn on_push(
    core: &ForgeCore,
    manager: &RepoManager,
    owner: &str,
    repo: &str,
    updates: &[RefUpdate],
) {
    // The smart-HTTP URL carries the `.git` suffix; the forge repo name does not.
    let repo = repo.trim_end_matches(".git");
    let Ok(resolved) = manager.resolve_parts(owner, repo) else {
        return;
    };
    let git_bin = manager.config().git_bin.clone();
    for update in updates {
        // Accumulate this head's recorded check-runs so the autonomy bridge can
        // run the evidence-gate judge over the live CI state once they all land.
        let mut ci_checks: Vec<(String, Option<CheckConclusion>)> = Vec::new();
        for (file, content) in read_workflows(&git_bin, &resolved.path, &update.new_oid) {
            let context = CompileContext::new(format!("{owner}/{repo}"), update.new_oid.clone());
            let Ok(pipeline) = Compiler::compile(&content, CiKind::GitHubActions, context) else {
                continue;
            };
            for job in &pipeline.jobs {
                let conclusion =
                    run_job(&git_bin, &resolved.path, &update.new_oid, job, owner, repo);
                let name = format!("{}/{}", workflow_stem(&file), job.name);
                let _ = core.create_check_run(
                    owner,
                    repo,
                    CreateCheckRunRequest {
                        name: name.clone(),
                        head_sha: update.new_oid.clone(),
                        status: Some(CheckRunStatus::Completed),
                        conclusion: Some(conclusion.clone()),
                        ..Default::default()
                    },
                );
                ci_checks.push((name, Some(conclusion)));
            }
        }
        // Agent-reviewed auto-merge: with the head's CI state recorded, let the
        // autonomy bridge judge it and merge an open PR on AllowMerge (R5 /
        // red-CI / hard-stops stay human). Best-effort; never fails the push.
        let changed = changed_paths(&git_bin, &resolved.path, &update.new_oid);
        crate::autonomy_bridge::evaluate_pushed_head(
            core,
            owner,
            repo,
            &update.new_oid,
            &ci_checks,
            &changed,
        );
    }
}

/// Files changed by `oid` relative to its first parent (root commit → all
/// files in its tree). Feeds the autonomy bridge's risk classifier.
fn changed_paths(git_bin: &str, bare: &Path, oid: &str) -> Vec<String> {
    let bare = bare.to_string_lossy().to_string();
    let parent = format!("{oid}^");
    // `git diff --name-only <oid>^ <oid>` for a normal commit; fall back to the
    // full tree listing for a root commit (no parent).
    let out = std::process::Command::new(git_bin)
        .args(["-C", &bare, "diff", "--name-only", &parent, oid])
        .output();
    let listing = match out {
        Ok(o) if o.status.success() => o.stdout,
        _ => match std::process::Command::new(git_bin)
            .args(["-C", &bare, "ls-tree", "-r", "--name-only", oid])
            .output()
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => return Vec::new(),
        },
    };
    String::from_utf8_lossy(&listing)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Execute a compiled job's `run` steps in the sandboxed runner and map the
/// receipt to a check-run conclusion.
fn run_job(
    git_bin: &str,
    bare: &Path,
    oid: &str,
    job: &jeryu_ci_ir::Job,
    owner: &str,
    repo: &str,
) -> CheckConclusion {
    let script = job
        .steps
        .iter()
        .filter_map(|step| step.command.clone())
        .collect::<Vec<_>>()
        .join("\n");
    if script.trim().is_empty() {
        // Action-only job with no executable shell step.
        return CheckConclusion::Skipped;
    }
    let Ok(workspace) = checkout_commit(git_bin, bare, oid) else {
        return CheckConclusion::Failure;
    };
    let request = CoreJobRequest {
        job_id: format!("{owner}-{repo}-{}", job.id),
        repo_id: format!("{owner}/{repo}"),
        commit_sha: oid.to_string(),
        workspace: workspace.clone(),
        command: "/bin/sh".to_string(),
        args: vec!["-lc".to_string(), script],
        env: BTreeMap::new(),
        trust_tier: TrustTier::T2InternalBranch,
        requested_runner: Some(RunnerClass::NativeRustClean),
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::None,
        token_policy: TokenPolicy::None,
        timeout_ms: 600_000,
        fork: false,
    };
    let receipt = submit_runner_job(request);
    let _ = std::fs::remove_dir_all(&workspace);
    match receipt.status {
        ReceiptStatus::Passed => CheckConclusion::Success,
        _ => CheckConclusion::Failure,
    }
}

/// Extract a commit's tree into a fresh workspace via `git archive | tar -x`.
fn checkout_commit(git_bin: &str, bare: &Path, oid: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let workspace = std::env::temp_dir().join(format!("jeryu-ci-{oid}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace)?;
    let archive = std::process::Command::new(git_bin)
        .args(["-C", &bare.to_string_lossy(), "archive", oid])
        .output()?;
    if archive.status.success() && !archive.stdout.is_empty() {
        let mut tar = std::process::Command::new("tar")
            .args(["-x", "-C", &workspace.to_string_lossy()])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = tar.stdin.take() {
            stdin.write_all(&archive.stdout)?;
        }
        let _ = tar.wait();
    }
    Ok(workspace)
}

fn workflow_stem(file: &str) -> &str {
    file.trim_end_matches(".yaml").trim_end_matches(".yml")
}

/// Read `.github/workflows/*.{yml,yaml}` from `oid` in a bare repo via `git`.
fn read_workflows(git_bin: &str, bare: &Path, oid: &str) -> Vec<(String, String)> {
    let bare = bare.to_string_lossy().to_string();
    let tree = format!("{oid}:.github/workflows");
    let Ok(listing) = std::process::Command::new(git_bin)
        .args(["-C", &bare, "ls-tree", "--name-only", &tree])
        .output()
    else {
        return Vec::new();
    };
    if !listing.status.success() {
        return Vec::new();
    }
    let mut workflows = Vec::new();
    for name in String::from_utf8_lossy(&listing.stdout).lines() {
        if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
            continue;
        }
        let spec = format!("{oid}:.github/workflows/{name}");
        if let Ok(blob) = std::process::Command::new(git_bin)
            .args(["-C", &bare, "show", &spec])
            .output()
            && blob.status.success()
        {
            workflows.push((
                name.to_string(),
                String::from_utf8_lossy(&blob.stdout).to_string(),
            ));
        }
    }
    workflows
}
