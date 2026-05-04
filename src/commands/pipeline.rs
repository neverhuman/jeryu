use crate::cli::PipelineCommands;
use crate::dispatch::{fetch_ci_job_runs, load_client};
use anyhow::Result;
use jeryu::{release, state};

pub(crate) async fn execute_pipeline_commands(subcmd: PipelineCommands) -> Result<()> {
    let (client, _) = load_client()?;
    match subcmd {
        PipelineCommands::Explain {
            project_id,
            pipeline_id,
            json,
        } => {
            let report =
                release::build_pipeline_explain_report(&client, project_id, pipeline_id).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_pipeline_explain_text(&report));
            }
        }
        PipelineCommands::Doctor {
            project_id,
            pipeline_id,
            json,
        } => {
            let report =
                release::build_pipeline_doctor_report(&client, project_id, pipeline_id).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_pipeline_doctor_text(&report));
            }
        }
        PipelineCommands::Jobs {
            project_id,
            pipeline_id,
            ingest,
            json,
        } => {
            let db = state::Db::open().await?;
            let runs = fetch_ci_job_runs(&client, project_id, pipeline_id).await?;
            if ingest {
                db.upsert_ci_job_runs(&runs).await?;
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else {
                println!("Pipeline {} — {} jobs", pipeline_id, runs.len());
                for run in &runs {
                    let dur = run
                        .duration_secs
                        .map(|value| format!("{value:.1}s"))
                        .unwrap_or_else(|| "-".to_string());
                    let queue = run
                        .queued_duration_secs
                        .map(|value| format!("{value:.1}s"))
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "  {:<34} {:<10} stage={:<14} run={:>8} queue={:>8} pipe={:<8} root={:<8} start={} finish={}",
                        run.job_name,
                        run.status,
                        run.stage,
                        dur,
                        queue,
                        run.pipeline_id,
                        run.root_pipeline_id,
                        run.started_at.as_deref().unwrap_or("-"),
                        run.finished_at.as_deref().unwrap_or("-")
                    );
                }
            }
        }
        PipelineCommands::Ingest {
            project_id,
            pipeline_id,
        } => {
            let db = state::Db::open().await?;
            let runs = fetch_ci_job_runs(&client, project_id, pipeline_id).await?;
            db.upsert_ci_job_runs(&runs).await?;
            println!(
                "ingested {} job runs for pipeline {}",
                runs.len(),
                pipeline_id
            );
        }
        PipelineCommands::Cancel {
            project_id,
            pipeline_id,
        } => {
            client.cancel_pipeline(project_id, pipeline_id).await?;
            println!("cancelled pipeline {}", pipeline_id);
        }
        PipelineCommands::Bottlenecks {
            project_id,
            ref_name,
            limit,
            json,
        } => {
            let db = state::Db::open().await?;
            let rows = db
                .ci_job_bottlenecks(project_id, ref_name.as_deref(), limit)
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                println!("CI bottlenecks — project {}", project_id);
                for row in &rows {
                    println!(
                        "  {:<34} avg={:>7.1}s latest={:>7} max={:>7} runs={:<4} stage={} pool={}",
                        row.job_name,
                        row.avg_duration_secs,
                        row.latest_duration_secs
                            .map(|value| format!("{value:.1}s"))
                            .unwrap_or_else(|| "-".to_string()),
                        row.max_duration_secs
                            .map(|value| format!("{value:.1}s"))
                            .unwrap_or_else(|| "-".to_string()),
                        row.runs,
                        row.stage,
                        row.runner_pool.as_deref().unwrap_or("-")
                    );
                }
            }
        }
    }
    Ok(())
}
