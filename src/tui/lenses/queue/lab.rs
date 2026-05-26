use crate::{api::read_model::QueuePoolSnapshot, tui::lenses::queue::QueueLensInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueCapacity {
    pub queued_jobs: u32,
    pub running_jobs: u32,
    pub online_runners: u32,
    pub busy_runners: u32,
    pub idle_runners: u32,
    pub active_parallel_limit: u32,
    pub configured_max_slots: u32,
    pub theoretical_limit: u32,
    pub saturation_pct: u8,
    pub add_runner_effect: AddRunnerEffect,
}

impl QueueCapacity {
    pub fn status_key(&self) -> &'static str {
        match self.add_runner_effect {
            AddRunnerEffect::NoQueue => "success",
            AddRunnerEffect::Helpful => "running",
            AddRunnerEffect::IdleCapacity | AddRunnerEffect::PausedCapacity => "waiting",
            AddRunnerEffect::AtConfiguredLimit
            | AddRunnerEffect::TagBound
            | AddRunnerEffect::SourceLimited => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddRunnerEffect {
    Helpful,
    NoQueue,
    IdleCapacity,
    TagBound,
    PausedCapacity,
    AtConfiguredLimit,
    SourceLimited,
}

impl AddRunnerEffect {
    pub fn label(self) -> &'static str {
        match self {
            Self::Helpful => "ADDING RUNNERS HELPS",
            Self::NoQueue => "NO QUEUE",
            Self::IdleCapacity => "IDLE CAPACITY",
            Self::TagBound => "TAG BOUND",
            Self::PausedCapacity => "PAUSED CAPACITY",
            Self::AtConfiguredLimit => "AT CONFIGURED LIMIT",
            Self::SourceLimited => "SOURCE LIMITED",
        }
    }

    pub fn explanation(self) -> &'static str {
        match self {
            Self::Helpful => "queued work is runner-saturated",
            Self::NoQueue => "there are no waiting jobs",
            Self::IdleCapacity => "idle runners exist; scheduling or locks are the limit",
            Self::TagBound => "waiting jobs require tags no configured pool has",
            Self::PausedCapacity => "paused pools can be resumed before adding runners",
            Self::AtConfiguredLimit => "raise pool limits or add nodes before adding managers",
            Self::SourceLimited => "no runner source is available",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDelta {
    pub additional_runners: u32,
    pub current_limit: u32,
    pub projected_limit: u32,
    pub jobs_unblocked: u32,
    pub effect: AddRunnerEffect,
}

pub fn analyze_capacity(input: QueueLensInput<'_>) -> QueueCapacity {
    let queued_jobs = queued_jobs(input);
    let running_jobs = running_jobs(input);
    let active_parallel_limit = active_parallel_limit(input);
    let configured_max_slots = configured_max_slots(input);
    let busy_runners = busy_slots(input);
    let idle_runners = active_parallel_limit.saturating_sub(busy_runners);
    let demand = queued_jobs.saturating_add(running_jobs);
    let theoretical_limit = demand.min(configured_max_slots);
    let saturation_pct = if active_parallel_limit == 0 {
        if queued_jobs > 0 { 100 } else { 0 }
    } else {
        ((busy_runners.min(active_parallel_limit) * 100) / active_parallel_limit) as u8
    };
    let add_runner_effect = add_runner_effect(
        input,
        queued_jobs,
        idle_runners,
        active_parallel_limit,
        configured_max_slots,
    );

    QueueCapacity {
        queued_jobs,
        running_jobs,
        online_runners: active_managers(input),
        busy_runners,
        idle_runners,
        active_parallel_limit,
        configured_max_slots,
        theoretical_limit,
        saturation_pct,
        add_runner_effect,
    }
}

pub fn runner_delta(input: QueueLensInput<'_>, additional_runners: u32) -> QueueDelta {
    let capacity = analyze_capacity(input);
    let demand = capacity.queued_jobs.saturating_add(capacity.running_jobs);
    let current_limit = demand.min(capacity.active_parallel_limit);
    let added_slots = match capacity.add_runner_effect {
        AddRunnerEffect::Helpful => {
            best_slots_per_manager(input).saturating_mul(additional_runners)
        }
        _ => 0,
    };
    let projected_active = capacity
        .active_parallel_limit
        .saturating_add(added_slots)
        .min(capacity.configured_max_slots);
    let projected_limit = demand.min(projected_active);

    QueueDelta {
        additional_runners,
        current_limit,
        projected_limit,
        jobs_unblocked: projected_limit.saturating_sub(current_limit),
        effect: capacity.add_runner_effect,
    }
}

fn add_runner_effect(
    input: QueueLensInput<'_>,
    queued_jobs: u32,
    idle_runners: u32,
    active_slots: u32,
    configured_max_slots: u32,
) -> AddRunnerEffect {
    if queued_jobs == 0 {
        AddRunnerEffect::NoQueue
    } else if configured_max_slots == 0 {
        AddRunnerEffect::SourceLimited
    } else if has_tag_bound_wait(input) {
        AddRunnerEffect::TagBound
    } else if idle_runners > 0 {
        AddRunnerEffect::IdleCapacity
    } else if has_paused_capacity(input) {
        AddRunnerEffect::PausedCapacity
    } else if active_slots >= configured_max_slots {
        AddRunnerEffect::AtConfiguredLimit
    } else {
        AddRunnerEffect::Helpful
    }
}

fn queued_jobs(input: QueueLensInput<'_>) -> u32 {
    if input.queue().total_waiting_jobs > 0 {
        input.queue().total_waiting_jobs
    } else if !input.queue().waiting_jobs.is_empty() {
        input.waiting_jobs().len() as u32
    } else {
        input.model.mission.queued_jobs
    }
}

fn running_jobs(input: QueueLensInput<'_>) -> u32 {
    if input.queue().total_running_jobs > 0 {
        input.queue().total_running_jobs
    } else if !input.queue().pools.is_empty() {
        input
            .queue()
            .pools
            .iter()
            .map(|pool| pool.running_jobs)
            .sum()
    } else {
        input.model.mission.running_jobs
    }
}

fn active_parallel_limit(input: QueueLensInput<'_>) -> u32 {
    if input.queue().pools.is_empty() {
        input
            .model
            .mission
            .total_runners
            .max(input.model.system.runners.online)
    } else {
        input
            .queue()
            .pools
            .iter()
            .map(QueuePoolSnapshot::active_slots)
            .sum()
    }
}

fn configured_max_slots(input: QueueLensInput<'_>) -> u32 {
    if input.queue().pools.is_empty() {
        input
            .model
            .mission
            .total_runners
            .max(input.model.system.runners.online)
    } else {
        input
            .queue()
            .pools
            .iter()
            .map(QueuePoolSnapshot::configured_max_slots)
            .sum()
    }
}

fn active_managers(input: QueueLensInput<'_>) -> u32 {
    if input.queue().pools.is_empty() {
        input.model.system.runners.online
    } else {
        input
            .queue()
            .pools
            .iter()
            .filter(|pool| !pool.paused)
            .map(|pool| pool.active_managers)
            .sum()
    }
}

fn busy_slots(input: QueueLensInput<'_>) -> u32 {
    if input.queue().pools.is_empty() {
        input.model.system.runners.busy
    } else {
        input
            .queue()
            .pools
            .iter()
            .filter(|pool| !pool.paused)
            .map(|pool| pool.running_jobs)
            .sum()
    }
}

fn best_slots_per_manager(input: QueueLensInput<'_>) -> u32 {
    input
        .queue()
        .pools
        .iter()
        .filter(|pool| !pool.paused && pool.active_managers < pool.max_managers)
        .filter(|pool| pool_fits_any_wait(input, pool))
        .map(|pool| pool.slots_per_manager.max(1))
        .max()
        .unwrap_or(1)
}

fn has_paused_capacity(input: QueueLensInput<'_>) -> bool {
    input
        .queue()
        .pools
        .iter()
        .filter(|pool| pool.paused && pool.configured_max_slots() > 0)
        .any(|pool| pool_fits_any_wait(input, pool))
}

fn has_tag_bound_wait(input: QueueLensInput<'_>) -> bool {
    !input.queue().pools.is_empty()
        && input.waiting_jobs().into_iter().any(|job| {
            !job.required_tags.is_empty()
                && !input
                    .queue()
                    .pools
                    .iter()
                    .any(|pool| pool.supports_tags(&job.required_tags))
        })
}

fn pool_fits_any_wait(input: QueueLensInput<'_>, pool: &QueuePoolSnapshot) -> bool {
    let waiting = input.waiting_jobs();
    waiting.is_empty()
        || waiting
            .iter()
            .any(|job| pool.supports_tags(&job.required_tags))
}
