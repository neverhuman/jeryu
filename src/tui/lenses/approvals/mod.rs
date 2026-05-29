//! Owner: Interactive TUI subsystem - Approvals queue lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::approvals::`
//! Invariants: Renders the pending MR/PR approvals queue as a pure projection
//!             from app state (the `approvals_queue` + `selected_approval_index`)
//!             via `ApprovalsLensInput`. Two panes: the approvals Table and a
//!             detail/actions inspector for the selected PR. Never performs
//!             backend I/O; approve/reject are intents gated by the registry.
//!
//! This lens replaces the legacy approvals tab (`ui_panels_body_approvals.rs`).
//! The orchestrator registers it in `lenses/mod.rs` (module + `LensId::Approvals`
//! + a `LENS_ID` const here) when it wires the lens into the deck.

pub mod data;
pub mod nav;
pub mod view;

pub use data::{ApprovalRow, ApprovalsLensInput};
pub use nav::{ApprovalsIntent, handle_key};
pub use view::draw;
