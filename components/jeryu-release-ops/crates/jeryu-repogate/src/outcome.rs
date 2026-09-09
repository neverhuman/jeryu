//! Shared gate outcome type and path-display helper used by every gate.

use std::path::Path;

/// Outcome of a gate: the lines to print on stdout and the process exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateOutcome {
    /// Lines to emit on stdout, in order.
    pub stdout: Vec<String>,
    /// Process exit code (0 = pass).
    pub exit_code: i32,
}

/// Render a path relative to `root` for display, falling back to the full path.
pub(crate) fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
