use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use jeryu_agent_auth::{AgentToolKind, materialize_run_home};
use jeryu_agentbridge::{AgentCli, CommandSpec, Effort, ModelSelect, adapter_for};
use jeryu_core::{CreateRepositoryRequest, ForgeError};
use jeryu_gitd::import::{classify_git_dir, inferred_owner, repo_name};
use jeryu_runnerd::{StartupSync, WorkcellClaimRequest};
use uuid::Uuid;

use crate::web::WebState;
use crate::web::workcells_support::{TypedError, forge_error, typed_error, workcell_error};

use super::errors::{agent_typed_error, auth_error};
use super::git::{current_head_sha, prepare_workspace, resolve_allowed_paths};
use super::state::{AgentRunOutcome, AgentRunSnapshot};
use super::types::{AgentRunSource, AgentWorkRequest};
use super::{AgentRunResult, EMPTY_TREE_SHA, boxed_response, now_millis};

#[derive(Clone)]
pub(super) struct PreparedRun {
    pub(super) agent_run_id: String,
    pub(super) workcell_id: String,
    pub(super) runner_id: String,
    pub(super) runner_epoch: u64,
    pub(super) agent: String,
    pub(super) model: String,
    pub(super) owner: String,
    pub(super) repo: String,
    pub(super) source_kind: String,
    pub(super) workspace_root: PathBuf,
    pub(super) base_sha: String,
    pub(super) allowed_paths: Vec<String>,
    pub(super) command_spec: CommandSpec,
    pub(super) export_branch: String,
    pub(super) base_ref: String,
}

impl From<&jeryu_agentbridge::AgentRunResult> for AgentRunOutcome {
    fn from(result: &jeryu_agentbridge::AgentRunResult) -> Self {
        Self {
            exit_code: result.exit_code,
            timed_out: result.timed_out,
            budget_exceeded: result.budget_exceeded,
            captured_bytes: result.captured_bytes,
            enforcement_level: result.enforcement_level.clone(),
            elapsed_ms: u64::try_from(result.elapsed.as_millis()).unwrap_or(u64::MAX),
            succeeded: result.succeeded(),
        }
    }
}

