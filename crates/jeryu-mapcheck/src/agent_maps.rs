//! Port of `scripts/check-agent-maps.py`: required roots + entry shape.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use crate::render::{py_repr_opt, py_str_list, py_str_list_owned, read_json};
use crate::report::{GateReport, is_blank};

/// The twelve required roots from `check-agent-maps.py` (bare directory names).
const AGENT_REQUIRED_ROOTS: [&str; 12] = [
    "agent", "bins", "config", "configs", "crates", "docs", "examples", "fixtures", "ops",
    "policies", "scripts", "tests",
];

#[derive(Debug, Deserialize)]
struct AgentOwnerEntry {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    owners: Vec<String>,
    #[serde(default)]
    required_reviews: i64,
}

#[derive(Debug, Deserialize)]
struct AgentOwnerMap {
    #[serde(default)]
    owners: Vec<AgentOwnerEntry>,
}

#[derive(Debug, Deserialize)]
struct AgentRouteEntry {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default)]
    proof_lane: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentTestMap {
    #[serde(default)]
    routes: Vec<AgentRouteEntry>,
}

/// First path segment, matching Python's `pattern.split("/", 1)[0]`.
fn first_segment(pattern: &str) -> &str {
    match pattern.split_once('/') {
        Some((head, _)) => head,
        None => pattern,
    }
}

/// Port of `scripts/check-agent-maps.py`.
///
/// Validates that the owner/test maps cover the twelve required repository
/// roots and that each entry carries its mandatory fields (owner entries must
/// also have `required_reviews >= 1`).
///
/// On failure this returns a single-line report whose text mirrors the
/// `SystemExit(...)` message the Python raised.
///
/// # Errors
/// Returns an error if either map file cannot be read or parsed as JSON.
pub fn agent_maps(owner_map: &Path, test_map: &Path) -> Result<GateReport> {
    let owner: AgentOwnerMap = read_json(owner_map)?;
    let tests: AgentTestMap = read_json(test_map)?;

    let owner_roots: BTreeSet<&str> = owner
        .owners
        .iter()
        .flat_map(|entry| entry.paths.iter().map(|p| first_segment(p)))
        .collect();
    let test_roots: BTreeSet<&str> = tests
        .routes
        .iter()
        .flat_map(|entry| entry.paths.iter().map(|p| first_segment(p)))
        .collect();

    let missing_owner: Vec<&str> = AGENT_REQUIRED_ROOTS
        .iter()
        .copied()
        .filter(|root| !owner_roots.contains(root))
        .collect();
    let missing_tests: Vec<&str> = AGENT_REQUIRED_ROOTS
        .iter()
        .copied()
        .filter(|root| !test_roots.contains(root))
        .collect();

    if !missing_owner.is_empty() || !missing_tests.is_empty() {
        return Ok(GateReport::fail(vec![format!(
            "missing owner={} tests={}",
            py_str_list(&missing_owner),
            py_str_list(&missing_tests)
        )]));
    }

    for entry in &owner.owners {
        if entry.paths.is_empty() || entry.owners.is_empty() || entry.required_reviews < 1 {
            return Ok(GateReport::fail(vec![format!(
                "bad owner map entry: {}",
                render_agent_owner_entry(entry)
            )]));
        }
    }
    for entry in &tests.routes {
        if entry.paths.is_empty()
            || entry.commands.is_empty()
            || is_blank(entry.proof_lane.as_deref())
        {
            return Ok(GateReport::fail(vec![format!(
                "bad test map entry: {}",
                render_agent_route_entry(entry)
            )]));
        }
    }

    Ok(GateReport::pass("agent maps cover repository paths"))
}

fn render_agent_owner_entry(e: &AgentOwnerEntry) -> String {
    format!(
        "{{'paths': {}, 'owners': {}, 'required_reviews': {}}}",
        py_str_list_owned(&e.paths),
        py_str_list_owned(&e.owners),
        e.required_reviews
    )
}

fn render_agent_route_entry(e: &AgentRouteEntry) -> String {
    format!(
        "{{'paths': {}, 'commands': {}, 'proof_lane': {}}}",
        py_str_list_owned(&e.paths),
        py_str_list_owned(&e.commands),
        py_repr_opt(e.proof_lane.as_deref())
    )
}
