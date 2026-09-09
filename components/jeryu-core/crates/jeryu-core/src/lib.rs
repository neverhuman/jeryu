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
mod overlap;
pub mod phase7;
mod receipt;
mod services;
mod webhooks;

pub use crate::branch_protection::{
    BranchProtectionEvaluation, EvaluationContext, MergeBlocker, RefOperation, RefOperationBlocker,
    RefOperationEvaluation, effective_reviews_for_head, effective_reviews_for_pull_request,
};
pub use crate::core::{
    AuditEntry, ForgeCore, MergeReadiness, MutationCoordinator, RepoMaterializer,
    RepositoryDeletion,
};
pub use crate::error::{AgentRepairHint, JeryuError, JeryuResult};
pub use crate::errors::{ForgeError, Result};
pub use crate::ids::{AgentId, PullRequestId, QueueEntryId, ReceiptId, RepoId};
pub use crate::model::*;
pub use crate::overlap::{
    ChangeSet, OpenPr, OverlapConfig, OverlapDecision, OverlapRouting, OverlapScore, decide,
    jaccard, route,
};
pub use crate::receipt::{Receipt, ReceiptKind};
pub use crate::services::{
    AuditReadService, BranchProtectionReadService, CheckReadService, ForgeReadService,
    PullRequestReadService, RepositoryReadService,
};
pub use crate::webhooks::{WebhookEventEnvelope, sign_webhook_payload};
