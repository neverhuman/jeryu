//! Owner: Interactive TUI subsystem - Repos + families lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::repos::`
//! Invariants: Family and repo scope routing affects every relevant lens
//!             consistently. Drilldown: fleet -> family -> repo.
//!
//! Wave 4 U18 takes this scaffold and fills in real widgets, real
//! family/repo enumeration from `TuiReadModel.repos`, scoped attention
//! filtering, and proof links. The current first-cut implementation
//! renders a typed `ReposLensInput` (from `TuiReadModel`) and supports
//! the universal keyboard grammar (Enter/Esc/e/?) as intents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::ReposLensInput;
pub use nav::{ReposIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Repos;
