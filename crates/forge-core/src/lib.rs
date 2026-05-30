//! Typed Phase 2 forge domain for JitForge Nitro.
//!
//! This crate intentionally has no HTTP dependency. It owns users, orgs, teams,
//! repositories, issues, pull requests, reviews, branch protection, commit
//! statuses, check runs, and webhook outbox receipts.

mod branch_protection;
mod core;
mod error;
mod errors;
mod ids;
mod model;
pub mod phase7;
mod receipt;
mod webhooks;

pub use crate::branch_protection::{BranchProtectionEvaluation, MergeBlocker};
pub use crate::core::ForgeCore;
pub use crate::error::{JitForgeError, JitForgeResult};
pub use crate::errors::{ForgeError, Result};
pub use crate::ids::{AgentId, PullRequestId, QueueEntryId, ReceiptId, RepoId};
pub use crate::model::*;
pub use crate::receipt::{Receipt, ReceiptKind};
pub use crate::webhooks::{sign_webhook_payload, WebhookEventEnvelope};
