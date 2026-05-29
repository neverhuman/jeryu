//! Delivery snapshot collector.

use chrono::Utc;

use crate::{
    release::ReleaseAttemptView,
    tui::lenses::workflow::{
        delivery::{fleet::compute_fleet_summary, inputs::PrInput, pipeline::build_pr_view},
        model::{DeliverySnapshot, PullRequestView},
    },
};

pub use super::demo::build_demo_delivery;

pub fn collect_delivery_snapshot(
    prs: &[PrInput],
    release: Option<&ReleaseAttemptView>,
) -> DeliverySnapshot {
    let now = Utc::now();
    let pull_requests: Vec<PullRequestView> = prs
        .iter()
        .map(|pr| build_pr_view(pr, release, now))
        .collect();
    let fleet_summary = compute_fleet_summary(&pull_requests, release);

    DeliverySnapshot {
        generated_at: now,
        pull_requests,
        selected_pr_idx: 0,
        fleet_summary,
        outdated: false,
        kill_bell_state: "armed".into(),
    }
}
