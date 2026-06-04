use std::path::Path;
use std::process::Command;

use axum::http::StatusCode;
use axum::response::Response as AxumResponse;

use crate::web::workcells_support::{TypedError, typed_error};

use super::source::normalize_pr_base;
use super::{AgentRunResult, boxed_response};

pub(super) fn prepare_workspace(
    git_bin: &str,
    origin: Option<&Path>,
    workspace_root: &Path,
    base_ref: &str,
    source_kind: &str,
) -> AgentRunResult<()> {
    if workspace_root.exists() {
        std::fs::remove_dir_all(workspace_root).map_err(|err| {
            boxed_response(git_error(
                "agent_run_workspace_prepare_failed",
                format!(
                    "remove existing workspace {}: {err}",
                    workspace_root.display()
                ),
            ))
        })?;
    }
    if let Some(parent) = workspace_root.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            boxed_response(git_error(
                "agent_run_workspace_prepare_failed",
                format!("create workspace parent {}: {err}", parent.display()),
            ))
        })?;
    }
    match source_kind {
        "scratch" => {
            run_git(
                git_bin,
                &["init", workspace_root.to_string_lossy().as_ref()],
                None,
            )?;
        }
        _ => {
            let origin = origin.ok_or_else(|| {
                boxed_response(git_error(
                    "agent_run_workspace_prepare_failed",
                    "missing clone origin for non-scratch workspace".to_string(),
                ))
            })?;
            run_git(
                git_bin,
                &[
                    "clone",
                    "--no-checkout",
                    origin.to_string_lossy().as_ref(),
                    workspace_root.to_string_lossy().as_ref(),
                ],
                None,
            )?;
            let normalized = normalize_pr_base(base_ref.to_string());
            if !normalized.is_empty() {
                let workspace = workspace_root.to_string_lossy().to_string();
                let args = [
                    "-C",
                    workspace.as_str(),
                    "checkout",
                    "-B",
                    normalized.as_str(),
                ];
                let _ = run_git(git_bin, &args, None);
            }
        }
    }
    Ok(())
}

pub(super) fn resolve_allowed_paths(
    allowed_paths: &[String],
    workspace_root: &Path,
) -> AgentRunResult<Vec<String>> {
    let mut resolved = Vec::with_capacity(allowed_paths.len());
    for path in allowed_paths {
        if !jeryu_codegraph::slice::is_valid_repo_relative_path(path) {
            return Err(boxed_response(typed_error(TypedError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "agent_run_allowed_path_invalid",
                purpose: "start an agent-edit run",
                reason: "allowed_paths must be strict repo-relative paths",
                common_fixes: &[
                    "remove absolute paths and parent-directory segments",
                    "use repo-relative prefixes under the run workspace",
                ],
                docs_url: "docs/testing.md#workcells",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
                message: "one or more allowed_paths entries were invalid",
            })));
        }
        resolved.push(workspace_root.join(path).display().to_string());
    }
    Ok(resolved)
}

pub(super) fn derive_allowed_prefixes(
    allowed_paths: &[String],
    workspace_root: &Path,
) -> AgentRunResult<Vec<String>> {
    let mut prefixes = Vec::new();
    for path in allowed_paths {
        let absolute = Path::new(path);
        let relative = absolute.strip_prefix(workspace_root).map_err(|_| {
            boxed_response(typed_error(TypedError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "agent_run_allowed_path_invalid",
                purpose: "export an agent-edit run into a pull request",
                reason: "allowed path escaped the run workspace",
                common_fixes: &[
                    "restrict allowed_paths to files under the run workspace",
                    "rerun the run with repo-relative allowed paths only",
                ],
                docs_url: "docs/testing.md#workcells",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
                message: "the run allowed path was not inside the source workspace",
            }))
        })?;
        prefixes.push(relative.to_string_lossy().to_string());
    }
    Ok(prefixes)
}

fn run_git(git_bin: &str, args: &[&str], cwd: Option<&Path>) -> AgentRunResult<()> {
    let mut cmd = Command::new(git_bin);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let output = cmd
        .output()
        .map_err(|err| boxed_response(git_error("agent_run_git_spawn_failed", err.to_string())))?;
    if output.status.success() {
        return Ok(());
    }
    Err(boxed_response(git_error(
        "agent_run_git_failed",
        format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    )))
}

