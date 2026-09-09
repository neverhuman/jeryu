//! JMCP/control-plane intelligence read model.
//!
//! This module is deliberately pure aggregation over already-owned local
//! surfaces: [`ForgeCore`], the runner fleet snapshot, the live agent-run store,
//! and the auxiliary codegraph/tool-build store. GitHub mirror data is optional
//! read-only evidence and is represented here as explicit `missing` state until
//! a live mirror adapter supplies it.
//!
//! The implementation is split across focused submodules — request/response
//! [`types`], axum [`handlers`], the snapshot [`model`], the runner [`runner`]
//! fabric, [`priorities`] ranking, the repo [`graph`] builder, and the [`mcp`]
//! facade — while this file re-exports the same surface the rest of the web
//! edge already depends on.

mod checks;
mod graph;
mod handlers;
mod mcp;
mod model;
mod priorities;
mod runner;
mod types;

#[cfg(test)]
mod tests;

const SCHEMA_VERSION: &str = "jeryu.control_plane/v1";
const RULES_VERSION: &str = "rules-v1";
const MIRROR_DOCS: &str = "docs/agent-native-standard.md";
const ARTIFACT_DOCS: &str = "docs/release.md#release-receipt";

pub(super) use checks::*;
pub(super) use graph::*;
pub(super) use handlers::*;
pub(super) use mcp::*;
pub(super) use model::*;
pub(super) use priorities::*;
pub(super) use runner::*;
pub(super) use types::*;
