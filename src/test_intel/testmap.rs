//! Owner: VTI Test Intelligence subsystem — testmap.toml parser
//! Proof: `cargo nextest run -p jeryu -- test_intel::testmap`
//! Invariants: Parsed test maps preserve lane semantics and reject ambiguous ownership where possible.
//! Parses `.jeryu/testmap.toml` files for external workspace integration.
//!
//! This module provides VTI support for repos *other than* JeRyu itself
//! (e.g., the dougx workspace) by reading a TOML-based subsystem map.

use super::subsystem::glob_match;
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

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// A plan produced by VTI for an external workspace.
#[derive(Debug, Clone)]
pub struct ExternalTestPlan {
    pub mode: ExternalPlanMode,
    pub selected_jobs: Vec<String>,
    pub skipped_jobs: Vec<String>,
    pub affected_subsystems: Vec<String>,
    pub confidence: f64,
    pub rationale: Vec<String>,
    pub changed_paths: Vec<String>,
    pub repair_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternalPlanMode {
    Full,
    Selected,
    DocsOnly,
}

impl ExternalTestPlan {
    pub fn repair_reason(&self) -> Option<&str> {
        self.repair_reason.as_deref()
    }
}

/// Compute a test plan from a `.jeryu/testmap.toml` and a list of changed paths.
pub fn plan_from_testmap(map: &TestMap, changed_paths: &[String]) -> ExternalTestPlan {
    let mut rationale = Vec::new();

    // 1. Empty diff → full
    if changed_paths.is_empty() {
        return ExternalTestPlan {
            mode: ExternalPlanMode::Full,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: vec![],
            confidence: 1.0,
            rationale: vec!["empty diff, running full pipeline".into()],
            changed_paths: vec![],
            repair_reason: Some("empty diff".into()),
        };
    }

    // 2. Check global invalidators
    for path in changed_paths {
        for pattern in &map.global_invalidators.paths {
            if glob_match(pattern, path) {
                rationale.push(format!(
                    "global invalidator hit: '{}' matched pattern '{}'",
                    path, pattern
                ));
                return ExternalTestPlan {
                    mode: ExternalPlanMode::Full,
                    selected_jobs: vec![],
                    skipped_jobs: vec![],
                    affected_subsystems: vec![],
                    confidence: 1.0,
                    rationale,
                    changed_paths: changed_paths.to_vec(),
                    repair_reason: Some(format!("global invalidator: {}", path)),
                };
            }
        }
    }

    // 3. Check docs-only
    let docs_patterns = map.docs.as_ref().map(|d| d.paths.as_slice()).unwrap_or(&[]);
    let all_docs = !changed_paths.is_empty()
        && changed_paths
            .iter()
            .all(|p| docs_patterns.iter().any(|pat| glob_match(pat, p)));
    if all_docs {
        rationale.push("all changed files match docs-only patterns".into());
        return ExternalTestPlan {
            mode: ExternalPlanMode::DocsOnly,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: vec![],
            confidence: 1.0,
            rationale,
            changed_paths: changed_paths.to_vec(),
            repair_reason: None,
        };
    }

    // 4. Match subsystems
    let mut affected: Vec<&TestMapSubsystem> = Vec::new();
    let mut unmatched_paths: Vec<&str> = Vec::new();

    for path in changed_paths {
        // Skip docs files when mixed with code
        if docs_patterns.iter().any(|pat| glob_match(pat, path)) {
            continue;
        }

        let mut matched = false;
        for sub in &map.subsystem {
            if sub.paths.iter().any(|pat| glob_match(pat, path)) {
                if !affected.iter().any(|a| a.id == sub.id) {
                    affected.push(sub);
                    rationale.push(format!(
                        "selected '{}' because '{}' matches its paths",
                        sub.id, path
                    ));
                }
                matched = true;
            }
        }
        if !matched {
            unmatched_paths.push(path);
        }
    }

    // 5. Unmatched paths → conservative recovery (if policy says so)
    if !unmatched_paths.is_empty() && map.policy.full_on_unknown {
        rationale.push(format!(
            "unmatched paths with full_on_unknown=true: {:?}",
            unmatched_paths
        ));
        return ExternalTestPlan {
            mode: ExternalPlanMode::Full,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: affected.iter().map(|s| s.id.clone()).collect(),
            confidence: 0.0,
            rationale,
            changed_paths: changed_paths.to_vec(),
            repair_reason: Some("unmatched paths".into()),
        };
    }

    // 6. Collect jobs from affected subsystems
    let mut selected_jobs: Vec<String> = Vec::new();
    let mut affected_ids: Vec<String> = Vec::new();
    let mut has_cross_cutting = false;

    for sub in &affected {
        affected_ids.push(sub.id.clone());
        if sub.cross_cutting {
            has_cross_cutting = true;
        }
        for job in &sub.ci_jobs {
            if !selected_jobs.contains(job) {
                selected_jobs.push(job.clone());
            }
        }
    }

    // 7. Compute all known jobs, then skipped = all - selected
    let all_jobs: Vec<String> = map
        .subsystem
        .iter()
        .flat_map(|s| s.ci_jobs.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let skipped_jobs: Vec<String> = all_jobs
        .iter()
        .filter(|j| !selected_jobs.contains(j))
        .cloned()
        .collect();

    // 8. Confidence scoring
    let mut confidence = 1.0_f64;
    if has_cross_cutting {
        confidence -= 0.10;
    }
    if affected.len() > 3 {
        confidence -= 0.05 * (affected.len() as f64 - 3.0);
    }
    if !unmatched_paths.is_empty() {
        confidence -= 0.15 * (unmatched_paths.len() as f64 / changed_paths.len() as f64);
    }
    confidence = confidence.clamp(0.0, 1.0);

    // 9. Check minimum confidence
    if confidence < map.policy.min_confidence {
        rationale.push(format!(
            "confidence {:.2} below threshold {:.2}; escalating to full",
            confidence, map.policy.min_confidence
        ));
        return ExternalTestPlan {
            mode: ExternalPlanMode::Full,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: affected_ids,
            confidence,
            rationale,
            changed_paths: changed_paths.to_vec(),
            repair_reason: Some(format!("low confidence: {:.2}", confidence)),
        };
    }

    ExternalTestPlan {
        mode: ExternalPlanMode::Selected,
        selected_jobs,
        skipped_jobs,
        affected_subsystems: affected_ids,
        confidence,
        rationale,
        changed_paths: changed_paths.to_vec(),
        repair_reason: None,
    }
}

#[path = "testmap_render.rs"]
mod render;

pub use render::{
    emit_external_gitlab_yaml, explain_external_json, explain_external_plan,
    explain_external_skipped_json,
};

#[cfg(test)]
#[path = "testmap_tests.rs"]
mod tests;
