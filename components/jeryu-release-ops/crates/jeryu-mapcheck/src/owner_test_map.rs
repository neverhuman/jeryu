//! Port of `scripts/check-owner-test-map.py`: required glob roots + entry shape.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use crate::render::{py_repr_opt, py_str_list, py_str_list_owned, read_json};
use crate::report::{GateReport, is_blank};

/// The twelve required glob roots from `check-owner-test-map.py`.
const OWNER_TEST_REQUIRED_ROOTS: [&str; 12] = [
    "agent/**",
    "bins/**",
    "config/**",
    "configs/**",
    "crates/**",
    "docs/**",
    "examples/**",
    "fixtures/**",
    "ops/**",
    "policies/**",
    "scripts/**",
    "tests/**",
];

/// An `owners` entry as consumed by `check-owner-test-map.py`.
#[derive(Debug, Deserialize)]
struct OwnerEntry {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    owners: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OwnerMapList {
    #[serde(default)]
    owners: Vec<OwnerEntry>,
}

/// A `routes` entry as consumed by `check-owner-test-map.py`.
#[derive(Debug, Deserialize)]
struct RouteEntry {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default)]
    proof_lane: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TestMapList {
    #[serde(default)]
    routes: Vec<RouteEntry>,
}

/// Port of `scripts/check-owner-test-map.py`.
///
/// Validates that the owner map and test map cover the twelve required glob
/// roots and that no entry is missing its required fields.
///
/// # Errors
/// Returns an error if either map file cannot be read or parsed as JSON.
pub fn owner_test_map(owner_map: &Path, test_map: &Path) -> Result<GateReport> {
    let owner: OwnerMapList = read_json(owner_map)?;
    let tests: TestMapList = read_json(test_map)?;

    let owner_patterns: BTreeSet<&str> = owner
        .owners
        .iter()
        .flat_map(|entry| entry.paths.iter().map(String::as_str))
        .collect();
    let test_patterns: BTreeSet<&str> = tests
        .routes
        .iter()
        .flat_map(|entry| entry.paths.iter().map(String::as_str))
        .collect();

    let missing_owner: Vec<&str> = OWNER_TEST_REQUIRED_ROOTS
        .iter()
        .copied()
        .filter(|root| !owner_patterns.contains(root))
        .collect();
    let missing_tests: Vec<&str> = OWNER_TEST_REQUIRED_ROOTS
        .iter()
        .copied()
        .filter(|root| !test_patterns.contains(root))
        .collect();

    let bad_owners: Vec<&OwnerEntry> = owner
        .owners
        .iter()
        .filter(|entry| entry.paths.is_empty() || entry.owners.is_empty())
        .collect();
    let bad_tests: Vec<&RouteEntry> = tests
        .routes
        .iter()
        .filter(|entry| {
            entry.paths.is_empty()
                || entry.commands.is_empty()
                || is_blank(entry.proof_lane.as_deref())
        })
        .collect();

    if missing_owner.is_empty()
        && missing_tests.is_empty()
        && bad_owners.is_empty()
        && bad_tests.is_empty()
    {
        return Ok(GateReport::pass("owner/test map ok"));
    }

    let mut lines = Vec::new();
    if !missing_owner.is_empty() {
        lines.push(format!(
            "missing owner paths: {}",
            py_str_list(&missing_owner)
        ));
    }
    if !missing_tests.is_empty() {
        lines.push(format!(
            "missing test paths: {}",
            py_str_list(&missing_tests)
        ));
    }
    if !bad_owners.is_empty() {
        lines.push(format!(
            "bad owner entries: {}",
            render_owner_entries(&bad_owners)
        ));
    }
    if !bad_tests.is_empty() {
        lines.push(format!(
            "bad test entries: {}",
            render_route_entries(&bad_tests)
        ));
    }
    Ok(GateReport::fail(lines))
}

fn render_owner_entries(entries: &[&OwnerEntry]) -> String {
    let inner = entries
        .iter()
        .map(|e| {
            format!(
                "{{'paths': {}, 'owners': {}}}",
                py_str_list_owned(&e.paths),
                py_str_list_owned(&e.owners)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

fn render_route_entries(entries: &[&RouteEntry]) -> String {
    let inner = entries
        .iter()
        .map(|e| {
            format!(
                "{{'paths': {}, 'commands': {}, 'proof_lane': {}}}",
                py_str_list_owned(&e.paths),
                py_str_list_owned(&e.commands),
                py_repr_opt(e.proof_lane.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}
