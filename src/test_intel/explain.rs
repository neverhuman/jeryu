//! Owner: VTI Test Intelligence subsystem — plan explanation
//! Proof: `cargo nextest run -p jeryu -- test_intel::explain`
//! Invariants: Explanations cite concrete map, dependency, or history evidence for every selected test.
//! Human-readable explanations for test plans.
//!
//! Provides a structured summary of what a test plan decided and why,
//! suitable for CLI output, TUI display, and CI job logs.

use crate::test_intel::planner::{TestPlan, TestPlanMode};

/// Render a human-readable explanation of a test plan.
pub fn explain(plan: &TestPlan) -> String {
    let mut out = String::new();

    // Header
    let mode_label = match &plan.mode {
        TestPlanMode::Full => "FULL (all tests)",
        TestPlanMode::Selected => "SELECTED (targeted tests)",
        TestPlanMode::DocsOnly => "DOCS-ONLY (no Rust tests)",
    };
    out.push_str("╭─ jeryu Test Intelligence Plan ─────────────────╮\n");
    out.push_str(&format!("│ Mode: {:<40} │\n", mode_label));
    out.push_str(&format!("│ Confidence: {:<34.2} │\n", plan.confidence));
    out.push_str("╰───────────────────────────────────────────────╯\n\n");

    // Changed files
    if !plan.changed_paths.is_empty() {
        out.push_str("Changed:\n");
        for path in &plan.changed_paths {
            out.push_str(&format!("  ● {}\n", path));
        }
        out.push('\n');
    }

    // Recovery reason
    if let Some(reason) = &plan.repair_reason {
        out.push_str(&format!("⚠ Recovery: {}\n\n", reason));
    }

    // Selected tests
    if !plan.selected_tests.is_empty() {
        out.push_str("Selected tests:\n");
        for test in &plan.selected_tests {
            out.push_str(&format!(
                "  ✓ [{}] {}\n    reason: {}\n",
                test.subsystem, test.command, test.reason
            ));
        }
        out.push('\n');
    }

    // Sentinel tests
    if !plan.sentinel_tests.is_empty() {
        out.push_str("Sentinel samples (allow_failure):\n");
        for test in &plan.sentinel_tests {
            out.push_str(&format!("  ◉ {}\n", test.command));
        }
        out.push('\n');
    }

    // Skipped subsystems
    if !plan.skipped_subsystems.is_empty() {
        out.push_str("Skipped subsystems:\n");
        for subsystem in &plan.skipped_subsystems {
            out.push_str(&format!("  ○ {}\n", subsystem));
        }
        out.push('\n');
    }

    // Rationale
    if !plan.rationale.is_empty() {
        out.push_str("Rationale:\n");
        for reason in &plan.rationale {
            out.push_str(&format!("  → {}\n", reason));
        }
        out.push('\n');
    }

    // Summary
    let selected = plan.selected_tests.len();
    let sentinels = plan.sentinel_tests.len();
    let skipped = plan.skipped_subsystems.len();
    out.push_str(&format!(
        "Summary: {} test commands selected, {} sentinel samples, {} subsystems skipped\n",
        selected, sentinels, skipped
    ));

    out
}

/// Render a JSON explanation of a test plan.
pub fn explain_json(plan: &TestPlan) -> serde_json::Value {
    serde_json::to_value(plan).unwrap_or(serde_json::json!({"error": "serialization failed"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_intel::planner;

    #[test]
    fn explain_full_plan() {
        let plan = planner::plan_tests(&["Cargo.toml".to_string()]);
        let output = explain(&plan);
        assert!(output.contains("FULL"));
        assert!(output.contains("global invalidator"));
    }

    #[test]
    fn explain_selected_plan() {
        let plan = planner::plan_tests(&["src/tui/ui.rs".to_string()]);
        let output = explain(&plan);
        assert!(output.contains("SELECTED"));
        assert!(output.contains("tui"));
        assert!(output.contains("Skipped subsystems"));
    }

    #[test]
    fn explain_docs_plan() {
        let plan = planner::plan_tests(&["README.md".to_string()]);
        let output = explain(&plan);
        assert!(output.contains("DOCS-ONLY"));
    }

    #[test]
    fn explain_json_roundtrips() {
        let plan = planner::plan_tests(&["src/pool.rs".to_string()]);
        let json = explain_json(&plan);
        assert_eq!(json["mode"], "selected");
        assert!(
            json["affected_subsystems"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "pool")
        );
    }
}
