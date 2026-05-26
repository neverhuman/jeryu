//! Owner: Interactive TUI subsystem - Workflow atlas (DAG + rails + inspector + logs)
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::workflow::`
//! Invariants: Multi-pipeline atlas. Drillpath: family -> repo -> MR/PR ->
//!             pipeline -> job -> trace -> evidence. Critical path is
//!             always visible.
//!
//! First-cut scaffold — model/delivery/canvas/rails/inspector/logs
//! submodules land in U19 (model + delivery split) and U20 (canvas +
//! rails + inspector + logs split). This is a multi-unit lens; see
//! TUI_RESET_PLAN_FINAL.md §3.2 for the existing oversized-file split
//! targets. Current first-cut implementation renders typed
//! `WorkflowLensInput` (from `TuiReadModel`) and supports the universal
//! keyboard grammar (Enter/Esc/a/e/l/n/?) as intents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::WorkflowLensInput;
pub use nav::{WorkflowIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Workflow;
