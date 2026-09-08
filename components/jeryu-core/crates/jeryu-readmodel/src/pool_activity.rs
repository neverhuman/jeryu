//! Server-wide runner-pool activity rollup across ALL repos on the server.
//!
//! This is the operator-grade contract that powers the TUI Pools/Health tab and
//! the web `/fleet` page: who is queued / running / failing, how saturated each
//! pool is, which runners are stuck, and which tag demand no pool can serve.
//!
//! [`PoolActivity`] is pure data assembled by the read-model assembler from the
//! scheduler + runner registry. The derived operator signals (bottlenecks,
//! health) are computed by the **pure selectors** below and never stored, so the
//! TUI and the web surface cannot disagree about what is wrong.
//!
//! This is a Claude↔Codex seam: Codex's runner registry / scheduler produces the
//! raw [`PoolRollup`] / [`RepoActivity`] counts; the panes consume the selectors.

use serde::{Deserialize, Serialize};

use crate::entity::HealthLevel;
use crate::freshness::SourceFreshness;

/// Server-wide runner-pool activity across every repo on the server.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PoolActivity {
    /// Per-repo job rollup across the whole server.
    pub repos: Vec<RepoActivity>,
    /// Per-pool rollup, aggregated across all repos.
    pub pools: Vec<PoolRollup>,
    /// Queued tag demand that no online pool can currently serve (tag-starvation).
    pub unplaceable: Vec<TagDemand>,
    /// Freshness of the underlying scheduler/registry read.
    pub freshness: Option<SourceFreshness>,
}

/// Per-repo job activity rolled up across the server.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoActivity {
    /// `owner/name`.
    pub repo: String,
    pub queued_jobs: u32,
    pub running_jobs: u32,
    pub failed_jobs: u32,
    /// Pools this repo's jobs are dispatched to.
    pub pools: Vec<String>,
}

/// Per-pool rollup aggregated across all repos.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PoolRollup {
    pub pool: String,
    pub tags: Vec<String>,
    pub trust_tier: String,
    pub paused: bool,
    pub queued_jobs: u32,
    pub running_jobs: u32,
    pub failed_jobs: u32,
    /// Slots currently available for work (0 when paused).
    pub active_slots: u32,
    pub configured_max_slots: u32,
    pub online_runners: u32,
    /// Runners that are online but not Healthy (stuck / degraded / unreachable).
    pub stuck_runners: u32,
}

impl Default for PoolRollup {
    fn default() -> Self {
        Self {
            pool: "unknown".into(),
            tags: Vec::new(),
            trust_tier: "trusted".into(),
            paused: false,
            queued_jobs: 0,
            running_jobs: 0,
            failed_jobs: 0,
            active_slots: 0,
            configured_max_slots: 0,
            online_runners: 0,
            stuck_runners: 0,
        }
    }
}

impl PoolRollup {
    pub fn new(pool: impl Into<String>) -> Self {
        Self {
            pool: pool.into(),
            ..Self::default()
        }
    }

    /// Idle slots available for new work.
    pub fn idle_slots(&self) -> u32 {
        self.active_slots.saturating_sub(self.running_jobs)
    }

    /// Fraction of active slots in use, clamped to `0.0..=1.0`. A paused or
    /// empty pool reports `0.0` (no capacity) rather than dividing by zero.
    pub fn utilization(&self) -> f64 {
        if self.active_slots == 0 {
            return 0.0;
        }
        (f64::from(self.running_jobs) / f64::from(self.active_slots)).clamp(0.0, 1.0)
    }

    /// Saturated = work is queued but no idle slot can take it.
    pub fn is_saturated(&self) -> bool {
        self.queued_jobs > 0 && self.idle_slots() == 0
    }
}

/// A set of required tags and how many queued jobs demand it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagDemand {
    /// The tag set the queued work requires.
    pub tags: Vec<String>,
    /// How many queued jobs require it.
    pub count: u32,
}

/// Server-wide totals derived from the rollup.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityTotals {
    pub repos: u32,
    pub pools: u32,
    pub queued_jobs: u32,
    pub running_jobs: u32,
    pub failed_jobs: u32,
    pub online_runners: u32,
    pub stuck_runners: u32,
}

/// A derived operator signal — something an operator should react to. Computed
/// by [`PoolActivity::bottlenecks`]; never stored on the wire model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Bottleneck {
    /// A pool has queued work but no idle slot can take it.
    Saturated {
        pool: String,
        queued: u32,
        utilization: f64,
    },
    /// Queued work whose required tags no online pool can serve.
    TagStarved { tags: Vec<String>, count: u32 },
    /// A pool has online-but-stuck runners.
    StuckRunners { pool: String, count: u32 },
}

