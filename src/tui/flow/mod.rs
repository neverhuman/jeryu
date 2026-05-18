//! Owner: Interactive TUI subsystem — CI flow view
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: Flow view modules expose read-only pipeline state and policy-gated actions.

pub mod builder;
pub mod collector;
pub mod eta;
pub mod inspector;
pub mod model;
pub mod recovery;
pub mod widget;

// Re-exports
pub use builder::*;
pub use collector::*;
pub use eta::*;
pub use inspector::*;
pub use model::*;
pub(crate) use recovery::*;
pub use widget::*;
