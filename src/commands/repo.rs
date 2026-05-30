//! Owner: Repo-local maintenance command wrappers
//! Proof: `cargo test -p jeryu -- repo`
//! Invariants: Repo-local maintenance paths stay in Rust and avoid shell helpers.

use anyhow::{Context, Result};

use crate::cli::RepoCommands;
use jeryu::repo;
use jeryu::repo_fleet::{DEFAULT_REGISTRY_PATH, RepoRegistry};
use jeryu::repo_standard::{RepoStandardMode, RepoStandardOptions};
use std::path::PathBuf;

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
        RepoCommands::AuditHygiene {
            path,
            fleet,
            registry,
            json,
        } => execute_audit_hygiene(path, fleet, registry, json).await,
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
        RepoCommands::Fleet(subcmd) => execute_repo_fleet_commands(subcmd).await,
        RepoCommands::Shadow { repo } => jeryu::repo_local::shadow_main_command(repo).await,
        RepoCommands::Backup { repo } => jeryu::repo_local::backup_command(repo).await,
        RepoCommands::JankuraiFast { changed_from } => repo::jankurai_fast(&changed_from).await,
        RepoCommands::RedlineStateProof => repo::state_proof().await,
        RepoCommands::CaptureTuiScreenshots { output_dir } => {
            repo::capture_tui_screenshots(output_dir).await
        }
    }
}

async fn execute_audit_hygiene(
    path: Option<PathBuf>,
    fleet: bool,
    registry: Option<PathBuf>,
    json: bool,
) -> Result<i32> {
    use jeryu::repo_hygiene_audit as hygiene;

    let roots: Vec<(String, PathBuf)> = if fleet {
        let (registry, _path) = load_fleet_registry(registry)?;
        registry
            .repo
            .iter()
            .map(|r| (r.slug.clone(), r.local_root.clone()))
            .collect()
    } else {
        let target = match path {
            Some(p) => p,
            None => std::env::current_dir()?,
        };
        let label = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.display().to_string());
        vec![(label, target)]
    };

    let reports = hygiene::audit_fleet(&roots);

    if json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
        let errors = reports.iter().filter(|r| r.has_error()).count();
        return Ok(if errors > 0 { 1 } else { 0 });
    }

    let error_count = hygiene::print_reports(&reports);
    Ok(if error_count > 0 { 1 } else { 0 })
}

async fn execute_repo_fleet_commands(cmd: crate::cli::RepoFleetCommands) -> Result<i32> {
    match cmd {
        crate::cli::RepoFleetCommands::List(cmd) => {
            let (registry, _path) = load_fleet_registry(cmd.registry)?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&registry)?);
            } else {
                jeryu::repo_fleet::print_registry_list(&registry);
            }
            Ok(0)
        }
        crate::cli::RepoFleetCommands::Status(cmd) => {
            let (registry, path) = load_fleet_registry(cmd.registry)?;
            let github = if cmd.github {
                let token = std::env::var("GITHUB_TOKEN")
                    .context("GITHUB_TOKEN is required for `jeryu repo fleet status --github`")?;
                Some(jeryu::git_host::GitHubClient::new(token))
            } else {
                None
            };
            let snapshot = jeryu::repo_fleet::collect_fleet_snapshot_from_registry(
                &registry,
                path,
                github.as_ref(),
            )
            .await?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else {
                jeryu::repo_fleet::print_fleet_status(&snapshot);
            }
            Ok(0)
        }
        crate::cli::RepoFleetCommands::Sync(cmd) => {
            let (registry, _path) = load_fleet_registry(cmd.registry)?;
            let db = jeryu::state::Db::open().await?;
            let repos = tracked_repositories_from_registry(&registry);
            db.upsert_tracked_repositories(&repos).await?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&repos)?);
            } else {
                println!("synced {} tracked repositories", repos.len());
            }
            Ok(0)
        }
    }
}

fn load_fleet_registry(path: Option<PathBuf>) -> Result<(RepoRegistry, PathBuf)> {
    let path = match path {
        Some(path) => path,
        None => std::env::current_dir()?.join(DEFAULT_REGISTRY_PATH),
    };
    let registry = jeryu::repo_fleet::load_registry_path(&path)?;
    Ok((registry, path))
}

fn tracked_repositories_from_registry(
    registry: &RepoRegistry,
) -> Vec<jeryu::state::TrackedRepository> {
    let now = chrono::Utc::now().to_rfc3339();
    registry
        .repo
        .iter()
        .map(|repo| jeryu::state::TrackedRepository {
            slug: repo.slug.clone(),
            alias: repo.alias.clone(),
            provider: repo.provider.clone(),
            remote: repo.remote.clone(),
            local_root: repo.local_root.display().to_string(),
            default_branch: repo.default_branch.clone(),
            visibility: repo.visibility.clone(),
            health_profile: repo.health_profile.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .collect()
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