pub(super) fn prepare_run(
    state: &Arc<WebState>,
    request: &AgentWorkRequest,
    env: &BTreeMap<String, String>,
) -> AgentRunResult<PreparedRun> {
    let agent_run_id = format!("ar-{}", Uuid::new_v4().simple());
    let data_home = match env.get("JERYU_AGENT_AUTH_DATA_HOME") {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => {
            return Err(boxed_response(auth_error(
                jeryu_agent_auth::AgentAuthError {
                    code: "agent_auth_missing".to_string(),
                    repair: Box::new(jeryu_agent_auth::AgentAuthRepair {
                        purpose: "verify imported agent auth".to_string(),
                        reason: "JERYU_AGENT_AUTH_DATA_HOME is not configured".to_string(),
                        common_fixes: vec![
                            "run jeryu agent auth import --from-host for the requested tool"
                                .to_string(),
                            "set JERYU_AGENT_AUTH_DATA_HOME to the Jeryu agent-auth data root"
                                .to_string(),
                        ],
                        docs_url: "docs/testing.md#workcells".to_string(),
                        repair_hint: "rerun cargo test -p jeryu-agent-auth --jobs 40".to_string(),
                    }),
                },
            )));
        }
    };
    let run_home = data_home.join("runs").join(&agent_run_id);
    materialize_run_home(&data_home, request.agent, &run_home)
        .map_err(|err| boxed_response(auth_error(err)))?;

    let tool_key = format!(
        "JERYU_AGENT_TOOL_{}_PATH",
        request.agent.as_str().to_ascii_uppercase()
    );
    let tool_program = env.get(&tool_key).cloned().ok_or_else(|| {
        boxed_response(agent_typed_error(
            StatusCode::FAILED_DEPENDENCY,
            "agent_tool_missing",
            "start an agent-edit run",
            format!("{tool_key} is not configured"),
            &[
                "install the native CLI named in agent/native-cli-manifest.toml",
                "run the agent-edit runner doctor before accepting work",
            ],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-agentbridge -p jeryu-egress --jobs 40",
        ))
    })?;

    let cli: AgentCli = request.agent.as_str().parse::<AgentCli>().map_err(|err| {
        boxed_response(agent_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_run_unknown_tool",
            "start an agent-edit run",
            err.to_string(),
            &[
                "use one of codex, claude, or jekko",
                "request a supported agent tool",
            ],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-agentbridge --jobs 40",
        ))
    })?;
    let effort = request.effort.parse::<Effort>().map_err(|err| {
        boxed_response(agent_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_run_unknown_effort",
            "start an agent-edit run",
            err.to_string(),
            &[
                "use low, medium, high, xhigh, or max",
                "keep the effort field aligned with the CLI adapter",
            ],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-agentbridge --jobs 40",
        ))
    })?;
    let launch_model = select_launch_model(request, effort)?;
    let launch_plan =
        adapter_for(cli).build_launch(&tool_program, &launch_model, request.stream.required);
    let source = resolve_source(state, request, &run_home, &agent_run_id)?;

    let mut command_spec = CommandSpec::new(launch_plan.program.clone());
    command_spec.args = launch_plan.args.clone();
    command_spec.env = launch_plan.extra_env.clone();
    command_spec
        .env
        .insert("HOME".to_string(), run_home.to_string_lossy().to_string());
    command_spec.env.insert(
        "JERYU_AGENT_RUN_HOME".to_string(),
        run_home.to_string_lossy().to_string(),
    );
    command_spec
        .env
        .insert("JERYU_AGENT_RUN_ID".to_string(), agent_run_id.clone());
    command_spec.env.insert(
        "JERYU_AGENT_WORKCELL_ID".to_string(),
        source.workcell_id.clone(),
    );
    command_spec.env.insert(
        "JERYU_AGENT_SOURCE_ROOT".to_string(),
        source.workspace_root.display().to_string(),
    );
    command_spec
        .env
        .insert("JERYU_AGENT_BASE_REF".to_string(), request.base_ref.clone());
    command_spec = command_spec.stdin(request.prompt.clone());
    let allowed_paths = source.allowed_paths.clone();
    let export_branch = default_export_branch_from_request(request, &source);

    Ok(PreparedRun {
        agent_run_id,
        workcell_id: source.workcell_id,
        runner_id: source.runner_id,
        runner_epoch: source.runner_epoch,
        agent: request.agent.to_string(),
        model: request.model.clone(),
        owner: source.owner,
        repo: source.repo,
        source_kind: source.source_kind,
        workspace_root: source.workspace_root,
        base_sha: source.base_sha,
        allowed_paths,
        command_spec,
        export_branch,
        base_ref: request.base_ref.clone(),
    })
}

struct SourceCheckout {
    owner: String,
    repo: String,
    source_kind: String,
    workspace_root: PathBuf,
    base_sha: String,
    workcell_id: String,
    runner_id: String,
    runner_epoch: u64,
    allowed_paths: Vec<String>,
}

fn resolve_source(
    state: &Arc<WebState>,
    request: &AgentWorkRequest,
    run_home: &Path,
    agent_run_id: &str,
) -> AgentRunResult<SourceCheckout> {
    let git_bin = state.repo_manager.config().git_bin.clone();
    let workspace_root = run_home.join("workspace");

    let (owner, repo, source_kind, origin) = match &request.source {
        AgentRunSource::Repo { repo } => {
            let (owner, repo) = split_repo_slug(repo)?;
            ensure_repository(state, &owner, &repo, &request.base_ref)?;
            let resolved = state
                .repo_manager
                .resolve_parts(&owner, &repo)
                .map_err(|err| boxed_response(forge_error(ForgeError::Storage(err.to_string()))))?;
            (owner, repo, "repo".to_string(), Some(resolved.path))
        }
        AgentRunSource::LocalPath { local_path } => {
            let canonical = local_path.canonicalize().map_err(|err| {
                boxed_response(typed_error(TypedError {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    code: "agent_run_source_missing",
                    purpose: "start an agent-edit run",
                    reason: &err.to_string(),
                    common_fixes: &[
                        "send a local_path that exists and is readable",
                        "point local_path at a real Git worktree",
                    ],
                    docs_url: "docs/testing.md#workcells",
                    repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
                    message: &err.to_string(),
                }))
            })?;
            if classify_git_dir(&canonical).is_none() {
                return Err(boxed_response(typed_error(TypedError {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    code: "agent_run_source_not_git",
                    purpose: "start an agent-edit run",
                    reason: "local_path must point at a Git worktree or bare repository",
                    common_fixes: &[
                        "use a real Git checkout as the local source",
                        "import the local repository before starting the run",
                    ],
                    docs_url: "docs/testing.md#workcells",
                    repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
                    message: "the selected local source is not a Git directory",
                })));
            }
            let owner = inferred_owner(&canonical);
            let repo = repo_name(&canonical);
            ensure_repository(state, &owner, &repo, &request.base_ref)?;
            (owner, repo, "local_path".to_string(), Some(canonical))
        }
        AgentRunSource::Scratch { name } => {
            let owner = "local".to_string();
            let repo = name
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("scratch-{}", &agent_run_id[3..11]));
            ensure_repository(state, &owner, &repo, &request.base_ref)?;
            (owner, repo, "scratch".to_string(), None)
        }
    };

    prepare_workspace(
        &git_bin,
        origin.as_deref(),
        &workspace_root,
        &request.base_ref,
        &source_kind,
    )?;
    let base_sha =
        current_head_sha(&git_bin, &workspace_root).unwrap_or_else(|| EMPTY_TREE_SHA.to_string());
    let allowed_paths = resolve_allowed_paths(&request.allowed_paths, &workspace_root)?;
    let (workcell_id, runner_id, runner_epoch) =
        claim_workcell(state, request, &workspace_root, &base_sha, &owner, &repo)?;

    Ok(SourceCheckout {
        owner,
        repo,
        source_kind,
        workspace_root,
        base_sha,
        workcell_id,
        runner_id,
        runner_epoch,
        allowed_paths,
    })
}

