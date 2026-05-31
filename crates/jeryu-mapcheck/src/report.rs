//! Shared gate-report type and small predicate helpers used by every check.

/// Python truthiness for an optional string field: a missing value (`None`) and
/// an empty string are both "falsy". Mirrors `not entry.get("proof_lane")`.
pub(crate) fn is_blank(value: Option<&str>) -> bool {
    match value {
        None => true,
        Some(s) => s.is_empty(),
    }
}

/// Result of running a single governance gate.
///
/// `lines` holds the diagnostic/signal lines the gate would print, in order.
/// When `ok` is `false` the gate fails (the Python equivalent raised
/// `SystemExit(1)` / exited non-zero); when `true` the final line is the
/// success marker that downstream tooling greps for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReport {
    /// Whether the gate passed.
    pub ok: bool,
    /// Diagnostic and/or signal lines, in emission order.
    pub lines: Vec<String>,
}

impl GateReport {
    pub(crate) fn pass(signal: &str) -> Self {
        Self {
            ok: true,
            lines: vec![signal.to_string()],
        }
    }

    pub(crate) fn fail(lines: Vec<String>) -> Self {
        Self { ok: false, lines }
    }
}
