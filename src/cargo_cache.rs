//! Owner: Cargo cache layout and local agent helpers
//! Proof: `cargo test -p jeryu -- cargo_cache`
//! Invariants: Cache keys are deterministic; target dirs stay scoped by repo/project, toolchain, and host triple; active leases are never collected.

pub const LEASES_DIR_NAME: &str = ".jeryu-leases";
pub const CACHE_STAMP_FILE: &str = ".jeryu-cache-stamp.json";
pub const CACHE_SEED_MARKERS_DIR: &str = ".jeryu-cache-seeds";
pub const CACHE_PROMOTION_MARKERS_DIR: &str = ".jeryu-cache-promotions";
pub const CACHE_HOME_DIR_NAME: &str = "cargo-home";
pub const RUSTUP_HOME_DIR_NAME: &str = "rustup-home";

#[path = "cargo_cache_helpers.rs"]
mod helpers;
pub use helpers::*;

#[path = "cargo_cache_layout.rs"]
mod layout;
pub use layout::*;

#[path = "cargo_cache_script.rs"]
mod script;
pub use script::*;

#[path = "cargo_cache_leases.rs"]
mod leases;
pub use leases::*;

#[cfg(test)]
#[path = "cargo_cache_tests.rs"]
mod tests;
