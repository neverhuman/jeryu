//! Owner: Interactive TUI subsystem - Secrets lens data selector
//! Proof: `cargo test -p jeryu-tui --lib lenses::secrets::data`
//! Invariants: Pure projection to `SecretsLensInput`. No I/O.
//!             SECURITY: carries ONLY audit metadata. Never the secret value.

use jeryu_readmodel::TuiReadModel;

/// Embedded type (not yet in readmodel contract).
#[derive(Debug, Clone, Default)]
pub struct SecretAuditEvent {
    pub id: Option<i64>,
    pub repo_name: String,
    pub version: String,
    pub target: String,
    pub action: String,
    pub status: String,
    pub detail: String,
    pub created_at: String,
}

/// One secret-audit row — ONLY metadata fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretAuditRow {
    pub created_at: String,
    pub action: String,
    pub status: String,
    pub repo_name: String,
}

impl SecretAuditRow {
    fn from_event(ev: &SecretAuditEvent) -> Self {
        Self {
            created_at: ev.created_at.clone(),
            action: ev.action.clone(),
            status: ev.status.clone(),
            repo_name: ev.repo_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SecretsLensInput {
    pub events: Vec<SecretAuditRow>,
    pub selected: usize,
    pub vault_status: String,
    pub active_grants: u32,
    pub active_taints: u32,
}

impl SecretsLensInput {
    pub fn from_state(events: &[SecretAuditEvent], selected: usize) -> Self {
        Self {
            events: events.iter().map(SecretAuditRow::from_event).collect(),
            selected,
            ..Default::default()
        }
    }

    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            events: Vec::new(),
            selected: 0,
            vault_status: model.system.vault.status_label().to_string(),
            active_grants: model.mission.active_grants,
            active_taints: model.mission.active_taints,
        }
    }

    pub fn clamped_selection(&self) -> Option<usize> {
        if self.events.is_empty() {
            None
        } else {
            Some(self.selected.min(self.events.len() - 1))
        }
    }

    pub fn selected_row(&self) -> Option<&SecretAuditRow> {
        self.clamped_selection().and_then(|i| self.events.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_yields_empty_input() {
        let input = SecretsLensInput::from_state(&[], 0);
        assert!(input.events.is_empty());
        assert_eq!(input.selected, 0);
        assert_eq!(input.clamped_selection(), None);
        assert!(input.selected_row().is_none());
    }

    #[test]
    fn from_read_model_captures_vault() {
        let model = TuiReadModel::default();
        let input = SecretsLensInput::from_read_model(&model);
        assert!(!input.vault_status.is_empty());
    }
}
