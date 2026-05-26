//! Owner: Interactive TUI subsystem - Agents + autonomy lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::agents::`
//! Invariants: Surfaces agent sessions, tasks, grants, races, branches,
//!             evidence, and logs. Agent sessions render as `INFERRED`
//!             until the agent lifecycle tables land upstream.
//!
//! Wave 5 U25 wires this scaffold to `AgentsDashboard` from
//! `src/api/dashboards/agents.rs` and adds sessions/tasks/races/launch
//! ledger subcomponents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::AgentsLensInput;
pub use nav::{AgentsIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Agents;
