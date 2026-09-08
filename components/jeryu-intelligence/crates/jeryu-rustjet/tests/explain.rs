mod common;

use jeryu_rustjet::{AffectedPlanner, ChangeSet, ExplainFormat, ExplainOutput, PlannerOptions};

#[test]
fn json_explain_contains_required_fields() {
    let graph = common::graph();
    let plan = AffectedPlanner::new(&graph).plan(
        &ChangeSet::new(["crates/core/src/lib.rs"]),
        &PlannerOptions::default(),
    );
    let json = ExplainOutput::render(&plan, ExplainFormat::Json).body;
    assert!(json.contains("\"runner_class\""));
    assert!(json.contains("\"sccache_mode\""));
    assert!(json.contains("\"affected_packages\""));
    assert!(json.contains("public-api-change"));
}

#[test]
fn text_explain_is_human_readable() {
    let graph = common::graph();
    let plan = AffectedPlanner::new(&graph)
        .plan(&ChangeSet::new(["README.md"]), &PlannerOptions::default());
    let text = ExplainOutput::render(&plan, ExplainFormat::Text).body;
    assert!(text.contains("runner="));
    assert!(text.contains("documentation-only"));
}
