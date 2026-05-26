//! Owner: Interactive TUI subsystem - Release / promotion lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::release::`
//! Invariants: Shows release candidate posture, canary status, gates,
//!             and rollback options. R3/R4 mutations gated by typed
//!             confirmation per `RiskTier`.
//!
//! First-cut scaffold — candidate/canary/production/gates/rollback
//! subcomponents land in U27 along with `ReleaseDashboard` wiring from
//! `src/api/dashboards/release.rs` and integration with U13 widgets
//! (virtual_table for candidates, dag for gates, progress_bar for canary).

pub mod data;
pub mod nav;
pub mod view;

pub use data::ReleaseLensInput;
pub use nav::{ReleaseIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Release;
