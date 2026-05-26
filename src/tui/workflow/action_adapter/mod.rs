//! Owner: Interactive TUI subsystem — Mission Control action adapter (Wave 6.A)
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter`
//! Invariants:
//!   - The trait is the ONLY surface `handle_delivery_action` touches. The TUI
//!     never imports `GitHubClient` or `KillBell` directly — those concrete
//!     types live behind [`ProductionActionAdapter`]. This keeps the action
//!     handler unit-testable against [`FakeActionAdapter`] without any
//!     network, FS, or embedded-database dependency.
//!   - Every public method returns `Result<_, String>` so failures surface as
//!     `ActionOutcome::Failed(msg)` in the TUI without exposing the underlying
//!     error type (which would otherwise leak `reqwest`/`anyhow` into the
//!     pure-render layer).
//!   - The `ProductionActionAdapter` is the only place that touches the
//!     signed-ledger pool. Cloning it is cheap (every wrapped field is
//!     already `Arc` / `AnyPool`-shaped).
//!
//! Wave 6.A wires the Wave 5.B action buttons (Approve / Block / Repair /
//! Freeze / KillBell) into real backends. Before this module the TUI logged
//! intent but did not act — see `docs/evidence-gate-spec.md` and
//! `tips/fullauto/tip8.txt` ("humans interrupt only at irreversible or
//! high-risk boundaries; every interrupt is a signed audit event").

use async_trait::async_trait;

use crate::autonomy::types::{GateDecision, LaunchLedgerEntry};

mod fake;
pub mod helpers;
mod production;

pub use fake::{FakeActionAdapter, RecordedCall};
pub use helpers as handler_helpers;
pub use production::ProductionActionAdapter;

#[cfg(test)]
mod tests;

/// Side-effect surface invoked by `App::handle_delivery_action` once an
/// operator confirms one of the 5 Mission Control buttons. Every method
/// returns `Result<_, String>` so the UI can surface failures verbatim via
/// `ActionOutcome::Failed(msg)`.
///
/// The trait is intentionally narrow:
/// - `post_passport_check` covers ApproveOnce + BlockVerdict (the GitHub-side
///   required check that gates merge).
/// - `post_mr_comment` covers BlockVerdict's secondary reason comment and
///   RequestRepair.
/// - `pause_kill_bell` covers the KillBell button (engaging the global pause).
/// - `append_ledger` covers EVERY action's "human intervention" audit row —
///   Tip4/6/8 require a signed ledger event per interrupt. This lives on the
///   adapter (not on a separate ledger seam) so the fake adapter has a single
///   place to record + assert ordering across calls.
#[async_trait]
pub trait ActionAdapter: Send + Sync {
    /// Post the canonical `vibegate/merge-passport` GitHub check on `head_sha`
    /// with the given `decision` and human-readable `summary`. Returns the
    /// host-side check-run id (or other dispatcher token) on success.
    async fn post_passport_check(
        &self,
        repo: &str,
        head_sha: &str,
        decision: GateDecision,
        summary: &str,
    ) -> Result<String, String>;

    /// Post a markdown comment on the merge request / pull request.
    async fn post_mr_comment(&self, repo: &str, mr_iid: &str, body: &str)
    -> Result<String, String>;

    /// Engage the Kill Bell for `ttl_seconds`. The adapter is responsible for
    /// signing + appending the canonical `KillBellEngaged` ledger event.
    /// Callers MUST NOT also append a `KillBellEngaged` row — see the
    /// `kill_bell_action_does_not_double_append_ledger_entry` regression
    /// guard in this module's tests.
    async fn pause_kill_bell(
        &self,
        reason: &str,
        paused_by: &str,
        ttl_seconds: u64,
    ) -> Result<(), String>;

    /// Append a signed human-intervention ledger entry. The handler builds
    /// the entry (kind, payload, actor) and the adapter signs + persists it.
    /// In tests this records the entry in `RecordedCall::AppendLedger` so
    /// assertions can verify ordering and duplicate-suppression.
    async fn append_ledger(&self, entry: LaunchLedgerEntry) -> Result<(), String>;

    /// Introspection hook: `"production"` for the real GitHub + SQL adapter,
    /// `"fake"` for the in-memory test/dry-run adapter. The App uses this to
    /// surface whether `try_install_production_adapter` swapped in a real
    /// backend without exposing the concrete adapter type.
    fn kind(&self) -> &'static str {
        "production"
    }
}
