//! Owner: Interactive TUI subsystem - Queue + theoretical limit lab lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::queue::`
//! Invariants: Distinguishes capacity shortage from DAG/cache/VTI/policy
//!             blockers; explains "does adding runners help?" via the
//!             three-tier capacity model (physics/fleet/policy).
//!
//! Scaffold only — view/data/nav/tests submodules land in U17.

pub const LENS_ID: super::LensId = super::LensId::Queue;
