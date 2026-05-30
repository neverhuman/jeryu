//! Agents lens — the agent fleet (active/blocked/grants), projected from the
//! read model's agents dashboard.

pub mod data;
pub mod view;

pub use data::{AgentRow, AgentsLensInput};
pub use view::draw;
