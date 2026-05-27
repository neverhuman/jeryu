//! Owner: Web Forge BFF — tree listing helper (W-B-09).
//! Proof: covered by `service.rs` tests.
//!
//! Thin wrapper over `GitHost::list_tree`. The Phase 2 service inlines the
//! call; this module exists so the file layout matches the FINAL §6.6
//! structure and future per-host enrichment (last-commit lookup per entry)
//! can land here without changing handler signatures.

pub use super::service::normalize_path;
