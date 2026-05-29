//! Owner: Interactive TUI subsystem - Secrets lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::secrets::data`
//! Invariants: Pure projection from the live `SecretAuditEvent` ledger to
//!             `SecretsLensInput`. No I/O. SECURITY: carries ONLY audit
//!             metadata (action / status / repo / created_at). Never the
//!             secret value, the rotation target, or any vaulted material.

use crate::state::SecretAuditEvent;

/// One secret-audit row, projected from a `SecretAuditEvent`.
///
/// SECURITY: deliberately omits any field that could leak secret material
/// (`detail`, `target`, `version`). Only the four metadata fields the
/// legacy panel rendered are carried forward.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretAuditRow {
    /// ISO-ish timestamp of the audit event (e.g. `2026-05-29T12:00:00Z`).
    pub created_at: String,
    /// Audit action verb: `rotate`, `fetch`, `revoke`, …
    pub action: String,
    /// Outcome word: `ok` / `success` / `error` / `failed` / pending.
    pub status: String,
    /// Repository the secret belongs to.
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

/// Immutable projection of the secret-audit ledger for the secrets lens.
///
/// Owns its rows (cloned metadata only) so the view never touches app state
/// or the DB during render.
#[derive(Debug, Clone, Default)]
pub struct SecretsLensInput {
    /// Audit rows, newest-first as supplied by the source ledger.
    pub events: Vec<SecretAuditRow>,
    /// Index of the highlighted row, clamped into range on access.
    pub selected: usize,
}

impl SecretsLensInput {
    /// Project the lens input from the live `SecretAuditEvent` ledger.
    ///
    /// `selected` is carried through verbatim; the view clamps it for display
    /// so an out-of-range cursor never panics.
    pub fn from_state(events: &[SecretAuditEvent], selected: usize) -> Self {
        Self {
            events: events.iter().map(SecretAuditRow::from_event).collect(),
            selected,
        }
    }

    /// Selection clamped to a valid row index, or `None` when empty.
    pub fn clamped_selection(&self) -> Option<usize> {
        if self.events.is_empty() {
            None
        } else {
            Some(self.selected.min(self.events.len() - 1))
        }
    }

    /// The currently highlighted row, if any.
    pub fn selected_row(&self) -> Option<&SecretAuditRow> {
        self.clamped_selection().and_then(|i| self.events.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(action: &str, status: &str, repo: &str, created: &str) -> SecretAuditEvent {
        SecretAuditEvent {
            id: Some(1),
            repo_name: repo.into(),
            version: "v3.0.1".into(),
            target: "GITLAB_TOKEN".into(),
            action: action.into(),
            status: status.into(),
            // `detail` could hold sensitive context — it must never reach the lens.
            detail: "super-secret-value-should-never-render".into(),
            created_at: created.into(),
        }
    }

    #[test]
    fn empty_state_yields_empty_input() {
        let input = SecretsLensInput::from_state(&[], 0);
        assert!(input.events.is_empty());
        assert_eq!(input.selected, 0);
        assert_eq!(input.clamped_selection(), None);
        assert!(input.selected_row().is_none());
    }

    #[test]
    fn selected_index_is_preserved() {
        let events = vec![
            ev("rotate", "ok", "jeryu", "2026-05-29T12:00:00Z"),
            ev("fetch", "error", "jankurai", "2026-05-29T12:05:00Z"),
            ev("revoke", "ok", "jeryu", "2026-05-29T12:10:00Z"),
        ];
        let input = SecretsLensInput::from_state(&events, 2);
        assert_eq!(input.events.len(), 3);
        assert_eq!(input.selected, 2);
        assert_eq!(input.clamped_selection(), Some(2));
        assert_eq!(input.selected_row().unwrap().action, "revoke");
    }

    #[test]
    fn out_of_range_selection_clamps_for_display() {
        let events = vec![ev("rotate", "ok", "jeryu", "2026-05-29T12:00:00Z")];
        let input = SecretsLensInput::from_state(&events, 99);
        // The raw selection is preserved verbatim …
        assert_eq!(input.selected, 99);
        // … but display access clamps so it never panics.
        assert_eq!(input.clamped_selection(), Some(0));
        assert_eq!(input.selected_row().unwrap().action, "rotate");
    }

    #[test]
    fn projection_carries_only_metadata_never_secret_material() {
        let events = vec![ev("rotate", "ok", "jeryu", "2026-05-29T12:00:00Z")];
        let input = SecretsLensInput::from_state(&events, 0);
        let row = &input.events[0];
        assert_eq!(row.created_at, "2026-05-29T12:00:00Z");
        assert_eq!(row.action, "rotate");
        assert_eq!(row.status, "ok");
        assert_eq!(row.repo_name, "jeryu");
        // The row type structurally cannot hold `detail` / `target` / `version`,
        // so vaulted material has no path into the lens. This is asserted by the
        // absence of those fields on `SecretAuditRow` (compile-time guarantee).
    }
}
