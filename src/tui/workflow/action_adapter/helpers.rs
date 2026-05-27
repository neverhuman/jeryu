//! Owner: Interactive TUI subsystem — Mission Control action handler helpers (Wave 6.A)
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter`
//! Invariants: Pure helpers consumed by `App::handle_delivery_action`; no I/O.

//! Pure helpers consumed by `App::handle_delivery_action`. Kept in this module
//! so the trait, the production wiring, and the per-action plumbing all live
//! under one `cargo test` filter (`tui::workflow::action_adapter`).

use crate::autonomy::signing::Signature;
use crate::autonomy::types::{LaunchLedgerEntry, LedgerKind, SchemaTag};
use crate::tui::workflow::model::DeliverySnapshot;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Minimal projection of the currently focused PR; chosen via `pr_idx`.
/// The handler builds this once at the top of each branch so every
/// downstream call (passport, comment, ledger entry) sees a consistent
/// view of the PR even if the snapshot mutates mid-await.
#[derive(Debug, Clone)]
pub struct PrCtx {
    pub pr_number: u64,
    pub head_sha: String,
    /// `owner/name` repo slug. The demo snapshot does not carry a repo
    /// field, so we synthesize a placeholder; the production wiring
    /// will surface the real slug once `PullRequestView::repo` lands.
    pub repo_slug: String,
}

pub fn pr_ctx(snapshot: &DeliverySnapshot, pr_idx: usize) -> Option<PrCtx> {
    let pr = snapshot.pull_requests.get(pr_idx)?;
    Some(PrCtx {
        pr_number: pr.number,
        head_sha: pr.head_sha.clone(),
        // TODO(wave6.B): `PullRequestView` does not yet carry the host
        // slug. Until then we tag the synthetic value so tests can
        // assert the seam was hit without asserting the placeholder.
        repo_slug: "tui-cockpit/demo".to_string(),
    })
}

/// Build an UNSIGNED `LaunchLedgerEntry` for a PR-scoped human action.
/// The adapter signs + persists it. `actor` is always `"tui.cockpit.v1"`.
pub fn ledger_entry(
    kind: LedgerKind,
    ctx: &PrCtx,
    payload: serde_json::Value,
    now: DateTime<Utc>,
) -> LaunchLedgerEntry {
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_tui_{}", Uuid::new_v4()),
        kind,
        subject_id: format!("pr#{}", ctx.pr_number),
        repo: Some(ctx.repo_slug.clone()),
        payload,
        recorded_at: now,
        actor: "tui.cockpit.v1".into(),
        signature: Signature::stub(),
    }
}

/// Build an UNSIGNED `LaunchLedgerEntry` for a non-PR-scoped intent
/// (e.g. `FreezeAutonomy`, whose subject is the autonomy plane, not
/// any single PR).
pub fn ledger_entry_subject(
    kind: LedgerKind,
    subject_id: &str,
    repo: Option<String>,
    payload: serde_json::Value,
    now: DateTime<Utc>,
) -> LaunchLedgerEntry {
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_tui_{}", Uuid::new_v4()),
        kind,
        subject_id: subject_id.into(),
        repo,
        payload,
        recorded_at: now,
        actor: "tui.cockpit.v1".into(),
        signature: Signature::stub(),
    }
}
