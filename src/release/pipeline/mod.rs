use super::*;

mod doctor;
#[path = "doctor_render.rs"]
mod doctor_render;
mod explain;
mod promotion;

pub use doctor::build_pipeline_doctor_report;
pub use doctor_render::render_pipeline_doctor_text;
pub use explain::{build_pipeline_explain_report, render_pipeline_explain_text};
pub use promotion::{maybe_trigger_production_promotion, trigger_production_promotion};

#[cfg(test)]
pub(crate) fn should_trigger_production_promotion_with_gate(
    view: &ReleaseAttemptView,
    prod_gate_exists: bool,
) -> bool {
    view.canary_state == "e2e-passed"
        && view.release_identity_ok
        && view.has_remote_gate
        && view.has_telemetry_gate
        && view.has_e2e_gate
        && !prod_gate_exists
}
