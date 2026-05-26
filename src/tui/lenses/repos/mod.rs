//! Repository fleet lens selectors, rendering, and local navigation.
//!
//! Rendering stays pure over `ReposLensInput`; no backend I/O is allowed here.

pub mod data;
pub mod nav;
pub mod shell;
pub mod view;

pub use crate::api::read_model::{RepoFamilySummary, RepoSummary, ReposSnapshot};
pub use data::{RepoFleetCounts, ReposLensInput, ReposSelection, select_repos_lens_input};
pub use nav::{ReposNavOutcome, ReposPane, activate_pane, move_focus};
pub use shell::draw_app_repos_lens;
pub use view::ReposLens;

#[cfg(test)]
mod tests;
