use super::*;

#[path = "dispatch_inspect.rs"]
mod inspect;

pub(crate) async fn run(command: Commands) -> Result<i32> {
    match command {
        // ---- Cache -------------------------------------------------------
        Commands::Cache(subcmd) => {
            let db = state::Db::open().await?;
            let sc = cache::SmartCache::new(db);
            match subcmd {
                CacheCommands::Enable => {
                    sc.enable().await?;
                }
                CacheCommands::Doctor => {
                    sc.doctor().await?;
                }
                CacheCommands::Status { json } => {
                    sc.status_with_options(json).await?;
                }
                CacheCommands::Gc {
                    dry_run,
                    json,
                    keep_active_managers,
                    older_than,
                    max_cache_gb,
                } => {
                    sc.gc_with_options(cache::GcOptions {
                        dry_run,
                        json,
                        keep_active_managers,
                        older_than,
                        max_cache_gb,
                        quiet: false,
                    })
                    .await?;
                }
            }
        }

        // ---- Local ------------------------------------------------------
        Commands::Local(subcmd) => match subcmd {
            LocalCommands::Cargo { repo, cargo_args } => {
                local::run_cargo(repo, cargo_args).await?;
            }
            LocalCommands::CargoEnv { repo, json } => {
                let layout = local::cargo_env(repo)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&layout)?);
                } else {
                    let exports = cargo_cache::shell_exports(&layout);
                    if !exports.is_empty() {
                        println!("{}", exports.join("\n"));
                    }
                }
            }
        },

        // ---- Logs --------------------------------------------------------
        Commands::Logs { manager_id, lines } => {
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;
            let log_lines = logs::tail_manager(&db, &docker_ctl, &manager_id, lines).await?;
            logs::print_manager_logs(&manager_id, &log_lines);
        }

        // ---- Agent -------------------------------------------------------
        Commands::Agent(subcmd) => {
            // `agent submit` is GitHub-only and must not require GITLAB_PAT.
            if let AgentCommands::Submit {
                task,
                issue,
                risk_tier,
                dry_run,
                json,
            } = subcmd
            {
                crate::commands::agent_submit::execute_agent_submit(
                    task, issue, risk_tier, dry_run, json,
                )
                .await?;
                return Ok(0);
            }

            let (client, _) = load_client()?;

            match subcmd {
                AgentCommands::Spawn { project_id, task } => {
                    let agent_task = agent::spawn_agent(&client, project_id, &task).await?;
                    println!("🤖 Agent spawned!");
                    println!("   Project:  {}", agent_task.project_id);
                    println!("   Branch:   {}", agent_task.branch_name);
                    println!("   Issue:    #{}", agent_task.issue_iid.unwrap_or(0));
                    println!("   Task:     {}", agent_task.task_description);
                }
                AgentCommands::List { project_id } => {
                    let agents = agent::list_agents(&client, project_id).await?;
                    if agents.is_empty() {
                        println!("No active agents.");
                    } else {
                        for a in &agents {
                            println!("  #{:<5} [{}] {}", a.iid, a.labels.join(", "), a.title);
                        }
                    }
                }
                AgentCommands::Merge {
                    project_id,
                    mr_iid,
                    trust_tier,
                } => {
                    let trust_tier = trust_tier
                        .parse::<decision::TrustTier>()
                        .unwrap_or(decision::TrustTier::Trusted);
                    let evaluation =
                        agent::merge_agent_mr(&client, project_id, mr_iid, trust_tier).await?;
                    println!("Risk gate: {:?}", evaluation.decision);
                    println!("Reason:    {}", evaluation.reason);
                }
                AgentCommands::Submit { .. } => unreachable!("handled above"),
            }
        }

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
        } => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let report =
                release::build_progress_report(&db, &client, project_id, &ref_name).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_progress_text(&report));
            }
        }

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
        Commands::Action(subcmd) => match subcmd {
            ActionCommands::List { json } => {
                use jeryu::tui::action_registry::{self, Surface};
                if json {
                    let entries: Vec<serde_json::Value> = action_registry::REGISTRY
                        .iter()
                        .map(|e| {
                            serde_json::json!({
                                "id": e.id,
                                "label": e.label,
                                "key_hint": e.key_hint,
                                "risk_tier": e.risk_tier.label(),
                                "dry_run": e.dry_run,
                                "description": e.description,
                                "surfaces": e.surfaces.iter().map(|s| match s {
                                    Surface::Cli => "cli",
                                    Surface::Tui => "tui",
                                    Surface::Capability => "capability",
                                }).collect::<Vec<_>>(),
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                } else {
                    println!("{:<24} {:<12} {:<10} DESCRIPTION", "ACTION", "RISK", "KEY");
                    println!("{}", "─".repeat(80));
                    for e in action_registry::REGISTRY {
                        println!(
                            "{:<24} {:<12} {:<10} {}",
                            e.id,
                            e.risk_tier.label(),
                            e.key_hint.unwrap_or(""),
                            e.description,
                        );
                    }
                }
            }
        },

        // ---- Capability Server -------------------------------------------
        Commands::Capability(subcmd) => match subcmd {
            CapabilityCommands::Serve { socket_path } => {
                let (client, _) = load_client()?;
                capability::start_capability_server(&socket_path, client).await?;
            }
        },

        // ---- MCP Adapter -------------------------------------------------
        Commands::Mcp(subcmd) => match subcmd {
            McpCommands::Serve => {
                let (client, _) = load_client()?;
                mcp::start_mcp_stdio(client).await?;
            }
            McpCommands::ServeHttp => {
                let (client, _) = load_client()?;
                let bind = settings::get().mcp.bind.clone();
                mcp::start_mcp_http(client, &bind).await?;
            }
            McpCommands::Tools { json } => {
                let manifest = mcp::tool_manifest();
                if json {
                    println!("{}", serde_json::to_string_pretty(&manifest)?);
                } else {
                    for tool in manifest {
                        println!(
                            "{:<28} {:<18} {}",
                            tool["name"].as_str().unwrap_or(""),
                            tool["title"].as_str().unwrap_or(""),
                            tool["description"].as_str().unwrap_or(""),
                        );
                    }
                }
            }
        },

        // ---- Next --------------------------------------------------------
        Commands::Next {
            project_id,
            ref_name,
        } => {
            inspect::run_next(project_id, ref_name).await?;
        }

        // ---- ExplainBlocker ----------------------------------------------
        Commands::ExplainBlocker {
            entity_type,
            entity_id,
        } => {
            inspect::run_explain_blocker(entity_type, entity_id).await?;
        }

        _ => unreachable!("dispatch_back handles cache and later commands"),
    }

    Ok(0)
}
