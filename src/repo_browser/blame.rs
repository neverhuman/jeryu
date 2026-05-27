//! Owner: Web Forge BFF — blame helper (W-B-10).
//! Proof: covered by `service.rs` tests.
//!
//! The Phase 2 implementation calls `GitHost::get_blame` which on GitLab
//! adapters returns `HostError::NotImplemented` until W-H-* lands. The REST
//! handler surfaces that as `upstream_unavailable` per §35.1.11.
