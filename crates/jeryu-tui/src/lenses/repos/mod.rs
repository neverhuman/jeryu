//! Repos lens — tracked-repository fleet, families, detail, scoped attention.

pub mod data;
pub mod view;

pub use data::{RepoFleetCounts, ReposLensInput};
pub use view::draw;
