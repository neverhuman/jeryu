use crate::cli::JobCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{logs, state};

pub(crate) async fn execute_job_commands(subcmd: JobCommands) -> Result<()> {
    let (client, _) = load_client()?;

    match subcmd {
        JobCommands::Clear => {
            let db = state::Db::open().await?;
            db.clear_history().await?;
            println!("🗑 All jobs and pipelines cleared from local DB.");
        }
        JobCommands::List { project_id, status } => {
            let scopes: Vec<&str> = status.split(',').collect();
            let jobs = client.list_jobs(project_id, &scopes).await?;
            println!(
                "{:<8} {:<20} {:<12} {:<10}",
                "ID", "NAME", "STATUS", "STAGE"
            );
            for j in &jobs {
                println!(
                    "{:<8} {:<20} {:<12} {:<10}",
                    j.id, j.name, j.status, j.stage
                );
            }
        }
        JobCommands::Trace { project_id, job_id } => {
            let trace = client.job_trace(project_id, job_id).await?;
            logs::print_trace(project_id, job_id, &trace);
        }
        JobCommands::Play { project_id, job_id } => {
            client.play_job(project_id, job_id).await?;
            println!("▶ Job {} triggered", job_id);
        }
        JobCommands::Cancel { project_id, job_id } => {
            client.cancel_job(project_id, job_id).await?;
            println!("⏹ Job {} cancelled", job_id);
        }
        JobCommands::Retry { project_id, job_id } => {
            client.requeue_job(project_id, job_id).await?;
            println!("🔄 Job {} requeued", job_id);
        }
        JobCommands::Explain { project_id, job_id } => {
            let db = state::Db::open().await?;
            if let Some(capsule) = db.latest_evidence_for_job(project_id, job_id).await? {
                let decision_record = db.latest_job_decision(project_id, job_id).await?;
                println!("Job:          {}", capsule.job_id);
                println!("Pipeline:     {}", capsule.pipeline_id.unwrap_or(0));
                println!("Stage:        {}", capsule.stage);
                println!("Ref:          {}", capsule.ref_name);
                println!("Commit:       {}", capsule.commit_sha);
                println!("Failure kind: {}", capsule.failure_kind);
                println!("Classified:   {:?}", capsule.classify());
                println!("Recovery advice: {:?}", capsule.recommended_recovery());
                println!("Summary:      {}", capsule.summary);
                if let Some(record) = decision_record {
                    println!("Last attempt: {} ({})", record.decision, record.reason);
                }
                println!("\nLog snippet:\n{}", capsule.log_snippet);
            } else {
                println!("No structured evidence capsule found for job {}", job_id);
            }
        }
    }
    Ok(())
}
