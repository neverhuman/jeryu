//! Owner: Interactive TUI subsystem - Secrets audit lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::secrets::`
//! Invariants: Pure lens over the secret-audit ledger. Renders the
//!             access/rotation event trail and vault status as a two-pane
//!             projection from app state, with HealthLevel-style colored
//!             status words. Replaces the legacy `draw_secrets_tab` panel.
//!
//! SECURITY: this lens surfaces ONLY audit metadata — action, status, repo,
//! and timestamp. It never renders a secret value, rotation target, or any
//! vaulted material; `SecretsLensInput` structurally cannot carry it.
//!
//! Note: `LENS_ID` (the `super::LensId::Secrets` constant the other lenses
//! expose) is intentionally not declared here — registering the lens in the
//! `LensId` enum is a `lenses/mod.rs` change that is out of scope for this
//! unit. It is added when the secrets lens is wired into the route table.

pub mod data;
pub mod nav;
pub mod view;

pub use data::{SecretAuditRow, SecretsLensInput};
pub use nav::{SecretsIntent, handle_key};
pub use view::draw;
