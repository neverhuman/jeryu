use super::*;

pub(crate) async fn run_action(subcmd: ActionCommands) -> Result<()> {
    match subcmd {
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
    }

    Ok(())
}

pub(crate) async fn run_capability(subcmd: CapabilityCommands) -> Result<()> {
    match subcmd {
        CapabilityCommands::Serve { socket_path } => {
            let (client, _) = load_client()?;
            capability::start_capability_server(&socket_path, client).await?;
        }
    }

    Ok(())
}

pub(crate) async fn run_mcp(subcmd: McpCommands) -> Result<()> {
    match subcmd {
        McpCommands::Serve => {
            let (client, _) = load_client_optional();
            mcp::start_mcp_stdio(client).await?;
        }
        McpCommands::ServeHttp => {
            let (client, _) = load_client_optional();
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
    }

    Ok(())
}

pub(crate) async fn run_next(project_id: i64, ref_name: String) -> Result<()> {
    inspect::run_next(project_id, ref_name).await?;
    Ok(())
}

pub(crate) async fn run_explain_blocker(entity_type: String, entity_id: i64) -> Result<()> {
    inspect::run_explain_blocker(entity_type, entity_id).await?;
    Ok(())
}
