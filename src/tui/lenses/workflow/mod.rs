//! Owner: Interactive TUI subsystem - Workflow atlas (DAG + rails + inspector + logs)
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::workflow::`
//! Invariants: Multi-pipeline atlas. Drillpath: family -> repo -> MR/PR ->
//!             pipeline -> job -> trace -> evidence. Critical path is
//!             always visible.
//!
//! Scaffold only — model/delivery/canvas/rails/inspector/logs submodules
//! land in U19 (model + delivery split) and U20 (canvas + rails + inspector
//! + logs split). This is a multi-unit lens; see TUI_RESET_PLAN_FINAL.md
//! §3.2 for the existing oversized-file split targets.

pub const LENS_ID: super::LensId = super::LensId::Workflow;
