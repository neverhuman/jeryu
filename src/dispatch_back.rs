use super::*;

#[path = "dispatch_inspect.rs"]
mod inspect;
#[path = "dispatch_back_late.rs"]
mod late;
#[path = "dispatch_back_ops.rs"]
mod ops;

pub(crate) async fn run(command: Commands) -> Result<i32> {
    match command {
        // ---- Cache -------------------------------------------------------
        Commands::Cache(subcmd) => ops::run_cache(subcmd).await?,

        // ---- Local ------------------------------------------------------
        Commands::Local(subcmd) => ops::run_local(subcmd).await?,

        // ---- Logs --------------------------------------------------------
        Commands::Logs { manager_id, lines } => ops::run_logs(manager_id, lines).await?,

        // ---- Agent -------------------------------------------------------
        Commands::Agent(subcmd) => ops::run_agent(subcmd).await?,

        // ---- Test --------------------------------------------------------
        Commands::Test(subcmd) => crate::commands::test::execute_test_commands(subcmd).await?,

        // ---- Settings ---------------------------------------------------
        Commands::Settings(subcmd) => {
            crate::commands::settings::execute_settings_commands(subcmd).await?
        }

        // ---- Release ----------------------------------------------------
        Commands::Release(subcmd) => {
            crate::commands::release::execute_release_commands(subcmd).await?
        }
        // ---- Secrets ----------------------------------------------------
        Commands::Secrets(subcmd) => {
            crate::commands::secrets::execute_secrets_commands(subcmd).await?
        }

        // ---- Progress ---------------------------------------------------
        Commands::Progress {
            project_id,
            ref_name,
            json,
        } => ops::run_progress(project_id, ref_name, json).await?,

        // ---- Repo --------------------------------------------------------
        Commands::Repo(subcmd) => {
            return crate::commands::repo::execute_repo_commands(subcmd).await;
        }

        // ---- Policy ------------------------------------------------------
        Commands::Policy(subcmd) => match subcmd {
            PolicyCommands::Audit { target, json } => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "target": target,
                            "protected_branches": ["main"],
                            "main_relay_actor": "jeryu",
                            "github_actions_required": false,
                            "status": "planned"
                        }))?
                    );
                } else {
                    println!("JeRyu policy audit target: {target}");
                    println!("  protected branches: main");
                    println!("  main relay actor:   jeryu");
                    println!("  GitHub Actions:     not required for deploy readiness");
                }
            }
        },

        // ---- Host --------------------------------------------------------
        Commands::Host(subcmd) => {
            return crate::commands::host::execute_host_commands(subcmd).await;
        }

        // ---- Exec -------------------------------------------------------
        Commands::Exec(_) => unreachable!("exec command is handled in main"), // allowlist: typed clap subcommand; invocations stay typed

        // ---- Server Hooks ------------------------------------------------
        Commands::ServerHook(subcmd) => match subcmd {
            ServerHookCommands::PreReceive => {
                admission::run_pre_receive_hook().await?;
            }
        },

        // ---- Action list -------------------------------------------------
        Commands::Action(subcmd) => late::run_action(subcmd).await?,

        // ---- Capability Server -------------------------------------------
        Commands::Capability(subcmd) => late::run_capability(subcmd).await?,

        // ---- MCP Adapter -------------------------------------------------
        Commands::Mcp(subcmd) => late::run_mcp(subcmd).await?,

        // ---- Next --------------------------------------------------------
        Commands::Next {
            project_id,
            ref_name,
        } => late::run_next(project_id, ref_name).await?,

        // ---- ExplainBlocker ----------------------------------------------
        Commands::ExplainBlocker {
            entity_type,
            entity_id,
        } => late::run_explain_blocker(entity_type, entity_id).await?,

        _ => unreachable!("dispatch_back handles cache and later commands"),
    }

    Ok(0)
}
