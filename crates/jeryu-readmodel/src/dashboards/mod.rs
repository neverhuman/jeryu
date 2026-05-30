//! Per-lens typed dashboard contracts. Each dashboard is a pure typed view
//! consumed by exactly one lens; real projections fill them and lenses render
//! whatever the projection produced (including empty/degraded states).
//!
//! The two dashboards embedded directly in [`crate::TuiReadModel`] are ported
//! here; the remaining per-lens dashboards (cache, evidence, release, ...) are
//! served via dedicated inspection routes and ported alongside those lenses.

pub mod runners;
pub mod source_doctor;
