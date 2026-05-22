use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::graph::load_witness_graph_if_present;
use crate::model::{CompilePackets, RepairBundle, RepairContext};

/// Assemble a minimal repair bundle from the most recent failure source.
///
/// Reads compile packets and/or runtime failure packets, merges with the
/// witness graph for dependency context, and outputs the absolute smallest
/// context an agent needs.
pub fn build_repair_bundle(workspace_root: &Path) -> Result<RepairBundle> {
    let compile_path = workspace_root.join("target/agent/compile-packets.json");
    if compile_path.exists()
        && let Some(bundle) = repair_from_compile_packets(workspace_root, &compile_path)?
    {
        return Ok(bundle);
    }

    let runtime_path = workspace_root.join("target/agent/last-failure.json");
    if runtime_path.exists() {
        return repair_from_runtime_packet(workspace_root, &runtime_path);
    }

    Ok(no_failure_bundle(
        "No compile diagnostics or runtime repair packets are present.",
        vec![
            "Run `cargo run -p cargo-witness -- diagnose` or `cargo run -p cargo-witness -- witness diagnose` after a compile failure.".to_string(),
            "Trigger a runtime failure with `witness-rt` installed to capture `target/agent/last-failure.json`.".to_string(),
        ],
    ))
}

/// Build a repair bundle from compile diagnostic packets.
fn repair_from_compile_packets(
    workspace_root: &Path,
    compile_path: &Path,
) -> Result<Option<RepairBundle>> {
    let content = fs::read_to_string(compile_path)
        .with_context(|| format!("failed to read {}", compile_path.display()))?;
    let packets: CompilePackets = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", compile_path.display()))?;

    // Find the first error (highest priority).
    let primary = match packets.packets.iter().find(|p| p.level == "error") {
        Some(err) => err,
        None => match packets.packets.first() {
            Some(first) => first,
            None => return Ok(None),
        },
    };

    // Check witness graph for pub-item context.
    let witness = load_witness_graph_if_present(workspace_root);
    let pub_items_in_scope = match witness
        .as_ref()
        .and_then(|graph| graph.crates.iter().find(|c| c.name == primary.owning_arc))
    {
        Some(crate_witness) => crate_witness
            .pub_items
            .iter()
            .map(|item| item.signature.clone())
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };

    // Estimate context bytes from the primary file.
    let context_bytes = fs::metadata(workspace_root.join(&primary.file))
        .map(|meta| meta.len())
        .unwrap_or(0);

    Ok(Some(RepairBundle {
        status: "action-required".to_string(),
        failure_type: "compile-error".to_string(),
        primary_arc: primary.owning_arc.clone(),
        primary_file: primary.file.clone(),
        primary_line: primary.line,
        error_summary: primary.message.clone(),
        repair_context: RepairContext {
            cell_purpose: primary.cell_purpose.clone(),
            invariants: primary.invariants.clone(),
            pub_items_in_scope,
            likely_causes: primary.compiler_suggestion.iter().cloned().collect(),
            hints: vec![],
            files_to_read: vec![primary.file.clone()],
            context_bytes,
        },
        validate_after_fix: primary.local_commands.clone(),
        escalate_if: Some("interface hash changes after fix".to_string()),
        notes: vec!["Derived from the highest-priority compile diagnostic packet.".to_string()],
    }))
}

/// Build a repair bundle from a runtime failure (panic hook) packet.
fn repair_from_runtime_packet(workspace_root: &Path, runtime_path: &Path) -> Result<RepairBundle> {
    let content = fs::read_to_string(runtime_path)
        .with_context(|| format!("failed to read {}", runtime_path.display()))?;
    let packet: witness_rt::RepairPacket = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", runtime_path.display()))?;

    let witness = load_witness_graph_if_present(workspace_root);
    let pub_items_in_scope = match &packet.cell {
        Some(cell) => match witness
            .as_ref()
            .and_then(|graph| graph.crates.iter().find(|c| &c.name == cell))
        {
            Some(crate_witness) => crate_witness
                .pub_items
                .iter()
                .map(|item| item.signature.clone())
                .collect::<Vec<_>>(),
            None => Vec::new(),
        },
        None => Vec::new(),
    };

    let context_bytes = fs::metadata(workspace_root.join(&packet.file))
        .map(|meta| meta.len())
        .unwrap_or(0);

    Ok(RepairBundle {
        status: "action-required".to_string(),
        failure_type: "runtime-panic".to_string(),
        primary_arc: match packet.cell.clone() {
            Some(cell) => cell,
            None => "<unmatched>".into(),
        },
        primary_file: packet.file.clone(),
        primary_line: packet.line,
        error_summary: format!("[{}] {}", packet.code, packet.message),
        repair_context: RepairContext {
            cell_purpose: packet.cell_purpose.clone(),
            invariants: packet.invariants.clone(),
            pub_items_in_scope,
            likely_causes: packet.likely_causes.clone(),
            hints: packet.hints.clone(),
            files_to_read: vec![packet.file.clone()],
            context_bytes,
        },
        validate_after_fix: packet.local_commands.clone(),
        escalate_if: Some("interface hash changes after fix".to_string()),
        notes: packet
            .match_provenance
            .iter()
            .map(|provenance| format!("Runtime packet matched via {provenance}."))
            .collect(),
    })
}

fn no_failure_bundle(summary: &str, notes: Vec<String>) -> RepairBundle {
    RepairBundle {
        status: "no-failure".to_string(),
        failure_type: "no-failure".to_string(),
        primary_arc: "<none>".to_string(),
        primary_file: String::new(),
        primary_line: 0,
        error_summary: summary.to_string(),
        repair_context: RepairContext {
            cell_purpose: None,
            invariants: Vec::new(),
            pub_items_in_scope: Vec::new(),
            likely_causes: Vec::new(),
            hints: Vec::new(),
            files_to_read: Vec::new(),
            context_bytes: 0,
        },
        validate_after_fix: Vec::new(),
        escalate_if: None,
        notes,
    }
}

/// Write a repair bundle to disk as JSON.
pub fn write_repair_bundle(workspace_root: &Path, bundle: &RepairBundle) -> Result<()> {
    let output_dir = workspace_root.join("target/agent");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let output_path = output_dir.join("repair-bundle.json");
    let json = serde_json::to_string_pretty(bundle)?;
    fs::write(&output_path, json)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
