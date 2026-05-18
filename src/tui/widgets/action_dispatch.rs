//! Owner: Interactive TUI subsystem — action execution runtime bridge
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::action_dispatch`
//! Invariants: Every mutation goes through preview → confirm → execute.

#[cfg(test)]
use crate::api::actions::ActionStatus;
use crate::api::actions::{
    ActionContext, ActionPreview, ActionResult, ActorKind, ActorRef, ContextualAction,
    actions_for_entity,
};
use crate::api::entity::{EntityKind, EntityRef};
use crate::tui::action_registry::{GrantRequirement, RiskTier, SideEffectClass};

/// Preview state for the action execution modal.
#[derive(Debug, Clone, Default)]
pub enum ActionExecutionState {
    /// No action being executed.
    #[default]
    Idle,
    /// Action selected — showing preview modal.
    Previewing {
        action_id: String,
        label: String,
        preview: ActionPreview,
        entity: Option<EntityRef>,
    },
    /// User confirmed — executing.
    Executing { action_id: String, label: String },
    /// Execution complete — showing result.
    Completed {
        action_id: String,
        result: ActionResult,
    },
}

impl ActionExecutionState {
    pub fn is_previewing(&self) -> bool {
        matches!(self, Self::Previewing { .. })
    }

    pub fn is_executing(&self) -> bool {
        matches!(self, Self::Executing { .. })
    }

    /// Start previewing an action from the command palette.
    #[allow(clippy::too_many_arguments)] // palette preview: matches ActionPreview's flat schema
    pub fn begin_preview(
        action_id: &str,
        label: &str,
        entity: Option<EntityRef>,
        risk: RiskTier,
        side_effect_class: SideEffectClass,
        description: &str,
        required_grant: GrantRequirement,
        _dry_run: bool,
    ) -> Self {
        let preview = ActionPreview {
            enabled: true,
            disabled_reason: None,
            risk,
            side_effect_class,
            side_effects: vec![description.to_string()],
            will_not: vec![
                "modify production state".to_string(),
                "bypass sandbox constraints".to_string(),
            ],
            summary: description.to_string(),
            evidence_expected: Vec::new(),
            required_grant,
            undo_action: compute_undo_action(action_id),
            confirm_prompt: Some(format!("[Enter] Execute {}   [Esc] Cancel", label)),
        };

        Self::Previewing {
            action_id: action_id.to_string(),
            label: label.to_string(),
            preview,
            entity,
        }
    }

    /// Transition from preview to executing.
    pub fn confirm(&mut self) {
        if let Self::Previewing {
            action_id, label, ..
        } = self
        {
            *self = Self::Executing {
                action_id: action_id.clone(),
                label: label.clone(),
            };
        }
    }

    /// Mark execution as complete.
    pub fn complete(&mut self, result: ActionResult) {
        if let Self::Executing { action_id, .. } = self {
            *self = Self::Completed {
                action_id: action_id.clone(),
                result,
            };
        }
    }

    /// Reset to idle.
    pub fn dismiss(&mut self) {
        *self = Self::Idle;
    }
}

/// Build the action context for the current selected entity.
pub fn build_context(entity: Option<EntityRef>, dry_run: bool) -> ActionContext {
    ActionContext {
        selected_entity: entity,
        actor: ActorRef {
            kind: ActorKind::Human,
            id: "tui-user".to_string(),
        },
        grant_id: None,
        dry_run,
        idempotency_key: Some(format!("tui-{}", chrono::Utc::now().timestamp_millis())),
    }
}

/// Get available actions for the current entity context.
pub fn available_actions(entity: Option<&EntityRef>) -> Vec<ContextualAction> {
    match entity {
        Some(e) => actions_for_entity(e),
        None => {
            // System-level actions
            actions_for_entity(&EntityRef::new(EntityKind::System, "global"))
        }
    }
}

/// Compute the undo action for a given action.
fn compute_undo_action(action_id: &str) -> Option<String> {
    match action_id {
        "pause_pool" => Some("resume_pool".to_string()),
        "requeue_job" => None,
        "remove_record" => None,
        "cancel_job" => Some("requeue_job".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_state_machine() {
        let mut state = ActionExecutionState::begin_preview(
            "requeue_job",
            "Retry Failed Job",
            Some(EntityRef::new(EntityKind::Job, "123")),
            RiskTier::Low,
            SideEffectClass::LocalState,
            "Requeue job #123",
            GrantRequirement::None,
            false,
        );
        assert!(state.is_previewing());

        state.confirm();
        assert!(state.is_executing());

        state.complete(ActionResult {
            status: ActionStatus::Completed,
            summary: "Job requeued".to_string(),
            event_cursor: Some(42),
            affected_entity: Some(EntityRef::new(EntityKind::Job, "123")),
            evidence_created: Vec::new(),
        });
        assert!(matches!(state, ActionExecutionState::Completed { .. }));

        state.dismiss();
        assert!(matches!(state, ActionExecutionState::Idle));
    }

    #[test]
    fn system_actions_always_available() {
        let actions = available_actions(None);
        let ids: Vec<_> = actions.iter().map(|a| a.action_id).collect();
        assert!(ids.contains(&"next_action"));
        assert!(ids.contains(&"get_system_snapshot"));
    }

    #[test]
    fn undo_action_mapping() {
        assert_eq!(
            compute_undo_action("pause_pool"),
            Some("resume_pool".to_string())
        );
        assert_eq!(
            compute_undo_action("cancel_job"),
            Some("requeue_job".to_string())
        );
        assert_eq!(compute_undo_action("remove_record"), None);
    }
}
