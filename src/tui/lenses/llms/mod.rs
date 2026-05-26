//! Owner: Interactive TUI subsystem - LLM governance lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::llms::`
//! Invariants: Shows LLM provider posture, budget, calls, and traces.
//!             Redacts tokens and secrets in every render. AdjustBudget
//!             is R3 (repo) and gated by preview.
//!
//! First-cut scaffold — calls/budget/traces/providers/keys subcomponents
//! land in U25 along with `LlmsDashboard` wiring from
//! `src/api/dashboards/llms.rs` and integration with U13 widgets
//! (virtual_table for calls, log_viewer for redacted traces).

pub mod data;
pub mod nav;
pub mod view;

pub use data::LlmsLensInput;
pub use nav::{LlmsIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Llms;
