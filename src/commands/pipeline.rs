use crate::cli::PipelineCommands;
use crate::dispatch::{fetch_ci_job_runs, load_client};
use anyhow::{Result, bail};
use jeryu::{
    gitlab_client::{GitlabClient, Pipeline},
    release, state,
};

fn parse_pipeline_trigger_vars(vars: &[String]) -> Result<Vec<(&str, &str)>> {
    let mut parsed = Vec::with_capacity(vars.len());
    for var in vars {
        let Some((key, value)) = var.split_once('=') else {
            bail!("pipeline variable must be KEY=VALUE: {var}");
        };
        if key.is_empty() {
            bail!("pipeline variable key must not be empty: {var}");
        }
        parsed.push((key, value));
    }
    Ok(parsed)
}

fn tracked_pipeline(project_id: i64, pipeline: &Pipeline) -> state::TrackedPipeline {
    state::TrackedPipeline {
        pipeline_id: pipeline.id,
        project_id,
        ref_name: pipeline.ref_name.clone(),
        sha: pipeline.sha.clone(),
        status: pipeline.status.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

async fn track_existing_pipeline(
    client: &GitlabClient,
    db: &state::Db,
    project_id: i64,
    pipeline_id: i64,
) -> Result<()> {
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    db.upsert_tracked_pipeline(&tracked_pipeline(project_id, &pipeline))
        .await
}

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
                track_existing_pipeline(&client, &db, project_id, pipeline_id).await?;
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else {
                println!("Pipeline {} — {} jobs", pipeline_id, runs.len());
                for run in &runs {
                    let dur = match run.duration_secs {
                        Some(value) => format!("{value:.1}s"),
                        None => "-".to_string(),
                    };
                    let queue = match run.queued_duration_secs {
                        Some(value) => format!("{value:.1}s"),
                        None => "-".to_string(),
                    };
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
            track_existing_pipeline(&client, &db, project_id, pipeline_id).await?;
            println!(
                "ingested {} job runs for pipeline {}",
                runs.len(),
                pipeline_id
            );
        }
        PipelineCommands::Trigger {
            project_id,
            ref_name,
            vars,
        } => {
            let parsed_vars = parse_pipeline_trigger_vars(&vars)?;
            let pipeline = client
                .trigger_pipeline_details(project_id, &ref_name, parsed_vars)
                .await?;
            let db = state::Db::open().await?;
            db.upsert_tracked_pipeline(&tracked_pipeline(project_id, &pipeline))
                .await?;
            println!(
                "triggered pipeline {} on {} ({})",
                pipeline.id, pipeline.ref_name, pipeline.status
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
                        match row.latest_duration_secs {
                            Some(value) => format!("{value:.1}s"),
                            None => "-".to_string(),
                        },
                        match row.max_duration_secs {
                            Some(value) => format!("{value:.1}s"),
                            None => "-".to_string(),
                        },
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
