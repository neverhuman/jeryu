//! Owner: Release Pipeline
//! Proof: `cargo test -p jeryu -- release`
//! Invariants: Exact-SHA evidence matching, canary gate ladder, immutable evidence dirs

use crate::gitlab_client::{GitlabClient, Job, Pipeline};
use crate::state::{Db, ReleaseAttempt};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;

#[path = "release_support.rs"]
mod support;
pub use support::*;

#[path = "release_types.rs"]
mod types;
pub use types::*;

#[path = "release_types_schema.rs"]
mod types_schema;
pub(crate) use types_schema::*;

mod canary;
mod capsule;
pub mod draft_loader;
// `foundry` is `pub mod` (not just `mod`) so the Wave 11.A db-boundary
// extraction in `src/db/release_repo.rs` can name its concrete types
// (`FoundryConfig`, `ReleaseCandidate`) via the explicit submodule path.
// Re-exports below (`pub use foundry::*`) still publish the same surface
// to outside crates.
pub mod foundry;
mod full_path;
mod gate;
mod lifecycle;
mod pipeline;
mod progress;
mod rollback;
// Wave 3.5.B: SQL-backed FoundryQueue (sister of in-memory `foundry::FoundryTrain`).
// Re-exported so callers (notably the `autonomy foundry` CLI in `src/bin/autonomy.rs`)
// can swap from the in-memory `FoundryTrain` to the restart-durable
// `SqlFoundryQueue` without reaching into private module paths.
pub mod sql_foundry_queue;
mod status;
#[cfg(test)]
mod tests;

pub use canary::*;
pub use capsule::*;
pub use foundry::*;
pub use full_path::*;
pub use gate::*;
pub use lifecycle::*;
pub use pipeline::*;
pub use progress::*;
pub use rollback::*;
pub use sql_foundry_queue::SqlFoundryQueue;
pub use status::*;
