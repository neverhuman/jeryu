//! Typed Phase 2 forge domain for Jeryu.
//!
//! This crate intentionally has no HTTP dependency. It owns users, orgs, teams,
//! repositories, issues, pull requests, reviews, branch protection, commit
//! statuses, check runs, and webhook outbox receipts.

mod branch_protection;
#[path = "engine/mod.rs"]
mod core;
mod error;
mod errors;
mod ids;
mod model;
pub mod phase7;
mod receipt;
mod webhooks;

pub use crate::branch_protection::{
    BranchProtectionEvaluation, EvaluationContext, MergeBlocker, RefOperation, RefOperationBlocker,
    RefOperationEvaluation,
};
pub use crate::core::ForgeCore;
pub use crate::error::{AgentRepairHint, JeryuError, JeryuResult};
pub use crate::errors::{ForgeError, Result};
pub use crate::ids::{AgentId, PullRequestId, QueueEntryId, ReceiptId, RepoId};
pub use crate::model::*;
pub use crate::receipt::{Receipt, ReceiptKind};
pub use crate::webhooks::{WebhookEventEnvelope, sign_webhook_payload};
