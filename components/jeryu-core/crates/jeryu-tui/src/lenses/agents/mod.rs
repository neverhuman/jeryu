//! Agents lens — the agent fleet (active/blocked/grants), projected from the
//! read model's agents dashboard.

pub mod data;
pub mod terminal;
pub mod view;

pub use data::{AgentRow, AgentsLensInput};
pub use terminal::AgentTerminalSession;
pub use view::draw;