fn claim_workcell(
    state: &Arc<WebState>,
    request: &AgentWorkRequest,
    workspace_root: &Path,
    base_sha: &str,
    owner: &str,
    repo: &str,
) -> AgentRunResult<(String, String, u64)> {
    let runner_epoch = now_millis();
    let runner_id = format!("runner-{}", Uuid::new_v4().simple());
    let claim = WorkcellClaimRequest {
        agent_id: request.agent.to_string(),
        workspace_root: workspace_root.to_path_buf(),
        repo_roots: vec![workspace_root.to_path_buf()],
        branch_budget: 1,
        runner_id: runner_id.clone(),
        runner_epoch,
        git_status_summary: format!("{owner}/{repo} agent run"),
        ci_snapshot_age_ms: None,
        startup: StartupSync::Rebased {
            main_ref: normalize_pr_base(request.base_ref.clone()),
            base_sha: base_sha.to_string(),
            head_sha: base_sha.to_string(),
        },
    };
    let mut workcells = state.workcells.lock().expect("workcell manager lock");
    let lease = workcells
        .claim(claim)
        .map_err(|err| boxed_response(workcell_error(err)))?;
    Ok((lease.workcell_id, lease.runner_id, lease.runner_epoch))
}

fn select_launch_model(request: &AgentWorkRequest, effort: Effort) -> AgentRunResult<ModelSelect> {
    if request.agent == AgentToolKind::Jekko {
        if let Some((provider, model)) = request.model.split_once('/') {
            return Ok(ModelSelect::new(model.to_string())
                .with_effort(effort)
                .with_provider(provider.to_string()));
        }
        if let Some((provider, model)) = request.model.split_once(':') {
            return Ok(ModelSelect::new(model.to_string())
                .with_effort(effort)
                .with_provider(provider.to_string()));
        }
    }
    Ok(ModelSelect::new(request.model.clone()).with_effort(effort))
}

fn default_export_branch_from_request(
    request: &AgentWorkRequest,
    source: &SourceCheckout,
) -> String {
    format!(
        "agents/{}/agent-runs/{}/{}",
        request.agent.as_str(),
        source.workcell_id,
        request.branch_suffix
    )
}

pub(super) fn default_export_branch(snapshot: &AgentRunSnapshot) -> String {
    format!(
        "agents/{}/agent-runs/{}/{}",
        snapshot.agent, snapshot.workcell_id, snapshot.branch_suffix
    )
}

fn ensure_repository(
    state: &Arc<WebState>,
    owner: &str,
    repo: &str,
    base_ref: &str,
) -> AgentRunResult<()> {
    match state.core.get_repository(owner, repo) {
        Ok(_) => Ok(()),
        Err(ForgeError::NotFound(_)) => state
            .core
            .create_repository(
                owner,
                CreateRepositoryRequest {
                    name: repo.to_string(),
                    private: false,
                    description: Some(format!("agent run backing repo for {owner}/{repo}")),
                    default_branch: Some(normalize_pr_base(base_ref.to_string())),
                },
            )
            .map(|_| ())
            .map_err(|err| boxed_response(forge_error(err))),
        Err(err) => Err(boxed_response(forge_error(err))),
    }
}

