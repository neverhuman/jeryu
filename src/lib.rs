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
//! | `state` | Postgres-primary state, SQLite fallback, all queries | `state-change` |
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
// warning-clean instead of flooding agents with placeholder item docs.
#![allow(missing_docs)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod admission;
pub mod agent;
pub mod agent_surface;
pub mod bootstrap;
pub mod buildkit;
pub mod cache;
pub mod cache_brain;
pub mod cache_proxy;
pub mod capability;
pub mod capsule;
pub mod cargo_cache;
pub mod config;
pub mod decision;
pub mod docker;
pub mod engine;
pub mod epoch;
pub mod exec;
pub mod explain;
pub mod gateway;
pub mod git;
pub mod gitlab_client;
pub mod honeypot;
pub mod host;
pub mod impact;
pub mod install;
pub mod install_demo;
pub mod local;
pub mod logs;
pub mod mcp;
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
pub mod shadow;
pub mod state;
pub mod taint;
pub mod telemetry;
pub mod test_intel;
pub mod test_runner;
pub mod tui;
pub mod witness;
