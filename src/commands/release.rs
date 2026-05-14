use crate::cli::ReleaseCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{release, state};
use std::path::PathBuf;

#[path = "release_ops.rs"]
mod release_ops;
use release_ops::*;

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
                println!(
                    "Preflight {}: {} blocker(s)",
                    if report.ok { "PASS" } else { "FAIL" },
                    report.blockers.len()
                );
                for b in &report.blockers {
                    println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
                }
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
                let status = if report.blockers.is_empty() { "OK" } else { "BLOCKED" };
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
                        println!("  - {:?}", b);
                    }
                }
            }
        }
        ReleaseCommands::Ready {
            pr,
            emit_status,
            dry_run,
            json,
        } => {
            let gate = release::compose_gate(pr, dry_run);
            if json {
                println!("{}", serde_json::to_string_pretty(&gate)?);
            } else {
                print!("{}", release::render_gate_text(&gate));
            }
            if emit_status && !dry_run {
                let repo = std::env::var("GITHUB_REPOSITORY")
                    .map_err(|_| anyhow::anyhow!("GITHUB_REPOSITORY env not set"))?;
                let sha = std::env::var("GITHUB_SHA")
                    .map_err(|_| anyhow::anyhow!("GITHUB_SHA env not set"))?;
                let resp = release::post_check_run(&gate, &repo, &sha)?;
                if !json {
                    println!(
                        "\nCheck Run posted. Response head: {}",
                        trim_head(&resp, 200)
                    );
                }
            }
            if !gate.is_pass() && !dry_run {
                std::process::exit(1);
            }
        }
        ReleaseCommands::DryRun { version, json } => {
            let report = run_release_dry_run(&version).await;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Release dry-run for {}", report.version);
                for (k, v) in &report.checks {
                    println!("  {k}: {v}");
                }
                if !report.blockers.is_empty() {
                    println!("\nBlockers:");
                    for b in &report.blockers {
                        println!("  - {b}");
                    }
                }
            }
            if !report.blockers.is_empty() {
                std::process::exit(1);
            }
        }
        ReleaseCommands::Submit {
            version,
            force,
            dry_run,
        } => {
            run_release_submit(&version, force, dry_run).await?;
        }
        ReleaseCommands::Approve {
            pr,
            as_user,
            dry_run,
        } => {
            run_release_approve(pr, as_user, dry_run).await?;
        }
        ReleaseCommands::Rollback {
            version,
            reason,
            dry_run,
            json,
        } => {
            let report = release::build_report(&version, &reason, dry_run);
            let dir = PathBuf::from(format!("ops/releases/{version}"));
            let written = release::write_evidence(&report, dir)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Rollback {} for {} — {}",
                    report.final_status, report.version, report.reason
                );
                println!("Evidence: {}", written.display());
                for s in &report.steps {
                    let suffix = s.detail.as_deref().unwrap_or("");
                    println!("  [{}] {} — {} ({})", s.n, s.kind, s.description, suffix);
                }
            }
        }
    }
    Ok(())
}
