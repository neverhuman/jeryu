//! Owner: Interactive TUI subsystem - Jankurai audit lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::jankurai::`
//! Invariants: Pure lens. Projects `JankuraiSnapshot` + selected index into an
//!             owned `JankuraiLensInput`, then draws the audit posture (score /
//!             decision / history chart / dimension breakdown / caps+findings
//!             list / selected-entry detail) with no I/O. Replaces the legacy
//!             `ui_panels_jankurai*` audit panel; carries forward all of its
//!             rendered information without depending on `ui_panels_*`/`ui_chrome`.

pub mod data;
pub mod nav;
pub mod view;

pub use data::JankuraiLensInput;
pub use nav::{JankuraiIntent, handle_key};
pub use view::draw;