impl Bottleneck {
    /// Human-readable one-line description for the TUI banner / web alert.
    pub fn describe(&self) -> String {
        match self {
            Bottleneck::Saturated {
                pool,
                queued,
                utilization,
            } => format!(
                "{queued} job(s) queued on saturated pool '{pool}' ({:.0}% slots busy)",
                utilization * 100.0
            ),
            Bottleneck::TagStarved { tags, count } => format!(
                "{count} job(s) waiting for tag [{}] — no pool serves it",
                tags.join(", ")
            ),
            Bottleneck::StuckRunners { pool, count } => {
                format!("{count} runner(s) STUCK on pool '{pool}'")
            }
        }
    }

    /// Severity of this bottleneck for ranking and banner colour.
    pub fn severity(&self) -> HealthLevel {
        match self {
            Bottleneck::TagStarved { .. } => HealthLevel::Critical,
            Bottleneck::StuckRunners { .. } => HealthLevel::Degraded,
            Bottleneck::Saturated { .. } => HealthLevel::Warning,
        }
    }

    fn severity_rank(&self) -> u8 {
        match self.severity() {
            HealthLevel::Critical => 0,
            HealthLevel::Degraded => 1,
            HealthLevel::Warning => 2,
            HealthLevel::Unknown => 3,
            HealthLevel::Healthy => 4,
        }
    }
}

impl PoolActivity {
    /// Server-wide totals (pure rollup).
    pub fn totals(&self) -> ActivityTotals {
        let mut t = ActivityTotals {
            repos: self.repos.len() as u32,
            pools: self.pools.len() as u32,
            ..ActivityTotals::default()
        };
        for p in &self.pools {
            t.queued_jobs = t.queued_jobs.saturating_add(p.queued_jobs);
            t.running_jobs = t.running_jobs.saturating_add(p.running_jobs);
            t.failed_jobs = t.failed_jobs.saturating_add(p.failed_jobs);
            t.online_runners = t.online_runners.saturating_add(p.online_runners);
            t.stuck_runners = t.stuck_runners.saturating_add(p.stuck_runners);
        }
        t
    }

    /// Total online-but-stuck runners across the server.
    pub fn stuck_runner_count(&self) -> u32 {
        self.totals().stuck_runners
    }

    /// All derived bottlenecks an operator should react to, most severe first.
    pub fn bottlenecks(&self) -> Vec<Bottleneck> {
        let mut out = Vec::new();
        for d in &self.unplaceable {
            if d.count > 0 {
                out.push(Bottleneck::TagStarved {
                    tags: d.tags.clone(),
                    count: d.count,
                });
            }
        }
        for p in &self.pools {
            if p.is_saturated() {
                out.push(Bottleneck::Saturated {
                    pool: p.pool.clone(),
                    queued: p.queued_jobs,
                    utilization: p.utilization(),
                });
            }
            if p.stuck_runners > 0 {
                out.push(Bottleneck::StuckRunners {
                    pool: p.pool.clone(),
                    count: p.stuck_runners,
                });
            }
        }
        out.sort_by_key(Bottleneck::severity_rank);
        out
    }

