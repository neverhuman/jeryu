//! High-level JeryuCache service API.

mod cache;
mod outcomes;

pub use cache::JeryuCache;
pub use outcomes::{JeryuCachePaths, RestoreOutcome, WriteOutcome};