pub(super) fn current_head_sha(git_bin: &str, workspace_root: &Path) -> Option<String> {
    let output = Command::new(git_bin)
        .args([
            "-C",
            workspace_root.to_string_lossy().as_ref(),
            "rev-parse",
            "HEAD",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}

pub(super) fn freeze_workspace(git_bin: &str, workspace_root: &Path) -> AgentRunResult<()> {
    run_git(
        git_bin,
        &["-C", workspace_root.to_string_lossy().as_ref(), "add", "-A"],
        None,
    )?;
    run_git(
        git_bin,
        &[
            "-C",
            workspace_root.to_string_lossy().as_ref(),
            "-c",
            "user.name=jeryu",
            "-c",
            "user.email=jeryu@localhost",
            "commit",
            "--allow-empty",
            "-m",
            "freeze agent run export",
        ],
        None,
    )?;
    Ok(())
}

pub(super) fn git_diff_names(
    git_bin: &str,
    workspace_root: &Path,
    base_sha: &str,
    head_sha: &str,
) -> AgentRunResult<Vec<String>> {
    let output = Command::new(git_bin)
        .args([
            "-C",
            workspace_root.to_string_lossy().as_ref(),
            "diff",
            "--name-only",
            &format!("{base_sha}..{head_sha}"),
        ])
        .output()
        .map_err(|err| boxed_response(git_error("agent_run_git_spawn_failed", err.to_string())))?;
    if !output.status.success() {
        return Err(boxed_response(git_error(
            "agent_run_git_failed",
            format!(
                "git diff --name-only {}..{} failed: {}",
                base_sha,
                head_sha,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .filter(|line| !line.trim().is_empty())
        .collect())
}

pub(super) fn git_branch_force(
    git_bin: &str,
    workspace_root: &Path,
    branch: &str,
    head_sha: &str,
) -> AgentRunResult<()> {
    run_git(
        git_bin,
        &[
            "-C",
            workspace_root.to_string_lossy().as_ref(),
            "branch",
            "-f",
            branch,
            head_sha,
        ],
        None,
    )
}

fn git_error(code: &'static str, message: impl Into<String>) -> AxumResponse {
    let message = message.into();
    typed_error(TypedError {
        status: StatusCode::FAILED_DEPENDENCY,
        code,
        purpose: "prepare or export an agent-edit run",
        reason: &message,
        common_fixes: &[
            "inspect the local git state for the run workspace",
            "rerun the focused agent-run proof lane",
        ],
        docs_url: "docs/testing.md#workcells",
        repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
        message: &message,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jeryu-agent-git-{tag}-{}",
            crate::web::agent_runs::now_millis()
        ))
    }

    #[test]
    fn git_helpers_prepare_freeze_diff_and_branch_scratch_workspace() {
        let root = unique_dir("scratch");
        prepare_workspace("git", None, &root, "refs/heads/main", "scratch")
            .expect("scratch workspace is initialized");
        assert!(root.join(".git").is_dir());
        assert!(current_head_sha("git", &root).is_none());

        let allowed = resolve_allowed_paths(&["src".to_string()], &root).unwrap();
        assert_eq!(allowed, vec![root.join("src").display().to_string()]);
        assert!(resolve_allowed_paths(&["../escape".to_string()], &root).is_err());

        let prefixes = derive_allowed_prefixes(&allowed, &root).unwrap();
        assert_eq!(prefixes, vec!["src".to_string()]);
        assert!(derive_allowed_prefixes(&["/not/in/workspace".to_string()], &root).is_err());

        std::fs::write(root.join("README.md"), "first\n").unwrap();
        freeze_workspace("git", &root).expect("first commit succeeds");
        let base = current_head_sha("git", &root).expect("first head exists");

        std::fs::write(root.join("README.md"), "second\n").unwrap();
        freeze_workspace("git", &root).expect("second commit succeeds");
        let head = current_head_sha("git", &root).expect("second head exists");
        assert_ne!(base, head);

        let changed = git_diff_names("git", &root, &base, &head).unwrap();
        assert_eq!(changed, vec!["README.md".to_string()]);
        git_branch_force("git", &root, "agents/codex/wc/fix", &head)
            .expect("branch force succeeds");

        assert!(run_git("git", &["not-a-real-git-command"], Some(&root)).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_workspace_clone_requires_origin_for_non_scratch() {
        let root = unique_dir("missing-origin");
        let err = prepare_workspace("git", None, &root, "main", "repo")
            .expect_err("non-scratch workspace needs an origin");
        assert_eq!(err.status(), axum::http::StatusCode::FAILED_DEPENDENCY);
        let _ = std::fs::remove_dir_all(root);
    }
}
