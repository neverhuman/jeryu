//! Owner: Runner scheduler
//! Proof: `cargo test -p jeryu --lib runner_scheduler test_runner`
//! Invariants: priority overrides FIFO; equal priority is fair across projects.

use std::collections::{BTreeMap, VecDeque};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulerPriority {
    Low,
    #[default]
    Normal,
    High,
    Override,
}

impl SchedulerPriority {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Override => "override",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulerReason {
    #[default]
    General,
    CherryPick,
    TestFix,
    ReleaseFix,
}

impl SchedulerReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::CherryPick => "cherry-pick",
            Self::TestFix => "test-fix",
            Self::ReleaseFix => "release-fix",
        }
    }

    pub fn default_priority(self) -> SchedulerPriority {
        match self {
            Self::General => SchedulerPriority::Normal,
            Self::CherryPick | Self::TestFix | Self::ReleaseFix => SchedulerPriority::High,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::ReleaseFix => 0,
            Self::TestFix => 1,
            Self::CherryPick => 2,
            Self::General => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerRequest {
    pub id: String,
    pub project_id: i64,
    pub priority: SchedulerPriority,
    pub reason: SchedulerReason,
    /// Monotonic enqueue position or timestamp. Lower values are older.
    pub submitted_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledRequest {
    pub id: String,
    pub project_id: i64,
    pub priority: SchedulerPriority,
    pub reason: SchedulerReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerPolicy {
    /// Normal/low items older than this are promoted to high priority.
    /// Set to zero to disable aging.
    pub age_boost_after: Option<u64>,
}

impl Default for SchedulerPolicy {
    fn default() -> Self {
        Self {
            age_boost_after: Some(45 * 60),
        }
    }
}

pub fn schedule_requests(
    requests: &[SchedulerRequest],
    capacity: usize,
    now: u64,
    policy: SchedulerPolicy,
) -> Vec<ScheduledRequest> {
    if capacity == 0 || requests.is_empty() {
        return Vec::new();
    }

    let mut remaining = capacity;
    let mut scheduled = Vec::with_capacity(capacity.min(requests.len()));
    for tier in [
        SchedulerPriority::Override,
        SchedulerPriority::High,
        SchedulerPriority::Normal,
        SchedulerPriority::Low,
    ] {
        if remaining == 0 {
            break;
        }

        let tier_items = requests
            .iter()
            .filter(|request| effective_priority(request, now, policy) == tier);
        let mut picked = drain_tier_round_robin(tier_items, remaining, tier);
        remaining -= picked.len();
        scheduled.append(&mut picked);
    }

    scheduled
}

fn effective_priority(
    request: &SchedulerRequest,
    now: u64,
    policy: SchedulerPolicy,
) -> SchedulerPriority {
    if request.priority >= SchedulerPriority::High {
        return request.priority;
    }

    if let Some(threshold) = policy.age_boost_after
        && threshold > 0
        && now.saturating_sub(request.submitted_at) >= threshold
    {
        return SchedulerPriority::High;
    }

    request.priority
}

fn drain_tier_round_robin<'a>(
    requests: impl Iterator<Item = &'a SchedulerRequest>,
    capacity: usize,
    tier: SchedulerPriority,
) -> Vec<ScheduledRequest> {
    let mut by_project: BTreeMap<i64, Vec<&SchedulerRequest>> = BTreeMap::new();
    for request in requests {
        by_project
            .entry(request.project_id)
            .or_default()
            .push(request);
    }

    let mut projects = VecDeque::new();
    let mut grouped = Vec::new();
    for (project_id, mut project_requests) in by_project {
        project_requests.sort_by_key(|request| (request.reason.rank(), request.submitted_at));
        let oldest = project_requests
            .iter()
            .map(|request| request.submitted_at)
            .min()
            .unwrap_or(u64::MAX);
        grouped.push((oldest, project_id, VecDeque::from(project_requests)));
    }
    grouped.sort_by_key(|(oldest, project_id, _)| (*oldest, *project_id));
    for (_, project_id, project_requests) in grouped {
        projects.push_back((project_id, project_requests));
    }

    let mut scheduled = Vec::with_capacity(capacity);
    while scheduled.len() < capacity {
        let Some((project_id, mut queue)) = projects.pop_front() else {
            break;
        };

        if let Some(request) = queue.pop_front() {
            scheduled.push(ScheduledRequest {
                id: request.id.clone(),
                project_id: request.project_id,
                priority: tier,
                reason: request.reason,
            });
        }

        if !queue.is_empty() {
            projects.push_back((project_id, queue));
        }
    }

    scheduled
}

#[cfg(test)]
#[path = "runner_scheduler_tests.rs"]
mod runner_scheduler_tests;
