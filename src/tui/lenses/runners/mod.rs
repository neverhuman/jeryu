//! Owner: Interactive TUI subsystem - Runners + nodes lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::runners::`
//! Invariants: Shows pool/node/tag/resource/trust health and scale/drain
//!             previews. Distinguishes paused from draining from saturated.
//!
//! Wave 5 U22 takes this scaffold and wires it to `RunnersDashboard` from
//! `src/api/dashboards/runners.rs`, fills in pools/nodes/telemetry/scale
//! preview subcomponents, and integrates with U13 widgets (heatmap for
//! CPU/mem, virtual_table for runner list).

pub mod data;
pub mod nav;
pub mod view;

pub use data::RunnersLensInput;
pub use nav::{RunnersIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Runners;
