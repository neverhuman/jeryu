//! Jankurai governance gates ported from the legacy `scripts/check-*.py` validators.
//!
//! Each gate is a faithful Rust port of one Python script. The public functions
//! return a [`GateReport`] that records the pass/fail outcome together with the
//! exact stdout signal lines the original scripts emitted, so the binary can map
//! the outcome onto identical exit codes and other tools can keep parsing the
//! same marker lines.
//!
//! Every gate lives in its own submodule (one per `check-*.py` script), sharing
//! the [`GateReport`] type and the Python-`repr()` rendering helpers. All public
//! items are re-exported here so the original `jeryu_mapcheck::Thing` import
//! paths keep resolving unchanged.

mod agent_maps;
mod db_boundary;
mod docs;
mod fixtures;
mod generated_zones;
mod owner_test_map;
mod render;
mod report;

pub use agent_maps::agent_maps;
pub use db_boundary::db_boundary;
pub use docs::{REQUIRED_DOCS, docs};
pub use fixtures::fixtures;
pub use generated_zones::generated_zones;
pub use owner_test_map::owner_test_map;
pub use report::GateReport;