fn split_repo_slug(slug: &str) -> AgentRunResult<(String, String)> {
    let (owner, repo) = slug.split_once('/').ok_or_else(|| {
        boxed_response(typed_error(TypedError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "agent_run_source_invalid",
            purpose: "start an agent-edit run",
            reason: "repo sources must be owner/repo slugs",
            common_fixes: &[
                "send a repo slug with one owner and one repository name",
                "use a local_path source for arbitrary filesystem checkouts",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
            message: "the repo source was not a valid owner/repo slug",
        }))
    })?;
    Ok((owner.to_string(), repo.trim_end_matches(".git").to_string()))
}

pub(super) fn current_env() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

pub(super) fn normalize_pr_base(ref_name: String) -> String {
    let without_heads = ref_name
        .strip_prefix("refs/heads/")
        .unwrap_or(ref_name.as_str());
    without_heads
        .strip_prefix("origin/")
        .unwrap_or(without_heads)
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use jeryu_agent_auth::AgentToolKind;
    use jeryu_core::ForgeCore;

    use super::*;
    use crate::web::agent_runs::types::{AgentRunBudget, AgentRunSource, AgentRunStreamOptions};

    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jeryu-agent-source-{tag}-{}",
            crate::web::agent_runs::now_millis()
        ))
    }

    fn request() -> AgentWorkRequest {
        AgentWorkRequest {
            source: AgentRunSource::Scratch {
                name: Some("demo".to_string()),
            },
            agent: AgentToolKind::Codex,
            prompt: "fix it".to_string(),
            model: "gpt-5.4-mini".to_string(),
            base_ref: "refs/heads/main".to_string(),
            effort: "high".to_string(),
            allowed_paths: vec!["src".to_string()],
            branch_suffix: "agent-edit".to_string(),
            budget: AgentRunBudget {
                wall_secs: 60,
                output_bytes: 4096,
            },
            stream: AgentRunStreamOptions { required: false },
        }
    }

    fn env_with_auth(data_home: &Path) -> BTreeMap<String, String> {
        let auth_dir = data_home.join("agent-auth/codex");
        std::fs::create_dir_all(&auth_dir).unwrap();
        std::fs::write(auth_dir.join("auth.json"), "{}").unwrap();
        let mut env = BTreeMap::new();
        env.insert(
            "JERYU_AGENT_AUTH_DATA_HOME".to_string(),
            data_home.display().to_string(),
        );
        env.insert(
            "JERYU_AGENT_TOOL_CODEX_PATH".to_string(),
            "/bin/echo".to_string(),
        );
        env
    }

    #[test]
    fn prepare_run_for_scratch_materializes_auth_and_claims_workcell() {
        let data_home = unique_dir("data");
        let env = env_with_auth(&data_home);
        let git_root = unique_dir("git");
        let state = Arc::new(crate::web::WebState::new_with_git_storage(
            ForgeCore::new(),
            git_root.clone(),
        ));
        let req = request();

        let prepared = prepare_run(&state, &req, &env).expect("scratch run prepares");
        assert_eq!(prepared.agent, "codex");
        assert_eq!(prepared.owner, "local");
        assert_eq!(prepared.repo, "demo");
        assert_eq!(prepared.source_kind, "scratch");
        assert_eq!(prepared.base_ref, "refs/heads/main");
        assert!(prepared.workspace_root.join(".git").is_dir());
        assert!(
            prepared
                .command_spec
                .env
                .get("JERYU_AGENT_RUN_ID")
                .is_some_and(|value| value == &prepared.agent_run_id)
        );
        assert!(prepared.export_branch.contains(&prepared.workcell_id));
        assert_eq!(prepared.allowed_paths.len(), 1);

        let _ = std::fs::remove_dir_all(data_home);
        let _ = std::fs::remove_dir_all(git_root);
    }

    #[test]
    fn source_helpers_route_models_and_reject_bad_sources() {
        let mut req = request();
        let model = select_launch_model(&req, Effort::High).unwrap();
        assert_eq!(model.model, "gpt-5.4-mini");

        req.agent = AgentToolKind::Jekko;
        req.model = "openai/gpt-5.4-mini".to_string();
        let jekko = select_launch_model(&req, Effort::Low).unwrap();
        assert_eq!(jekko.provider.as_deref(), Some("openai"));

        assert_eq!(normalize_pr_base("origin/main".to_string()), "main");
        assert!(split_repo_slug("missing-slash").is_err());
        assert!(current_env().contains_key("PATH"));
    }
}
