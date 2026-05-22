//! Owner: VTI Test Intelligence subsystem — testmap.toml parser
//! Proof: `cargo nextest run -p jeryu -- test_intel::testmap`
//! Invariants: Parsed test maps preserve lane semantics and reject ambiguous ownership where possible.
//! Parses `.jeryu/testmap.toml` files for external workspace integration.
//!
//! This module provides VTI support for repos *other than* JeRyu itself
//! (e.g., the dougx workspace) by reading a TOML-based subsystem map.

use serde::Deserialize;
use std::path::Path;

// ---------------------------------------------------------------------------
// TOML schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TestMap {
    pub policy: TestMapPolicy,
    pub global_invalidators: TestMapPaths,
    #[serde(default)]
    pub docs: Option<TestMapPaths>,
    #[serde(default)]
    pub subsystem: Vec<TestMapSubsystem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestMapPolicy {
    pub full_on_unknown: bool,
    pub min_confidence: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestMapPaths {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestMapSubsystem {
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub paths: Vec<String>,
    pub ci_jobs: Vec<String>,
    #[serde(default)]
    pub cross_cutting: bool,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Load and parse a `.jeryu/testmap.toml` file.
pub fn load_testmap(path: &Path) -> Result<TestMap, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    toml::from_str(&content).map_err(|e| format!("failed to parse {}: {}", path.display(), e))
}

#[path = "testmap_plan.rs"]
mod plan;
pub use plan::{ExternalPlanMode, ExternalTestPlan, plan_from_testmap};

#[path = "testmap_render.rs"]
mod render;

pub use render::{
    emit_external_gitlab_yaml, explain_external_json, explain_external_plan,
    explain_external_skipped_json,
};

#[cfg(test)]
#[path = "testmap_tests.rs"]
mod tests;
