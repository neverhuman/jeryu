//! Domain-owned repair surface.
//!
//! Keep this crate free of transport, persistence, subprocess, and adapter
//! concerns. Domain failures that cross an agent boundary should carry a
//! typed repair hint so the next rerun is local and evidence-backed.
//! `RepairHint` is the domain agent-friendly exception pattern: it names the
//! purpose, reason, common fixes, local docs URL, and narrow rerun command.

pub mod exceptions;

pub use exceptions::{AGENT_FRIENDLY_EXCEPTION_PATTERN_FIELDS, DomainFailure, RepairHint};
