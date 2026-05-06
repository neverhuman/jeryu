use crate::cli::ReleaseCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{release, state};

pub(crate) async fn execute_release_commands(subcmd: ReleaseCommands) -> Result<()> {
    match subcmd {
        ReleaseCommands::Status {
            project_id,
            ref_name,
            sha,
            limit,
            json,
        } => {
            let db = state::Db::open().await?;
            let report = release::build_release_status_report(
                &db,
                release::ReleaseStatusQuery {
                    project_id: Some(project_id),
                    ref_name: Some(ref_name),
                    sha,
                    limit,
                },
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_release_status_text(&report));
            }
        }
        ReleaseCommands::Watch {
            project_id,
            ref_name,
            sha,
            limit,
            interval_secs,
            json,
        } => {
            let db = state::Db::open().await?;
            release::watch_release_status(
                &db,
                release::ReleaseStatusQuery {
                    project_id: Some(project_id),
                    ref_name: Some(ref_name),
                    sha,
                    limit,
                },
                json,
                interval_secs,
            )
            .await?;
        }
        ReleaseCommands::Reconcile {
            project_id,
            ref_name,
            json,
        } => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let report =
                release::reconcile_release_for_ref(&db, &client, project_id, &ref_name).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_release_status_text(&report));
            }
        }
        ReleaseCommands::PromoteProd {
            project_id,
            ref_name,
            version,
        } => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let pipeline_id =
                release::trigger_production_promotion(&db, &client, project_id, &ref_name, version)
                    .await?;
            println!("Triggered production-promotion pipeline {pipeline_id}");
        }
        ReleaseCommands::Preflight { ssh_host, json } => {
            let report = release::release_preflight(ssh_host.as_deref()).await;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let status = if report.ok { "PASS" } else { "FAIL" };
                println!("Preflight: {status}");
                for (k, v) in &report.checks {
                    println!("  {k}: {v}");
                }
                if !report.blockers.is_empty() {
                    println!("\nBlockers:");
                    for b in &report.blockers {
                        println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
                    }
                }
            }
            if !report.ok {
                std::process::exit(1);
            }
        }
        ReleaseCommands::Doctor {
            version,
            preflight,
            json,
        } => {
            let db = state::Db::open().await?;
            let ver = if let Some(v) = version {
                v
            } else {
                // Use latest known version from release status
                let report = release::build_release_status_report(
                    &db,
                    release::ReleaseStatusQuery {
                        project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                        ref_name: Some("main".into()),
                        sha: None,
                        limit: 1,
                    },
                )
                .await?;
                if let Some(latest) = report.latest.as_ref() {
                    latest.attempt.version.clone()
                } else {
                    return Err(anyhow::anyhow!("no known release version; use --version"));
                }
            };
            let report = release::release_doctor(&ver, preflight).await;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let status = if report.blockers.is_empty() {
                    "OK"
                } else {
                    "BLOCKED"
                };
                println!("Doctor [{status}]: {}", report.version);
                println!("  next_action: {}", report.next_action);
                println!("  canary_complete: {}", report.canary_complete);
                println!("  prod_complete: {}", report.prod_complete);
                println!("  safe_to_reconcile: {}", report.safe_to_reconcile);
                if !report.preflight.is_empty() {
                    println!("\nPreflight:");
                    for (k, v) in &report.preflight {
                        println!("  {k}: {v}");
                    }
                }
                println!("\nGates:");
                for (k, v) in &report.gates {
                    println!("  {k}: {}", if *v { "present" } else { "MISSING" });
                }
                if !report.blockers.is_empty() {
                    println!("\nBlockers:");
                    for b in &report.blockers {
                        println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
                    }
                }
            }
        }
    }
    Ok(())
}
