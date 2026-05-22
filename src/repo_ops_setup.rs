use anyhow::{Context, Result, bail};
use std::fs;

use super::repo_direct::{
    configure_hook_mode, configure_remote, ensure_local_gitlab_project, local_gitlab_reachable,
    local_gitlab_ssh_url, remove_other_remotes, run_git, seed_missing_local_branch,
    write_jeryu_configs,
};
use super::{DirectRepoOptions, DirectRepoPlan, HookMode, HookProfile, JeryuConfigSpec, RepoMode};

pub(crate) async fn setup_direct_repo(opts: DirectRepoOptions) -> Result<i32> {
    let mode = match opts.hooks {
        HookMode::Off => RepoMode::Direct,
        HookMode::Advisory => RepoMode::Observed,
        HookMode::Enforce => RepoMode::Enforced,
    };
    let remote_name = if opts.new_repo || opts.replace_origin {
        "origin"
    } else {
        "jeryu"
    };
    let remote_url = local_gitlab_ssh_url(&opts.namespace, &opts.name);
    let plan = DirectRepoPlan {
        action: if opts.new_repo { "init" } else { "adopt" }.into(),
        repo_path: opts.path.display().to_string(),
        namespace: opts.namespace.clone(),
        name: opts.name.clone(),
        branch: opts.branch.clone(),
        remote_name: remote_name.into(),
        remote_url: remote_url.clone(),
        mode,
        hooks: opts.hooks,
        protect_main: opts.protect_main,
        main_relay: opts.main_relay,
        offline_release_remote: opts.offline_release_remote.clone(),
        dry_run: opts.dry_run,
        steps: vec![
            "verify local JeRyu/GitLab endpoints or print recovery command".into(),
            "create or reuse the local GitLab project".into(),
            "configure git remotes without recurring --mirror pushes".into(),
            "write deterministic .jeryu policy files without secrets".into(),
            "configure optional local hooks".into(),
        ],
    };

    if opts.dry_run {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(0);
    }

    if opts.new_repo {
        fs::create_dir_all(&opts.path)
            .with_context(|| format!("creating {}", opts.path.display()))?;
        if !opts.path.join(".git").is_dir() {
            run_git(&opts.path, &["init", "-b", &opts.branch])?;
        }
    } else if !opts.path.join(".git").is_dir() {
        bail!("{} is not a git checkout", opts.path.display());
    }

    if !local_gitlab_reachable().await {
        bail!(
            "local JeRyu/GitLab is not reachable; recover with `jeryu install server --yes` or `jeryu init`"
        );
    }
    let project = ensure_local_gitlab_project(&opts.namespace, &opts.name, opts.new_repo).await?;
    seed_missing_local_branch(&opts.path, &remote_url, &opts.branch)?;
    if opts.protect_main {
        dotenvy::from_path(crate::config::env_file()).ok();
        let pat = std::env::var("GITLAB_PAT").context(
            "GITLAB_PAT not found; recover with `jeryu bootstrap` or `jeryu install server --yes`",
        )?;
        let client = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:8929", Some(pat));
        client
            .protect_branch_mr_only(project.id, &opts.branch)
            .await
            .with_context(|| format!("protecting branch {}", opts.branch))?;
    }

    configure_remote(&opts.path, remote_name, &remote_url, opts.replace_origin)?;
    if opts.replace_origin {
        remove_other_remotes(&opts.path, remote_name)?;
    }
    if remote_name == "jeryu" {
        run_git(
            &opts.path,
            &["config", "--local", "remote.pushDefault", "jeryu"],
        )?;
    }
    run_git(
        &opts.path,
        &[
            "config",
            "--local",
            &format!("branch.{}.remote", opts.branch),
            remote_name,
        ],
    )?;
    run_git(
        &opts.path,
        &[
            "config",
            "--local",
            &format!("branch.{}.merge", opts.branch),
            &format!("refs/heads/{}", opts.branch),
        ],
    )?;
    write_jeryu_configs(
        &opts.path,
        JeryuConfigSpec {
            mode,
            hooks: opts.hooks,
            namespace: &opts.namespace,
            name: &opts.name,
            branch: &opts.branch,
            protect_main: opts.protect_main,
            main_relay: opts.main_relay,
            offline_release_remote: opts.offline_release_remote.as_deref(),
        },
    )?;
    configure_hook_mode(&opts.path, opts.hooks, HookProfile::All)?;
    println!("Configured direct JeRyu repo at {}", opts.path.display());
    println!("Remote {remote_name}: {remote_url}");
    Ok(0)
}
