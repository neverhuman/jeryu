//! Push -> CI bridge.
//!
//! When a push lands a new commit on a branch, read its GitHub-Actions
//! workflows from the bare repo, compile each, and create check-runs in the
//! forge so the autonomy gate has real CI state for that commit. Executing the
//! compiled jobs (turning check-runs green/red) is the runner's job; this
//! bridge owns the compile-and-register step that a `git push` triggers.

use jeryu_ci_compiler::{CiKind, CompileContext, Compiler};
use jeryu_core::{CheckRunStatus, CreateCheckRunRequest, ForgeCore};
use jeryu_gitd::RepoManager;
use jeryu_gitd::refs::GitRef;

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

/// For each updated commit, compile its workflows and register check-runs.
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
        for (file, content) in read_workflows(&git_bin, &resolved.path, &update.new_oid) {
            let context = CompileContext::new(format!("{owner}/{repo}"), update.new_oid.clone());
            let Ok(pipeline) = Compiler::compile(&content, CiKind::GitHubActions, context) else {
                continue;
            };
            for job in &pipeline.jobs {
                let _ = core.create_check_run(
                    owner,
                    repo,
                    CreateCheckRunRequest {
                        name: format!("{}/{}", workflow_stem(&file), job.name),
                        head_sha: update.new_oid.clone(),
                        status: Some(CheckRunStatus::Queued),
                        ..Default::default()
                    },
                );
            }
        }
    }
}

fn workflow_stem(file: &str) -> &str {
    file.trim_end_matches(".yaml").trim_end_matches(".yml")
}

/// Read `.github/workflows/*.{yml,yaml}` from `oid` in a bare repo via `git`.
fn read_workflows(git_bin: &str, bare: &std::path::Path, oid: &str) -> Vec<(String, String)> {
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
