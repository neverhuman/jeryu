//! Runners lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`RunnersLensInput`].
//! No I/O. Per-node rows project from the read model's [`RunnersDashboard`]
//! items; live per-node telemetry (storage probes etc.) belongs to the daemon /
//! assembler, so the standalone lens reads only the contract.

use jeryu_readmodel::{
    ActivityTotals, Bottleneck, HealthLevel, PoolRollup, RunnersItem, RunnersSummary, TuiReadModel,
};

/// One runner-node row in the fleet grid.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerNodeRow {
    pub label: String,
    pub runner_id: String,
    pub pool: String,
    pub status: HealthLevel,
    pub tags: Vec<String>,
    /// Human-friendly last-contact, derived from `last_seen` presence.
    pub last_contact: String,
}

impl RunnerNodeRow {
    fn from_item(item: &RunnersItem) -> Self {
        Self {
            label: item.label.clone(),
            runner_id: item.runner_id.clone(),
            pool: item.pool.clone(),
            status: item.status,
            tags: item.tags.clone(),
            last_contact: match item.last_seen {
                Some(_) => "seen".into(),
                None => "—".into(),
            },
        }
    }

    /// Low-noise status word: stuck > busy > idle > online.
    ///
    /// `HealthLevel::Critical` reads as a stuck node (online but not making
    /// progress); `Degraded` reads as busy/saturated; `Warning` as idle.
    pub fn status_word(&self) -> &'static str {
        match self.status {
            HealthLevel::Critical => "STUCK",
            HealthLevel::Degraded => "busy",
            HealthLevel::Warning => "idle",
            HealthLevel::Healthy => "online",
            HealthLevel::Unknown => "probing",
        }
    }

    /// True when this node needs attention (stuck or unreachable/probing).
    pub fn is_alerting(&self) -> bool {
        matches!(self.status, HealthLevel::Critical)
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunnersLensInput {
    pub active_runners: u32,
    pub total_runners: u32,
    pub paused_runners: u32,
    pub draining_runners: u32,
    pub nodes: Vec<RunnerNodeRow>,
    pub event_cursor: u64,
}

impl RunnersLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.runners.summary.clone().unwrap_or(RunnersSummary {
            total_runners: model.mission.total_runners,
            active_runners: model.mission.active_runners,
            paused_runners: 0,
            draining_runners: 0,
        });
        let nodes: Vec<RunnerNodeRow> = model
            .runners
            .items
            .iter()
            .map(RunnerNodeRow::from_item)
            .collect();
        Self {
            active_runners: summary.active_runners,
            total_runners: summary.total_runners,
            paused_runners: summary.paused_runners,
            draining_runners: summary.draining_runners,
            nodes,
            event_cursor: model.event_cursor,
        }
    }

    /// Fleet utilization as whole-percent (0 when no runners are known).
    pub fn utilization_pct(&self) -> u32 {
        if self.total_runners == 0 {
            0
        } else {
            (self.active_runners as f64 / self.total_runners as f64 * 100.0).round() as u32
        }
    }

    /// Count of nodes flagged stuck — drives the fleet alert banner.
    pub fn stuck_nodes(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_alerting()).count()
    }
}

/// One per-pool row in the operator Pools/Health grid.
///
/// Pure projection of a [`PoolRollup`]; the derived slot/utilization math is read
/// straight from the contract's selectors so the TUI and the web `/fleet` page
/// cannot disagree.
#[derive(Debug, Clone, PartialEq)]
pub struct PoolHealthRow {
    pub pool: String,
    pub tags: Vec<String>,
    pub trust_tier: String,
    pub paused: bool,
    pub queued_jobs: u32,
    pub running_jobs: u32,
    pub failed_jobs: u32,
    pub active_slots: u32,
    pub idle_slots: u32,
    pub configured_max_slots: u32,
    pub online_runners: u32,
    pub stuck_runners: u32,
    /// Whole-percent utilization (`running / active_slots`).
    pub utilization_pct: u32,
    pub saturated: bool,
}

impl PoolHealthRow {
    fn from_rollup(p: &PoolRollup) -> Self {
        Self {
            pool: p.pool.clone(),
            tags: p.tags.clone(),
            trust_tier: p.trust_tier.clone(),
            paused: p.paused,
            queued_jobs: p.queued_jobs,
            running_jobs: p.running_jobs,
            failed_jobs: p.failed_jobs,
            active_slots: p.active_slots,
            idle_slots: p.idle_slots(),
            configured_max_slots: p.configured_max_slots,
            online_runners: p.online_runners,
            stuck_runners: p.stuck_runners,
            utilization_pct: (p.utilization() * 100.0).round() as u32,
            saturated: p.is_saturated(),
        }
    }
}

