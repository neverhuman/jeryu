use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::model::{CompilePacket, CompilePackets, CompileSummary};

/// Run `cargo check --message-format=json` and route each diagnostic
/// to its owning ARC.
///
/// Enriches each diagnostic with cell purpose, invariants, and local
/// test commands from the workspace snapshot.
pub fn diagnose_workspace(
    workspace_root: &Path,
    manifest_path: Option<&Path>,
) -> Result<CompilePackets> {
    let manifest_path = manifest_path
        .map(|path| {
            path.canonicalize()
                .with_context(|| format!("failed to canonicalize {}", path.display()))
        })
        .transpose()?;
    let snapshot = cargo_vrc::load_workspace(manifest_path.as_deref())?;

    // Run cargo check and capture JSON diagnostics.
    let mut command = Command::new("cargo");
    command
        .arg("check")
        .arg("--workspace")
        .arg("--message-format=json")
        .current_dir(workspace_root);
    if let Some(path) = manifest_path.as_deref() {
        command.arg("--manifest-path").arg(path);
    }

    let output = command
        .output()
        .context("failed to run cargo check --message-format=json")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut packets = Vec::new();
    let mut arcs_affected = BTreeSet::new();
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;

    for line in stdout.lines() {
        let Ok(msg) = serde_json::from_str::<CargoMessage>(line) else {
            continue;
        };

        if msg.reason != "compiler-message" {
            continue;
        }

        let Some(diagnostic) = msg.message else {
            continue;
        };

        // Only route errors and warnings, not notes/help.
        if !matches!(diagnostic.level.as_str(), "error" | "warning") {
            continue;
        }

        let (file, line_num, column) = resolve_diagnostic_location(&diagnostic.spans);

        // Route to owning ARC.
        let owning_pkg = snapshot.packages.iter().find(|package| {
            let pkg_root = match package.package_root.strip_prefix(&snapshot.workspace_root) {
                Ok(rel) => rel.display().to_string(),
                Err(_) => String::new(),
            };
            file.starts_with(&pkg_root)
        });
        let owning_arc = match owning_pkg {
            Some(package) => package.name.clone(),
            None => "<unmatched>".to_string(),
        };

        let pkg = snapshot.packages.iter().find(|p| p.name == owning_arc);
        let cell_purpose = match pkg {
            Some(p) if !p.agent.purpose.is_empty() => Some(p.agent.purpose.clone()),
            _ => None,
        };
        let invariants = match pkg {
            Some(p) => p.agent.invariants.clone(),
            None => Vec::new(),
        };
        let local_commands = match pkg {
            Some(p) => p.agent.local_validate.clone(),
            None => Vec::new(),
        };

        // Extract compiler suggestion if present.
        let compiler_suggestion = diagnostic
            .children
            .iter()
            .find(|child| child.level == "help")
            .map(|child| child.message.clone());

        match diagnostic.level.as_str() {
            "error" => total_errors += 1,
            "warning" => total_warnings += 1,
            _ => {}
        }
        arcs_affected.insert(owning_arc.clone());

        packets.push(CompilePacket {
            level: diagnostic.level,
            code: diagnostic.code.as_ref().map(|c| c.code.clone()),
            message: diagnostic.message,
            file,
            line: line_num,
            column,
            owning_arc,
            cell_purpose,
            invariants,
            local_commands,
            compiler_suggestion,
        });
    }

    Ok(CompilePackets {
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        packets,
        summary: CompileSummary {
            total_errors,
            total_warnings,
            arcs_affected: arcs_affected.len(),
        },
    })
}

// ── Span selection ─────────────────────────────────────────────────────

/// Outcome of picking a span to attribute a diagnostic to.
///
/// Cargo normally marks one span with `is_primary = true`, but it
/// occasionally emits diagnostics where no span is primary (for
/// example, certain crate-level lints). In that case we deliberately
/// use the first span as a secondary location so the diagnostic still
/// routes to a real file/line rather than `<unknown>`.
enum SpanChoice<'a> {
    /// A span explicitly marked `is_primary = true`.
    Primary(&'a DiagnosticSpan),
    /// No primary span existed; first span used as a documented secondary location.
    FirstSecondary(&'a DiagnosticSpan),
    /// Diagnostic carried no spans at all.
    None,
}

impl<'a> SpanChoice<'a> {
    fn from_spans(spans: &'a [DiagnosticSpan]) -> Self {
        if let Some(span) = spans.iter().find(|span| span.is_primary) {
            SpanChoice::Primary(span)
        } else if let Some(span) = spans.first() {
            SpanChoice::FirstSecondary(span)
        } else {
            SpanChoice::None
        }
    }

    fn span(&self) -> Option<&'a DiagnosticSpan> {
        match self {
            SpanChoice::Primary(span) | SpanChoice::FirstSecondary(span) => Some(span),
            SpanChoice::None => None,
        }
    }
}

/// Resolve `(file, line, column)` for a diagnostic, preferring the
/// primary span and using the first span as a secondary location when
/// cargo emits no primary. When the diagnostic has no spans at all,
/// returns the typed unknown sentinel `("<unknown>", 0, 0)`.
fn resolve_diagnostic_location(spans: &[DiagnosticSpan]) -> (String, u32, u32) {
    match SpanChoice::from_spans(spans).span() {
        Some(span) => (span.file_name.clone(), span.line_start, span.column_start),
        None => ("<unknown>".to_string(), 0, 0),
    }
}

// ── Cargo JSON diagnostic types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CargoMessage {
    reason: String,
    #[serde(default)]
    message: Option<Diagnostic>,
}

#[derive(Debug, Deserialize)]
struct Diagnostic {
    level: String,
    message: String,
    #[serde(default)]
    code: Option<DiagnosticCode>,
    #[serde(default)]
    spans: Vec<DiagnosticSpan>,
    #[serde(default)]
    children: Vec<DiagnosticChild>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticCode {
    code: String,
}

#[derive(Debug, Deserialize)]
struct DiagnosticSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    #[serde(default)]
    is_primary: bool,
}

#[derive(Debug, Deserialize)]
struct DiagnosticChild {
    level: String,
    message: String,
}
