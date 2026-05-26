//! Fleet rollup for delivery snapshots.

use crate::{
    release::ReleaseAttemptView,
    tui::lenses::workflow::model::{
        CanonicalPhase, Environment, FleetSummary, PrStatus, PullRequestView, WorkflowNodeKind,
    },
};

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
    let canary_url = release
        .and_then(|view| view.canary_public_url.clone())
        .or_else(|| node_canary_url(prs));

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

fn node_canary_url(prs: &[PullRequestView]) -> Option<String> {
    prs.iter().find_map(|pr| {
        pr.snapshot.nodes.iter().find_map(|node| {
            matches!(
                node.kind,
                WorkflowNodeKind::Promote {
                    env: Environment::Dev
                }
            )
            .then(|| node.reason.clone())
            .flatten()
        })
    })
}
