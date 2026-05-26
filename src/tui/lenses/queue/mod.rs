//! Queue lens selectors, capacity math, rendering, and local navigation.
//!
//! Rendering stays pure over `QueueLensInput`; no backend I/O is allowed here.

pub mod data;
pub mod lab;
pub mod nav;
pub mod view;

pub use crate::api::read_model::{QueueJobSummary, QueuePoolSnapshot, QueueSnapshot};
pub use data::{QueueLensInput, QueueStageSummary, select_queue_lens_input};
pub use lab::{AddRunnerEffect, QueueCapacity, QueueDelta, analyze_capacity, runner_delta};
pub use nav::{QueueNavOutcome, QueuePane, activate_pane, move_focus};
pub use view::QueueLens;

#[cfg(test)]
mod tests;
