//! Owner: Interactive TUI subsystem - Mission Control lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::mission::`
//! Invariants: Renders global posture, top blocker, source freshness, next
//!             action. Never performs backend I/O during draw. Canonical
//!             5-file lens shape: mod / view / data / nav / tests.
//!
//! Wave 4 U16 takes this scaffold and fills in real widgets, real
//! attention ranking, real next-action wiring, and real proof links. The
//! current implementation renders typed `MissionLensInput` (from
//! `TuiReadModel`) and supports the universal keyboard grammar
//! (Enter/Esc/a/e/?) as intents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::MissionLensInput;
pub use nav::{MissionIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Mission;
