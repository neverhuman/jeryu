//! Runners lens — per-node fleet health (online/busy/idle/stuck, disk,
//! reachability), projected purely from the read model's runners dashboard.

pub mod data;
pub mod view;

pub use data::{RunnerNodeRow, RunnersLensInput};
pub use view::draw;
