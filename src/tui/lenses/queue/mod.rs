//! Owner: Interactive TUI subsystem - Queue + theoretical limit lab lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::queue::`
//! Invariants: Distinguishes capacity shortage from DAG/cache/VTI/policy
//!             blockers; explains "does adding runners help?" via the
//!             three-tier capacity model (physics/fleet/policy).
//!
//! Wave 4 U17 takes this scaffold and fills in real widgets, real
//! physics-floor capacity math, real "does scaling pools help?" scoring,
//! and real proof links. The current first-cut implementation renders
//! typed `QueueLensInput` (from `TuiReadModel`) and supports the universal
//! keyboard grammar (Enter/Esc/a/e/?) as intents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::QueueLensInput;
pub use nav::{QueueIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Queue;
