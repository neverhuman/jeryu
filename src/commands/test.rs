use crate::cli::TestCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::state;

#[path = "test_back.rs"]
mod test_back;
#[allow(unused_imports)]
pub(crate) use test_back::test_back_support::{current_commit_sha, parse_tag_list};

#[path = "test_intel_commands.rs"]
mod test_intel_commands;
use test_intel_commands::{
    handle_choose_command, handle_explain_plan_command, handle_impact_command,
};

#[path = "test_pipeline_commands.rs"]
mod test_pipeline_commands;
use test_pipeline_commands::{
    handle_batch_command, handle_failed_command, handle_plan_command, handle_requeue_command,
    handle_results_command, handle_run_command,
};

pub(crate) async fn execute_test_commands(subcmd: TestCommands) -> Result<()> {
    let (client, _) = load_client()?;
    let db = state::Db::open().await?;

    match subcmd {
        TestCommands::Run {
            command,
            project_id,
            image,
            tags,
            timeout,
            force,
        } => {
            handle_run_command(
                &client, &db, command, project_id, image, tags, timeout, force,
            )
            .await?;
        }
        TestCommands::Plan {
            command,
            project_id,
            image,
            tags,
            timeout,
        } => {
            handle_plan_command(command, project_id, image, tags, timeout)?;
        }
        TestCommands::Batch {
            commands,
            project_id,
            image,
            tags,
            timeout,
            max_parallel,
            force,
        } => {
            handle_batch_command(
                &client,
                &db,
                commands,
                project_id,
                image,
                tags,
                timeout,
                max_parallel,
                force,
            )
            .await?;
        }
        TestCommands::Results {
            pipeline_id,
            project_id,
        } => {
            handle_results_command(&client, pipeline_id, project_id).await?;
        }
        TestCommands::Requeue {
            pipeline_id,
            job_name,
            project_id,
        } => {
            handle_requeue_command(&client, pipeline_id, job_name, project_id).await?;
        }
        TestCommands::Failed {
            pipeline_id,
            project_id,
        } => {
            handle_failed_command(&client, pipeline_id, project_id).await?;
        }
        TestCommands::Impact {
            base,
            head,
            repo_root,
            json,
        } => {
            handle_impact_command(base, head, repo_root, json).await?;
        }
        TestCommands::Choose {
            base,
            head,
            repo_root,
            explain,
            json,
            emit_gitlab,
            emit_plan,
            emit_receipt,
        } => {
            handle_choose_command(
                base,
                head,
                repo_root,
                explain,
                json,
                emit_gitlab,
                emit_plan,
                emit_receipt,
            )?;
        }
        TestCommands::ExplainPlan { plan_path } => {
            handle_explain_plan_command(plan_path)?;
        }
        other => return test_back::run(other, &db).await,
    }
    Ok(())
}
