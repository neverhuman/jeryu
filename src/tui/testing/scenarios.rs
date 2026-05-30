use chrono::{DateTime, TimeZone, Utc};

use crate::{
    api::{
        actions::{ActionStatus, ActionStreamEvent, ActionStreamPage, ActionStreamPhase},
        entity::{
            ActionRef, BlockerSummary, DataFreshness, EntityKind, EntityRef, HealthLevel, Severity,
        },
        events::{TuiEvent, TuiEventKind},
        freshness::{FreshnessState, SourceFreshness, SourceKind},
        read_model::{
            ActionSafety, AttentionItem, ComponentHealth, MissionSnapshot,
            NextActionRecommendation, RunnerHealth, SCHEMA_VERSION, SystemHealth, TuiReadModel,
        },
        runtime_profile::RuntimeProfile,
    },
    tui::action_registry::RiskTier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixtureScenario {
    Healthy,
    Empty,
    Stale,
    Aged,
    Degraded,
    SourceDown,
    Security,
    Release,
    Cache,
    Vti,
    Agent,
    Bug,
    Jankurai,
    Incident,
}

impl FixtureScenario {
    pub const ALL: &'static [Self] = &[
        Self::Healthy,
        Self::Empty,
        Self::Stale,
        Self::Aged,
        Self::Degraded,
        Self::SourceDown,
        Self::Security,
        Self::Release,
        Self::Cache,
        Self::Vti,
        Self::Agent,
        Self::Bug,
        Self::Jankurai,
        Self::Incident,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Empty => "empty",
            Self::Stale => "stale",
            Self::Aged => "aged",
            Self::Degraded => "degraded",
            Self::SourceDown => "source_down",
            Self::Security => "security",
            Self::Release => "release",
            Self::Cache => "cache",
            Self::Vti => "vti",
            Self::Agent => "agent",
            Self::Bug => "bug",
            Self::Jankurai => "jankurai",
            Self::Incident => "incident",
        }
    }

    fn primary_entity(self) -> EntityRef {
        match self {
            Self::Healthy | Self::Empty => EntityRef::new(EntityKind::System, self.label()),
            Self::Stale | Self::Aged | Self::SourceDown => {
                EntityRef::new(EntityKind::Source, self.label())
            }
            Self::Degraded => EntityRef::new(EntityKind::RunnerPool, "pool/default"),
            Self::Security => EntityRef::new(EntityKind::SecurityFinding, "sec/high-risk-secret"),
            Self::Release => EntityRef::new(EntityKind::ReleaseGate, "rel/canary"),
            Self::Cache => EntityRef::new(EntityKind::CacheTaint, "cache/root"),
            Self::Vti => EntityRef::new(EntityKind::TestPlan, "vti/accelerated"),
            Self::Agent => EntityRef::new(EntityKind::AgentSession, "agent/session-1"),
            Self::Bug => EntityRef::new(EntityKind::Bug, "bug/ready-1"),
            Self::Jankurai => EntityRef::new(EntityKind::JankuraiFinding, "jankurai/score"),
            Self::Incident => EntityRef::new(EntityKind::System, "incident/pinned"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScenarioFixture {
    pub scenario: FixtureScenario,
    pub generated_at: DateTime<Utc>,
    pub read_model: TuiReadModel,
    pub runtime: RuntimeProfile,
    pub sources: Vec<SourceFreshness>,
    pub events: Vec<TuiEvent>,
    pub action_stream: ActionStreamPage,
}

impl ScenarioFixture {
    pub fn build(scenario: FixtureScenario) -> Self {
        let generated_at = fixture_time();
        let mut model = base_model(generated_at);
        let mut sources = vec![source(
            SourceKind::Fixture,
            FreshnessState::Live,
            generated_at,
            0,
            scenario.label(),
        )];

        apply_scenario(scenario, &mut model, &mut sources, generated_at);
        super::repo_scenarios::apply_repo_fixture(scenario, &mut model);
        let event = event_for(scenario, generated_at, model.event_cursor + 1);
        model.event_cursor = event.seq;

        Self {
            scenario,
            generated_at,
            read_model: model,
            runtime: RuntimeProfile::new("fixture", "sqlite", "kafka"),
            sources,
            events: vec![event],
            action_stream: action_stream(scenario, generated_at),
        }
    }
}

fn base_model(generated_at: DateTime<Utc>) -> TuiReadModel {
    let mut model = TuiReadModel::default();
    model.schema_version = SCHEMA_VERSION.into();
    model.generated_at = generated_at;
    model.freshness = DataFreshness {
        gitlab_ms: Some(120),
        state_store_ms: Some(20),
        docker_ms: Some(90),
        cache_ms: Some(35),
        vault_ms: Some(50),
        overall_stale: false,
    };
    model.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: false,
        active_agents: 2,
        running_jobs: 3,
        queued_jobs: 1,
        active_runners: 4,
        total_runners: 4,
        evidence_count: 12,
        cache_hit_ratio: 0.91,
        ..MissionSnapshot::default()
    };
    model.system = SystemHealth {
        gitlab: ComponentHealth::ok("gitlab", 18),
        database: ComponentHealth::ok("database", 6),
        docker: ComponentHealth::ok("docker", 12),
        cache: ComponentHealth::ok("cache", 8),
        vault: ComponentHealth::ok("vault", 15),
        runners: RunnerHealth {
            online: 4,
            busy: 2,
            idle: 2,
            degraded: 0,
        },
    };
    model.next_action = Some(next_action(
        "open_logs",
        "Inspect latest proof",
        "healthy fixture recommends read-only inspection",
        RiskTier::ReadOnly,
        generated_at,
    ));
    model
}

fn apply_scenario(
    scenario: FixtureScenario,
    model: &mut TuiReadModel,
    sources: &mut Vec<SourceFreshness>,
    generated_at: DateTime<Utc>,
) {
    match scenario {
        FixtureScenario::Healthy => {}
        FixtureScenario::Empty => {
            model.mission = MissionSnapshot::default();
            model.next_action = None;
        }
        FixtureScenario::Stale | FixtureScenario::Aged => {
            model.freshness.overall_stale = true;
            model.freshness.gitlab_ms = Some(900_000);
            sources.push(source(
                SourceKind::GitLab,
                FreshnessState::Aged,
                generated_at,
                900_000,
                "gitlab:stale",
            ));
            add_attention(
                model,
                scenario,
                Severity::Warning,
                "GitLab data is stale",
                generated_at,
            );
        }
        FixtureScenario::Degraded => {
            model.mission.overall = HealthLevel::Degraded;
            model.mission.safe_to_merge = false;
            model.mission.failed_jobs = 2;
            model.mission.blocked_agents = 1;
            model.system.runners.degraded = 1;
            set_blocker(model, scenario, Severity::Error, "runner pool is degraded");
            add_attention(
                model,
                scenario,
                Severity::Error,
                "Runner pool degraded",
                generated_at,
            );
        }
        FixtureScenario::SourceDown => {
            model.mission.overall = HealthLevel::Critical;
            model.mission.safe_to_code = false;
            model.freshness.overall_stale = true;
            model.system.gitlab = ComponentHealth {
                name: "gitlab".into(),
                status: HealthLevel::Critical,
                latency_ms: None,
                detail: Some("source down".into()),
            };
            sources.push(SourceFreshness::source_down(
                SourceKind::GitLab,
                "connection refused",
            ));
            set_blocker(model, scenario, Severity::Critical, "GitLab source is down");
            add_attention(
                model,
                scenario,
                Severity::Critical,
                "GitLab source down",
                generated_at,
            );
        }
        FixtureScenario::Security => {
            model.mission.overall = HealthLevel::Critical;
            model.mission.safe_to_release = false;
            set_blocker(
                model,
                scenario,
                Severity::Critical,
                "security finding blocks release",
            );
            add_attention(
                model,
                scenario,
                Severity::Critical,
                "Secret exposure requires review",
                generated_at,
            );
        }
        FixtureScenario::Release => {
            model.mission.safe_to_release = true;
            model.next_action = Some(next_action(
                "request_merge",
                "Review release gate",
                "release fixture has a canary candidate",
                RiskTier::Production,
                generated_at,
            ));
            add_attention(
                model,
                scenario,
                Severity::Warning,
                "Release gate waiting on proof",
                generated_at,
            );
        }
        FixtureScenario::Cache => {
            model.mission.cache_hit_ratio = 0.42;
            model.mission.active_taints = 2;
            model.mission.taint_count = 2;
            set_blocker(
                model,
                scenario,
                Severity::Warning,
                "cache taint lowers confidence",
            );
            add_attention(
                model,
                scenario,
                Severity::Warning,
                "Cache taint detected",
                generated_at,
            );
        }
        FixtureScenario::Vti => {
            model.mission.selector_misses_24h = 3;
            add_attention(
                model,
                scenario,
                Severity::Info,
                "VTI accelerated a safe test subset",
                generated_at,
            );
        }
        FixtureScenario::Agent => {
            model.mission.active_agents = 5;
            model.mission.active_grants = 2;
            add_attention(
                model,
                scenario,
                Severity::Info,
                "Agent session is waiting for proof",
                generated_at,
            );
        }
        FixtureScenario::Bug => {
            model.next_action = Some(next_action(
                "bug_ready",
                "Open ready bug",
                "bug fixture exposes a ready issue",
                RiskTier::Low,
                generated_at,
            ));
            add_attention(
                model,
                scenario,
                Severity::Warning,
                "Ready bug has failed attempt",
                generated_at,
            );
        }
        FixtureScenario::Jankurai => {
            model.mission.overall = HealthLevel::Warning;
            add_attention(
                model,
                scenario,
                Severity::Warning,
                "Jankurai score has one finding",
                generated_at,
            );
        }
        FixtureScenario::Incident => {
            model.mission.overall = HealthLevel::Critical;
            model.mission.safe_to_code = false;
            model.mission.safe_to_merge = false;
            model.mission.safe_to_release = false;
            set_blocker(
                model,
                scenario,
                Severity::Critical,
                "incident mode is pinned",
            );
            add_attention(
                model,
                scenario,
                Severity::Critical,
                "Incident decision needed",
                generated_at,
            );
        }
    }
}

fn set_blocker(
    model: &mut TuiReadModel,
    scenario: FixtureScenario,
    severity: Severity,
    summary: &str,
) {
    model.mission.top_blocker = Some(BlockerSummary {
        kind: scenario.label().into(),
        severity,
        summary: summary.into(),
        entity: Some(scenario.primary_entity()),
        recommended_action: None,
    });
}

fn add_attention(
    model: &mut TuiReadModel,
    scenario: FixtureScenario,
    severity: Severity,
    title: &str,
    timestamp: DateTime<Utc>,
) {
    model.attention.push(AttentionItem {
        id: format!("attn-{}", scenario.label()),
        severity,
        title: title.into(),
        why_it_matters: format!("{} scenario fixture", scenario.label()),
        entity: scenario.primary_entity(),
        evidence: vec![format!("proof/{}", scenario.label())],
        recommended_actions: Vec::new(),
        created_at: timestamp,
        last_seen_at: timestamp,
    });
}

fn next_action(
    action_id: &str,
    label: &str,
    why: &str,
    risk: RiskTier,
    timestamp: DateTime<Utc>,
) -> NextActionRecommendation {
    NextActionRecommendation {
        action_ref: ActionRef {
            action_id: action_id.into(),
            label: label.into(),
            risk: Some(risk),
        },
        label: label.into(),
        why: format!("{why} @ {}", timestamp.format("%H:%M:%S")),
        entity: None,
        confidence: 0.9,
        safety: ActionSafety::Safe,
        risk,
    }
}

fn event_for(scenario: FixtureScenario, timestamp: DateTime<Utc>, seq: u64) -> TuiEvent {
    let (kind, severity) = match scenario {
        FixtureScenario::Healthy | FixtureScenario::Empty => {
            (TuiEventKind::SnapshotRefreshed, Severity::Info)
        }
        FixtureScenario::Stale | FixtureScenario::Aged => {
            (TuiEventKind::SystemHealthUpdated, Severity::Warning)
        }
        FixtureScenario::SourceDown => (TuiEventKind::RunnerNodeUnreachable, Severity::Warning),
        FixtureScenario::Degraded => (TuiEventKind::FleetUnderfilled, Severity::Error),
        FixtureScenario::Security => (TuiEventKind::PolicyViolation, Severity::Critical),
        FixtureScenario::Release => (TuiEventKind::ReleaseGateUpdated, Severity::Warning),
        FixtureScenario::Cache => (TuiEventKind::CacheTaintCreated, Severity::Warning),
        FixtureScenario::Vti => (TuiEventKind::TestVtiAccelerated, Severity::Info),
        FixtureScenario::Agent => (TuiEventKind::AgentSessionCreated, Severity::Info),
        FixtureScenario::Bug | FixtureScenario::Jankurai => {
            (TuiEventKind::SnapshotRefreshed, Severity::Warning)
        }
        FixtureScenario::Incident => (TuiEventKind::HungRunnerDetected, Severity::Critical),
    };
    TuiEvent {
        seq,
        timestamp,
        kind,
        severity,
        entity: scenario.primary_entity(),
        parent: None,
        summary: format!("{} fixture event", scenario.label()),
        correlation_id: Some(format!("fixture-{}", scenario.label())),
        evidence_refs: vec![format!("proof/{}", scenario.label())],
        next_actions: Vec::new(),
        stale_after_ms: 60_000,
    }
}

fn action_stream(scenario: FixtureScenario, timestamp: DateTime<Utc>) -> ActionStreamPage {
    ActionStreamPage::single(ActionStreamEvent {
        seq: 1,
        action_id: "next_action".into(),
        phase: ActionStreamPhase::Preview,
        status: ActionStatus::Accepted,
        summary: format!("{} fixture action preview", scenario.label()),
        receipt_id: None,
        timestamp,
    })
}

fn source(
    source: SourceKind,
    state: FreshnessState,
    timestamp: DateTime<Utc>,
    age_ms: u64,
    cursor: &str,
) -> SourceFreshness {
    SourceFreshness {
        source,
        state,
        observed_at: Some(timestamp),
        age_ms: Some(age_ms),
        cursor: Some(cursor.into()),
        ttl_ms: Some(60_000),
        confidence: if state == FreshnessState::SourceDown {
            0.0
        } else {
            1.0
        },
        last_error: None,
        degraded_reason: None,
    }
}

fn fixture_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 26, 12, 0, 0)
        .single()
        .expect("fixed fixture timestamp")
}
