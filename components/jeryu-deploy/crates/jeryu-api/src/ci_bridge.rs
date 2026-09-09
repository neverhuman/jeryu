//! Push -> CI bridge.
//!
//! When a push lands a new commit on a branch, read its GitHub Actions
//! workflows from the bare repo, compile them, **execute** each job's steps in
//! the real sandboxed runner, and record a check-run with the actual result so
//! the autonomy gate has live CI state for the pushed commit. Execution runs
//! synchronously on the blocking pool (the caller holds the receive-pack
//! response until it finishes), so a `git push` produces real green/red CI.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use jeryu_ci_compiler::{CiKind, CompileContext, Compiler};
use jeryu_core::{
    CheckConclusion, CheckRunStatus, CreateCheckRunRequest, ForgeCore, RecordJankuraiScoreRequest,
};
use jeryu_gitd::RepoManager;
use jeryu_gitd::refs::GitRef;
use jeryu_runner_core::JobRequest as CoreJobRequest;
use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::receipt::ReceiptStatus;
use jeryu_runner_core::trust::{RunnerClass, TrustTier};
use jeryu_runnerd::submit as submit_runner_job;
use sha2::{Digest, Sha256};

/// All-zero oid: a ref delete, which carries no commit to build.
const ZERO_OID: &str = "0000000000000000000000000000000000000000";
const HOST_JANKURAI_MINIMUM_SCORE: u32 = 85;

/// A branch ref whose tip a push moved to a new commit.
pub(crate) struct RefUpdate {
    pub ref_name: String,
    pub old_oid: String,
    pub new_oid: String,
}

/// Branch refs whose tip changed between two ref snapshots (new branches are
/// treated as updates; deletes and tags are ignored).
pub(crate) fn ref_updates(before: &[GitRef], after: &[GitRef]) -> Vec<RefUpdate> {
    after
        .iter()
        .filter(|r| r.name.starts_with("refs/heads/") && r.oid != ZERO_OID)
        .filter_map(|r| {
            let previous_oid = before
                .iter()
                .find(|b| b.name == r.name)
                .map(|b| b.oid.clone());
            match &previous_oid {
                Some(previous) if *previous == r.oid => None,
                _ => Some(RefUpdate {
                    ref_name: r.name.clone(),
                    old_oid: previous_oid.unwrap_or_else(|| ZERO_OID.to_owned()),
                    new_oid: r.oid.clone(),
                }),
            }
        })
        .collect()
}

