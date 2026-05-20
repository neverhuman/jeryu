//! Owner: db-boundary (Wave 11.A) — closes `HLT-006-DIRECT-DB-WRONG-LAYER`.
//!
//! This module is the ONLY place in the crate (outside the schema in
//! `db/state.rs` and the seam helpers in `db/taint.rs`) that imports
//! `sqlx::` directly. Every "Sql*" wrapper type elsewhere in the tree
//! routes its queries through a typed repo here so callers can stay
//! ignorant of the wire-level SQL driver.
//!
//! Public re-exports (`AnyPool`, `AnyPoolOptions`, `install_default_drivers`,
//! `query`) exist so the rest of the crate — and the existing in-memory
//! test fixtures — can name those types as `crate::db::AnyPool` etc.
//! without taking a fresh `sqlx::` dependency. The RedlineDB SQLx bridge is
//! installed through this module so the consumer startup path has one place
//! to initialize the driver registry before any `AnyPool` is opened.
//!
//! Boundaries:
//!   - `autonomy_repo` — launch ledger, kill bell, verdict store.
//!   - `admission_repo` — hook admission decisions and capability grants.
//!   - `release_repo` — foundry queue.
//!   - `budget_repo`  — LLM budget ledger.
//!
//! Each repo owns the SQL string; callers own only typed Rust values.

#[path = "../../db/config.rs"]
pub mod config;

pub mod admission_repo;
pub mod autonomy_repo;
pub mod budget_repo;
pub mod bugtracker_repo;
pub mod release_repo;

// Re-exports so callers can name the canonical pool type as
// `crate::db::AnyPool` without re-introducing a `use sqlx::` line.
pub use sqlx::AnyPool;
pub use sqlx::any::AnyPoolOptions;

/// Install the RedlineDB SQLx driver exactly once for this process.
pub fn install_default_drivers() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(redlinedb_sqlx::install_default_drivers);
}

/// Re-export of `sqlx::query` so test fixtures that install DDL through
/// the db boundary do not have to import `sqlx::` themselves.
pub use sqlx::query as raw_query;

/// Re-export of `sqlx::Row` so a small number of `try_get`-style call
/// sites in tests can compile without naming `sqlx::` directly.
pub use sqlx::Row;
