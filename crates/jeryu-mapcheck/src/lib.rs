//! Jankurai governance gates ported from the legacy `scripts/check-*.py` validators.
//!
//! Each gate is a faithful Rust port of one Python script. The public functions
//! return a [`GateReport`] that records the pass/fail outcome together with the
//! exact stdout signal lines the original scripts emitted, so the binary can map
//! the outcome onto identical exit codes and other tools can keep parsing the
//! same marker lines.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Python truthiness for an optional string field: a missing value (`None`) and
/// an empty string are both "falsy". Mirrors `not entry.get("proof_lane")`.
fn is_blank(value: Option<&str>) -> bool {
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
    fn pass(signal: &str) -> Self {
        Self {
            ok: true,
            lines: vec![signal.to_string()],
        }
    }

    fn fail(lines: Vec<String>) -> Self {
        Self { ok: false, lines }
    }
}

// ---------------------------------------------------------------------------
// owner-test-map  (scripts/check-owner-test-map.py)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// agent-maps  (scripts/check-agent-maps.py)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// generated-zones  (scripts/check-generated-zones.py)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Zone {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    generator: Option<String>,
    #[serde(default)]
    manual_edits: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GeneratedZones {
    #[serde(default)]
    zones: Vec<Zone>,
}

/// Required generated zones and their expected generators, in the order the
/// Python dict declared them.
const REQUIRED_ZONES: [(&str, &str); 2] = [
    ("docs/generated/**", "scripts/render-policy-docs.sh"),
    ("receipts/generated/**", "jeryu-cache-service"),
];

/// Port of `scripts/check-generated-zones.py`.
///
/// Validates that the generated-zones manifest declares each required zone with
/// the expected generator and `manual_edits = false`.
///
/// # Errors
/// Returns an error if the manifest cannot be read or parsed as TOML.
pub fn generated_zones(zones_toml: &Path) -> Result<GateReport> {
    let raw = std::fs::read_to_string(zones_toml)
        .with_context(|| format!("reading {}", zones_toml.display()))?;
    let config: GeneratedZones = toml::from_str(&raw)
        .with_context(|| format!("parsing {} as TOML", zones_toml.display()))?;

    let mut missing = Vec::new();
    for (zone_path, generator) in REQUIRED_ZONES {
        let zone = config
            .zones
            .iter()
            .find(|z| z.path.as_deref() == Some(zone_path));
        match zone {
            None => missing.push(format!("missing generated zone: {zone_path}")),
            Some(zone) => {
                if zone.generator.as_deref() != Some(generator) {
                    missing.push(format!(
                        "generated zone {zone_path} has generator {}, expected {}",
                        py_repr_opt(zone.generator.as_deref()),
                        py_repr(generator)
                    ));
                } else if zone.manual_edits != Some(false) {
                    missing.push(format!(
                        "generated zone {zone_path} must set manual_edits = false"
                    ));
                }
            }
        }
    }

    if missing.is_empty() {
        Ok(GateReport::pass("generated zones ok"))
    } else {
        Ok(GateReport::fail(missing))
    }
}

// ---------------------------------------------------------------------------
// fixtures  (scripts/check-fixtures.py)
// ---------------------------------------------------------------------------

/// Port of `scripts/check-fixtures.py`.
///
/// Recursively parses every `*.json` file under `fixtures_dir`, skipping
/// AppleDouble (`._*`) sidecar files and any path component beginning with
/// `._`. A parse failure fails the gate.
///
/// # Errors
/// Returns an error if the fixtures directory cannot be traversed, if a file
/// cannot be read, or if a fixture is not valid JSON (mirroring the Python
/// `json.loads` raising).
pub fn fixtures(fixtures_dir: &Path) -> Result<GateReport> {
    let mut json_files = Vec::new();
    collect_json_files(fixtures_dir, &mut json_files)?;
    json_files.sort();

    for path in json_files {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str::<serde_json::Value>(&text)
            .with_context(|| format!("parsing {} as JSON", path.display()))?;
    }

    Ok(GateReport::pass("fixtures ok"))
}

/// Whether a single path component is an AppleDouble sidecar.
fn is_apple_double(component: &str) -> bool {
    component.starts_with("._")
}

/// Recursively gather `*.json` files, skipping any `._`-prefixed component.
fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("reading entry in {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("stat {}", path.display()))?;
        if file_type.is_dir() {
            collect_json_files(&path, out)?;
        } else if file_type.is_file() {
            // Mirror Python: skip if the file (or any path part) starts with "._",
            // and only consider files ending in ".json".
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".json") {
                continue;
            }
            // Check every path component for a "._" prefix, matching
            // `any(part.startswith("._") for part in path.parts)` plus the
            // explicit `path.name.startswith("._")` guard in the Python.
            let skip = path
                .components()
                .any(|c| is_apple_double(&c.as_os_str().to_string_lossy()));
            if skip || is_apple_double(&name) {
                continue;
            }
            out.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// docs  (scripts/check-docs.py)
// ---------------------------------------------------------------------------

/// Required documents and the substrings (markers) each must contain, ported
/// verbatim from `check-docs.py`.
pub const REQUIRED_DOCS: [(&str, &[&str]); 4] = [
    ("README.md", &["Jeryu", "JeryuCache", "Phase 12"]),
    (
        "docs/engineering_spec.md",
        &[
            "Jeryu Engineering Spec",
            "Cache correctness beats cache hit rate",
        ],
    ),
    ("docs/PHASE12_SPEC.md", &["Phase 12", "Zero false hits"]),
    ("docs/RUNBOOKS.md", &["runbooks"]),
];

/// Port of `scripts/check-docs.py`.
///
/// For each required doc (resolved relative to `base`), checks the file exists
/// and contains every required marker substring.
///
/// # Errors
/// Returns an error only if a file that exists cannot be read as UTF-8 text. A
/// missing file or a missing marker is a normal gate failure (not an error),
/// mirroring the Python behavior.
pub fn docs(base: &Path) -> Result<GateReport> {
    let mut missing = Vec::new();
    for (rel, needles) in REQUIRED_DOCS {
        let path = base.join(rel);
        if !path.exists() {
            missing.push(format!("missing required doc: {rel}"));
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        for needle in needles {
            if !text.contains(needle) {
                missing.push(format!("{rel} missing marker: {needle}"));
            }
        }
    }

    if missing.is_empty() {
        Ok(GateReport::pass("docs ok"))
    } else {
        Ok(GateReport::fail(missing))
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing {} as JSON", path.display()))
}

/// Render a list of strings the way Python prints `['a', 'b']`.
fn py_str_list(items: &[&str]) -> String {
    let inner = items
        .iter()
        .map(|s| py_repr(s))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

/// Render a Python `repr()` of a string: single-quoted.
fn py_repr(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Render a Python `repr()` of an optional string (`None` when absent).
fn py_repr_opt(s: Option<&str>) -> String {
    match s {
        Some(v) => py_repr(v),
        None => "None".to_string(),
    }
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

fn py_str_list_owned(items: &[String]) -> String {
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    py_str_list(&refs)
}
