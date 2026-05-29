//! Split workflow delivery facade.
//!
//! The existing delivery collector stays the behavioral source while the reset
//! grows smaller delivery modules behind this import path.

mod agent_review;
mod auto_merge;
pub mod collector;
mod demo;
mod fleet;
pub mod inputs;
mod pipeline;
mod post_merge;
mod promotion;

pub use collector::{build_demo_delivery, collect_delivery_snapshot};
pub use inputs::{AGENT_REVIEW_AUTO_PASS_DELAY_SECS, DeploymentProgress, PrInput, TestSpec};

#[cfg(test)]
mod tests;
