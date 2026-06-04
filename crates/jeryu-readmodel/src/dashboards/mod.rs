//! Per-lens typed dashboard contracts. Each dashboard is a pure typed view
//! consumed by exactly one lens; real projections fill them and lenses render
//! whatever the projection produced (including empty/degraded states).
//!
//! The dashboards embedded directly in [`crate::TuiReadModel`] are ported here;
//! the remaining per-lens dashboards (cache, vti, ...) are served via dedicated
//! inspection routes and ported alongside those lenses.

pub mod agent_runs;
pub mod agents;
pub mod approvals;
pub mod codegraph;
pub mod evidence;
pub mod release;
pub mod runners;
pub mod source_doctor;
pub mod workcells;
pub mod workflow;
