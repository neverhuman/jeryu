//! Owner: Interactive TUI subsystem - Repos + families lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::repos::`
//! Invariants: Family and repo scope routing affects every relevant lens
//!             consistently. Drilldown: fleet -> family -> repo.
//!
//! Scaffold only — view/data/nav/tests submodules land in U18.

pub const LENS_ID: super::LensId = super::LensId::Repos;
