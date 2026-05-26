//! Owner: Interactive TUI subsystem - Autonomy / kill-bell lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::autonomy::`
//! Invariants: Shows kill bell, freeze posture, active grants, and
//!             autonomy verdicts. KillBell is R5 (irreversible); Freeze
//!             and GrantApprove are R3 (repo) and gated by preview.
//!
//! First-cut scaffold — kill bell/freeze/verdicts/policy/launch ledger
//! subcomponents land in U25 along with `AutonomyDashboard` wiring from
//! `src/api/dashboards/autonomy.rs` and integration with U13 widgets
//! (virtual_table for grants, inspector_card for verdicts).

pub mod data;
pub mod nav;
pub mod view;

pub use data::AutonomyLensInput;
pub use nav::{AutonomyIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Autonomy;
