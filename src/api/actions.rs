//! Owner: TUI Control-Plane API — action dispatch and preview
//! Proof: `cargo nextest run -p jeryu -- api::actions`
//! Invariants: Every mutating action goes through preview before execution;
//!             actions are typed, risk-tagged, and grant-checked.

use serde::{Deserialize, Serialize};

use super::entity::{EntityKind, EntityRef};
use crate::tui::action_registry::{GrantRequirement, RiskTier, SideEffectClass};

// ── Action Context ──────────────────────────────────────────────────────

/// The context in which an action is being requested.
#[derive(Debug, Clone)]
pub struct ActionContext {
    /// The entity the action targets (e.g. a selected job).
    pub selected_entity: Option<EntityRef>,
    /// Who is requesting the action.
    pub actor: ActorRef,
    /// Grant ID proving authorization (for agent actions).
    pub grant_id: Option<String>,
    /// If true, only compute the preview without side effects.
    pub dry_run: bool,
    /// Idempotency key to prevent duplicate execution.
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorRef {
    pub kind: ActorKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
    Agent,
    System,
}

// ── Action Preview ──────────────────────────────────────────────────────

/// What will happen if the action is executed.
/// Shown in the preview modal before confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPreview {
    /// Is this action currently available?
    pub enabled: bool,
    /// If disabled, why?
    pub disabled_reason: Option<String>,
    /// Risk classification.
    pub risk: RiskTier,
    /// Coarse side-effect class.
    pub side_effect_class: SideEffectClass,
    /// Human-readable blast radius description.
    pub side_effects: Vec<String>,
    /// What this action will NOT do (for clarity).
    pub will_not: Vec<String>,
    /// One-line summary.
    pub summary: String,
    /// Evidence records expected to be created.
    pub evidence_expected: Vec<String>,
    /// Required grant for execution.
    pub required_grant: GrantRequirement,
    /// Compensating action if this one needs to be undone.
    pub undo_action: Option<String>,
    /// Confirmation prompt for the user.
    pub confirm_prompt: Option<String>,
}

impl Default for ActionPreview {
    fn default() -> Self {
        Self {
            enabled: false,
            disabled_reason: Some("no handler registered".into()),
            risk: RiskTier::ReadOnly,
            side_effect_class: SideEffectClass::ReadOnly,
            side_effects: Vec::new(),
            will_not: Vec::new(),
            summary: String::new(),
            evidence_expected: Vec::new(),
            required_grant: GrantRequirement::None,
            undo_action: None,
            confirm_prompt: None,
        }
    }
}

// ── Action Result ───────────────────────────────────────────────────────

/// Outcome of executing an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub status: ActionStatus,
    pub summary: String,
    /// Event cursor to follow for result streaming.
    pub event_cursor: Option<u64>,
    /// Entity created or modified by the action.
    pub affected_entity: Option<EntityRef>,
    /// Evidence records created.
    pub evidence_created: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    /// Action was accepted and completed.
    Completed,
    /// Action was accepted and is running asynchronously.
    Accepted,
    /// Action requires approval before it can proceed.
    RequiresApproval,
    /// Action was rejected (preconditions not met).
    Rejected,
    /// Action failed during execution.
    Failed,
}

// ── Contextual Action ───────────────────────────────────────────────────

/// An action entry enriched with dynamic availability for the current
/// entity and state. Used by the command palette and inspector.
#[derive(Debug, Clone)]
pub struct ContextualAction {
    pub action_id: &'static str,
    pub label: &'static str,
    pub key_hint: Option<&'static str>,
    pub risk: RiskTier,
    pub available: bool,
    pub disabled_reason: Option<String>,
    pub context_types: Vec<EntityKind>,
}

/// Compute which actions are available for a given entity.
pub fn actions_for_entity(entity: &EntityRef) -> Vec<ContextualAction> {
    use crate::tui::action_registry::REGISTRY;

    REGISTRY
        .iter()
        .filter_map(|entry| {
            // Determine which entity kinds this action applies to
            let context_types = action_context_types(entry.id);
            if context_types.is_empty() || context_types.contains(&entity.kind) {
                Some(ContextualAction {
                    action_id: entry.id,
                    label: entry.label,
                    key_hint: entry.key_hint,
                    risk: entry.risk_tier,
                    available: true,
                    disabled_reason: None,
                    context_types,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Map action IDs to the entity kinds they operate on.
fn action_context_types(action_id: &str) -> Vec<EntityKind> {
    match action_id {
        "open_logs" | "requeue_job" | "remove_record" => {
            vec![EntityKind::Job]
        }
        "pause_pool" => vec![EntityKind::Pool],
        "explain_blockers" => vec![
            EntityKind::Job,
            EntityKind::Pipeline,
            EntityKind::ReleaseAttempt,
            EntityKind::MergeRequest,
        ],
        "fetch_capsule" => vec![EntityKind::Job, EntityKind::EvidenceCapsule],
        "propose_patch" | "race_patches" => vec![EntityKind::MergeRequest, EntityKind::AgentTask],
        "request_merge" => vec![EntityKind::MergeRequest],
        "run_tests" => vec![
            EntityKind::MergeRequest,
            EntityKind::Pipeline,
            EntityKind::AgentTask,
        ],
        "plan_validation" => vec![EntityKind::TestPlan, EntityKind::MergeRequest],
        "next_action" | "get_system_snapshot" => vec![EntityKind::System],
        // Tab navigation and UI actions apply everywhere
        _ => Vec::new(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::EntityKind;

    #[test]
    fn actions_for_job_include_requeue_and_logs() {
        let entity = EntityRef::new(EntityKind::Job, "14445");
        let actions = actions_for_entity(&entity);
        let ids: Vec<_> = actions.iter().map(|a| a.action_id).collect();
        assert!(ids.contains(&"open_logs"), "missing open_logs");
        assert!(ids.contains(&"requeue_job"), "missing requeue_job");
        assert!(
            ids.contains(&"explain_blockers"),
            "missing explain_blockers"
        );
    }

    #[test]
    fn actions_for_pool_include_pause() {
        let entity = EntityRef::new(EntityKind::Pool, "rust-large");
        let actions = actions_for_entity(&entity);
        let ids: Vec<_> = actions.iter().map(|a| a.action_id).collect();
        assert!(ids.contains(&"pause_pool"), "missing pause_pool");
        assert!(
            !ids.contains(&"requeue_job"),
            "requeue_job should not apply to pool"
        );
    }

    #[test]
    fn actions_for_mr_include_merge_and_tests() {
        let entity = EntityRef::new(EntityKind::MergeRequest, "42");
        let actions = actions_for_entity(&entity);
        let ids: Vec<_> = actions.iter().map(|a| a.action_id).collect();
        assert!(ids.contains(&"request_merge"), "missing request_merge");
        assert!(ids.contains(&"run_tests"), "missing run_tests");
        assert!(ids.contains(&"propose_patch"), "missing propose_patch");
    }

    #[test]
    fn default_preview_is_disabled() {
        let preview = ActionPreview::default();
        assert!(!preview.enabled);
        assert!(preview.disabled_reason.is_some());
    }
}