    /// Overall pool-fabric health: Critical on any tag-starvation, Warning on
    /// any saturation, Degraded on any stuck runner, else Healthy. An empty
    /// rollup (no pools, no repos) is Unknown — never falsely green.
    pub fn health(&self) -> HealthLevel {
        if self.pools.is_empty() && self.repos.is_empty() {
            return HealthLevel::Unknown;
        }
        if self.unplaceable.iter().any(|d| d.count > 0) {
            return HealthLevel::Critical;
        }
        if self.pools.iter().any(PoolRollup::is_saturated) {
            return HealthLevel::Warning;
        }
        if self.stuck_runner_count() > 0 {
            return HealthLevel::Degraded;
        }
        HealthLevel::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(name: &str) -> PoolRollup {
        PoolRollup::new(name)
    }

    #[test]
    fn default_is_empty_and_health_unknown() {
        let a = PoolActivity::default();
        assert!(a.pools.is_empty());
        assert!(a.repos.is_empty());
        assert_eq!(a.health(), HealthLevel::Unknown);
        assert!(a.bottlenecks().is_empty());
        assert_eq!(a.stuck_runner_count(), 0);
    }

    #[test]
    fn pool_slot_math_is_saturating() {
        let mut p = pool("trusted");
        p.active_slots = 4;
        p.running_jobs = 6; // more running than slots — saturating
        assert_eq!(p.idle_slots(), 0);
        assert!((p.utilization() - 1.0).abs() < f64::EPSILON);

        p.running_jobs = 1;
        assert_eq!(p.idle_slots(), 3);
        assert!((p.utilization() - 0.25).abs() < f64::EPSILON);

        let paused = pool("p"); // active_slots 0
        assert_eq!(paused.utilization(), 0.0);
        assert_eq!(paused.idle_slots(), 0);
    }

    #[test]
    fn totals_sum_across_pools_and_count_repos() {
        let mut p1 = pool("a");
        p1.queued_jobs = 2;
        p1.running_jobs = 3;
        p1.failed_jobs = 1;
        p1.online_runners = 4;
        p1.stuck_runners = 1;
        let mut p2 = pool("b");
        p2.queued_jobs = 5;
        p2.online_runners = 2;
        let a = PoolActivity {
            repos: vec![RepoActivity::default(), RepoActivity::default()],
            pools: vec![p1, p2],
            ..PoolActivity::default()
        };

        let t = a.totals();
        assert_eq!(t.repos, 2);
        assert_eq!(t.pools, 2);
        assert_eq!(t.queued_jobs, 7);
        assert_eq!(t.running_jobs, 3);
        assert_eq!(t.failed_jobs, 1);
        assert_eq!(t.online_runners, 6);
        assert_eq!(t.stuck_runners, 1);
        assert_eq!(a.stuck_runner_count(), 1);
    }

    #[test]
    fn saturation_produces_warning_health_and_bottleneck() {
        let mut p = pool("trusted");
        p.active_slots = 2;
        p.running_jobs = 2;
        p.queued_jobs = 3; // queued but no idle slot
        let a = PoolActivity {
            pools: vec![p],
            ..PoolActivity::default()
        };
        assert!(a.pools[0].is_saturated());
        assert_eq!(a.health(), HealthLevel::Warning);
        let bn = a.bottlenecks();
        assert_eq!(bn.len(), 1);
        assert!(matches!(bn[0], Bottleneck::Saturated { queued: 3, .. }));
        assert!(bn[0].describe().contains("saturated pool 'trusted'"));
    }

    #[test]
    fn tag_starvation_is_critical_and_ranks_first() {
        let mut p = pool("trusted");
        p.active_slots = 2;
        p.running_jobs = 2;
        p.queued_jobs = 1; // also saturated (Warning)
        p.stuck_runners = 1; // also stuck (Degraded)
        let a = PoolActivity {
            pools: vec![p],
            unplaceable: vec![TagDemand {
                tags: vec!["rust-hot".into()],
                count: 4,
            }],
            ..PoolActivity::default()
        };
        assert_eq!(a.health(), HealthLevel::Critical);
        let bn = a.bottlenecks();
        // Critical (tag-starved) must rank before Degraded (stuck) before Warning (saturated).
        assert!(matches!(bn[0], Bottleneck::TagStarved { count: 4, .. }));
        assert!(matches!(bn[1], Bottleneck::StuckRunners { count: 1, .. }));
        assert!(matches!(bn[2], Bottleneck::Saturated { .. }));
        assert!(bn[0].describe().contains("no pool serves it"));
    }

    #[test]
    fn stuck_only_is_degraded() {
        let mut p = pool("trusted");
        p.active_slots = 4;
        p.running_jobs = 1; // healthy capacity
        p.stuck_runners = 2;
        let a = PoolActivity {
            pools: vec![p],
            ..PoolActivity::default()
        };
        assert_eq!(a.health(), HealthLevel::Degraded);
        assert_eq!(a.bottlenecks().len(), 1);
        assert!(matches!(
            a.bottlenecks()[0],
            Bottleneck::StuckRunners { count: 2, .. }
        ));
    }

    #[test]
    fn clean_pool_is_healthy() {
        let mut p = pool("trusted");
        p.active_slots = 4;
        p.running_jobs = 1;
        p.online_runners = 4;
        let a = PoolActivity {
            pools: vec![p],
            repos: vec![RepoActivity {
                repo: "neverhuman/jeryu".into(),
                running_jobs: 1,
                ..RepoActivity::default()
            }],
            ..PoolActivity::default()
        };
        assert_eq!(a.health(), HealthLevel::Healthy);
        assert!(a.bottlenecks().is_empty());
    }

    #[test]
    fn serde_round_trips() {
        let a = PoolActivity {
            repos: vec![RepoActivity {
                repo: "neverhuman/jeryu".into(),
                queued_jobs: 1,
                ..RepoActivity::default()
            }],
            pools: vec![pool("trusted")],
            unplaceable: vec![TagDemand {
                tags: vec!["x".into()],
                count: 1,
            }],
            freshness: None,
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: PoolActivity = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}
