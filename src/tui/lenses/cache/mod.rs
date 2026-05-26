//! Owner: Interactive TUI subsystem - Cache observatory lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::cache::`
//! Invariants: Shows SmartCache hit/miss/taint posture and category
//!             breakdown. Distinguishes hot from cold from tainted from
//!             leased entries. R3 mutations (flush) gated by preview.
//!
//! First-cut scaffold — categories/hot/GC/lease/verdict subcomponents
//! land in U23 along with `CacheDashboard` wiring from
//! `src/api/dashboards/cache.rs` and integration with U13 widgets
//! (virtual_table for entries, heatmap for category fullness).

pub mod data;
pub mod nav;
pub mod view;

pub use data::CacheLensInput;
pub use nav::{CacheIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Cache;
