//! Domain-owned repair surface.
//!
//! Keep this crate free of transport, persistence, subprocess, and adapter
//! concerns. Domain failures that cross an agent boundary should carry a
//! typed repair hint so the next rerun is local and evidence-backed.

pub mod exceptions;

pub use exceptions::{DomainFailure, RepairHint};
