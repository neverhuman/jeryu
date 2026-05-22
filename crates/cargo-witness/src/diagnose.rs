use std::fs;
use std::path::Path;

use crate::model::CompilePackets;
use anyhow::{Context, Result};
#[path = "diagnose_workspace.rs"]
mod workspace;

pub use workspace::diagnose_workspace;

/// Write compile packets to disk.
pub fn write_compile_packets(workspace_root: &Path, packets: &CompilePackets) -> Result<()> {
    let output_dir = workspace_root.join("target/agent");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let output_path = output_dir.join("compile-packets.json");
    let json = serde_json::to_string_pretty(packets)?;
    fs::write(&output_path, json)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}
