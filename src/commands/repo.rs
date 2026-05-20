//! Owner: Repo-local maintenance command wrappers
//! Proof: `cargo test -p jeryu -- repo`
//! Invariants: Repo-local maintenance paths stay in Rust and avoid shell helpers.

use anyhow::Result;

use crate::cli::RepoCommands;
use jeryu::repo;
use jeryu::repo_standard::{RepoStandardMode, RepoStandardOptions};

pub(crate) async fn execute_repo_commands(cmd: RepoCommands) -> Result<i32> {
    match cmd {
        RepoCommands::RenderAgentIndex { check } => {
            jeryu::agent_surface::render_agent_index(check)?;
            Ok(0)
        }
        RepoCommands::AuditAgentSurface { json } => {
            jeryu::agent_surface::audit_agent_surface(json)?;
            Ok(0)
        }
        RepoCommands::InstallGitHooks => repo::install_git_hooks().await,
        RepoCommands::Init(cmd) => {
            if !cmd.direct {
                anyhow::bail!("repo init currently requires --direct");
            }
            repo::init_direct_repo(repo::DirectRepoOptions {
                path: std::env::current_dir()?.join(&cmd.name),
                name: cmd.name,
                namespace: cmd.namespace,
                branch: cmd.branch,
                protect_main: cmd.protect_main,
                hooks: cmd.hooks,
                replace_origin: true,
                new_repo: true,
                dry_run: cmd.dry_run,
                main_relay: cmd.main_relay,
                offline_release_remote: cmd.offline_release_remote,
            })
            .await
        }
        RepoCommands::Adopt(cmd) => {
            if !cmd.direct {
                anyhow::bail!("repo adopt currently requires --direct");
            }
            repo::adopt_direct_repo(repo::DirectRepoOptions {
                path: cmd.path,
                name: cmd.name,
                namespace: cmd.namespace,
                branch: "main".into(),
                protect_main: cmd.protect_main,
                hooks: cmd.hooks,
                replace_origin: cmd.replace_origin,
                new_repo: false,
                dry_run: cmd.dry_run,
                main_relay: cmd.main_relay,
                offline_release_remote: cmd.offline_release_remote,
            })
            .await
        }
        RepoCommands::Mode { mode } => repo::set_repo_mode(mode).await,
        RepoCommands::Hooks(subcmd) => match subcmd {
            crate::cli::RepoHookCommands::Status => repo::hooks_status().await,
            crate::cli::RepoHookCommands::Enable { mode } => repo::hooks_enable(mode).await,
            crate::cli::RepoHookCommands::Disable => repo::hooks_disable().await,
            crate::cli::RepoHookCommands::Install { profile, mode } => {
                repo::hooks_install(profile, mode).await
            }
        },
        RepoCommands::Standard(subcmd) => match subcmd {
            crate::cli::RepoStandardCommands::Plan(cmd) => {
                jeryu::repo_standard::run_standard(RepoStandardMode::Plan, standard_options(cmd))
            }
            crate::cli::RepoStandardCommands::Apply(cmd) => {
                jeryu::repo_standard::run_standard(RepoStandardMode::Apply, standard_options(cmd))
            }
            crate::cli::RepoStandardCommands::Verify(cmd) => {
                jeryu::repo_standard::run_standard(RepoStandardMode::Verify, standard_options(cmd))
            }
        },
        RepoCommands::Shadow { repo } => jeryu::repo_local::shadow_main_command(repo).await,
        RepoCommands::Backup { repo } => jeryu::repo_local::backup_command(repo).await,
        RepoCommands::JankuraiFast { changed_from } => repo::jankurai_fast(&changed_from).await,
        RepoCommands::RedlineStateProof => repo::state_proof().await,
        RepoCommands::CaptureTuiScreenshots { output_dir } => {
            repo::capture_tui_screenshots(output_dir).await
        }
    }
}

fn standard_options(cmd: crate::cli::RepoStandardCommand) -> RepoStandardOptions {
    RepoStandardOptions {
        path: cmd.path,
        profile: cmd.profile,
        provider: cmd.provider,
        base_branch: cmd.base_branch,
        repo_slug: cmd.repo,
        autonomy_dir: cmd.autonomy_dir,
        configure_git_hooks: cmd.configure_git_hooks,
        json: cmd.json,
    }
}
