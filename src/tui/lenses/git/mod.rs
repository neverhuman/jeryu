//! Owner: Interactive TUI subsystem - Git command / sync lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::git::`
//! Invariants: Pure lens projecting the recent git-command event ledger from
//!             app state (`recent_git_events`). Preserves the git command
//!             history / sync view of the legacy git-sync panel
//!             (`draw_git_tab`) so that panel can be deleted. Renders only the
//!             already-redacted argv — never a raw command.
//!
//! Registry wiring (the `LensId::Git` variant and the `pub mod git;`
//! declaration in `lenses/mod.rs`) is added where `lenses/mod.rs` is edited;
//! this module is self-contained so it can be reviewed and tested in isolation.

pub mod data;
pub mod nav;
pub mod view;

pub use data::{GitEventRow, GitLensInput};
pub use nav::{GitIntent, handle_key};
pub use view::draw;
