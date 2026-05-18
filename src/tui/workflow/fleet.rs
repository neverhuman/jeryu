//! Owner: Interactive TUI subsystem — fleet summary + CanonicalPhase ordering
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure computation; no side effects.

use super::model::*;
use crate::release::ReleaseAttemptView;

pub(super) fn compute_fleet_summary(
    prs: &[PullRequestView],
    release: Option<&ReleaseAttemptView>,
) -> FleetSummary {
    let open_prs = prs
        .iter()
        .filter(|pr| pr.status != PrStatus::Closed)
        .count() as u32;
    let ready_to_ship = prs
        .iter()
        .filter(|pr| pr.phase >= CanonicalPhase::PromoteProd)
        .count() as u32;
    let running = prs
        .iter()
        .filter(|pr| pr.status == PrStatus::Running)
        .count() as u32;
    let blocked = prs
        .iter()
        .filter(|pr| pr.status == PrStatus::Blocked)
        .count() as u32;
    let merged_today = prs
        .iter()
        .filter(|pr| pr.status == PrStatus::Merged)
        .count() as u32;

    let canary_in_flight = prs.iter().any(|pr| pr.phase == CanonicalPhase::PromoteDev);
    let prod_in_flight = prs.iter().any(|pr| pr.phase == CanonicalPhase::PromoteProd);

    let canary_url = match release.and_then(|v| v.canary_public_url.clone()) {
        Some(url) => Some(url),
        None => prs.iter().find_map(|pr| {
            pr.snapshot.nodes.iter().find_map(|n| {
                matches!(
                    n.kind,
                    WorkflowNodeKind::Promote {
                        env: Environment::Dev
                    }
                )
                .then(|| n.reason.clone())
                .flatten()
            })
        }),
    };

    FleetSummary {
        open_prs,
        ready_to_ship,
        running,
        blocked,
        merged_today,
        canary_in_flight,
        prod_in_flight,
        canary_url,
        top_blocker: None,
    }
}

// ─── PartialOrd / Ord for CanonicalPhase ────────────────────────────────────

impl PartialOrd for CanonicalPhase {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CanonicalPhase {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let lhs = CanonicalPhase::ALL
            .iter()
            .position(|p| p == self)
            .unwrap_or(0);
        let rhs = CanonicalPhase::ALL
            .iter()
            .position(|p| p == other)
            .unwrap_or(0);
        lhs.cmp(&rhs)
    }
}
