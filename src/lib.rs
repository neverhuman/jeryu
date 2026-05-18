//! Owner: jeryu crate root (see module map below)
//! Proof: `cargo check -p jeryu --message-format=json`
//! Invariants: Public module exports remain explicit and aligned with AGENTS.md ownership routing.
//! # jeryu — CI/CD Control Plane
//!
//! Single-binary orchestrator for GitLab-based pipelines. Consumes CI semantics
//! from `dougx/apps/veox-testctl/src/ci.rs`; owns scheduling, reconciliation,
//! canary promotion, and supply-chain detection.
//!
//! ## Module Map (→ change type for proof routing)
//!
//! | Module | Responsibility | Change Type |
//! |---|---|---|
//! | `engine` | Webhook server, reconciliation | `api-change` |
//! | `release` | Release pipeline, canary | `release-change` |
//! | `state` | RedlineDB-primary state, backend-neutral path, all queries | `state-change` |
//! | `exec` | Custom executor, sandbox | `security-relevant` |
//! | `secrets` | Vault, rotation | `security-relevant` |
//! | `honeypot` | Supply-chain detonation | `security-relevant` |
//! | `sandbox` | Network-namespace isolation | `security-relevant` |
//! | `admission` | Git hook admission | `security-relevant` |
//! | `taint` | Taint tracking | `security-relevant` |
//! | `decision` | Risk gates, supersedence | `cross-module` |
//! | `test_intel` | VTI smart test selection | `api-change` |
//! | `tui` | Ratatui TUI dashboard | `leaf-bugfix` |
//! | `gateway` | Registry proxy | `leaf-bugfix` |
//!
//! See `proof-lanes.toml` for change-type → validation-command mapping.
//! See `.cross-repo.toml` for consumed surfaces from `dougx`.

// Public API documentation is tracked through module ownership headers,
// generated agent indexes, and the paper/docs surface. Keep proof runs
// warning-clean instead of flooding agents with generated item docs.
#![allow(missing_docs)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
// HLT-001-DEAD-MARKER: jankurai audit requires explicit default handling patterns.
// This is intentional for proof/safety and is not a bug.
#![allow(clippy::manual_unwrap_or_default)]
// Doc comment formatting: the crate uses a mix of indentation styles inherited from refactors
// of Wave 1-10. The lints are noise; content is correct. Fixing all ~11 instances is lower
// priority than CI parity. TODO: normalize doc formatting in a separate refactor.
#![allow(clippy::doc_overindented_list_items)]
// New clippy lints introduced between rustc 1.92 and 1.95 (toolchain bump in v3.3.1).
// Each is a style preference, not a correctness bug. Auditing every site is out of
// scope for the bump PR — they're allowed at crate root with the intent to revisit
// in a separate refactor.
#![allow(clippy::expect_fun_call)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::unnecessary_unwrap)]
#![allow(clippy::manual_checked_ops)]

pub mod admission;
pub mod agent;
pub mod agent_review;
pub mod agent_surface;
pub mod api;
pub mod approval;
pub mod autonomy;
pub mod bootstrap;
pub mod buildkit;
pub mod cache;
pub mod cache_brain;
pub mod cache_proxy;
pub mod capability;
pub mod capsule;
pub mod cargo_cache;
pub mod config;
pub mod db;
pub mod decision;
pub mod docker;
pub mod engine;
pub mod epoch;
pub mod exec;
pub mod explain;
pub mod gateway;
pub mod git;
pub mod git_host;
pub mod gitlab_client;
pub mod honeypot;
pub mod host;
pub mod impact;
pub mod install;
pub mod install_demo;
pub mod llm;
pub mod local;
pub mod logs;
pub mod mcp;
pub mod messaging;
pub mod policy;
pub mod pool;
pub mod reclaim;
pub mod redact;
pub mod release;
pub mod remote;
pub mod repo;
pub mod sandbox;
pub mod sccache_mgr;
pub mod secrets;
pub mod settings;
#[path = "../db/state.rs"]
pub mod state;
#[path = "../db/taint.rs"]
pub mod taint;
pub mod telemetry;
pub mod test_intel;
pub mod test_runner;
pub(crate) mod test_sync;
pub mod tui;
pub mod witness;

pub fn install_state_storage_drivers() {
    db::install_default_drivers();
}
