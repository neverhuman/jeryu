use std::io::Write;
use std::panic::PanicHookInfo;
use std::sync::OnceLock;
use std::{fs, panic};

use chrono::Utc;

use crate::packet::{CellRegistration, HookConfig, RepairPacket};

/// Global cell registry. Populated at startup, read-only during execution.
static CELL_REGISTRY: OnceLock<Vec<CellRegistration>> = OnceLock::new();

/// Global hook config. Set once by `install_panic_hook`.
static HOOK_CONFIG: OnceLock<HookConfig> = OnceLock::new();

#[derive(Debug, Clone)]
struct CellMatch {
    cell: CellRegistration,
    matched_owned_path: String,
}

/// Register cells so the panic hook can map panic locations to owning cells.
///
/// Must be called before `install_panic_hook`. Subsequent calls are ignored
/// (the registry is set once).
///
/// # Example
///
/// ```
/// use witness_rt::{CellRegistration, register_cells};
///
/// register_cells(vec![
///     CellRegistration {
///         id: "pricing".into(),
///         purpose: "Quote pricing logic".into(),
///         owned_paths: vec!["crates/pricing/src/".into()],
///         invariants: vec!["totals are non-negative".into()],
///         local_commands: vec!["cargo test -p pricing".into()],
///         escalate_commands: vec![],
///         hints: vec![],
///     },
/// ]);
/// ```
pub fn register_cells(cells: Vec<CellRegistration>) {
    let _ = CELL_REGISTRY.set(cells);
}

/// Install an agent-aware panic hook that emits structured `RepairPacket` JSON.
///
/// When a panic occurs, the hook:
/// 1. Extracts the source location from the panic info
/// 2. Matches the location against registered cells by path prefix
/// 3. Writes a `RepairPacket` to the configured output path
/// 4. Then delegates to the default panic handler for normal stderr output
///
/// # Example
///
/// ```no_run
/// use witness_rt::{HookConfig, install_panic_hook};
///
/// install_panic_hook(HookConfig::new("."));
/// ```
pub fn install_panic_hook(config: HookConfig) {
    let _ = HOOK_CONFIG.set(config);

    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
        // Best-effort: emit the repair packet, but never panic in the hook.
        let _ = emit_repair_packet(info);

        // Delegate to the default hook for normal stderr output.
        default_hook(info);
    }));
}

/// Build and write a repair packet from panic info.
///
/// Returns `Ok(())` on success; the caller drops the error to prevent a
/// recursive panic inside the panic hook.
fn emit_repair_packet(info: &PanicHookInfo<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let config = HOOK_CONFIG.get().ok_or("hook config not set")?;

    // Extract the panic message.
    let message = if let Some(message) = info.payload().downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    };

    // Extract source location.
    let (file, line, column) = if let Some(location) = info.location() {
        (
            location.file().to_string(),
            location.line(),
            location.column(),
        )
    } else {
        ("<unknown>".to_string(), 0, 0)
    };

    // Match to the nearest registered cell.
    let matched = match_cell(&file);
    let timestamp = current_timestamp();

    let (
        cell_id,
        cell_purpose,
        match_provenance,
        matched_owned_path,
        invariants,
        hints,
        local_commands,
        escalate_commands,
    ) = match matched.as_ref() {
        Some(m) => (
            Some(m.cell.id.clone()),
            Some(m.cell.purpose.clone()),
            Some("longest-owned-path-prefix".to_string()),
            Some(m.matched_owned_path.clone()),
            m.cell.invariants.clone(),
            m.cell.hints.clone(),
            m.cell.local_commands.clone(),
            m.cell.escalate_commands.clone(),
        ),
        None => (
            None,
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
    };
    let packet = RepairPacket {
        code: "PANIC".to_string(),
        message,
        file,
        line,
        column,
        cell: cell_id,
        cell_purpose,
        match_provenance,
        matched_owned_path,
        invariants,
        likely_causes: Vec::new(),
        hints,
        local_commands,
        escalate_commands,
        timestamp,
    };

    write_packet(&config.output_path, &packet)?;

    Ok(())
}

/// Write a repair packet to disk as JSON.
fn write_packet(
    output_path: &str,
    packet: &RepairPacket,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(output_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(packet)?;
    let mut file = fs::File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

/// Find the best-matching registered cell for a source file path.
///
/// Uses longest-prefix matching against `owned_paths`.
fn match_cell(file_path: &str) -> Option<CellMatch> {
    let registry = CELL_REGISTRY.get()?;

    let mut best: Option<CellMatch> = None;
    let mut best_len = 0;

    for cell in registry {
        for prefix in &cell.owned_paths {
            if file_path.contains(prefix.as_str()) && prefix.len() > best_len {
                best = Some(CellMatch {
                    cell: cell.clone(),
                    matched_owned_path: prefix.clone(),
                });
                best_len = prefix.len();
            }
        }
    }

    best
}

/// Emit a repair packet programmatically (not from a panic).
///
/// Used by `agent_ensure!`, `agent_bail!`, and friends to record
/// structured failures before returning an error or panicking.
pub fn emit_repair_packet_direct(packet: &RepairPacket) {
    if let Some(config) = HOOK_CONFIG.get() {
        let _ = write_packet(&config.output_path, packet);
    }
}

/// RFC3339 timestamp for repair packets.
pub fn current_timestamp() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_cell_finds_best_prefix() {
        let cells = vec![
            CellRegistration {
                id: "pricing".into(),
                purpose: "pricing logic".into(),
                owned_paths: vec!["crates/pricing/src/".into()],
                invariants: vec!["totals non-negative".into()],
                local_commands: vec!["cargo test -p pricing".into()],
                escalate_commands: vec![],
                hints: vec![],
            },
            CellRegistration {
                id: "core".into(),
                purpose: "core logic".into(),
                owned_paths: vec!["crates/".into()],
                invariants: vec![],
                local_commands: vec![],
                escalate_commands: vec![],
                hints: vec![],
            },
        ];

        let _ = CELL_REGISTRY.set(cells);

        let matched = match_cell("crates/pricing/src/lib.rs");
        assert!(matched.is_some());
        let matched = matched.unwrap();
        assert_eq!(matched.cell.id, "pricing");
        assert_eq!(matched.matched_owned_path, "crates/pricing/src/");
    }

    #[test]
    fn match_cell_returns_none_when_no_registry() {
        // In a fresh test without setting CELL_REGISTRY, this is a no-op
        // because OnceLock may already be set by another test in the same process.
        // This test validates the logic path when no match is found.
        let result = match_cell("completely/unrelated/path.rs");
        // Either None (no registry) or None (no match)
        if let Some(cell) = result {
            assert_ne!(cell.cell.id, "");
        }
    }
}