/// Operator Pools/Health projection: a pure read of [`PoolActivity`] for the
/// runner-pool fleet tab. Mirrors [`RunnersLensInput::from_read_model`] — fleet
/// totals via `totals()`, a per-pool table, and the ranked bottleneck/health
/// banner from the contract's pure selectors.
///
/// [`PoolActivity`]: jeryu_readmodel::PoolActivity
#[derive(Debug, Clone)]
pub struct PoolHealthInput {
    pub totals: ActivityTotals,
    pub health: HealthLevel,
    pub pools: Vec<PoolHealthRow>,
    /// Ranked operator bottlenecks (most severe first), pre-described.
    pub bottlenecks: Vec<Bottleneck>,
    pub event_cursor: u64,
}

impl Default for PoolHealthInput {
    fn default() -> Self {
        // Mirrors `PoolActivity::default()`: an empty rollup is Unknown, never
        // falsely green.
        Self {
            totals: ActivityTotals::default(),
            health: HealthLevel::Unknown,
            pools: Vec::new(),
            bottlenecks: Vec::new(),
            event_cursor: 0,
        }
    }
}

impl PoolHealthInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let activity = &model.pool_activity;
        Self {
            totals: activity.totals(),
            health: activity.health(),
            pools: activity
                .pools
                .iter()
                .map(PoolHealthRow::from_rollup)
                .collect(),
            bottlenecks: activity.bottlenecks(),
            event_cursor: model.event_cursor,
        }
    }

    /// Fleet-wide whole-percent utilization (running jobs over online slots).
    pub fn fleet_utilization_pct(&self) -> u32 {
        let online: u32 = self.pools.iter().map(|p| p.active_slots).sum();
        if online == 0 {
            0
        } else {
            (self.totals.running_jobs as f64 / online as f64 * 100.0).round() as u32
        }
    }

    /// True when the operator has at least one signal to react to.
    pub fn has_bottlenecks(&self) -> bool {
        !self.bottlenecks.is_empty()
    }

    /// The single highest-severity banner line, or a healthy/idle summary.
    pub fn banner_line(&self) -> String {
        match self.bottlenecks.first() {
            Some(top) => top.describe(),
            None if self.pools.is_empty() && self.totals.repos == 0 => {
                "No pool telemetry yet — awaiting scheduler/registry read.".to_string()
            }
            None => "Fleet healthy — no pool bottlenecks.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{RunnersDashboard, sample_read_model};

    #[test]
    fn from_default_read_model_has_no_nodes() {
        let input = RunnersLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_runners, 0);
        assert_eq!(input.total_runners, 0);
        assert!(input.nodes.is_empty());
        assert_eq!(input.utilization_pct(), 0);
        assert_eq!(input.stuck_nodes(), 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn projects_summary_and_nodes_from_dashboard() {
        let model = sample_read_model();
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.total_runners, 8);
        assert_eq!(input.active_runners, 6);
        assert_eq!(input.paused_runners, 1);
        assert_eq!(input.draining_runners, 1);
        assert_eq!(input.nodes.len(), 1);
        assert_eq!(input.nodes[0].label, "oci-runner-1");
        assert_eq!(input.utilization_pct(), 75);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn status_word_classifies_health_levels() {
        let mut row = RunnerNodeRow {
            label: "n".into(),
            runner_id: "r".into(),
            pool: "trusted".into(),
            status: HealthLevel::Healthy,
            tags: vec![],
            last_contact: "seen".into(),
        };
        assert_eq!(row.status_word(), "online");
        row.status = HealthLevel::Degraded;
        assert_eq!(row.status_word(), "busy");
        row.status = HealthLevel::Warning;
        assert_eq!(row.status_word(), "idle");
        row.status = HealthLevel::Critical;
        assert_eq!(row.status_word(), "STUCK");
        assert!(row.is_alerting());
    }

    #[test]
    fn falls_back_to_mission_counts_without_summary() {
        let mut model = TuiReadModel::default();
        model.mission.active_runners = 2;
        model.mission.total_runners = 5;
        model.runners = RunnersDashboard::default();
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.active_runners, 2);
        assert_eq!(input.total_runners, 5);
        assert_eq!(input.utilization_pct(), 40);
    }

    #[test]
    fn stuck_nodes_counts_critical() {
        let mut model = TuiReadModel::default();
        model.runners.items = vec![
            jeryu_readmodel::RunnersItem {
                id: "1".into(),
                label: "n1".into(),
                runner_id: "r1".into(),
                pool: "trusted".into(),
                status: HealthLevel::Critical,
                tags: vec![],
                last_seen: None,
            },
            jeryu_readmodel::RunnersItem {
                id: "2".into(),
                label: "n2".into(),
                runner_id: "r2".into(),
                pool: "trusted".into(),
                status: HealthLevel::Healthy,
                tags: vec![],
                last_seen: None,
            },
        ];
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.stuck_nodes(), 1);
    }

    // ── Pools/Health projection (PoolActivity) ──────────────────────────────

    use jeryu_readmodel::{PoolActivity, RepoActivity, TagDemand};

    #[test]
    fn pool_health_default_is_unknown_and_empty() {
        let input = PoolHealthInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.health, HealthLevel::Unknown);
        assert!(input.pools.is_empty());
        assert!(input.bottlenecks.is_empty());
        assert!(!input.has_bottlenecks());
        assert_eq!(input.fleet_utilization_pct(), 0);
        assert!(input.banner_line().contains("No pool telemetry"));
    }

    #[test]
    fn pool_health_projects_rollup_and_ranked_bottlenecks() {
        let mut saturated = PoolRollup::new("trusted");
        saturated.tags = vec!["oci".into()];
        saturated.active_slots = 2;
        saturated.running_jobs = 2;
        saturated.queued_jobs = 5;
        saturated.failed_jobs = 1;
        saturated.online_runners = 2;
        saturated.stuck_runners = 1;

        let model = TuiReadModel {
            event_cursor: 91,
            pool_activity: PoolActivity {
                repos: vec![RepoActivity {
                    repo: "neverhuman/jeryu".into(),
                    queued_jobs: 5,
                    running_jobs: 2,
                    ..RepoActivity::default()
                }],
                pools: vec![saturated],
                unplaceable: vec![TagDemand {
                    tags: vec!["gpu".into()],
                    count: 3,
                }],
                freshness: None,
            },
            ..Default::default()
        };

        let input = PoolHealthInput::from_read_model(&model);
        // Totals roll up from the contract selectors.
        assert_eq!(input.totals.pools, 1);
        assert_eq!(input.totals.repos, 1);
        assert_eq!(input.totals.queued_jobs, 5);
        assert_eq!(input.totals.running_jobs, 2);
        assert_eq!(input.totals.stuck_runners, 1);
        assert_eq!(input.event_cursor, 91);

        // Per-pool row math mirrors PoolRollup selectors.
        assert_eq!(input.pools.len(), 1);
        let row = &input.pools[0];
        assert_eq!(row.pool, "trusted");
        assert_eq!(row.utilization_pct, 100);
        assert_eq!(row.idle_slots, 0);
        assert!(row.saturated);

        // Tag-starvation (Critical) ranks first, then stuck (Degraded), then
        // saturation (Warning).
        assert_eq!(input.health, HealthLevel::Critical);
        assert!(input.has_bottlenecks());
        assert!(matches!(
            input.bottlenecks[0],
            jeryu_readmodel::Bottleneck::TagStarved { count: 3, .. }
        ));
        assert!(input.banner_line().contains("no pool serves it"));
        // Fleet utilization: 2 running / 2 active slots = 100%.
        assert_eq!(input.fleet_utilization_pct(), 100);
    }

    #[test]
    fn pool_health_healthy_pool_has_no_bottlenecks() {
        let mut clean = PoolRollup::new("trusted");
        clean.active_slots = 4;
        clean.running_jobs = 1;
        clean.online_runners = 4;
        let model = TuiReadModel {
            pool_activity: PoolActivity {
                pools: vec![clean],
                repos: vec![RepoActivity {
                    repo: "neverhuman/jeryu".into(),
                    running_jobs: 1,
                    ..RepoActivity::default()
                }],
                ..PoolActivity::default()
            },
            ..Default::default()
        };
        let input = PoolHealthInput::from_read_model(&model);
        assert_eq!(input.health, HealthLevel::Healthy);
        assert!(!input.has_bottlenecks());
        assert!(input.banner_line().contains("Fleet healthy"));
        assert_eq!(input.fleet_utilization_pct(), 25);
    }
}
