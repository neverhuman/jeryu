//! Owner: Repo-local maintenance command wrappers
//! Proof: `cargo test -p jeryu -- repo`
//! Invariants: Repo-local maintenance paths stay in Rust and avoid shell helpers.

use anyhow::Result;

use crate::cli::RepoCommands;
use jeryu::repo;

pub(crate) async fn execute_repo_commands(cmd: RepoCommands) -> Result<i32> {
    match cmd {
        RepoCommands::RenderAgentIndex { check } => {
            jeryu::agent_surface::render_agent_index(check)?;
            Ok(0)
        }
        RepoCommands::AuditAgentSurface { json } => {
            jeryu::agent_surface::audit_agent_surface(json)?;
            Ok(0)
        }
        RepoCommands::PostgresStateProof => repo::postgres_state_proof().await,
        RepoCommands::CaptureTuiScreenshots { output_dir } => {
            repo::capture_tui_screenshots(output_dir).await
        }
    }
}
