use anyhow::Result;
use std::path::PathBuf;

use super::test_back::{handle_choose, handle_impact};
use jeryu::test_intel;

pub(crate) async fn handle_impact_command(
    base: String,
    head: String,
    repo_root: PathBuf,
    json: bool,
) -> Result<()> {
    handle_impact(base, head, repo_root, json).await
}

#[allow(clippy::too_many_arguments)] // CLI flag passthrough; this dispatcher is intentionally flat
pub(crate) fn handle_choose_command(
    base: String,
    head: String,
    repo_root: Option<PathBuf>,
    explain: bool,
    json: bool,
    emit_gitlab: Option<PathBuf>,
    emit_plan: Option<PathBuf>,
    emit_receipt: Option<PathBuf>,
) -> Result<()> {
    let cwd = match repo_root {
        Some(path) => path,
        None => match std::env::current_dir() {
            Ok(dir) => dir,
            Err(_) => PathBuf::from("."),
        },
    };
    handle_choose(
        base,
        head,
        cwd,
        explain,
        json,
        emit_gitlab,
        emit_plan,
        emit_receipt,
    )
}

pub(crate) fn handle_explain_plan_command(plan_path: PathBuf) -> Result<()> {
    let contents = std::fs::read_to_string(&plan_path)?;
    let plan: test_intel::planner::TestPlan = serde_json::from_str(&contents)?;
    print!("{}", test_intel::explain::explain(&plan));
    Ok(())
}
