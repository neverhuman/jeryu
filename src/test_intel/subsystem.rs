//! Owner: VTI Test Intelligence subsystem — subsystem ownership graph
//! Proof: `cargo nextest run -p jeryu -- test_intel::subsystem`
//! Invariants: Subsystem mappings stay deterministic and reflect the shared VTI contract.
//! Subsystem rules: maps source file paths to named subsystems and test commands.
//!
//! Each subsystem owns a set of source paths (via simple glob patterns), a nextest
//! filter expression for unit tests, a list of integration test binaries, and
//! a set of paths that force a full test run if changed.
//!
//! Uses a lightweight glob matcher (no external crate) since our patterns are
//! simple: `foo/*`, `foo/**`, `dir/**/*.ext`, and `*.ext`.

use serde::{Deserialize, Serialize};

#[path = "subsystem_glob.rs"]
mod glob;
pub(crate) use glob::{
    affected_subsystems, glob_match, has_global_invalidator, has_subsystem_force_full,
    is_docs_only, matches_any,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A named subsystem with its owned paths and test commands.
#[derive(Debug, Clone)]
pub struct Subsystem {
    pub id: &'static str,
    pub description: &'static str,
    /// Glob patterns for source files owned by this subsystem.
    pub owned_paths: &'static [&'static str],
    /// Nextest filter expression for unit tests.
    pub unit_filter: &'static str,
    /// Integration test binary names (from `tests/` directory).
    pub integration_tests: &'static [&'static str],
    /// If any of these paths change, force full test run.
    pub force_full_paths: &'static [&'static str],
    /// Runner tags required for this subsystem's tests.
    pub runner_tags: &'static [&'static str],
    /// Whether this subsystem is cross-cutting (changes affect many others).
    pub cross_cutting: bool,
}

/// Serializable representation for JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemInfo {
    pub id: String,
    pub description: String,
    pub owned_paths: Vec<String>,
    pub unit_filter: String,
    pub integration_tests: Vec<String>,
    pub cross_cutting: bool,
}

impl From<&Subsystem> for SubsystemInfo {
    fn from(s: &Subsystem) -> Self {
        Self {
            id: s.id.to_string(),
            description: s.description.to_string(),
            owned_paths: s.owned_paths.iter().map(|p| p.to_string()).collect(),
            unit_filter: s.unit_filter.to_string(),
            integration_tests: s.integration_tests.iter().map(|p| p.to_string()).collect(),
            cross_cutting: s.cross_cutting,
        }
    }
}

#[path = "subsystem_registry.rs"]
mod registry;
pub use registry::{DOCS_PATTERNS, GLOBAL_INVALIDATORS, SUBSYSTEMS};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "subsystem_tests.rs"]
mod tests;