/// For each updated commit, compile its workflows, run each job in the sandbox,
/// and record a completed check-run with the real conclusion.
/// The bridge must preserve Git refs: version and changelog changes belong in
/// the reviewed candidate, before its checks and approval bind the exact head.
pub(crate) fn on_push(
    core: &ForgeCore,
    manager: &RepoManager,
    owner: &str,
    repo: &str,
    updates: &[RefUpdate],
    origin_base_url: &str,
) {
    // The smart-HTTP URL carries the `.git` suffix; the forge repo name does not.
    let repo = repo.trim_end_matches(".git");
    let Ok(resolved) = manager.resolve_parts(owner, repo) else {
        return;
    };
    let git_bin = manager.config().git_bin.clone();
    let origin_url = resolved.path.to_string_lossy().to_string();
    for update in updates {
        if let Some(branch) = update.ref_name.strip_prefix("refs/heads/") {
            let _ = core.refresh_pull_request_heads_for_ref(owner, repo, branch, &update.new_oid);
        }
        // Compute the host-authoritative jankurai diff-score for every changed
        // branch head and publish `jankurai/proof` from that result. Push
        // transport, merge, and seeded PR-head exports all route here; failures
        // stay visibly red or unproven rather than becoming synthetic success.
        record_authoritative_jankurai_score(core, &git_bin, &resolved.path, owner, repo, update);
        // Accumulate this head's recorded check-runs so the autonomy bridge can
        // run the evidence-gate judge over the live CI state once they all land.
        let mut ci_checks: Vec<(String, Option<CheckConclusion>)> = Vec::new();
        for (file, content) in read_workflows(&git_bin, &resolved.path, &update.new_oid) {
            // The forge does not execute GitHub Actions runners, so it does not execute these
            // workflows: they run on the GitHub mirror's real runners, and the forge
            // PR gate is host-ci's comprehensive `jeryu/ci` (ops/ci/pr-ci.sh). Seeding
            // them here only produced all-red check-runs that misrepresent CI. Seed
            // synthetic conclusions ONLY under JERYU_CI_MOCK — the in-process
            // CI-seeding-flow tests (workcell export) that assert a recorded check-run.
            if !ci_mock_enabled() {
                continue;
            }
            if !workflow_runs_for_branch_head(&content, &update.ref_name) {
                continue;
            }
            let context = CompileContext::new(format!("{owner}/{repo}"), update.new_oid.clone());
            let Ok(pipeline) = Compiler::compile(&content, CiKind::GitHubActions, context) else {
                continue;
            };
            let job_context = CiJobContext {
                git_bin: &git_bin,
                bare: &resolved.path,
                oid: &update.new_oid,
                origin_url: &origin_url,
                origin_base_url,
                owner,
                repo,
                ref_name: &update.ref_name,
            };
            for job in &pipeline.jobs {
                let conclusion = run_job(&job_context, job);
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
        // Record-only autonomy verdict: with the head's CI state recorded, let
        // the autonomy bridge judge it and write an advisory check-run. The
        // bridge never merges; best-effort, so it never fails the push.
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

/// Seed CI for a PR head that was created without going through the git push
/// transport. This preserves GitHub-like parity for branch exports and other
/// local PR creation paths: if the head already has check-runs, leave them
/// alone; otherwise reuse the push bridge to compile workflows and record the
/// real check-runs for the new head.
pub(crate) fn seed_pull_request_head(
    core: &ForgeCore,
    manager: &RepoManager,
    owner: &str,
    repo: &str,
    ref_name: &str,
    head_sha: &str,
    origin_base_url: &str,
) {
    if core
        .list_check_runs(owner, repo, Some(head_sha))
        .map(|runs| runs.total_count > 0)
        .unwrap_or(false)
    {
        return;
    }
    let update = RefUpdate {
        ref_name: ref_name.to_string(),
        old_oid: ZERO_OID.to_string(),
        new_oid: head_sha.to_string(),
    };
    on_push(core, manager, owner, repo, &[update], origin_base_url);
}

pub(crate) fn default_origin_base_url() -> String {
    std::env::var("JERYU_BASE")
        .ok()
        .filter(|host| !host.trim().is_empty())
        .map(|host| format!("http://{host}"))
        .unwrap_or_else(|| "http://127.0.0.1:8787".to_string())
}

fn run_git_status(git_bin: &str, cwd: Option<&Path>, args: &[&str]) -> bool {
    let mut command = Command::new(git_bin);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_git_stdout(git_bin: &str, cwd: Option<&Path>, args: &[&str]) -> Option<String> {
    let mut command = Command::new(git_bin);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn write_empty_tree(git_bin: &str, repo: &Path) -> Option<String> {
    let output = Command::new(git_bin)
        .args(["hash-object", "-t", "tree", "-w", "--stdin"])
        .current_dir(repo)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let oid = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!oid.is_empty()).then_some(oid)
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

/// jeryu-managed fallback audit policy, written into the throwaway worktree when a
/// pushed head carries none of its own — "forced scoring for unconfigured repos".
/// Mirrors jeryu-tool/policy/default-audit-policy.toml; the `required_tool_version`
/// is kept in lockstep with tool-manifest.toml by ops/render-tool-manifest.sh.
const DEFAULT_AUDIT_POLICY_TOML: &str = r#"schema_version = "1.0.0"
workspace = "unconfigured"
minimum_score = 85
hard_findings_allowed = 0
required_tool = "jankurai"
required_tool_version = "1.6.11"

[scan]
excluded_paths = [".jankurai/", "apps/web/dist/"]
"#;

const GOVERNED_JANKURAI_PATH: &str = "/home/ubuntu/.jeryu/bin/jankurai";
const GOVERNED_JANKURAI_RECEIPT_DIR: &str = "/home/ubuntu/.jeryu/receipts/jankurai/sha256";
const GOVERNED_JANKURAI_VERSION: &str = "jankurai 1.6.11";
const GOVERNED_JANKURAI_SHA256: &str =
    "9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c";
const GOVERNED_JANKURAI_SOURCE_REPO: &str = "http://127.0.0.1:8787/git/jeryu/jankurai.git";
const GOVERNED_JANKURAI_SOURCE_TAG: &str = "v1.6.11-deadlang-precision-split.3";
const GOVERNED_JANKURAI_SOURCE_REV: &str = "b88562fdb124aa86dedd70ab972e7d0d87e58be1";
const GOVERNED_JANKURAI_SOURCE_TREE: &str = "611229e54938c0e8808896e369fd54d095d258f7";
const GOVERNED_JANKURAI_SOURCE_ARCHIVE_SHA256: &str =
    "903a231eca8f6a1f050953b603d5a278a1606abcdf47434eb1b45262d74068aa";
const GOVERNED_JANKURAI_CARGO_LOCK_SHA256: &str =
    "b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a";
const GOVERNED_JANKURAI_RUSTC_VERSION: &str = "rustc 1.95.0 (59807616e 2026-04-14)";
const GOVERNED_JANKURAI_CARGO_VERSION: &str = "cargo 1.95.0 (f2d3ce0bd 2026-03-21)";
const GOVERNED_JANKURAI_TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
const GOVERNED_JANKURAI_BUILD_MODE: &str = "oci-vendor-locked-offline-workspace-member-v2";
const GOVERNED_JANKURAI_INSTALLATION_RECEIPT_JSON: &str =
    include_str!("../../../images/agent-sandbox/jankurai-installation-receipt.json");
const GOVERNED_JANKURAI_MANIFEST_REPO: &str = "http://127.0.0.1:8787/git/jeryu/jeryu-tool.git";
const GOVERNED_JANKURAI_MANIFEST_COMMIT: &str = "e8218f9f39bf38277646f31daf2f2ee56e9a7eff";
const GOVERNED_JANKURAI_MANIFEST_TREE: &str = "dae370c78d9e71123bf239ef06add4ee4d04e34e";
const GOVERNED_JANKURAI_MANIFEST_SHA256: &str =
    "be001dc52c66da5669167f3e429d882184931baa3d7a0e53b605c17425872b5a";

mod jankurai;

use jankurai::record_authoritative_jankurai_score;
#[cfg(test)]
use jankurai::*;

/// Execute a compiled job's `run` steps in the sandboxed runner and map the
/// receipt to a check-run conclusion.
struct CiJobContext<'a> {
    git_bin: &'a str,
    bare: &'a Path,
    oid: &'a str,
    origin_url: &'a str,
    origin_base_url: &'a str,
    owner: &'a str,
    repo: &'a str,
    ref_name: &'a str,
}

fn run_job(context: &CiJobContext<'_>, job: &jeryu_ci_ir::Job) -> CheckConclusion {
    let script = job
        .steps
        .iter()
        .filter_map(|step| step.command.as_deref())
        .filter(|command| !is_workflow_toolchain_bootstrap(command))
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    if ci_mock_enabled() {
        return if script.trim().is_empty() {
            CheckConclusion::Skipped
        } else {
            CheckConclusion::Success
        };
    }
    if script.trim().is_empty() {
        // Action-only job with no executable shell step.
        return CheckConclusion::Skipped;
    }
    let Ok(workspace) = checkout_commit(
        context.git_bin,
        context.bare,
        context.oid,
        context.origin_url,
    ) else {
        return CheckConclusion::Failure;
    };
    let mut env = BTreeMap::new();
    env.insert(
        "GITHUB_SERVER_URL".to_string(),
        context.origin_base_url.trim_end_matches('/').to_string(),
    );
    env.insert(
        "GITHUB_REPOSITORY".to_string(),
        format!("{}/{}", context.owner, context.repo),
    );
    env.insert("GITHUB_REF".to_string(), context.ref_name.to_string());
    env.insert("GITHUB_SHA".to_string(), context.oid.to_string());
    env.insert("CARGO_HOME".to_string(), "/home/ubuntu/.cargo".to_string());
    env.insert("CARGO_NET_OFFLINE".to_string(), "true".to_string());
    env.insert("JERYU_CI_USE_SCCACHE".to_string(), "0".to_string());
    let request = CoreJobRequest {
        job_id: format!("{}-{}-{}", context.owner, context.repo, job.id),
        repo_id: format!("{}/{}", context.owner, context.repo),
        commit_sha: context.oid.to_string(),
        workspace: workspace.clone(),
        command: "/bin/sh".to_string(),
        args: vec!["-lc".to_string(), script],
        env,
        trust_tier: TrustTier::T2InternalBranch,
        requested_runner: Some(RunnerClass::NativeRustClean),
        network_policy: NetworkPolicy::EgressOnly,
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

/// Materialize a pushed commit as a real Git checkout.
///
/// Split-repo CI expects `.git`, an HTTP `origin`, and a fetchable
/// `origin/main`, so this deliberately uses clone/fetch rather than a tar
/// archive.
fn checkout_commit(
    git_bin: &str,
    bare: &Path,
    oid: &str,
    origin_url: &str,
) -> std::io::Result<PathBuf> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let workspace =
        std::env::temp_dir().join(format!("jeryu-ci-{oid}-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    run_git(
        git_bin,
        &[
            "clone",
            "--no-checkout",
            &bare.to_string_lossy(),
            &workspace.to_string_lossy(),
        ],
    )?;
    run_git(
        git_bin,
        &[
            "-C",
            &workspace.to_string_lossy(),
            "remote",
            "set-url",
            "origin",
            origin_url,
        ],
    )?;
    run_git(
        git_bin,
        &[
            "-C",
            &workspace.to_string_lossy(),
            "fetch",
            "--force",
            "origin",
            "+refs/heads/main:refs/remotes/origin/main",
        ],
    )?;
    run_git(
        git_bin,
        &[
            "-C",
            &workspace.to_string_lossy(),
            "checkout",
            "--detach",
            oid,
        ],
    )?;
    Ok(workspace)
}

fn run_git(git_bin: &str, args: &[&str]) -> std::io::Result<()> {
    let output = std::process::Command::new(git_bin).args(args).output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(std::io::Error::other(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    )))
}

fn workflow_stem(file: &str) -> &str {
    file.trim_end_matches(".yaml").trim_end_matches(".yml")
}

fn workflow_runs_for_branch_head(content: &str, ref_name: &str) -> bool {
    let Some(branch) = ref_name.strip_prefix("refs/heads/") else {
        return false;
    };
    let Some(on_block) = on_block(content) else {
        return false;
    };
    on_block_has_trigger(&on_block, "pull_request")
        || branch_push_trigger_matches(&on_block, branch)
}

fn is_workflow_toolchain_bootstrap(command: &str) -> bool {
    let mut saw_rustup = false;
    for line in command.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with("rustup toolchain install ") {
            return false;
        }
        saw_rustup = true;
    }
    saw_rustup
}

fn ci_mock_enabled() -> bool {
    mock_flag_set(std::env::var("JERYU_CI_MOCK").ok().as_deref())
}

/// Pure mock-flag predicate. The forge seeds `.github/workflows` check-runs ONLY
/// when this is set — the in-process CI-seeding-flow tests opt in via
/// `JERYU_CI_MOCK`. The production forge leaves it unset and seeds nothing (it has
/// no GitHub Actions runners; host-ci's `jeryu/ci` is the gate). Pure so it is
/// unit-tested without mutating shared process env.
fn mock_flag_set(value: Option<&str>) -> bool {
    matches!(value, Some(v) if { let v = v.trim(); !v.is_empty() && v != "0" })
}

#[derive(Debug, Clone)]
struct WorkflowLine {
    indent: usize,
    text: String,
}

fn on_block(content: &str) -> Option<Vec<WorkflowLine>> {
    let lines = workflow_lines(content);
    let (index, line) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.text == "on:" || line.text.starts_with("on:"))?;
    if let Some(inline) = line.text.strip_prefix("on:") {
        let inline = inline.trim();
        if !inline.is_empty() {
            return Some(vec![WorkflowLine {
                indent: line.indent + 2,
                text: inline.to_string(),
            }]);
        }
    }
    let on_indent = line.indent;
    Some(
        lines
            .into_iter()
            .skip(index + 1)
            .take_while(|child| child.indent > on_indent)
            .collect(),
    )
}

fn workflow_lines(content: &str) -> Vec<WorkflowLine> {
    content
        .lines()
        .filter_map(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let uncommented = trimmed
                .split_once(" #")
                .map(|(before, _)| before.trim())
                .unwrap_or(trimmed);
            if uncommented.is_empty() {
                return None;
            }
            Some(WorkflowLine {
                indent: raw.len() - raw.trim_start().len(),
                text: uncommented.to_string(),
            })
        })
        .collect()
}

fn on_block_has_trigger(block: &[WorkflowLine], trigger: &str) -> bool {
    block.iter().any(|line| {
        trigger_tokens(&line.text)
            .iter()
            .any(|token| token == trigger)
    })
}

fn branch_push_trigger_matches(block: &[WorkflowLine], branch: &str) -> bool {
    if block.iter().any(|line| {
        let text = line.text.trim();
        if text == "push:" || text.starts_with("push:") {
            return false;
        }
        trigger_tokens(&line.text)
            .iter()
            .any(|token| token == "push")
    }) {
        return true;
    }

    let Some((index, push)) = block
        .iter()
        .enumerate()
        .find(|(_, line)| line.text == "push" || line.text.starts_with("push:"))
    else {
        return false;
    };
    let inline = push.text.strip_prefix("push:").map(str::trim).unwrap_or("");
    if !inline.is_empty() {
        return trigger_tokens(inline).iter().any(|token| token == "push");
    }

    let children: Vec<_> = block
        .iter()
        .skip(index + 1)
        .take_while(|line| line.indent > push.indent)
        .cloned()
        .collect();
    if children.is_empty() {
        return true;
    }

    let branches = trigger_patterns(&children, "branches");
    if !branches.is_empty() {
        return branches.iter().any(|pattern| glob_match(pattern, branch));
    }

    let ignored = trigger_patterns(&children, "branches-ignore");
    if ignored.iter().any(|pattern| glob_match(pattern, branch)) {
        return false;
    }

    let tags = trigger_patterns(&children, "tags");
    if !tags.is_empty() {
        return false;
    }

    true
}

fn trigger_patterns(block: &[WorkflowLine], key: &str) -> Vec<String> {
    let mut patterns = Vec::new();
    for (index, line) in block.iter().enumerate() {
        if line.text == key || line.text.starts_with(&format!("{key}:")) {
            if let Some(inline) = line.text.strip_prefix(&format!("{key}:")) {
                patterns.extend(trigger_tokens(inline));
            }
            patterns.extend(
                block
                    .iter()
                    .skip(index + 1)
                    .take_while(|child| child.indent > line.indent)
                    .filter_map(|child| child.text.strip_prefix("- ").map(str::trim))
                    .flat_map(trigger_tokens),
            );
        }
    }
    patterns
}

fn trigger_tokens(raw: &str) -> Vec<String> {
    raw.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|part| {
            part.trim()
                .trim_start_matches("- ")
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches(':')
                .trim()
                .to_string()
        })
        .filter(|part| !part.is_empty())
        .collect()
}

fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern == value {
        return true;
    }
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index) = (0, 0);
    let mut star = None;
    let mut match_index = 0;
    while value_index < value.len() {
        if pattern_index < pattern.len() && pattern[pattern_index] == value[value_index] {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            match_index = value_index;
            pattern_index += 1;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            match_index += 1;
            value_index = match_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
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

#[cfg(test)]
mod tests;
