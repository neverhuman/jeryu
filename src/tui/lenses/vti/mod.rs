//! Owner: Interactive TUI subsystem - VTI (Virtual Test Index) lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::vti::`
//! Invariants: Surfaces VTI plan confidence, selector misses, and flake
//!             radar. Low-confidence plans never render as green. R2
//!             mutations (force full run) gated by preview.
//!
//! First-cut scaffold — plan/selector_miss/confidence/repair/flake_radar
//! subcomponents land in U24 along with `VtiDashboard` wiring from
//! `src/api/dashboards/vti.rs` and integration with U13 widgets
//! (virtual_table for plans, heatmap for selector hot/miss).

pub mod data;
pub mod nav;
pub mod view;

pub use data::VtiLensInput;
pub use nav::{VtiIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Vti;
