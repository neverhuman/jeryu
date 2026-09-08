//! Minimal typed route dispatcher for Phase 10 features.

use jeryu_bench::{ReplayPlan, Scorecard, sample_phase10_harness};
use jeryu_enterprise::{Action, RbacPolicy, Resource, Role, TenantId};
use jeryu_obs::{AuditLog, ReliabilitySoak, phase10_grafana_dashboard};

/// API response used by tests and CLI.
///
/// `headers` carries advisory response headers (e.g. the overlap engine's
/// `X-Jeryu-Reused-PR` signal) for in-process consumers and conformance tests.
/// It defaults to empty; the GitHub edge only needs `status`/`body` for the
/// majority of routes.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Response {
    pub status: u16,
    pub body: String,
    #[allow(clippy::struct_field_names)]
    pub headers: Vec<(String, String)>,
}

/// API state.
#[derive(Clone, Debug)]
pub struct ApiState {
    pub audit_log: AuditLog,
    pub default_policy: RbacPolicy,
}

impl Default for ApiState {
    fn default() -> Self {
        Self {
            audit_log: AuditLog::new(),
            default_policy: RbacPolicy {
                actor: "system".to_owned(),
                tenant: TenantId::new("default"),
                roles: vec![Role::tenant_admin()],
            },
        }
    }
}

/// Route dispatcher.
#[derive(Clone, Debug, Default)]
pub struct Router {
    pub state: ApiState,
}

impl Router {
    /// Handle a GET-style route.
    pub fn get(&mut self, path: &str) -> Response {
        match path {
            "/api/phase10/ready" => self.ready(),
            "/api/phase10/benchmarks/scorecard" => self.scorecard(),
            "/api/phase10/benchmarks/replay" => self.replay(),
            "/api/phase10/slo/dashboard" => self.dashboard(),
            "/api/phase10/reliability/soak" => self.soak(),
            "/api/phase10/rbac/self-test" => self.rbac_self_test(),
            _ => Response {
                status: 404,
                body: "not found".to_owned(),
                headers: Vec::new(),
            },
        }
    }

    fn ready(&mut self) -> Response {
        self.state
            .audit_log
            .append("system", "phase10.ready", "default", "phase10", "allow");
        Response {
            status: 200,
            body: "phase10 ready".to_owned(),
            headers: Vec::new(),
        }
    }

    fn scorecard(&mut self) -> Response {
        let receipts = sample_phase10_harness().provider_neutral_comparison_receipts();
        let scorecard = Scorecard::from_receipts(&receipts);
        let status = if scorecard.passed() { 200 } else { 503 };
        self.state.audit_log.append(
            "system",
            "scorecard.read",
            "default",
            "jeryu_bench",
            "allow",
        );
        Response {
            status,
            body: scorecard.to_markdown(),
            headers: Vec::new(),
        }
    }

    fn replay(&mut self) -> Response {
        let receipts = sample_phase10_harness().provider_neutral_comparison_receipts();
        let mut replay = ReplayPlan::new();
        for receipt in receipts {
            if let Err(error) = replay.add_receipt(receipt) {
                return Response {
                    status: 500,
                    body: format!("invalid receipt: {error:?}"),
                    headers: Vec::new(),
                };
            }
        }
        let verdict = replay.verify();
        let status = if verdict.passed() { 200 } else { 503 };
        self.state.audit_log.append(
            "system",
            "benchmark.replay",
            "default",
            "jeryu_bench",
            if verdict.passed() { "allow" } else { "deny" },
        );
        Response {
            status,
            body: format!(
                "checked={} failures={}",
                verdict.checked,
                verdict.failures.len()
            ),
            headers: Vec::new(),
        }
    }

    fn dashboard(&mut self) -> Response {
        self.state
            .audit_log
            .append("system", "dashboard.read", "default", "slo", "allow");
        Response {
            status: 200,
            body: phase10_grafana_dashboard(),
            headers: Vec::new(),
        }
    }

    fn soak(&mut self) -> Response {
        let soak = ReliabilitySoak::phase10_100_run();
        let passed = soak.passes_phase10_gate();
        self.state.audit_log.append(
            "system",
            "soak.read",
            "default",
            "reliability",
            if passed { "allow" } else { "deny" },
        );
        Response {
            status: if passed { 200 } else { 503 },
            body: format!("runs={} passing={}", soak.runs.len(), soak.passing_runs()),
            headers: Vec::new(),
        }
    }

    fn rbac_self_test(&mut self) -> Response {
        let resource = Resource {
            tenant: TenantId::new("default"),
            kind: "benchmark".to_owned(),
            id: "scorecard".to_owned(),
        };
        let decision = self
            .state
            .default_policy
            .authorize(Action::PublishBenchmark, &resource);
        self.state.audit_log.append(
            "system",
            "rbac.self-test",
            "default",
            "rbac",
            if decision.allowed { "allow" } else { "deny" },
        );
        Response {
            status: if decision.allowed { 200 } else { 403 },
            body: decision.reason,
            headers: Vec::new(),
        }
    }
}
