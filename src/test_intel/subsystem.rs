//! Owner: VTI Test Intelligence subsystem — subsystem ownership graph
//! Proof: `cargo nextest run -p jeryu -- test_intel::subsystem`
//! Invariants: Subsystem mappings stay deterministic and reflect the shared VTI contract.
//! Subsystem rules: maps source file paths to named subsystems and test commands.
//!
//! Each subsystem owns a set of source paths (via simple glob patterns), a nextest
//! filter expression for unit tests, a list of integration test binaries, and
//! a set of paths that force a full test run if changed.
//!
//! Uses a lightweight glob matcher (no external crate) since our patterns are
//! simple: `foo/*`, `foo/**`, `dir/**/*.ext`, and `*.ext`.

use serde::{Deserialize, Serialize};

#[path = "subsystem_glob.rs"]
mod glob;
pub(crate) use glob::{
    affected_subsystems, glob_match, has_global_invalidator, has_subsystem_force_full,
    is_docs_only, matches_any,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A named subsystem with its owned paths and test commands.
#[derive(Debug, Clone)]
pub struct Subsystem {
    pub id: &'static str,
    pub description: &'static str,
    /// Glob patterns for source files owned by this subsystem.
    pub owned_paths: &'static [&'static str],
    /// Nextest filter expression for unit tests.
    pub unit_filter: &'static str,
    /// Integration test binary names (from `tests/` directory).
    pub integration_tests: &'static [&'static str],
    /// If any of these paths change, force full test run.
    pub force_full_paths: &'static [&'static str],
    /// Runner tags required for this subsystem's tests.
    pub runner_tags: &'static [&'static str],
    /// Whether this subsystem is cross-cutting (changes affect many others).
    pub cross_cutting: bool,
}

/// Serializable representation for JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemInfo {
    pub id: String,
    pub description: String,
    pub owned_paths: Vec<String>,
    pub unit_filter: String,
    pub integration_tests: Vec<String>,
    pub cross_cutting: bool,
}

impl From<&Subsystem> for SubsystemInfo {
    fn from(s: &Subsystem) -> Self {
        Self {
            id: s.id.to_string(),
            description: s.description.to_string(),
            owned_paths: s.owned_paths.iter().map(|p| p.to_string()).collect(),
            unit_filter: s.unit_filter.to_string(),
            integration_tests: s.integration_tests.iter().map(|p| p.to_string()).collect(),
            cross_cutting: s.cross_cutting,
        }
    }
}

/// Paths that always trigger a full test run regardless of subsystem.
pub const GLOBAL_INVALIDATORS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rust-toolchain",
    ".cargo/*",
    ".gitlab-ci.yml",
    ".github/workflows/*",
    "build.rs",
    "src/admission.rs",
    "src/policy.rs",
];

/// File patterns that indicate a docs-only change.
pub const DOCS_PATTERNS: &[&str] = &["*.md", "docs/*", "LICENSE", ".gitignore", ".editorconfig"];

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// The complete set of subsystem rules for the JeRyu jeryu codebase.
pub const SUBSYSTEMS: &[Subsystem] = &[
    Subsystem {
        id: "pool",
        description: "Runner pool management and Docker container lifecycle",
        owned_paths: &["src/pool.rs", "src/docker.rs"],
        unit_filter: "test(/pool|docker|runner/)",
        integration_tests: &["pool_tests", "job_tests"],
        force_full_paths: &[],
        runner_tags: &["build", "docker-build"],
        cross_cutting: false,
    },
    Subsystem {
        id: "cache",
        description: "SmartCache, gateway, taint, epoch, and witness subsystems",
        owned_paths: &[
            "src/cache.rs",
            "src/cache_brain.rs",
            "src/cache_proxy.rs",
            "src/gateway/**",
            "src/epoch.rs",
            "src/taint.rs",
            "src/witness.rs",
            "src/sccache_mgr.rs",
        ],
        unit_filter: "test(/cache|singleflight|gateway|taint|epoch|witness|sccache/)",
        integration_tests: &["cache_integration_test"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "agent",
        description: "Autonomous agent flow and capability RPC",
        owned_paths: &["src/agent.rs", "src/capability.rs"],
        unit_filter: "test(/agent|capability|risk_gate/)",
        integration_tests: &["agent_tests"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "engine",
        description: "Webhook receiver, reconciliation, push/pipeline/job handling",
        owned_paths: &["src/engine.rs"],
        unit_filter: "test(/webhook|pipeline|supersedence|reconcil/)",
        integration_tests: &["job_tests"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "release",
        description: "Release promotion, canary, and secrets management",
        owned_paths: &["src/release.rs", "src/secrets.rs"],
        unit_filter: "test(/release|canary|secret|vault|promote/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "decision",
        description: "Failure classification, recovery logic, risk gates, trust tiers",
        owned_paths: &["src/decision.rs", "src/capsule.rs"],
        unit_filter: "test(/decision|risk_gate|recover|classif|capsule/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "tui",
        description: "Terminal user interface",
        owned_paths: &["src/tui/**"],
        unit_filter: "test(/tui|snapshot|render|widget/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "state",
        description: "Postgres-primary state database, SQLite recovery, migrations, CRUD operations",
        owned_paths: &["src/state.rs"],
        unit_filter: "test(/state|sqlite|db|migrat/)",
        integration_tests: &[
            "pool_tests",
            "job_tests",
            "agent_tests",
            "cache_integration_test",
        ],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: true,
    },
    Subsystem {
        id: "config",
        description: "Configuration, templates, bootstrap",
        owned_paths: &["src/config.rs", "src/bootstrap.rs"],
        unit_filter: "test(/config|template|bootstrap/)",
        integration_tests: &["pool_tests"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "impact",
        description: "Impact analysis and test runner",
        owned_paths: &["src/impact.rs", "src/test_runner.rs", "src/test_intel/**"],
        unit_filter: "test(/impact|test_run|test_intel|plan_from/)",
        integration_tests: &[],
        // Changes to the selector itself should trigger full testing
        // until we have nightly audit confirming correctness.
        force_full_paths: &["src/test_intel/**"],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "exec",
        description: "Custom executor, sandbox, honeypot",
        owned_paths: &["src/exec.rs", "src/sandbox.rs", "src/honeypot.rs"],
        unit_filter: "test(/exec|sandbox|honeypot|custom_exec/)",
        integration_tests: &["e2e"],
        force_full_paths: &[],
        runner_tags: &["build", "docker-build"],
        cross_cutting: false,
    },
    Subsystem {
        id: "gitlab_client",
        description: "GitLab REST API client",
        owned_paths: &["src/gitlab_client.rs"],
        unit_filter: "test(/gitlab|client|api|endpoint/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "observability",
        description: "Telemetry and logging observability",
        owned_paths: &["src/telemetry.rs", "src/logs.rs"],
        unit_filter: "test(/telemetry|log/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "explain_mod",
        description: "Pipeline explain and buildkit",
        owned_paths: &["src/explain.rs", "src/buildkit.rs"],
        unit_filter: "test(/explain|buildkit/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "reclaim",
        description: "Disk reclaim and garbage collection",
        owned_paths: &["src/reclaim.rs"],
        unit_filter: "test(/reclaim|gc|garbage/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "subsystem_tests.rs"]
mod tests;
