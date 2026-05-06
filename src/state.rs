//! Owner: State Store (Postgres primary, SQLite recovery)
//! Proof: `cargo test -p jeryu -- state`
//! Invariants: append-only event log; manager state machine (starting→online→draining→stopped)
//!
//! Owns the schema, migrations, and all CRUD operations for pools,
//! managers, and job events. This is the single source of truth for
//! fleet state that survives restarts.

use anyhow::{Context, Result};
use sqlx::any::AnyQueryResult;
use sqlx::any::{AnyConnectOptions, AnyPoolOptions, AnyRow, install_default_drivers};
use sqlx::{AnyPool, FromRow, Row};
use std::borrow::Cow;
use std::str::FromStr;

use crate::capsule::FailureCapsule;
use crate::config;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Pool {
    pub name: String,
    pub gitlab_runner_id: i64,
    pub auth_token: String,
    pub tags: String,
    pub executor: String,
    pub min_warm: i64,
    pub max_managers: i64,
    pub concurrent: i64,
    pub request_concurrency: i64,
    pub paused: bool,
    pub trust_tier: String,
}

impl<'r> FromRow<'r, AnyRow> for Pool {
    fn from_row(row: &'r AnyRow) -> std::result::Result<Self, sqlx::Error> {
        Ok(Self {
            name: row.try_get("name")?,
            gitlab_runner_id: row.try_get("gitlab_runner_id")?,
            auth_token: row.try_get("auth_token")?,
            tags: row.try_get("tags")?,
            executor: row.try_get("executor")?,
            min_warm: row.try_get("min_warm")?,
            max_managers: row.try_get("max_managers")?,
            concurrent: row.try_get("concurrent")?,
            request_concurrency: row.try_get("request_concurrency")?,
            paused: any_bool(row, "paused")?,
            trust_tier: row.try_get("trust_tier")?,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct Manager {
    pub id: String,
    pub pool_name: String,
    pub docker_container_id: String,
    pub system_id: Option<String>,
    pub state: String, // starting, online, draining, stopped, failed
    pub config_dir: String,
    pub started_at: Option<String>,
    pub last_contact_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct JobEvent {
    pub job_id: i64,
    pub project_id: i64,
    pub pipeline_id: Option<i64>,
    pub status: String,
    pub job_name: Option<String>,
    pub pool_name: Option<String>,
    pub system_id: Option<String>,
    pub queued_duration: Option<f64>,
    pub received_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct CiJobRun {
    pub job_id: i64,
    pub project_id: i64,
    pub pipeline_id: i64,
    pub root_pipeline_id: i64,
    pub pipeline_sha: String,
    pub ref_name: String,
    pub job_name: String,
    pub stage: String,
    pub status: String,
    pub runner: Option<String>,
    pub runner_pool: Option<String>,
    pub queued_duration_secs: Option<f64>,
    pub duration_secs: Option<f64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub web_url: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct CiJobBottleneck {
    pub job_name: String,
    pub stage: String,
    pub runner_pool: Option<String>,
    pub avg_duration_secs: f64,
    pub latest_duration_secs: Option<f64>,
    pub max_duration_secs: Option<f64>,
    pub runs: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct EventLog {
    pub id: i64,
    pub event_type: String,
    pub timestamp: String,
    pub project_id: Option<i64>,
    pub job_id: Option<i64>,
    pub actor: String,
    pub payload: String, // JSON payload string
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct GitCommandEventRecord {
    pub id: i64,
    pub request_id: String,
    pub actor: String,
    pub cwd: String,
    pub repo_root: Option<String>,
    pub argv_redacted: String,
    pub argv_hash: String,
    pub command_class: String,
    pub risk: String,
    pub mode: String,
    pub before_head: Option<String>,
    pub before_branch: Option<String>,
    pub before_dirty: Option<i64>,
    pub after_head: Option<String>,
    pub after_branch: Option<String>,
    pub after_dirty: Option<i64>,
    pub exit_code: i32,
    pub sidecar_status: String,
    pub mirror_status: String,
    pub created_at: String,
    pub payload: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct GitRefUpdate {
    pub id: i64,
    pub request_id: String,
    pub ref_name: String,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct GitMirrorJob {
    pub id: i64,
    pub request_id: String,
    pub remote_name: String,
    pub branch_name: Option<String>,
    pub status: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct GitRiskApproval {
    pub id: i64,
    pub request_id: String,
    pub actor: String,
    pub command_class: String,
    pub risk: String,
    pub approved: i64,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct GitCommandArtifact {
    pub id: i64,
    pub request_id: String,
    pub artifact_kind: String,
    pub artifact_path: String,
    pub digest: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct TrackedPipeline {
    pub pipeline_id: i64,
    pub project_id: i64,
    pub ref_name: String,
    pub sha: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ReleaseAttempt {
    pub id: i64,
    pub project_id: i64,
    pub ref_name: String,
    pub sha: String,
    pub version: String,
    pub upstream_pipeline_id: Option<i64>,
    pub upstream_status: String,
    pub release_pipeline_id: Option<i64>,
    pub release_pipeline_status: Option<String>,
    pub production_pipeline_id: Option<i64>,
    pub production_pipeline_status: Option<String>,
    pub canary_status: String,
    pub canary_started_at: Option<String>,
    pub canary_finished_at: Option<String>,
    pub canary_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct EvidenceRecord {
    pub id: i64,
    pub event_type: String,
    pub project_id: i64,
    pub job_id: i64,
    pub pipeline_id: Option<i64>,
    pub commit_sha: String,
    pub ref_name: String,
    pub stage: String,
    pub exit_code: i32,
    pub failure_kind: String,
    pub classification: String,
    pub created_at: String,
    pub payload: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct RetryRecord {
    pub id: i64,
    pub project_id: i64,
    pub job_id: i64,
    pub commit_sha: String,
    pub ref_name: String,
    pub decision: String,
    pub reason: String,
    pub created_at: String,
}

fn any_bool(row: &AnyRow, column: &str) -> std::result::Result<bool, sqlx::Error> {
    if let Ok(value) = row.try_get::<bool, _>(column) {
        Ok(value)
    } else if let Ok(value) = row.try_get::<i32, _>(column) {
        Ok(value != 0)
    } else {
        row.try_get::<i64, _>(column).map(|value| value != 0)
    }
}

const POOL_SELECT: &str = r#"SELECT
    name,
    gitlab_runner_id,
    auth_token,
    tags,
    executor,
    min_warm,
    max_managers,
    concurrent,
    request_concurrency,
    CAST(CASE WHEN paused THEN 1 ELSE 0 END AS BIGINT) AS paused,
    trust_tier
FROM pools"#;

fn postgres_schema(sqlite_schema: &str) -> String {
    sqlite_schema
        .replace("INTEGER PRIMARY KEY AUTOINCREMENT", "BIGSERIAL PRIMARY KEY")
        .replace("INTEGER PRIMARY KEY", "BIGINT PRIMARY KEY")
        .replace(" INTEGER", " BIGINT")
        .replace(" REAL", " DOUBLE PRECISION")
        .replace(
            "paused              BIGINT NOT NULL DEFAULT 0",
            "paused              BOOLEAN NOT NULL DEFAULT FALSE",
        )
        .replace(
            "enabled BIGINT NOT NULL DEFAULT 0",
            "enabled BOOLEAN NOT NULL DEFAULT FALSE",
        )
        .replace("hit BIGINT NOT NULL", "hit BOOLEAN NOT NULL")
        .replace(
            "repaired        BIGINT NOT NULL DEFAULT 0",
            "repaired        BOOLEAN NOT NULL DEFAULT FALSE",
        )
}

#[derive(Debug, Clone, FromRow)]
pub struct SecretAuthority {
    pub name: String,
    pub kind: String,
    pub address: String,
    pub status: String,
    pub mount: String,
    pub prefix: String,
    pub token_fingerprint: String,
    pub metadata_path: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ReleaseSecretSet {
    pub repo_name: String,
    pub version: String,
    pub target: String,
    pub authority_name: String,
    pub status: String,
    pub rendered_deploy_env_path: String,
    pub rendered_runtime_env_path: String,
    pub audit_path: String,
    pub bundle_path: Option<String>,
    pub report_path: Option<String>,
    pub runtime_secret_vault_path: Option<String>,
    pub recovery_password_vault_path: Option<String>,
    pub expires_at: Option<String>,
    pub rotated_at: String,
    pub finalized_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct SecretAuditEvent {
    pub id: Option<i64>,
    pub repo_name: String,
    pub version: String,
    pub target: String,
    pub action: String,
    pub status: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
/// Persisted capability intent requested by an agent-facing action.
pub struct CapabilityIntentRecord {
    /// SQLite row id.
    pub id: i64,
    /// Stable request identifier for idempotency and audit correlation.
    pub request_id: String,
    /// High-level intent name, such as `propose_patch`.
    pub intent_type: String,
    /// Registry action id that authorized the intent.
    pub action_id: String,
    /// GitLab project id when the intent targets a project.
    pub project_id: Option<i64>,
    /// Primary ref affected by the intent.
    pub ref_name: Option<String>,
    /// Target branch or ref used as the merge/test base.
    pub target_ref: Option<String>,
    /// Actor label recorded by the capability service.
    pub actor: String,
    /// Intent lifecycle status.
    pub status: String,
    /// JSON payload with intent-specific parameters and evidence.
    pub payload: String,
    /// UTC creation timestamp.
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
/// Persisted grant that admission can match against an attempted ref update.
pub struct CapabilityGrantRecord {
    /// SQLite row id.
    pub id: i64,
    /// Owning capability intent row id.
    pub intent_id: i64,
    /// Stable grant identifier returned to callers and admission logs.
    pub grant_id: String,
    /// Registry action id that issued the grant.
    pub action_id: String,
    /// GitLab project id when scoped to a project.
    pub project_id: Option<i64>,
    /// Fully qualified ref, such as `refs/heads/agent/task`.
    pub ref_name: String,
    /// Optional exact post-update SHA binding.
    pub new_sha: Option<String>,
    /// Capability name required to use this grant.
    pub required_grant: String,
    /// Grant lifecycle status.
    pub status: String,
    /// UTC issue timestamp.
    pub issued_at: String,
    /// UTC expiration timestamp.
    pub expires_at: String,
    /// JSON payload with grant scope and provenance.
    pub payload: String,
}

#[derive(Debug, Clone)]
/// Input row for recording a new capability intent.
pub struct NewCapabilityIntent<'a> {
    /// Stable request identifier for idempotency and audit correlation.
    pub request_id: &'a str,
    /// High-level intent name, such as `propose_patch`.
    pub intent_type: &'a str,
    /// Registry action id that authorized the intent.
    pub action_id: &'a str,
    /// GitLab project id when the intent targets a project.
    pub project_id: Option<i64>,
    /// Primary ref affected by the intent.
    pub ref_name: Option<&'a str>,
    /// Target branch or ref used as the merge/test base.
    pub target_ref: Option<&'a str>,
    /// Actor label recorded by the capability service.
    pub actor: &'a str,
    /// Intent lifecycle status.
    pub status: &'a str,
    /// JSON payload with intent-specific parameters and evidence.
    pub payload: &'a str,
}

#[derive(Debug, Clone)]
/// Input row for approving a capability grant.
pub struct NewCapabilityGrant<'a> {
    /// Owning capability intent row id.
    pub intent_id: i64,
    /// Stable grant identifier returned to callers and admission logs.
    pub grant_id: &'a str,
    /// Registry action id that issued the grant.
    pub action_id: &'a str,
    /// GitLab project id when scoped to a project.
    pub project_id: Option<i64>,
    /// Fully qualified ref, such as `refs/heads/agent/task`.
    pub ref_name: &'a str,
    /// Optional exact post-update SHA binding.
    pub new_sha: Option<&'a str>,
    /// Capability name required to use this grant.
    pub required_grant: &'a str,
    /// Grant lifecycle status.
    pub status: &'a str,
    /// UTC expiration timestamp.
    pub expires_at: &'a str,
    /// JSON payload with grant scope and provenance.
    pub payload: &'a str,
}

#[derive(Debug, Clone)]
/// Input row for recording a pre-receive admission decision.
pub struct NewAdmissionDecision<'a> {
    /// Raw pre-receive line received from Git.
    pub raw_input: &'a str,
    /// Final admission verdict label.
    pub verdict: &'a str,
    /// Actor class inferred from the ref update.
    pub actor_kind: &'a str,
    /// Ref being updated, when the input line is parseable.
    pub ref_name: Option<&'a str>,
    /// Previous SHA from the pre-receive line.
    pub old_sha: Option<&'a str>,
    /// New SHA from the pre-receive line.
    pub new_sha: Option<&'a str>,
    /// Matched capability grant id, if any.
    pub grant_id: Option<&'a str>,
    /// Policy version that produced the decision.
    pub policy_version: &'a str,
    /// JSON array of human-readable reasons.
    pub reasons_json: &'a str,
    /// JSON payload with normalized evaluation context.
    pub payload: &'a str,
}

// ---------------------------------------------------------------------------
// Test TUI Tracking Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, FromRow)]
pub struct TestExecution {
    pub id: i64,
    pub test_name: String,
    pub version: String,
    pub duration_ms: i64,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct TestBottleneck {
    pub test_name: String,
    pub avg_duration_ms: f64,
    pub latest_duration_ms: i64,
    pub count: i64,
}

// ---------------------------------------------------------------------------
// SmartCache Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, FromRow)]
pub struct CacheObject {
    pub key: String,
    pub digest: String,
    pub size_bytes: i64,
    pub category: String,   // 'apt', 'cargo', 'npm', 'docker', 'git'
    pub mutability: String, // 'immutable', 'mutable'
    pub created_at: String,
    pub last_accessed_at: String,
    pub hits: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct CacheRequest {
    pub id: i64,
    pub url: String,
    pub method: String,
    pub hit: bool,
    pub reason_code: String,
    pub bytes_served: i64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    pub bytes_served: i64,
    pub total_requests: i64,
    pub hit_count: i64,
    pub miss_count: i64,
    pub object_count: i64,
    pub hit_ratio: f64,
    pub singleflight_coalesced: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct HotCacheEntry {
    pub key: String,
    pub size_bytes: i64,
    pub last_accessed_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct BuildSignature {
    pub hash: String,
    pub project_id: i64,
    pub job_id: i64,
    pub components: String, // JSON
    pub target_artifact_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ImageSignature {
    pub hash: String,
    pub digest: String,
    pub dockerfile_hash: String,
    pub context_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ForceRefreshRule {
    pub id: i64,
    pub pattern: String,
    pub category: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum StateAction {
    RecordCacheRequest {
        url: String,
        method: String,
        hit: bool,
        reason_code: String,
        bytes_served: i64,
        timestamp: String,
    },
}

#[derive(Clone)]
pub struct Db {
    pool: AnyPool,
    backend: StateBackend,
    telemetry_tx: Option<tokio::sync::mpsc::Sender<StateAction>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateBackend {
    /// Local embedded SQLite database.
    Sqlite,
    /// Production Postgres database.
    Postgres,
}

impl StateBackend {
    fn from_url(database_url: &str) -> Result<Self> {
        if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
            Ok(Self::Postgres)
        } else if database_url.starts_with("sqlite:") {
            Ok(Self::Sqlite)
        } else {
            anyhow::bail!(
                "unsupported JERYU_DATABASE_URL scheme; expected postgres://, postgresql://, or sqlite:"
            )
        }
    }
}

pub(crate) fn postgres_bind_params(sql: &str) -> String {
    let mut converted = String::with_capacity(sql.len() + 16);
    let mut next = 1;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double_quote => {
                converted.push(ch);
                if in_single_quote && chars.peek() == Some(&'\'') {
                    converted.push(chars.next().expect("peeked escaped quote"));
                } else {
                    in_single_quote = !in_single_quote;
                }
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                converted.push(ch);
            }
            '?' if !in_single_quote && !in_double_quote => {
                converted.push('$');
                converted.push_str(&next.to_string());
                next += 1;
            }
            _ => converted.push(ch),
        }
    }

    converted
}

pub(crate) fn backend_sql(backend: StateBackend, sql: &'static str) -> Cow<'static, str> {
    match backend {
        StateBackend::Sqlite => Cow::Borrowed(sql),
        StateBackend::Postgres => Cow::Owned(postgres_bind_params(sql)),
    }
}

pub(crate) fn backend_sql_owned(backend: StateBackend, sql: String) -> String {
    match backend {
        StateBackend::Sqlite => sql,
        StateBackend::Postgres => postgres_bind_params(&sql),
    }
}

static DB_INSTANCE: tokio::sync::OnceCell<Db> = tokio::sync::OnceCell::const_new();

impl Db {
    /// Expose the backend-neutral SQL pool for manager modules.
    pub fn pool(&self) -> AnyPool {
        self.pool.clone()
    }

    /// Return the active state backend.
    pub fn backend(&self) -> StateBackend {
        self.backend
    }

    fn sql(&self, sql: &'static str) -> Cow<'static, str> {
        backend_sql(self.backend, sql)
    }

    fn sql_owned(&self, sql: String) -> String {
        backend_sql_owned(self.backend, sql)
    }

    /// Open the configured database and run migrations. Uses a global connection pool.
    pub async fn open() -> Result<Self> {
        let db = DB_INSTANCE
            .get_or_try_init(|| async {
                install_default_drivers();
                dotenvy::from_path(config::env_file()).ok();
                if let Some(database_url) = config::database_url() {
                    return Self::open_url(&database_url).await;
                }

                let db_path = config::db_path();
                if let Some(parent) = db_path.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creating db directory: {}", parent.display()))?;
                }

                let database_url = format!("sqlite:{}?mode=rwc", db_path.display());
                Self::open_url(&database_url).await
            })
            .await?;
        Ok(db.clone())
    }

    /// Open a database URL directly.
    pub async fn open_url(database_url: &str) -> Result<Self> {
        install_default_drivers();
        let backend = StateBackend::from_url(database_url)?;
        let pool = AnyPoolOptions::new()
            .max_connections(match backend {
                StateBackend::Sqlite => 4,
                StateBackend::Postgres => 16,
            })
            .connect_with(AnyConnectOptions::from_str(database_url)?)
            .await
            .with_context(|| format!("opening database: {}", database_url))?;

        let mut db = Self {
            pool: pool.clone(),
            backend,
            telemetry_tx: None,
        };
        db.migrate().await?;

        // Start Actor
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StateAction>(10000);
        let actor_pool = pool.clone();
        tokio::spawn(async move {
            let mut batch = Vec::new();
            while let Some(msg) = rx.recv().await {
                batch.push(msg);
                // Batch up to 100 messages or if channel is momentarily empty
                if batch.len() >= 100 || rx.is_empty() {
                    if let Ok(mut tx) = actor_pool.begin().await {
                        for action in batch.drain(..) {
                            match action {
                                StateAction::RecordCacheRequest {
                                    url,
                                    method,
                                    hit,
                                    reason_code,
                                    bytes_served,
                                    timestamp,
                                } => {
                                    let _ = sqlx::query(
                                        "INSERT INTO cache_requests (url, method, hit, reason_code, bytes_served, timestamp) VALUES (?, ?, ?, ?, ?, ?)"
                                    )
                                    .bind(url)
                                    .bind(method)
                                    .bind(hit)
                                    .bind(reason_code)
                                    .bind(bytes_served)
                                    .bind(timestamp)
                                    .execute(&mut *tx)
                                    .await;
                                }
                            }
                        }
                        let _ = tx.commit().await;
                    } else {
                        batch.clear(); // Drop to prevent ballooning on critical DB failure
                    }
                }
            }
        });

        db.telemetry_tx = Some(tx);
        Ok(db)
    }

    /// Open an in-memory database for testing. Uses the same migration as production
    /// to validate schema consistency.
    pub async fn open_memory() -> Result<Self> {
        install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        let db = Self {
            pool,
            backend: StateBackend::Sqlite,
            telemetry_tx: None,
        };
        db.migrate().await?;
        Ok(db)
    }

    /// Open the optional Postgres integration database from `JERYU_TEST_POSTGRES_URL`.
    pub async fn open_test_postgres() -> Result<Option<Self>> {
        match std::env::var("JERYU_TEST_POSTGRES_URL") {
            Ok(url) if !url.trim().is_empty() => Self::open_url(&url).await.map(Some),
            _ => Ok(None),
        }
    }

    async fn inserted_id(&self, result: AnyQueryResult) -> Result<i64> {
        match self.backend {
            StateBackend::Sqlite => {
                let row: (i64,) = sqlx::query_as("SELECT last_insert_rowid()")
                    .fetch_one(&self.pool)
                    .await?;
                Ok(row.0)
            }
            StateBackend::Postgres => result
                .last_insert_id()
                .ok_or_else(|| anyhow::anyhow!("Postgres insert did not return an id")),
        }
    }

    async fn migrate(&self) -> Result<()> {
        let sqlite_schema = r#"
            CREATE TABLE IF NOT EXISTS pools (
                name                TEXT PRIMARY KEY,
                gitlab_runner_id    INTEGER NOT NULL,
                auth_token          TEXT NOT NULL,
                tags                TEXT NOT NULL DEFAULT '',
                executor            TEXT NOT NULL DEFAULT 'docker',
                min_warm            INTEGER NOT NULL DEFAULT 1,
                max_managers        INTEGER NOT NULL DEFAULT 4,
                concurrent          INTEGER NOT NULL DEFAULT 8,
                request_concurrency INTEGER NOT NULL DEFAULT 4,
                paused              INTEGER NOT NULL DEFAULT 0,
                trust_tier          TEXT NOT NULL DEFAULT 'trusted'
            );

            CREATE TABLE IF NOT EXISTS managers (
                id                  TEXT PRIMARY KEY,
                pool_name           TEXT NOT NULL REFERENCES pools(name),
                docker_container_id TEXT NOT NULL UNIQUE,
                system_id           TEXT,
                state               TEXT NOT NULL DEFAULT 'starting',
                config_dir          TEXT NOT NULL,
                started_at          TEXT,
                last_contact_at     TEXT
            );

            CREATE TABLE IF NOT EXISTS job_events (
                job_id          INTEGER NOT NULL,
                project_id      INTEGER NOT NULL,
                pipeline_id     INTEGER,
                status          TEXT NOT NULL,
                job_name        TEXT,
                pool_name       TEXT,
                system_id       TEXT,
                queued_duration REAL,
                received_at     TEXT NOT NULL,
                PRIMARY KEY (job_id, status)
            );

            CREATE TABLE IF NOT EXISTS ci_job_runs (
                job_id                 INTEGER PRIMARY KEY,
                project_id             INTEGER NOT NULL,
                pipeline_id            INTEGER NOT NULL,
                root_pipeline_id       INTEGER NOT NULL,
                pipeline_sha           TEXT NOT NULL,
                ref_name               TEXT NOT NULL,
                job_name               TEXT NOT NULL,
                stage                  TEXT NOT NULL,
                status                 TEXT NOT NULL,
                runner                 TEXT,
                runner_pool            TEXT,
                queued_duration_secs   REAL,
                duration_secs          REAL,
                started_at             TEXT,
                finished_at            TEXT,
                web_url                TEXT,
                observed_at            TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_ci_job_runs_pipeline
                ON ci_job_runs(project_id, pipeline_id);
            CREATE INDEX IF NOT EXISTS idx_ci_job_runs_bottlenecks
                ON ci_job_runs(project_id, ref_name, job_name, observed_at);

            CREATE TABLE IF NOT EXISTS events (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type      TEXT NOT NULL,
                timestamp       TEXT NOT NULL,
                project_id      INTEGER,
                job_id          INTEGER,
                actor           TEXT NOT NULL,
                payload         TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS capability_intents (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id      TEXT NOT NULL UNIQUE,
                intent_type     TEXT NOT NULL,
                action_id       TEXT NOT NULL,
                project_id      INTEGER,
                ref_name        TEXT,
                target_ref      TEXT,
                actor           TEXT NOT NULL,
                status          TEXT NOT NULL,
                payload         TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS capability_grants (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                intent_id       INTEGER NOT NULL REFERENCES capability_intents(id),
                grant_id        TEXT NOT NULL UNIQUE,
                action_id       TEXT NOT NULL,
                project_id      INTEGER,
                ref_name        TEXT NOT NULL,
                new_sha         TEXT,
                required_grant  TEXT NOT NULL,
                status          TEXT NOT NULL,
                issued_at       TEXT NOT NULL,
                expires_at      TEXT NOT NULL,
                payload         TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_capability_grants_ref
                ON capability_grants(ref_name, status, expires_at);

            CREATE TABLE IF NOT EXISTS admission_decisions (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                raw_input       TEXT NOT NULL,
                verdict         TEXT NOT NULL,
                actor_kind      TEXT NOT NULL,
                ref_name        TEXT,
                old_sha         TEXT,
                new_sha         TEXT,
                grant_id        TEXT,
                policy_version  TEXT NOT NULL,
                reasons_json    TEXT NOT NULL,
                payload         TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_admission_decisions_ref
                ON admission_decisions(ref_name, created_at DESC);

            CREATE TABLE IF NOT EXISTS tracked_pipelines (
                pipeline_id      INTEGER PRIMARY KEY,
                project_id       INTEGER NOT NULL,
                ref_name         TEXT NOT NULL,
                sha              TEXT NOT NULL,
                status           TEXT NOT NULL,
                updated_at       TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tracked_pipelines_ref
            ON tracked_pipelines (project_id, ref_name, status);

            CREATE TABLE IF NOT EXISTS release_attempts (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id          INTEGER NOT NULL,
                ref_name            TEXT NOT NULL,
                sha                 TEXT NOT NULL,
                version             TEXT NOT NULL,
                upstream_pipeline_id INTEGER,
                upstream_status     TEXT NOT NULL,
                release_pipeline_id INTEGER,
                release_pipeline_status TEXT,
                production_pipeline_id INTEGER,
                production_pipeline_status TEXT,
                canary_status       TEXT NOT NULL,
                canary_started_at   TEXT,
                canary_finished_at  TEXT,
                canary_note         TEXT,
                created_at          TEXT NOT NULL,
                updated_at          TEXT NOT NULL,
                UNIQUE(project_id, ref_name, sha)
            );

            CREATE TABLE IF NOT EXISTS evidence_capsules (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type       TEXT NOT NULL,
                project_id       INTEGER NOT NULL,
                job_id           INTEGER NOT NULL,
                pipeline_id      INTEGER,
                commit_sha       TEXT NOT NULL,
                ref_name         TEXT NOT NULL,
                stage            TEXT NOT NULL,
                exit_code        INTEGER NOT NULL,
                failure_kind     TEXT NOT NULL,
                classification   TEXT NOT NULL,
                created_at       TEXT NOT NULL,
                payload          TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_evidence_job
            ON evidence_capsules (project_id, job_id, created_at DESC);

            CREATE INDEX IF NOT EXISTS idx_evidence_ref
            ON evidence_capsules (project_id, ref_name, commit_sha, created_at DESC);

            CREATE TABLE IF NOT EXISTS retry_decisions (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id       INTEGER NOT NULL,
                job_id           INTEGER NOT NULL,
                commit_sha       TEXT NOT NULL,
                ref_name         TEXT NOT NULL,
                decision         TEXT NOT NULL,
                reason           TEXT NOT NULL,
                created_at       TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS git_command_events (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id      TEXT NOT NULL,
                actor           TEXT NOT NULL,
                cwd             TEXT NOT NULL,
                repo_root       TEXT,
                argv_redacted   TEXT NOT NULL,
                argv_hash       TEXT NOT NULL,
                command_class   TEXT NOT NULL,
                risk            TEXT NOT NULL,
                mode            TEXT NOT NULL,
                before_head     TEXT,
                before_branch   TEXT,
                before_dirty    INTEGER,
                after_head      TEXT,
                after_branch    TEXT,
                after_dirty     INTEGER,
                exit_code       INTEGER NOT NULL,
                sidecar_status  TEXT NOT NULL,
                mirror_status   TEXT NOT NULL,
                created_at      TEXT NOT NULL,
                payload         TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS git_ref_updates (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id      TEXT NOT NULL,
                ref_name        TEXT NOT NULL,
                before_sha      TEXT,
                after_sha       TEXT,
                status          TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS git_mirror_jobs (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id      TEXT NOT NULL,
                remote_name     TEXT NOT NULL,
                branch_name     TEXT,
                status          TEXT NOT NULL,
                detail          TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS git_risk_approvals (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id      TEXT NOT NULL,
                actor           TEXT NOT NULL,
                command_class   TEXT NOT NULL,
                risk            TEXT NOT NULL,
                approved        INTEGER NOT NULL DEFAULT 0,
                reason          TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS git_command_artifacts (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id      TEXT NOT NULL,
                artifact_kind   TEXT NOT NULL,
                artifact_path   TEXT NOT NULL,
                digest          TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS secret_authorities (
                name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                address TEXT NOT NULL,
                status TEXT NOT NULL,
                mount TEXT NOT NULL,
                prefix TEXT NOT NULL,
                token_fingerprint TEXT NOT NULL,
                metadata_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS release_secret_sets (
                repo_name TEXT NOT NULL,
                version TEXT NOT NULL,
                target TEXT NOT NULL,
                authority_name TEXT NOT NULL,
                status TEXT NOT NULL,
                rendered_deploy_env_path TEXT NOT NULL,
                rendered_runtime_env_path TEXT NOT NULL,
                audit_path TEXT NOT NULL,
                bundle_path TEXT,
                report_path TEXT,
                runtime_secret_vault_path TEXT,
                recovery_password_vault_path TEXT,
                expires_at TEXT,
                rotated_at TEXT NOT NULL,
                finalized_at TEXT,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (repo_name, version, target)
            );

            CREATE TABLE IF NOT EXISTS secret_audit_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repo_name TEXT NOT NULL,
                version TEXT NOT NULL,
                target TEXT NOT NULL,
                action TEXT NOT NULL,
                status TEXT NOT NULL,
                detail TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cache_objects (
                key TEXT PRIMARY KEY,
                digest TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                category TEXT NOT NULL,
                mutability TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_accessed_at TEXT NOT NULL,
                hits INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS cache_requests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                method TEXT NOT NULL,
                hit INTEGER NOT NULL,
                reason_code TEXT NOT NULL DEFAULT 'unknown',
                bytes_served INTEGER NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS hot_cache_entries (
                key TEXT PRIMARY KEY REFERENCES cache_objects(key) ON DELETE CASCADE,
                size_bytes INTEGER NOT NULL,
                last_accessed_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS build_signatures (
                hash TEXT PRIMARY KEY,
                project_id INTEGER NOT NULL,
                job_id INTEGER NOT NULL,
                components TEXT NOT NULL,
                target_artifact_path TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS image_signatures (
                hash TEXT PRIMARY KEY,
                digest TEXT NOT NULL,
                dockerfile_hash TEXT NOT NULL,
                context_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS force_refresh_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                category TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS resolved_refs (
                alias TEXT PRIMARY KEY,
                identity_type TEXT NOT NULL,
                identity_value TEXT NOT NULL,
                resolved_at TEXT NOT NULL,
                ttl_seconds INTEGER,
                trust_level TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cache_taints (
                object_hash TEXT PRIMARY KEY,
                reason TEXT NOT NULL,
                created_at TEXT NOT NULL,
                author_job_id INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cache_leases (
                resource_key TEXT NOT NULL,
                job_id INTEGER NOT NULL,
                acquired_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                PRIMARY KEY (resource_key, job_id)
            );

            CREATE TABLE IF NOT EXISTS cache_verdicts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL,
                action_key TEXT NOT NULL,
                object_hash TEXT NOT NULL,
                inputs_hash TEXT NOT NULL,
                verdict TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'untrusted',
                reasons TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cache_promotions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL,
                action_key TEXT NOT NULL,
                source_namespace TEXT NOT NULL,
                target_namespace TEXT NOT NULL,
                promoted_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS material_objects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                digest TEXT UNIQUE NOT NULL,
                origin TEXT NOT NULL,
                trust_label TEXT NOT NULL,
                auth_scope TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS material_aliases (
                alias TEXT PRIMARY KEY,
                material_id INTEGER NOT NULL REFERENCES material_objects(id),
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS action_cache (
                action_key TEXT PRIMARY KEY,
                manifest TEXT NOT NULL,
                namespace TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cache_epochs (
                scope TEXT PRIMARY KEY,
                current_epoch INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                author_job_id INTEGER NOT NULL DEFAULT 0,
                reason TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS toolchain_fingerprints (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                toolchain TEXT NOT NULL,
                digest TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS test_executions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                test_name TEXT NOT NULL,
                version TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS test_plans (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id      INTEGER NOT NULL,
                base_sha        TEXT NOT NULL,
                head_sha        TEXT NOT NULL,
                mode            TEXT NOT NULL,
                confidence      REAL NOT NULL DEFAULT 1.0,
                selected_count  INTEGER NOT NULL,
                skipped_count   INTEGER NOT NULL,
                subsystems      TEXT NOT NULL,
                fallback_reason TEXT,
                payload         TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS test_plan_items (
                plan_id         INTEGER NOT NULL REFERENCES test_plans(id),
                test_id         TEXT NOT NULL,
                action          TEXT NOT NULL,
                reason          TEXT NOT NULL,
                confidence      REAL NOT NULL,
                PRIMARY KEY (plan_id, test_id)
            );

            CREATE TABLE IF NOT EXISTS selector_misses (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                plan_id         INTEGER REFERENCES test_plans(id),
                missed_test     TEXT NOT NULL,
                failed_sha      TEXT NOT NULL,
                detected_by     TEXT NOT NULL,
                repaired        INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL
            );
            "#;
        let schema = match self.backend {
            StateBackend::Sqlite => sqlite_schema.to_string(),
            StateBackend::Postgres => postgres_schema(sqlite_schema),
        };
        for statement in schema.split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .with_context(|| format!("running migration statement: {}", statement))?;
        }

        if self.backend == StateBackend::Postgres {
            return Ok(());
        }

        // Safe alter logic for job_name extension
        let _ = sqlx::query("ALTER TABLE job_events ADD COLUMN job_name TEXT;")
            .execute(&self.pool)
            .await;
        // Safe alter logic for reason_code extension
        let _ = sqlx::query(
            "ALTER TABLE cache_requests ADD COLUMN reason_code TEXT NOT NULL DEFAULT 'unknown';",
        )
        .execute(&self.pool)
        .await;

        let _ = sqlx::query("ALTER TABLE release_attempts ADD COLUMN release_pipeline_id INTEGER;")
            .execute(&self.pool)
            .await;
        let _ =
            sqlx::query("ALTER TABLE release_attempts ADD COLUMN release_pipeline_status TEXT;")
                .execute(&self.pool)
                .await;
        let _ =
            sqlx::query("ALTER TABLE release_attempts ADD COLUMN production_pipeline_id INTEGER;")
                .execute(&self.pool)
                .await;
        let _ =
            sqlx::query("ALTER TABLE release_attempts ADD COLUMN production_pipeline_status TEXT;")
                .execute(&self.pool)
                .await;
        let _ = sqlx::query(
            "ALTER TABLE ci_job_runs ADD COLUMN root_pipeline_id INTEGER NOT NULL DEFAULT 0;",
        )
        .execute(&self.pool)
        .await;
        let _ = sqlx::query(
            "UPDATE ci_job_runs SET root_pipeline_id = pipeline_id WHERE root_pipeline_id = 0",
        )
        .execute(&self.pool)
        .await;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_ci_job_runs_root_pipeline
                ON ci_job_runs(project_id, root_pipeline_id)",
        )
        .execute(&self.pool)
        .await?;
        // Migrate selector_misses.plan_id from NOT NULL to nullable.
        // SQLite cannot ALTER COLUMN, so we rename → recreate → copy → drop.
        let has_old_schema: bool = sqlx::query_scalar::<_, String>(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='selector_misses'",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .map(|sql| sql.contains("NOT NULL REFERENCES test_plans"))
        .unwrap_or(false);

        if has_old_schema {
            let _ = sqlx::query("ALTER TABLE selector_misses RENAME TO _selector_misses_old")
                .execute(&self.pool)
                .await;
            let _ = sqlx::query(
                r#"CREATE TABLE IF NOT EXISTS selector_misses (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    plan_id     INTEGER REFERENCES test_plans(id),
                    missed_test TEXT NOT NULL,
                    failed_sha  TEXT NOT NULL,
                    detected_by TEXT NOT NULL,
                    repaired    INTEGER NOT NULL DEFAULT 0,
                    created_at  TEXT NOT NULL
                )"#,
            )
            .execute(&self.pool)
            .await;
            let _ = sqlx::query("INSERT INTO selector_misses SELECT * FROM _selector_misses_old")
                .execute(&self.pool)
                .await;
            let _ = sqlx::query("DROP TABLE IF EXISTS _selector_misses_old")
                .execute(&self.pool)
                .await;
        }

        Ok(())
    }

    // -- Cache Tracking ----------------------------------------------------
    pub async fn record_cache_request(
        &self,
        url: &str,
        method: &str,
        hit: bool,
        reason_code: &str,
        bytes_served: i64,
    ) -> Result<()> {
        let timestamp = chrono::Utc::now().to_rfc3339();
        if let Some(tx) = &self.telemetry_tx {
            let _ = tx
                .send(StateAction::RecordCacheRequest {
                    url: url.to_string(),
                    method: method.to_string(),
                    hit,
                    reason_code: reason_code.to_string(),
                    bytes_served,
                    timestamp,
                })
                .await;
        } else {
            // Recovery path for tests or direct execution without background actor.
            sqlx::query(
                "INSERT INTO cache_requests (url, method, hit, reason_code, bytes_served, timestamp) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(url)
            .bind(method)
            .bind(hit)
            .bind(reason_code)
            .bind(bytes_served)
            .bind(timestamp)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_cache_metrics(&self) -> Result<CacheMetrics> {
        let row: (i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                 COALESCE(SUM(bytes_served), 0),
                 COUNT(*),
                 COALESCE(SUM(CASE WHEN hit THEN 1 ELSE 0 END), 0),
                 (SELECT COUNT(*) FROM cache_objects),
                 COALESCE(SUM(CASE WHEN reason_code = 'singleflight_coalesced' THEN 1 ELSE 0 END), 0)
               FROM cache_requests"#
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0, 0, 0, 0, 0));

        let hit_count = row.2;
        let total_requests = row.1;
        let miss_count = total_requests - hit_count;
        let hit_ratio = if total_requests > 0 {
            (hit_count as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        Ok(CacheMetrics {
            bytes_served: row.0,
            total_requests,
            hit_count,
            miss_count,
            object_count: row.3,
            hit_ratio,
            singleflight_coalesced: row.4,
        })
    }

    pub async fn prune_cache_requests(&self, days_old: i64) -> Result<u64> {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::try_days(days_old).unwrap_or_else(|| chrono::Duration::days(0));
        let cutoff_str = cutoff.to_rfc3339();
        let result = sqlx::query("DELETE FROM cache_requests WHERE timestamp < ?")
            .bind(cutoff_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    // -- Pool operations ---------------------------------------------------

    pub async fn insert_pool(&self, p: &Pool) -> Result<()> {
        let sql = self.sql(
            r#"INSERT INTO pools
               (name, gitlab_runner_id, auth_token, tags, executor,
                min_warm, max_managers, concurrent, request_concurrency, paused, trust_tier)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        );
        sqlx::query(&sql)
            .bind(&p.name)
            .bind(p.gitlab_runner_id)
            .bind(&p.auth_token)
            .bind(&p.tags)
            .bind(&p.executor)
            .bind(p.min_warm)
            .bind(p.max_managers)
            .bind(p.concurrent)
            .bind(p.request_concurrency)
            .bind(p.paused)
            .bind(&p.trust_tier)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_pools(&self) -> Result<Vec<Pool>> {
        let sql = self.sql_owned(format!("{POOL_SELECT} ORDER BY name"));
        let pools = sqlx::query_as::<_, Pool>(&sql)
            .fetch_all(&self.pool)
            .await?;
        Ok(pools)
    }

    pub async fn get_pool(&self, name: &str) -> Result<Option<Pool>> {
        let sql = self.sql_owned(format!("{POOL_SELECT} WHERE name = ?"));
        let pool = sqlx::query_as::<_, Pool>(&sql)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(pool)
    }

    pub async fn update_pool_paused(&self, name: &str, paused: bool) -> Result<()> {
        let sql = self.sql("UPDATE pools SET paused = ? WHERE name = ?");
        sqlx::query(&sql)
            .bind(paused)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_pool_token(&self, name: &str, token: &str) -> Result<()> {
        let sql = self.sql("UPDATE pools SET auth_token = ? WHERE name = ?");
        sqlx::query(&sql)
            .bind(token)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -- Manager operations ------------------------------------------------

    pub async fn insert_manager(&self, m: &Manager) -> Result<()> {
        let sql = self.sql(
            r#"INSERT INTO managers
               (id, pool_name, docker_container_id, system_id, state,
                config_dir, started_at, last_contact_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        );
        sqlx::query(&sql)
            .bind(&m.id)
            .bind(&m.pool_name)
            .bind(&m.docker_container_id)
            .bind(&m.system_id)
            .bind(&m.state)
            .bind(&m.config_dir)
            .bind(&m.started_at)
            .bind(&m.last_contact_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_managers(&self, pool_name: Option<&str>) -> Result<Vec<Manager>> {
        let managers = match pool_name {
            Some(pn) => {
                let sql =
                    self.sql("SELECT * FROM managers WHERE pool_name = ? ORDER BY started_at");
                sqlx::query_as::<_, Manager>(&sql)
                    .bind(pn)
                    .fetch_all(&self.pool)
                    .await?
            }
            None => {
                sqlx::query_as::<_, Manager>(
                    "SELECT * FROM managers ORDER BY pool_name, started_at",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(managers)
    }

    pub async fn get_manager(&self, id: &str) -> Result<Option<Manager>> {
        let sql = self.sql("SELECT * FROM managers WHERE id = ?");
        let m = sqlx::query_as::<_, Manager>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(m)
    }

    pub async fn update_manager_state(&self, id: &str, state: &str) -> Result<()> {
        let sql = self.sql("UPDATE managers SET state = ? WHERE id = ?");
        sqlx::query(&sql)
            .bind(state)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_manager_system_id(&self, id: &str, system_id: &str) -> Result<()> {
        let sql = self.sql("UPDATE managers SET system_id = ? WHERE id = ?");
        sqlx::query(&sql)
            .bind(system_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_manager(&self, id: &str) -> Result<()> {
        let sql = self.sql("DELETE FROM managers WHERE id = ?");
        sqlx::query(&sql).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_pool(&self, name: &str) -> Result<()> {
        let delete_managers = self.sql("DELETE FROM managers WHERE pool_name = ?");
        sqlx::query(&delete_managers)
            .bind(name)
            .execute(&self.pool)
            .await?;
        let delete_pool = self.sql("DELETE FROM pools WHERE name = ?");
        sqlx::query(&delete_pool)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn count_active_managers(&self, pool_name: &str) -> Result<i64> {
        let sql = self.sql(
            "SELECT COUNT(*) FROM managers WHERE pool_name = ? AND state IN ('starting','online')",
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(pool_name)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    // -- Job event operations ----------------------------------------------

    pub async fn upsert_job_event(&self, e: &JobEvent) -> Result<()> {
        let sql = self.sql(
            r#"INSERT INTO job_events
               (job_id, project_id, pipeline_id, status, job_name, pool_name,
                system_id, queued_duration, received_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(job_id, status) DO UPDATE SET
                 project_id = excluded.project_id,
                 pipeline_id = excluded.pipeline_id,
                 job_name = excluded.job_name,
                 pool_name = excluded.pool_name,
                 system_id = excluded.system_id,
                 queued_duration = excluded.queued_duration,
                 received_at = excluded.received_at"#,
        );
        sqlx::query(&sql)
            .bind(e.job_id)
            .bind(e.project_id)
            .bind(e.pipeline_id)
            .bind(&e.status)
            .bind(&e.job_name)
            .bind(&e.pool_name)
            .bind(&e.system_id)
            .bind(e.queued_duration)
            .bind(&e.received_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn recent_job_events(&self, limit: i64) -> Result<Vec<JobEvent>> {
        let sql = self.sql("SELECT * FROM job_events ORDER BY received_at DESC LIMIT ?");
        let events = sqlx::query_as::<_, JobEvent>(&sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(events)
    }

    pub async fn upsert_ci_job_run(&self, run: &CiJobRun) -> Result<()> {
        let sql = self.sql(
            r#"INSERT INTO ci_job_runs
               (job_id, project_id, pipeline_id, root_pipeline_id, pipeline_sha, ref_name, job_name,
                stage, status, runner, runner_pool, queued_duration_secs, duration_secs,
                started_at, finished_at, web_url, observed_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(job_id) DO UPDATE SET
                project_id = excluded.project_id,
                pipeline_id = excluded.pipeline_id,
                root_pipeline_id = excluded.root_pipeline_id,
                pipeline_sha = excluded.pipeline_sha,
                ref_name = excluded.ref_name,
                job_name = excluded.job_name,
                stage = excluded.stage,
                status = excluded.status,
                runner = excluded.runner,
                runner_pool = excluded.runner_pool,
                queued_duration_secs = excluded.queued_duration_secs,
                duration_secs = excluded.duration_secs,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                web_url = excluded.web_url,
                observed_at = excluded.observed_at"#,
        );
        sqlx::query(&sql)
            .bind(run.job_id)
            .bind(run.project_id)
            .bind(run.pipeline_id)
            .bind(run.root_pipeline_id)
            .bind(&run.pipeline_sha)
            .bind(&run.ref_name)
            .bind(&run.job_name)
            .bind(&run.stage)
            .bind(&run.status)
            .bind(&run.runner)
            .bind(&run.runner_pool)
            .bind(run.queued_duration_secs)
            .bind(run.duration_secs)
            .bind(&run.started_at)
            .bind(&run.finished_at)
            .bind(&run.web_url)
            .bind(&run.observed_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn upsert_ci_job_runs(&self, runs: &[CiJobRun]) -> Result<()> {
        for run in runs {
            self.upsert_ci_job_run(run).await?;
        }
        Ok(())
    }

    pub async fn list_ci_job_runs(
        &self,
        project_id: i64,
        pipeline_id: i64,
    ) -> Result<Vec<CiJobRun>> {
        let sql = self.sql(
            r#"SELECT * FROM ci_job_runs
               WHERE project_id = ? AND (pipeline_id = ? OR root_pipeline_id = ?)
               ORDER BY stage, job_name"#,
        );
        sqlx::query_as::<_, CiJobRun>(&sql)
            .bind(project_id)
            .bind(pipeline_id)
            .bind(pipeline_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn ci_job_bottlenecks(
        &self,
        project_id: i64,
        ref_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<CiJobBottleneck>> {
        if let Some(ref_name) = ref_name {
            sqlx::query_as::<_, CiJobBottleneck>(
                r#"SELECT job_name,
                          stage,
                          runner_pool,
                          AVG(duration_secs) AS avg_duration_secs,
                          (SELECT duration_secs
                             FROM ci_job_runs latest
                            WHERE latest.project_id = ci_job_runs.project_id
                              AND latest.ref_name = ci_job_runs.ref_name
                              AND latest.job_name = ci_job_runs.job_name
                            ORDER BY observed_at DESC
                            LIMIT 1) AS latest_duration_secs,
                          MAX(duration_secs) AS max_duration_secs,
                          COUNT(*) AS runs
                     FROM ci_job_runs
                    WHERE project_id = ? AND ref_name = ? AND duration_secs IS NOT NULL
                    GROUP BY job_name, stage, runner_pool
                    ORDER BY avg_duration_secs DESC
                    LIMIT ?"#,
            )
            .bind(project_id)
            .bind(ref_name)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
        } else {
            sqlx::query_as::<_, CiJobBottleneck>(
                r#"SELECT job_name,
                          stage,
                          runner_pool,
                          AVG(duration_secs) AS avg_duration_secs,
                          (SELECT duration_secs
                             FROM ci_job_runs latest
                            WHERE latest.project_id = ci_job_runs.project_id
                              AND latest.job_name = ci_job_runs.job_name
                            ORDER BY observed_at DESC
                            LIMIT 1) AS latest_duration_secs,
                          MAX(duration_secs) AS max_duration_secs,
                          COUNT(*) AS runs
                     FROM ci_job_runs
                    WHERE project_id = ? AND duration_secs IS NOT NULL
                    GROUP BY job_name, stage, runner_pool
                    ORDER BY avg_duration_secs DESC
                    LIMIT ?"#,
            )
            .bind(project_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
        }
    }

    pub async fn count_pending_jobs(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(DISTINCT job_id) 
               FROM job_events 
               WHERE status = 'pending' 
                 AND job_id NOT IN (
                     SELECT job_id FROM job_events 
                     WHERE status IN ('running', 'success', 'failed', 'canceled')
                 )"#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn count_latest_jobs_with_statuses(&self, statuses: &[&str]) -> Result<i64> {
        if statuses.is_empty() {
            return Ok(0);
        }

        let bind_slots = std::iter::repeat_n("?", statuses.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = self.sql_owned(format!(
            r#"SELECT COUNT(*)
               FROM job_events current
              WHERE current.status IN ({bind_slots})
                AND current.received_at = (
                    SELECT MAX(latest.received_at)
                      FROM job_events latest
                     WHERE latest.job_id = current.job_id
                )"#
        ));
        let mut query = sqlx::query_as::<_, (i64,)>(&sql);
        for status in statuses {
            query = query.bind(status);
        }
        let row = query.fetch_one(&self.pool).await?;
        Ok(row.0)
    }

    pub async fn count_queued_jobs(&self) -> Result<i64> {
        self.count_latest_jobs_with_statuses(&["created", "pending"])
            .await
    }

    pub async fn count_running_jobs(&self) -> Result<i64> {
        self.count_latest_jobs_with_statuses(&["running"]).await
    }

    pub async fn clear_history(&self) -> Result<()> {
        sqlx::query("DELETE FROM job_events")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM ci_job_runs")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM tracked_pipelines")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM evidence_capsules")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM retry_decisions")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM events")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_pipeline(&self, pipeline_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM job_events WHERE pipeline_id = ?")
            .bind(pipeline_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM ci_job_runs WHERE pipeline_id = ?")
            .bind(pipeline_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM tracked_pipelines WHERE pipeline_id = ?")
            .bind(pipeline_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_job_event(&self, job_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM job_events WHERE job_id = ?")
            .bind(job_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -- Pipeline tracking -------------------------------------------------

    pub async fn upsert_tracked_pipeline(&self, pipeline: &TrackedPipeline) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO tracked_pipelines
               (pipeline_id, project_id, ref_name, sha, status, updated_at)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT(pipeline_id) DO UPDATE SET
                   project_id = excluded.project_id,
                   ref_name = excluded.ref_name,
                   sha = excluded.sha,
                   status = excluded.status,
                   updated_at = excluded.updated_at"#,
        )
        .bind(pipeline.pipeline_id)
        .bind(pipeline.project_id)
        .bind(&pipeline.ref_name)
        .bind(&pipeline.sha)
        .bind(&pipeline.status)
        .bind(&pipeline.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_release_attempt(
        &self,
        project_id: i64,
        ref_name: &str,
        sha: &str,
        version: &str,
        upstream_pipeline_id: Option<i64>,
        upstream_status: &str,
        canary_status: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"INSERT INTO release_attempts
               (project_id, ref_name, sha, version, upstream_pipeline_id, upstream_status,
                canary_status, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(project_id, ref_name, sha) DO UPDATE SET
                   version = excluded.version,
                   upstream_pipeline_id = excluded.upstream_pipeline_id,
                   upstream_status = excluded.upstream_status,
                   canary_status = excluded.canary_status,
                   updated_at = excluded.updated_at"#,
        )
        .bind(project_id)
        .bind(ref_name)
        .bind(sha)
        .bind(version)
        .bind(upstream_pipeline_id)
        .bind(upstream_status)
        .bind(canary_status)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_release_attempt(
        &self,
        project_id: i64,
        ref_name: &str,
        sha: &str,
    ) -> Result<Option<ReleaseAttempt>> {
        let attempt = sqlx::query_as::<_, ReleaseAttempt>(
            r#"SELECT * FROM release_attempts
               WHERE project_id = ? AND ref_name = ? AND sha = ?"#,
        )
        .bind(project_id)
        .bind(ref_name)
        .bind(sha)
        .fetch_optional(&self.pool)
        .await?;
        Ok(attempt)
    }

    pub async fn latest_release_attempt(
        &self,
        project_id: i64,
        ref_name: &str,
    ) -> Result<Option<ReleaseAttempt>> {
        let attempt = sqlx::query_as::<_, ReleaseAttempt>(
            r#"SELECT * FROM release_attempts
               WHERE project_id = ? AND ref_name = ?
               ORDER BY updated_at DESC, id DESC
               LIMIT 1"#,
        )
        .bind(project_id)
        .bind(ref_name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(attempt)
    }

    pub async fn latest_release_attempt_any(&self) -> Result<Option<ReleaseAttempt>> {
        let attempt = sqlx::query_as::<_, ReleaseAttempt>(
            r#"SELECT * FROM release_attempts
               ORDER BY updated_at DESC, id DESC
               LIMIT 1"#,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(attempt)
    }

    pub async fn recent_release_attempts(
        &self,
        project_id: Option<i64>,
        ref_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ReleaseAttempt>> {
        let mut query = String::from("SELECT * FROM release_attempts");
        let mut clauses = Vec::new();
        if project_id.is_some() {
            clauses.push("project_id = ?");
        }
        if ref_name.is_some() {
            clauses.push("ref_name = ?");
        }
        if !clauses.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&clauses.join(" AND "));
        }
        query.push_str(" ORDER BY updated_at DESC, id DESC LIMIT ?");

        let mut stmt = sqlx::query_as::<_, ReleaseAttempt>(&query);
        if let Some(project_id) = project_id {
            stmt = stmt.bind(project_id);
        }
        if let Some(ref_name) = ref_name {
            stmt = stmt.bind(ref_name);
        }
        stmt = stmt.bind(limit);
        let attempts = stmt.fetch_all(&self.pool).await?;
        Ok(attempts)
    }

    pub async fn claim_release_canary(
        &self,
        project_id: i64,
        ref_name: &str,
        sha: &str,
        version: &str,
        upstream_pipeline_id: Option<i64>,
    ) -> Result<bool> {
        self.upsert_release_attempt(
            project_id,
            ref_name,
            sha,
            version,
            upstream_pipeline_id,
            "success",
            "pending",
        )
        .await?;

        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"UPDATE release_attempts
               SET canary_status = 'running',
                   canary_started_at = ?,
                   canary_finished_at = NULL,
                   canary_note = NULL,
                   updated_at = ?
               WHERE project_id = ? AND ref_name = ? AND sha = ?
                 AND canary_status NOT IN ('running', 'passed')"#,
        )
        .bind(&now)
        .bind(&now)
        .bind(project_id)
        .bind(ref_name)
        .bind(sha)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn finish_release_canary(
        &self,
        project_id: i64,
        ref_name: &str,
        sha: &str,
        status: &str,
        note: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE release_attempts
               SET canary_status = ?,
                   canary_finished_at = ?,
                   canary_note = ?,
                   updated_at = ?
               WHERE project_id = ? AND ref_name = ? AND sha = ?"#,
        )
        .bind(status)
        .bind(&now)
        .bind(note)
        .bind(&now)
        .bind(project_id)
        .bind(ref_name)
        .bind(sha)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn attach_release_pipeline(
        &self,
        project_id: i64,
        ref_name: &str,
        sha: &str,
        release_pipeline_id: i64,
        release_pipeline_status: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE release_attempts
               SET release_pipeline_id = ?,
                   release_pipeline_status = ?,
                   canary_status = 'running',
                   canary_finished_at = NULL,
                   canary_note = NULL,
                   updated_at = ?
               WHERE project_id = ? AND ref_name = ? AND sha = ?"#,
        )
        .bind(release_pipeline_id)
        .bind(release_pipeline_status)
        .bind(&now)
        .bind(project_id)
        .bind(ref_name)
        .bind(sha)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_release_pipeline_status(
        &self,
        release_pipeline_id: i64,
        status: &str,
    ) -> Result<Option<ReleaseAttempt>> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE release_attempts
               SET release_pipeline_status = ?,
                   canary_status = ?,
                   canary_finished_at = CASE
                       WHEN ? IN ('success', 'failed', 'canceled', 'skipped') THEN ?
                       ELSE canary_finished_at
                   END,
                   updated_at = ?
               WHERE release_pipeline_id = ?"#,
        )
        .bind(status)
        .bind(match status {
            "failed" | "canceled" | "skipped" => "failed",
            _ => "running",
        })
        .bind(status)
        .bind(&now)
        .bind(&now)
        .bind(release_pipeline_id)
        .execute(&self.pool)
        .await?;

        sqlx::query_as::<_, ReleaseAttempt>(
            r#"SELECT * FROM release_attempts WHERE release_pipeline_id = ?"#,
        )
        .bind(release_pipeline_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn release_attempt_by_release_pipeline_id(
        &self,
        release_pipeline_id: i64,
    ) -> Result<Option<ReleaseAttempt>> {
        sqlx::query_as::<_, ReleaseAttempt>(
            r#"SELECT * FROM release_attempts WHERE release_pipeline_id = ?"#,
        )
        .bind(release_pipeline_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn attach_production_pipeline(
        &self,
        project_id: i64,
        ref_name: &str,
        sha: &str,
        production_pipeline_id: i64,
        production_pipeline_status: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE release_attempts
               SET production_pipeline_id = ?,
                   production_pipeline_status = ?,
                   updated_at = ?
               WHERE project_id = ? AND ref_name = ? AND sha = ?"#,
        )
        .bind(production_pipeline_id)
        .bind(production_pipeline_status)
        .bind(&now)
        .bind(project_id)
        .bind(ref_name)
        .bind(sha)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_production_pipeline_status(
        &self,
        production_pipeline_id: i64,
        status: &str,
    ) -> Result<Option<ReleaseAttempt>> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE release_attempts
               SET production_pipeline_status = ?,
                   updated_at = ?
               WHERE production_pipeline_id = ?"#,
        )
        .bind(status)
        .bind(&now)
        .bind(production_pipeline_id)
        .execute(&self.pool)
        .await?;

        sqlx::query_as::<_, ReleaseAttempt>(
            r#"SELECT * FROM release_attempts WHERE production_pipeline_id = ?"#,
        )
        .bind(production_pipeline_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn release_attempt_by_production_pipeline_id(
        &self,
        production_pipeline_id: i64,
    ) -> Result<Option<ReleaseAttempt>> {
        sqlx::query_as::<_, ReleaseAttempt>(
            r#"SELECT * FROM release_attempts WHERE production_pipeline_id = ?"#,
        )
        .bind(production_pipeline_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn list_active_pipelines_for_ref(
        &self,
        project_id: i64,
        ref_name: &str,
    ) -> Result<Vec<TrackedPipeline>> {
        let rows = sqlx::query_as::<_, TrackedPipeline>(
            r#"SELECT * FROM tracked_pipelines
               WHERE project_id = ?
                 AND ref_name = ?
                 AND status IN ('created', 'pending', 'running')
               ORDER BY updated_at DESC"#,
        )
        .bind(project_id)
        .bind(ref_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_tracked_pipelines(&self, limit: i64) -> Result<Vec<TrackedPipeline>> {
        let rows = sqlx::query_as::<_, TrackedPipeline>(
            "SELECT * FROM tracked_pipelines ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    // -- Evidence capsules -------------------------------------------------

    pub async fn insert_evidence_capsule(
        &self,
        event_type: &str,
        capsule: &FailureCapsule,
    ) -> Result<i64> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let classification = format!("{:?}", capsule.classify()).to_ascii_lowercase();
        let row: (i64,) = sqlx::query_as(
            r#"INSERT INTO evidence_capsules
               (event_type, project_id, job_id, pipeline_id, commit_sha, ref_name, stage,
                exit_code, failure_kind, classification, created_at, payload)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               RETURNING id"#,
        )
        .bind(event_type)
        .bind(capsule.project_id)
        .bind(capsule.job_id)
        .bind(capsule.pipeline_id)
        .bind(&capsule.commit_sha)
        .bind(&capsule.ref_name)
        .bind(&capsule.stage)
        .bind(capsule.exit_code)
        .bind(&capsule.failure_kind)
        .bind(classification)
        .bind(created_at)
        .bind(capsule.to_json())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn latest_evidence_for_job(
        &self,
        project_id: i64,
        job_id: i64,
    ) -> Result<Option<FailureCapsule>> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"SELECT payload FROM evidence_capsules
               WHERE project_id = ? AND job_id = ?
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(project_id)
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(payload,)| serde_json::from_str(&payload).ok()))
    }

    /// Look up the most recent failure capsule for a job_id across all projects.
    /// Used by the capability server where the caller may not know the project_id.
    pub async fn latest_evidence_by_job_id(&self, job_id: i64) -> Result<Option<FailureCapsule>> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"SELECT payload FROM evidence_capsules
               WHERE job_id = ?
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(payload,)| serde_json::from_str(&payload).ok()))
    }

    pub async fn recent_evidence_all(&self, limit: i64) -> Result<Vec<EvidenceRecord>> {
        let rows = sqlx::query_as::<_, EvidenceRecord>(
            r#"SELECT * FROM evidence_capsules
               ORDER BY created_at DESC
               LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn all_recent_secret_audit_events(
        &self,
        limit: i64,
    ) -> Result<Vec<SecretAuditEvent>> {
        sqlx::query_as::<_, SecretAuditEvent>(
            r#"SELECT id, repo_name, version, target, action, status, detail, created_at
               FROM secret_audit_events
               ORDER BY created_at DESC, id DESC
               LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn list_evidence_for_ref(
        &self,
        project_id: i64,
        ref_name: &str,
        limit: i64,
    ) -> Result<Vec<EvidenceRecord>> {
        let rows = sqlx::query_as::<_, EvidenceRecord>(
            r#"SELECT * FROM evidence_capsules
               WHERE project_id = ? AND ref_name = ?
               ORDER BY created_at DESC
               LIMIT ?"#,
        )
        .bind(project_id)
        .bind(ref_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    // -- Retry decisions ---------------------------------------------------

    pub async fn insert_recovery_decision(
        &self,
        project_id: i64,
        job_id: i64,
        commit_sha: &str,
        ref_name: &str,
        decision: &str,
        reason: &str,
    ) -> Result<i64> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let row: (i64,) = sqlx::query_as(
            r#"INSERT INTO retry_decisions
               (project_id, job_id, commit_sha, ref_name, decision, reason, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               RETURNING id"#,
        )
        .bind(project_id)
        .bind(job_id)
        .bind(commit_sha)
        .bind(ref_name)
        .bind(decision)
        .bind(reason)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn count_recovery_decisions(&self, project_id: i64, job_id: i64) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM retry_decisions WHERE project_id = ? AND job_id = ?",
        )
        .bind(project_id)
        .bind(job_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn latest_job_decision(
        &self,
        project_id: i64,
        job_id: i64,
    ) -> Result<Option<RetryRecord>> {
        let row = sqlx::query_as::<_, RetryRecord>(
            r#"SELECT * FROM retry_decisions
               WHERE project_id = ? AND job_id = ?
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(project_id)
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // -- Event log operations ----------------------------------------------

    pub async fn append_event(
        &self,
        event_type: &str,
        project_id: Option<i64>,
        job_id: Option<i64>,
        actor: &str,
        payload: &str,
    ) -> Result<i64> {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let sql = self.sql(
            r#"INSERT INTO events (event_type, timestamp, project_id, job_id, actor, payload)
               VALUES (?, ?, ?, ?, ?, ?) RETURNING id"#,
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(event_type)
            .bind(timestamp)
            .bind(project_id)
            .bind(job_id)
            .bind(actor)
            .bind(payload)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn get_events(&self, limit: i64) -> Result<Vec<EventLog>> {
        let sql = self.sql("SELECT * FROM events ORDER BY id DESC LIMIT ?");
        let events = sqlx::query_as::<_, EventLog>(&sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(events)
    }

    pub async fn record_git_command_event(
        &self,
        event: &crate::git::event::GitCommandEvent,
    ) -> Result<i64> {
        let sql = self.sql(
            r#"INSERT INTO git_command_events
               (request_id, actor, cwd, repo_root, argv_redacted, argv_hash, command_class, risk, mode,
                before_head, before_branch, before_dirty, after_head, after_branch, after_dirty,
                exit_code, sidecar_status, mirror_status, created_at, payload)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               RETURNING id"#,
        );
        let payload = serde_json::to_string(event)?;
        let argv_redacted = serde_json::to_string(&event.argv_redacted)?;
        let before_dirty = event
            .before
            .dirty
            .map(|dirty| if dirty { 1_i64 } else { 0_i64 });
        let after_dirty = event.after.as_ref().and_then(|snapshot| {
            snapshot
                .dirty
                .map(|dirty| if dirty { 1_i64 } else { 0_i64 })
        });
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(&event.request_id)
            .bind(&event.actor)
            .bind(&event.cwd)
            .bind(&event.repo_root)
            .bind(argv_redacted)
            .bind(&event.argv_hash)
            .bind(&event.class)
            .bind(format!("{:?}", event.risk))
            .bind(&event.mode)
            .bind(&event.before.head)
            .bind(&event.before.branch)
            .bind(before_dirty)
            .bind(
                event
                    .after
                    .as_ref()
                    .and_then(|snapshot| snapshot.head.clone()),
            )
            .bind(
                event
                    .after
                    .as_ref()
                    .and_then(|snapshot| snapshot.branch.clone()),
            )
            .bind(after_dirty)
            .bind(event.exit_code)
            .bind(&event.sidecar_status)
            .bind(&event.mirror_status)
            .bind(&event.created_at)
            .bind(payload)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn recent_git_command_events(
        &self,
        limit: i64,
    ) -> Result<Vec<GitCommandEventRecord>> {
        let sql = self.sql("SELECT * FROM git_command_events ORDER BY id DESC LIMIT ?");
        let events = sqlx::query_as::<_, GitCommandEventRecord>(&sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(events)
    }

    pub async fn record_git_ref_change(
        &self,
        request_id: String,
        ref_name: String,
        before_sha: Option<String>,
        after_sha: Option<String>,
        status: String,
        created_at: String,
    ) -> Result<i64> {
        let record = GitRefUpdate {
            id: 0,
            request_id,
            ref_name,
            before_sha,
            after_sha,
            status,
            created_at,
        };
        self.record_git_ref_update(&record).await
    }

    pub async fn record_git_ref_update(&self, update: &GitRefUpdate) -> Result<i64> {
        let sql = self.sql(
            r#"INSERT INTO git_ref_updates
               (request_id, ref_name, before_sha, after_sha, status, created_at)
               VALUES (?, ?, ?, ?, ?, ?) RETURNING id"#,
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(&update.request_id)
            .bind(&update.ref_name)
            .bind(&update.before_sha)
            .bind(&update.after_sha)
            .bind(&update.status)
            .bind(&update.created_at)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn recent_git_ref_updates(&self, limit: i64) -> Result<Vec<GitRefUpdate>> {
        let sql = self.sql("SELECT * FROM git_ref_updates ORDER BY id DESC LIMIT ?");
        let updates = sqlx::query_as::<_, GitRefUpdate>(&sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(updates)
    }

    pub async fn record_git_mirror_job(&self, job: &GitMirrorJob) -> Result<i64> {
        let sql = self.sql(
            r#"INSERT INTO git_mirror_jobs
               (request_id, remote_name, branch_name, status, detail, created_at)
               VALUES (?, ?, ?, ?, ?, ?) RETURNING id"#,
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(&job.request_id)
            .bind(&job.remote_name)
            .bind(&job.branch_name)
            .bind(&job.status)
            .bind(&job.detail)
            .bind(&job.created_at)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn recent_git_mirror_jobs(&self, limit: i64) -> Result<Vec<GitMirrorJob>> {
        let sql = self.sql("SELECT * FROM git_mirror_jobs ORDER BY id DESC LIMIT ?");
        let jobs = sqlx::query_as::<_, GitMirrorJob>(&sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(jobs)
    }

    pub async fn record_git_risk_approval(&self, approval: &GitRiskApproval) -> Result<i64> {
        let sql = self.sql(
            r#"INSERT INTO git_risk_approvals
               (request_id, actor, command_class, risk, approved, reason, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id"#,
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(&approval.request_id)
            .bind(&approval.actor)
            .bind(&approval.command_class)
            .bind(&approval.risk)
            .bind(approval.approved)
            .bind(&approval.reason)
            .bind(&approval.created_at)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn record_git_command_artifact(&self, artifact: &GitCommandArtifact) -> Result<i64> {
        let sql = self.sql(
            r#"INSERT INTO git_command_artifacts
               (request_id, artifact_kind, artifact_path, digest, created_at)
               VALUES (?, ?, ?, ?, ?) RETURNING id"#,
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(&artifact.request_id)
            .bind(&artifact.artifact_kind)
            .bind(&artifact.artifact_path)
            .bind(&artifact.digest)
            .bind(&artifact.created_at)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    /// Return tracked pipelines whose ref_name starts with "agent/".
    pub async fn list_agent_pipelines(&self) -> Result<Vec<TrackedPipeline>> {
        sqlx::query_as::<_, TrackedPipeline>(
            "SELECT * FROM tracked_pipelines WHERE ref_name LIKE 'agent/%' ORDER BY updated_at DESC LIMIT 20",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn upsert_secret_authority(&self, authority: &SecretAuthority) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO secret_authorities
               (name, kind, address, status, mount, prefix, token_fingerprint, metadata_path, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(name) DO UPDATE SET
                   kind = excluded.kind,
                   address = excluded.address,
                   status = excluded.status,
                   mount = excluded.mount,
                   prefix = excluded.prefix,
                   token_fingerprint = excluded.token_fingerprint,
                   metadata_path = excluded.metadata_path,
                   updated_at = excluded.updated_at"#,
        )
        .bind(&authority.name)
        .bind(&authority.kind)
        .bind(&authority.address)
        .bind(&authority.status)
        .bind(&authority.mount)
        .bind(&authority.prefix)
        .bind(&authority.token_fingerprint)
        .bind(&authority.metadata_path)
        .bind(&authority.created_at)
        .bind(&authority.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_secret_authority(&self, name: &str) -> Result<Option<SecretAuthority>> {
        sqlx::query_as::<_, SecretAuthority>("SELECT * FROM secret_authorities WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn upsert_release_secret_set(&self, set: &ReleaseSecretSet) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO release_secret_sets
               (repo_name, version, target, authority_name, status, rendered_deploy_env_path,
                rendered_runtime_env_path, audit_path, bundle_path, report_path,
                runtime_secret_vault_path, recovery_password_vault_path, expires_at,
                rotated_at, finalized_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(repo_name, version, target) DO UPDATE SET
                   authority_name = excluded.authority_name,
                   status = excluded.status,
                   rendered_deploy_env_path = excluded.rendered_deploy_env_path,
                   rendered_runtime_env_path = excluded.rendered_runtime_env_path,
                   audit_path = excluded.audit_path,
                   bundle_path = excluded.bundle_path,
                   report_path = excluded.report_path,
                   runtime_secret_vault_path = excluded.runtime_secret_vault_path,
                   recovery_password_vault_path = excluded.recovery_password_vault_path,
                   expires_at = excluded.expires_at,
                   rotated_at = excluded.rotated_at,
                   finalized_at = excluded.finalized_at,
                   updated_at = excluded.updated_at"#,
        )
        .bind(&set.repo_name)
        .bind(&set.version)
        .bind(&set.target)
        .bind(&set.authority_name)
        .bind(&set.status)
        .bind(&set.rendered_deploy_env_path)
        .bind(&set.rendered_runtime_env_path)
        .bind(&set.audit_path)
        .bind(&set.bundle_path)
        .bind(&set.report_path)
        .bind(&set.runtime_secret_vault_path)
        .bind(&set.recovery_password_vault_path)
        .bind(&set.expires_at)
        .bind(&set.rotated_at)
        .bind(&set.finalized_at)
        .bind(&set.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_release_secret_set(
        &self,
        repo_name: &str,
        version: &str,
        target: &str,
    ) -> Result<Option<ReleaseSecretSet>> {
        sqlx::query_as::<_, ReleaseSecretSet>(
            r#"SELECT * FROM release_secret_sets
               WHERE repo_name = ? AND version = ? AND target = ?"#,
        )
        .bind(repo_name)
        .bind(version)
        .bind(target)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn latest_release_secret_set(
        &self,
        repo_name: &str,
    ) -> Result<Option<ReleaseSecretSet>> {
        sqlx::query_as::<_, ReleaseSecretSet>(
            r#"SELECT * FROM release_secret_sets
               WHERE repo_name = ?
               ORDER BY updated_at DESC
               LIMIT 1"#,
        )
        .bind(repo_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn mark_release_secret_set_finalized(
        &self,
        repo_name: &str,
        version: &str,
        target: &str,
        finalized_at: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE release_secret_sets
               SET status = 'finalized',
                   finalized_at = ?,
                   updated_at = ?
               WHERE repo_name = ? AND version = ? AND target = ?"#,
        )
        .bind(finalized_at)
        .bind(finalized_at)
        .bind(repo_name)
        .bind(version)
        .bind(target)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_secret_audit_event(&self, event: &SecretAuditEvent) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO secret_audit_events
               (repo_name, version, target, action, status, detail, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&event.repo_name)
        .bind(&event.version)
        .bind(&event.target)
        .bind(&event.action)
        .bind(&event.status)
        .bind(&event.detail)
        .bind(&event.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn recent_secret_audit_events(
        &self,
        repo_name: &str,
        limit: i64,
    ) -> Result<Vec<SecretAuditEvent>> {
        sqlx::query_as::<_, SecretAuditEvent>(
            r#"SELECT id, repo_name, version, target, action, status, detail, created_at
               FROM secret_audit_events
               WHERE repo_name = ?
               ORDER BY created_at DESC, id DESC
               LIMIT ?"#,
        )
        .bind(repo_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    // -- Test TUI Tracking ----------------------------------------------------

    pub async fn record_test_execution(
        &self,
        test_name: &str,
        version: &str,
        duration_ms: i64,
        status: &str,
    ) -> Result<()> {
        let created_at = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO test_executions (test_name, version, duration_ms, status, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(test_name)
        .bind(version)
        .bind(duration_ms)
        .bind(status)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn latest_successful_test_execution(
        &self,
        test_name: &str,
    ) -> Result<Option<TestExecution>> {
        sqlx::query_as(
            r#"
            SELECT id, test_name, version, duration_ms, status, created_at
            FROM test_executions
            WHERE test_name = ? AND status = 'success'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(test_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get_test_bottlenecks(
        &self,
        mode: &str,
        limit: i64,
    ) -> Result<Vec<TestBottleneck>> {
        if mode == "average" {
            sqlx::query_as(
                r#"
                SELECT test_name,
                       AVG(duration_ms) as avg_duration_ms,
                       MAX(duration_ms) as latest_duration_ms,
                       COUNT(*) as count
                FROM test_executions
                GROUP BY test_name
                ORDER BY avg_duration_ms DESC
                LIMIT ?
                "#,
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
        } else {
            // mode == "latest"
            sqlx::query_as(
                r#"
                SELECT test_name,
                       AVG(duration_ms) as avg_duration_ms,
                       (SELECT duration_ms FROM test_executions t2 WHERE t2.test_name = t1.test_name ORDER BY created_at DESC LIMIT 1) as latest_duration_ms,
                       COUNT(*) as count
                FROM test_executions t1
                GROUP BY test_name
                ORDER BY latest_duration_ms DESC
                LIMIT ?
                "#
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
        }
    }

    pub async fn get_test_history(
        &self,
        test_name: &str,
        limit: i64,
    ) -> Result<Vec<TestExecution>> {
        sqlx::query_as(
            "SELECT * FROM test_executions WHERE test_name = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(test_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    // -- VTI Test Intelligence Tracking ----------------------------------------

    /// Record a computed test plan.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_test_plan(
        &self,
        project_id: i64,
        base_sha: &str,
        head_sha: &str,
        mode: &str,
        confidence: f64,
        selected_count: i64,
        skipped_count: i64,
        subsystems: &str,
        fallback_reason: Option<&str>,
        payload: &str,
    ) -> Result<i64> {
        let created_at = chrono::Utc::now().to_rfc3339();
        if self.backend == StateBackend::Postgres {
            let row: (i64,) = sqlx::query_as(
                r#"INSERT INTO test_plans
                   (project_id, base_sha, head_sha, mode, confidence,
                    selected_count, skipped_count, subsystems, fallback_reason, payload, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                   RETURNING id"#,
            )
            .bind(project_id)
            .bind(base_sha)
            .bind(head_sha)
            .bind(mode)
            .bind(confidence)
            .bind(selected_count)
            .bind(skipped_count)
            .bind(subsystems)
            .bind(fallback_reason)
            .bind(payload)
            .bind(&created_at)
            .fetch_one(&self.pool)
            .await?;
            return Ok(row.0);
        }
        let result = sqlx::query(
            r#"INSERT INTO test_plans
               (project_id, base_sha, head_sha, mode, confidence,
                selected_count, skipped_count, subsystems, fallback_reason, payload, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(project_id)
        .bind(base_sha)
        .bind(head_sha)
        .bind(mode)
        .bind(confidence)
        .bind(selected_count)
        .bind(skipped_count)
        .bind(subsystems)
        .bind(fallback_reason)
        .bind(payload)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        self.inserted_id(result).await
    }

    /// Record an individual test selection decision within a plan.
    pub async fn record_test_plan_item(
        &self,
        plan_id: i64,
        test_id: &str,
        action: &str,
        reason: &str,
        confidence: f64,
    ) -> Result<()> {
        let sql = self.sql(
            r#"INSERT INTO test_plan_items (plan_id, test_id, action, reason, confidence)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(plan_id, test_id) DO UPDATE SET
                 action = excluded.action,
                 reason = excluded.reason,
                 confidence = excluded.confidence"#,
        );
        sqlx::query(&sql)
            .bind(plan_id)
            .bind(test_id)
            .bind(action)
            .bind(reason)
            .bind(confidence)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Record a selector miss (a test that was skipped but later found to fail).
    pub async fn record_selector_miss(
        &self,
        plan_id: Option<i64>,
        missed_test: &str,
        failed_sha: &str,
        detected_by: &str,
    ) -> Result<()> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let sql = self.sql(
            r#"INSERT INTO selector_misses
               (plan_id, missed_test, failed_sha, detected_by, repaired, created_at)
               VALUES (?, ?, ?, ?, FALSE, ?)"#,
        );
        sqlx::query(&sql)
            .bind(plan_id)
            .bind(missed_test)
            .bind(failed_sha)
            .bind(detected_by)
            .bind(created_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Count unrepaired selector misses within a time window.
    pub async fn count_selector_misses_since(&self, since: &str) -> Result<i64> {
        let sql = self
            .sql("SELECT COUNT(*) FROM selector_misses WHERE repaired = FALSE AND created_at > ?");
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(since)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    /// Get the most recent test plan for a project.
    pub async fn latest_test_plan(&self, project_id: i64) -> Result<Option<(i64, String)>> {
        let sql = self.sql(
            "SELECT id, payload FROM test_plans WHERE project_id = ? ORDER BY created_at DESC LIMIT 1",
        );
        let row: Option<(i64, String)> = sqlx::query_as(&sql)
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    // ------------------------------------------------------------------
    // Test verdict cache
    // ------------------------------------------------------------------

    /// Store a test verdict in the cache.
    #[allow(clippy::too_many_arguments)]
    pub async fn store_test_verdict(
        &self,
        job_id: i64,
        action_key: &str,
        object_hash: &str,
        inputs_hash: &str,
        verdict: &str,
        tier: &str,
        reasons: &str,
    ) -> Result<()> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let sql = self.sql(
            r#"INSERT INTO cache_verdicts
               (job_id, action_key, object_hash, inputs_hash, verdict, tier, reasons, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        );
        sqlx::query(&sql)
            .bind(job_id)
            .bind(action_key)
            .bind(object_hash)
            .bind(inputs_hash)
            .bind(verdict)
            .bind(tier)
            .bind(reasons)
            .bind(created_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Look up a cached test verdict by its inputs hash.
    pub async fn lookup_test_verdict(
        &self,
        inputs_hash: &str,
    ) -> Result<Option<(String, String, String)>> {
        let sql = self.sql(
            "SELECT verdict, action_key, created_at FROM cache_verdicts WHERE inputs_hash = ? ORDER BY created_at DESC LIMIT 1",
        );
        let row: Option<(String, String, String)> = sqlx::query_as(&sql)
            .bind(inputs_hash)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    /// Prune expired cache verdicts older than the given cutoff date.
    pub async fn prune_test_verdicts(&self, older_than: &str) -> Result<u64> {
        let sql = self.sql("DELETE FROM cache_verdicts WHERE created_at < ?");
        let result = sqlx::query(&sql)
            .bind(older_than)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Prune expired action cache entries older than the given cutoff date.
    pub async fn prune_action_cache(&self, older_than: &str) -> Result<u64> {
        let sql = self.sql("DELETE FROM action_cache WHERE created_at < ?");
        let result = sqlx::query(&sql)
            .bind(older_than)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Upsert an action cache manifest after a successful build step.
    pub async fn upsert_action_cache(
        &self,
        action_key: &str,
        manifest: &str,
        namespace: &str,
    ) -> Result<()> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let sql = self.sql(
            r#"INSERT INTO action_cache (action_key, manifest, namespace, created_at)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(action_key) DO UPDATE SET
                 manifest = excluded.manifest,
                 namespace = excluded.namespace,
                 created_at = excluded.created_at"#,
        );
        sqlx::query(&sql)
            .bind(action_key)
            .bind(manifest)
            .bind(namespace)
            .bind(created_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -- Capability and admission ledger ----------------------------------

    pub async fn record_capability_intent(&self, intent: NewCapabilityIntent<'_>) -> Result<i64> {
        let created_at = chrono::Utc::now().to_rfc3339();
        if self.backend == StateBackend::Postgres {
            let row: (i64,) = sqlx::query_as(
                r#"INSERT INTO capability_intents
                   (request_id, intent_type, action_id, project_id, ref_name, target_ref, actor, status, payload, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                   RETURNING id"#,
            )
            .bind(intent.request_id)
            .bind(intent.intent_type)
            .bind(intent.action_id)
            .bind(intent.project_id)
            .bind(intent.ref_name)
            .bind(intent.target_ref)
            .bind(intent.actor)
            .bind(intent.status)
            .bind(intent.payload)
            .bind(&created_at)
            .fetch_one(&self.pool)
            .await?;
            return Ok(row.0);
        }
        let result = sqlx::query(
            r#"INSERT INTO capability_intents
               (request_id, intent_type, action_id, project_id, ref_name, target_ref, actor, status, payload, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(intent.request_id)
        .bind(intent.intent_type)
        .bind(intent.action_id)
        .bind(intent.project_id)
        .bind(intent.ref_name)
        .bind(intent.target_ref)
        .bind(intent.actor)
        .bind(intent.status)
        .bind(intent.payload)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        self.inserted_id(result).await
    }

    pub async fn approve_capability_grant(&self, grant: NewCapabilityGrant<'_>) -> Result<i64> {
        let issued_at = chrono::Utc::now().to_rfc3339();
        if self.backend == StateBackend::Postgres {
            let row: (i64,) = sqlx::query_as(
                r#"INSERT INTO capability_grants
                   (intent_id, grant_id, action_id, project_id, ref_name, new_sha, required_grant, status, issued_at, expires_at, payload)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                   RETURNING id"#,
            )
            .bind(grant.intent_id)
            .bind(grant.grant_id)
            .bind(grant.action_id)
            .bind(grant.project_id)
            .bind(grant.ref_name)
            .bind(grant.new_sha)
            .bind(grant.required_grant)
            .bind(grant.status)
            .bind(&issued_at)
            .bind(grant.expires_at)
            .bind(grant.payload)
            .fetch_one(&self.pool)
            .await?;
            return Ok(row.0);
        }
        let result = sqlx::query(
            r#"INSERT INTO capability_grants
               (intent_id, grant_id, action_id, project_id, ref_name, new_sha, required_grant, status, issued_at, expires_at, payload)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(grant.intent_id)
        .bind(grant.grant_id)
        .bind(grant.action_id)
        .bind(grant.project_id)
        .bind(grant.ref_name)
        .bind(grant.new_sha)
        .bind(grant.required_grant)
        .bind(grant.status)
        .bind(issued_at)
        .bind(grant.expires_at)
        .bind(grant.payload)
        .execute(&self.pool)
        .await?;
        self.inserted_id(result).await
    }

    pub async fn active_capability_grant_for_ref(
        &self,
        ref_name: &str,
        new_sha: Option<&str>,
    ) -> Result<Option<CapabilityGrantRecord>> {
        let now = chrono::Utc::now().to_rfc3339();
        let sql = self.sql(
            r#"SELECT * FROM capability_grants
               WHERE ref_name = ?
                 AND status = 'approved'
                 AND expires_at > ?
                 AND (new_sha IS NULL OR new_sha = ?)
               ORDER BY issued_at DESC
               LIMIT 1"#,
        );
        let grant = sqlx::query_as::<_, CapabilityGrantRecord>(&sql)
            .bind(ref_name)
            .bind(now)
            .bind(new_sha)
            .fetch_optional(&self.pool)
            .await?;
        Ok(grant)
    }

    pub async fn record_admission_decision(
        &self,
        decision: NewAdmissionDecision<'_>,
    ) -> Result<i64> {
        let created_at = chrono::Utc::now().to_rfc3339();
        if self.backend == StateBackend::Postgres {
            let row: (i64,) = sqlx::query_as(
                r#"INSERT INTO admission_decisions
                   (raw_input, verdict, actor_kind, ref_name, old_sha, new_sha, grant_id, policy_version, reasons_json, payload, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                   RETURNING id"#,
            )
            .bind(decision.raw_input)
            .bind(decision.verdict)
            .bind(decision.actor_kind)
            .bind(decision.ref_name)
            .bind(decision.old_sha)
            .bind(decision.new_sha)
            .bind(decision.grant_id)
            .bind(decision.policy_version)
            .bind(decision.reasons_json)
            .bind(decision.payload)
            .bind(&created_at)
            .fetch_one(&self.pool)
            .await?;
            return Ok(row.0);
        }
        let result = sqlx::query(
            r#"INSERT INTO admission_decisions
               (raw_input, verdict, actor_kind, ref_name, old_sha, new_sha, grant_id, policy_version, reasons_json, payload, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(decision.raw_input)
        .bind(decision.verdict)
        .bind(decision.actor_kind)
        .bind(decision.ref_name)
        .bind(decision.old_sha)
        .bind(decision.new_sha)
        .bind(decision.grant_id)
        .bind(decision.policy_version)
        .bind(decision.reasons_json)
        .bind(decision.payload)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        self.inserted_id(result).await
    }
}

pub async fn record_admission_decision_for_hook(
    raw_input: &str,
    evaluation: &crate::admission::AdmissionEvaluation,
) -> bool {
    let Ok(db) = Db::open().await else {
        return false;
    };
    let reasons_json =
        serde_json::to_string(&evaluation.reasons).unwrap_or_else(|_| "[]".to_string());
    let payload = serde_json::to_string(evaluation).unwrap_or_else(|_| "{}".to_string());
    db.record_admission_decision(NewAdmissionDecision {
        raw_input,
        verdict: evaluation.verdict.label(),
        actor_kind: &evaluation.actor_kind,
        ref_name: evaluation.ref_name.as_deref(),
        old_sha: evaluation.old_sha.as_deref(),
        new_sha: evaluation.new_sha.as_deref(),
        grant_id: evaluation.grant_id.as_deref(),
        policy_version: &evaluation.policy_version,
        reasons_json: &reasons_json,
        payload: &payload,
    })
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, sleep};

    async fn setup_db() -> Result<Db> {
        Db::open_memory().await
    }

    fn unique_suffix() -> String {
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .to_string()
    }

    async fn exercise_core_state_backend(db: &Db, suffix: &str) -> Result<()> {
        let pool_name = format!("pg-proof-{suffix}");
        let manager_id = format!("manager-{suffix}");
        let container_id = format!("container-{suffix}");
        let now = chrono::Utc::now().to_rfc3339();

        let pool = Pool {
            name: pool_name.clone(),
            gitlab_runner_id: 42,
            auth_token: "secret".into(),
            tags: "proof,postgres".into(),
            executor: "docker".into(),
            min_warm: 1,
            max_managers: 3,
            concurrent: 4,
            request_concurrency: 2,
            paused: false,
            trust_tier: "trusted".into(),
        };
        db.insert_pool(&pool).await?;
        assert!(!db.get_pool(&pool_name).await?.expect("pool").paused);
        db.update_pool_paused(&pool_name, true).await?;
        assert!(db.get_pool(&pool_name).await?.expect("pool").paused);
        db.update_pool_token(&pool_name, "rotated").await?;
        assert_eq!(
            db.get_pool(&pool_name).await?.expect("pool").auth_token,
            "rotated"
        );
        assert!(db.list_pools().await?.iter().any(|p| p.name == pool_name));

        db.insert_manager(&Manager {
            id: manager_id.clone(),
            pool_name: pool_name.clone(),
            docker_container_id: container_id,
            system_id: None,
            state: "starting".into(),
            config_dir: format!("/tmp/{suffix}"),
            started_at: Some(now.clone()),
            last_contact_at: None,
        })
        .await?;
        assert_eq!(db.count_active_managers(&pool_name).await?, 1);
        db.update_manager_state(&manager_id, "online").await?;
        db.update_manager_system_id(&manager_id, "system-proof")
            .await?;
        assert_eq!(
            db.get_manager(&manager_id)
                .await?
                .expect("manager")
                .system_id
                .as_deref(),
            Some("system-proof")
        );
        assert_eq!(db.list_managers(Some(&pool_name)).await?.len(), 1);

        db.upsert_job_event(&JobEvent {
            job_id: 9001,
            project_id: 77,
            pipeline_id: Some(7001),
            status: "pending".into(),
            job_name: Some("unit".into()),
            pool_name: Some(pool_name.clone()),
            system_id: Some("system-proof".into()),
            queued_duration: Some(1.25),
            received_at: now.clone(),
        })
        .await?;
        assert!(
            db.recent_job_events(5)
                .await?
                .iter()
                .any(|event| event.job_id == 9001)
        );

        db.upsert_ci_job_run(&CiJobRun {
            job_id: 9001,
            project_id: 77,
            pipeline_id: 7001,
            root_pipeline_id: 7001,
            pipeline_sha: "0123456789abcdef0123456789abcdef01234567".into(),
            ref_name: format!("refs/heads/agent/{suffix}"),
            job_name: "unit".into(),
            stage: "test".into(),
            status: "success".into(),
            runner: Some("runner-proof".into()),
            runner_pool: Some(pool_name.clone()),
            queued_duration_secs: Some(1.0),
            duration_secs: Some(2.0),
            started_at: Some(now.clone()),
            finished_at: Some(now.clone()),
            web_url: None,
            observed_at: now.clone(),
        })
        .await?;
        assert_eq!(db.list_ci_job_runs(77, 7001).await?.len(), 1);

        let event_id = db
            .append_event("proof", Some(77), Some(9001), "state-test", "{}")
            .await?;
        assert!(event_id > 0);
        assert!(
            db.get_events(5)
                .await?
                .iter()
                .any(|event| event.id == event_id)
        );

        let plan_id = db
            .record_test_plan(
                77, "base", "head", "targeted", 0.95, 1, 2, "state", None, "{}",
            )
            .await?;
        db.record_test_plan_item(plan_id, "state::proof", "run", "state", 0.95)
            .await?;
        db.record_selector_miss(Some(plan_id), "state::proof", "head", "oracle")
            .await?;
        assert!(
            db.count_selector_misses_since("1970-01-01T00:00:00Z")
                .await?
                >= 1
        );
        assert_eq!(db.latest_test_plan(77).await?.expect("plan").0, plan_id);

        let inputs_hash = format!("inputs-{suffix}");
        db.store_test_verdict(
            9001,
            "action-proof",
            "object-proof",
            &inputs_hash,
            "pass",
            "trusted",
            "[]",
        )
        .await?;
        assert_eq!(
            db.lookup_test_verdict(&inputs_hash)
                .await?
                .expect("verdict")
                .0,
            "pass"
        );
        assert_eq!(db.prune_test_verdicts("1970-01-01T00:00:00Z").await?, 0);
        assert_eq!(db.prune_action_cache("1970-01-01T00:00:00Z").await?, 0);

        let cache_key = format!("action-cache-{suffix}");
        let child_hash = format!("child-cache-{suffix}");
        db.upsert_action_cache(&cache_key, "{\"kind\":\"proof\"}", "trusted")
            .await?;
        db.store_test_verdict(
            9002,
            "child-action",
            &child_hash,
            &cache_key,
            "pass",
            "trusted",
            "[]",
        )
        .await?;

        let epoch_manager = crate::epoch::EpochManager::with_backend(db.pool(), db.backend());
        assert_eq!(epoch_manager.get_epoch("proof-scope").await?, 0);
        let bumped = epoch_manager
            .bump_epoch("proof-scope", 9001, "proof bump")
            .await?;
        assert_eq!(bumped, 1);

        let taint_manager = crate::taint::TaintManager::with_backend(db.pool(), db.backend());
        let store = cache_brain_adapter::SqlxActionCacheStore::boxed(
            db.pool(),
            match db.backend() {
                crate::state::StateBackend::Sqlite => cache_brain_adapter::AdapterBackend::Sqlite,
                crate::state::StateBackend::Postgres => {
                    cache_brain_adapter::AdapterBackend::Postgres
                }
            },
        );
        let cache_brain =
            crate::cache_brain::CacheBrain::with_store(epoch_manager, taint_manager.clone(), store);
        let unit = crate::cache_brain::BuildUnit {
            unit_type: crate::cache_brain::BuildUnitType::GenericStep {
                name: "proof".into(),
            },
            input_signature: cache_key.clone(),
            environment_signature: "env".into(),
            scope: "proof-scope".into(),
            trust_tier: crate::policy::TrustTier::Trusted,
        };
        assert!(matches!(
            cache_brain.plan_step(&unit).await?,
            crate::explain::CacheVerdict::HitExact
        ));
        taint_manager
            .propagate_taint(&cache_key, "proof taint", 9001)
            .await?;
        assert!(taint_manager.is_tainted(&cache_key).await?);
        assert!(taint_manager.is_tainted(&child_hash).await?);
        assert!(matches!(
            cache_brain.plan_step(&unit).await?,
            crate::explain::CacheVerdict::Denied { .. }
        ));

        db.delete_manager(&manager_id).await?;
        db.delete_pool(&pool_name).await?;
        Ok(())
    }

    #[test]
    fn postgres_bind_rewrite_skips_quoted_question_marks() {
        assert_eq!(
            postgres_bind_params("SELECT '?' AS q, col FROM t WHERE a = ? AND b = ?"),
            "SELECT '?' AS q, col FROM t WHERE a = $1 AND b = $2"
        );
        assert_eq!(
            postgres_bind_params("SELECT 'it''s ?' AS q WHERE id = ?"),
            "SELECT 'it''s ?' AS q WHERE id = $1"
        );
    }

    #[test]
    fn state_backend_detects_supported_urls() -> Result<()> {
        assert_eq!(
            StateBackend::from_url("postgres://jeryu:secret@127.0.0.1/jeryu")?,
            StateBackend::Postgres
        );
        assert_eq!(
            StateBackend::from_url("postgresql://jeryu:secret@127.0.0.1/jeryu")?,
            StateBackend::Postgres
        );
        assert_eq!(
            StateBackend::from_url("sqlite:/tmp/jeryu.db?mode=rwc")?,
            StateBackend::Sqlite
        );
        assert!(StateBackend::from_url("mysql://localhost/jeryu").is_err());
        Ok(())
    }

    #[tokio::test]
    async fn open_memory_uses_sqlite_recovery() -> Result<()> {
        let db = setup_db().await?;
        assert_eq!(db.backend(), StateBackend::Sqlite);
        Ok(())
    }

    #[tokio::test]
    async fn sqlite_migration_adds_root_pipeline_id_before_index() -> Result<()> {
        install_default_drivers();
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join(".db");
        let database_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await?;
        sqlx::query(
            r#"CREATE TABLE ci_job_runs (
                job_id                 INTEGER PRIMARY KEY,
                project_id             INTEGER NOT NULL,
                pipeline_id            INTEGER NOT NULL,
                pipeline_sha           TEXT NOT NULL,
                ref_name               TEXT NOT NULL,
                job_name               TEXT NOT NULL,
                stage                  TEXT NOT NULL,
                status                 TEXT NOT NULL,
                runner                 TEXT,
                runner_pool            TEXT,
                queued_duration_secs   REAL,
                duration_secs          REAL,
                started_at             TEXT,
                finished_at            TEXT,
                web_url                TEXT,
                observed_at            TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await?;
        pool.close().await;

        let db = Db::open_url(&database_url).await?;
        let root_column: Option<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('ci_job_runs') WHERE name = ?")
                .bind("root_pipeline_id")
                .fetch_optional(&db.pool)
                .await?;
        assert_eq!(
            root_column.as_ref().map(|row| row.0.as_str()),
            Some("root_pipeline_id")
        );

        let root_index: Option<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?")
                .bind("idx_ci_job_runs_root_pipeline")
                .fetch_optional(&db.pool)
                .await?;
        assert_eq!(
            root_index.as_ref().map(|row| row.0.as_str()),
            Some("idx_ci_job_runs_root_pipeline")
        );
        Ok(())
    }

    #[tokio::test]
    async fn sqlite_core_state_backend_smoke() -> Result<()> {
        let db = setup_db().await?;
        exercise_core_state_backend(&db, &unique_suffix()).await
    }

    #[tokio::test]
    async fn postgres_backend_smoke_test_when_configured() -> Result<()> {
        let Some(db) = Db::open_test_postgres().await? else {
            return Ok(());
        };

        assert_eq!(db.backend(), StateBackend::Postgres);
        let suffix = unique_suffix();
        exercise_core_state_backend(&db, &suffix).await?;
        let request_id = format!("req-pg-{suffix}");
        let grant_id = format!("grant-pg-{suffix}");
        let payload = "{}";
        let intent_id = db
            .record_capability_intent(NewCapabilityIntent {
                request_id: &request_id,
                intent_type: "ProposePatch",
                action_id: "propose_patch",
                project_id: Some(77),
                ref_name: Some("refs/heads/agent/postgres-smoke"),
                target_ref: Some("main"),
                actor: "postgres-smoke-test",
                status: "executed",
                payload,
            })
            .await?;
        assert!(intent_id > 0);

        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        let grant_row = db
            .approve_capability_grant(NewCapabilityGrant {
                intent_id,
                grant_id: &grant_id,
                action_id: "propose_patch",
                project_id: Some(77),
                ref_name: "refs/heads/agent/postgres-smoke",
                new_sha: Some("0123456789abcdef0123456789abcdef01234567"),
                required_grant: "agent_task",
                status: "approved",
                expires_at: &expires_at,
                payload,
            })
            .await?;
        assert!(grant_row > 0);

        let decision_id = db
            .record_admission_decision(NewAdmissionDecision {
                raw_input: "0000000000000000000000000000000000000000 0123456789abcdef0123456789abcdef01234567 refs/heads/agent/postgres-smoke",
                verdict: "allow",
                actor_kind: "agent",
                ref_name: Some("refs/heads/agent/postgres-smoke"),
                old_sha: Some("0000000000000000000000000000000000000000"),
                new_sha: Some("0123456789abcdef0123456789abcdef01234567"),
                grant_id: Some(&grant_id),
                policy_version: "test",
                reasons_json: "[]",
                payload,
            })
            .await?;
        assert!(decision_id > 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_crud() -> Result<()> {
        let db = setup_db().await?;

        let p1 = Pool {
            name: "test_pool".into(),
            gitlab_runner_id: 1,
            auth_token: "secret".into(),
            tags: "tag1".into(),
            executor: "docker".into(),
            min_warm: 2,
            max_managers: 10,
            concurrent: 5,
            request_concurrency: 2,
            paused: false,
            trust_tier: "trusted".into(),
        };

        db.insert_pool(&p1).await?;

        let pools = db.list_pools().await?;
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0].name, "test_pool");
        assert_eq!(pools[0].min_warm, 2);

        db.update_pool_paused("test_pool", true).await?;
        let p = db.get_pool("test_pool").await?.unwrap();
        assert!(p.paused);

        db.update_pool_token("test_pool", "new_secret").await?;
        let p = db.get_pool("test_pool").await?.unwrap();
        assert_eq!(p.auth_token, "new_secret");

        Ok(())
    }

    #[tokio::test]
    async fn capability_grant_lookup_respects_expiry_and_sha() -> Result<()> {
        let db = setup_db().await?;
        let payload = "{}";
        let intent_id = db
            .record_capability_intent(NewCapabilityIntent {
                request_id: "req-state-ledger",
                intent_type: "ProposePatch",
                action_id: "propose_patch",
                project_id: Some(42),
                ref_name: Some("refs/heads/agent/demo"),
                target_ref: Some("main"),
                actor: "capability-api",
                status: "executed",
                payload,
            })
            .await?;
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        db.approve_capability_grant(NewCapabilityGrant {
            intent_id,
            grant_id: "grant-state-ledger",
            action_id: "propose_patch",
            project_id: Some(42),
            ref_name: "refs/heads/agent/demo",
            new_sha: Some("0123456789abcdef0123456789abcdef01234567"),
            required_grant: "agent_task",
            status: "approved",
            expires_at: &expires_at,
            payload,
        })
        .await?;

        let exact = db
            .active_capability_grant_for_ref(
                "refs/heads/agent/demo",
                Some("0123456789abcdef0123456789abcdef01234567"),
            )
            .await?;
        assert_eq!(
            exact.as_ref().map(|grant| grant.grant_id.as_str()),
            Some("grant-state-ledger")
        );

        let wrong_sha = db
            .active_capability_grant_for_ref(
                "refs/heads/agent/demo",
                Some("fedcba9876543210fedcba9876543210fedcba98"),
            )
            .await?;
        assert!(wrong_sha.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_manager_crud() -> Result<()> {
        let db = setup_db().await?;

        let p1 = Pool {
            name: "test_pool".into(),
            gitlab_runner_id: 1,
            auth_token: "secret".into(),
            tags: "tag1".into(),
            executor: "docker".into(),
            min_warm: 2,
            max_managers: 10,
            concurrent: 5,
            request_concurrency: 2,
            paused: false,
            trust_tier: "trusted".into(),
        };
        db.insert_pool(&p1).await?;

        let m1 = Manager {
            id: "uuid-1".into(),
            pool_name: "test_pool".into(),
            docker_container_id: "def456".into(),
            system_id: None,
            state: "starting".into(),
            config_dir: "/tmp/uuid-1".into(),
            started_at: Some("2024-01-01T00:00:00Z".into()),
            last_contact_at: None,
        };
        db.insert_manager(&m1).await?;

        assert_eq!(db.count_active_managers("test_pool").await?, 1);

        db.update_manager_system_id("uuid-1", "sys-uuid").await?;
        db.update_manager_state("uuid-1", "online").await?;

        let fetched = db.get_manager("uuid-1").await?.unwrap();
        assert_eq!(fetched.system_id.unwrap(), "sys-uuid");
        assert_eq!(fetched.state, "online");
        assert_eq!(db.count_active_managers("test_pool").await?, 1);

        db.update_manager_state("uuid-1", "stopped").await?;
        assert_eq!(db.count_active_managers("test_pool").await?, 0);

        db.delete_manager("uuid-1").await?;
        let fetched = db.get_manager("uuid-1").await?;
        assert!(fetched.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_bulk_job_events() -> Result<()> {
        let db = setup_db().await?;

        for i in 1..=50 {
            let e = JobEvent {
                job_id: i,
                project_id: 100,
                pipeline_id: Some(10),
                status: "pending".into(),
                job_name: None,
                pool_name: None,
                system_id: None,
                queued_duration: None,
                received_at: format!("2024-01-01T00:00:{:02}Z", i % 60),
            };
            db.upsert_job_event(&e).await?;
        }

        let events = db.recent_job_events(10).await?;
        assert_eq!(events.len(), 10);

        // Upsert to modify status
        let e_mod = JobEvent {
            job_id: 1,
            project_id: 100,
            pipeline_id: Some(10),
            status: "running".into(), // Same id, new status -> should insert a row because PK is (job_id, status)
            job_name: None,
            pool_name: Some("test_pool".into()),
            system_id: Some("sys-1".into()),
            queued_duration: Some(1.5),
            received_at: "2024-01-01T00:01:00Z".into(),
        };
        db.upsert_job_event(&e_mod).await?;

        // 50 pending + 1 running = 51 total rows, but we only have a limited check.

        Ok(())
    }

    #[tokio::test]
    async fn latest_job_status_counts_exclude_superseded_events() -> Result<()> {
        let db = setup_db().await?;

        db.upsert_job_event(&JobEvent {
            job_id: 1,
            project_id: 100,
            pipeline_id: Some(10),
            status: "pending".into(),
            job_name: None,
            pool_name: None,
            system_id: None,
            queued_duration: None,
            received_at: "2024-01-01T00:00:00Z".into(),
        })
        .await?;
        db.upsert_job_event(&JobEvent {
            job_id: 1,
            project_id: 100,
            pipeline_id: Some(10),
            status: "running".into(),
            job_name: None,
            pool_name: Some("build".into()),
            system_id: Some("sys-1".into()),
            queued_duration: None,
            received_at: "2024-01-01T00:01:00Z".into(),
        })
        .await?;
        db.upsert_job_event(&JobEvent {
            job_id: 2,
            project_id: 100,
            pipeline_id: Some(10),
            status: "created".into(),
            job_name: None,
            pool_name: None,
            system_id: None,
            queued_duration: None,
            received_at: "2024-01-01T00:02:00Z".into(),
        })
        .await?;
        db.upsert_job_event(&JobEvent {
            job_id: 3,
            project_id: 100,
            pipeline_id: Some(10),
            status: "pending".into(),
            job_name: None,
            pool_name: None,
            system_id: None,
            queued_duration: None,
            received_at: "2024-01-01T00:03:00Z".into(),
        })
        .await?;
        db.upsert_job_event(&JobEvent {
            job_id: 3,
            project_id: 100,
            pipeline_id: Some(10),
            status: "failed".into(),
            job_name: None,
            pool_name: None,
            system_id: None,
            queued_duration: None,
            received_at: "2024-01-01T00:04:00Z".into(),
        })
        .await?;

        assert_eq!(db.count_queued_jobs().await?, 1);
        assert_eq!(db.count_running_jobs().await?, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_ci_job_runs_feed_bottlenecks() -> Result<()> {
        let db = setup_db().await?;
        let observed_at = "2026-04-23T00:00:00Z".to_string();
        let root_run = CiJobRun {
            job_id: 42,
            project_id: 2,
            pipeline_id: 433,
            root_pipeline_id: 433,
            pipeline_sha: "abc123".to_string(),
            ref_name: "main".to_string(),
            job_name: "compile-workspace".to_string(),
            stage: "compile".to_string(),
            status: "success".to_string(),
            runner: Some("jeryu-build".to_string()),
            runner_pool: Some("build".to_string()),
            queued_duration_secs: Some(1.0),
            duration_secs: Some(12.5),
            started_at: Some(observed_at.clone()),
            finished_at: Some("2026-04-23T00:00:13Z".to_string()),
            web_url: Some("http://localhost/root/dougx/-/jobs/42".to_string()),
            observed_at,
        };
        db.upsert_ci_job_run(&root_run).await?;
        db.upsert_ci_job_run(&CiJobRun {
            job_id: 43,
            project_id: 2,
            pipeline_id: 434,
            root_pipeline_id: 433,
            pipeline_sha: "abc123".to_string(),
            ref_name: "main".to_string(),
            job_name: "test-rust-nextest-1".to_string(),
            stage: "test".to_string(),
            status: "success".to_string(),
            runner: Some("jeryu-build".to_string()),
            runner_pool: Some("build".to_string()),
            queued_duration_secs: Some(0.5),
            duration_secs: Some(9.0),
            started_at: Some("2026-04-23T00:00:10Z".to_string()),
            finished_at: Some("2026-04-23T00:00:19Z".to_string()),
            web_url: Some("http://localhost/root/dougx/-/jobs/43".to_string()),
            observed_at: "2026-04-23T00:00:10Z".to_string(),
        })
        .await?;

        let runs = db.list_ci_job_runs(2, 433).await?;
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].job_name, "compile-workspace");
        assert_eq!(runs[0].root_pipeline_id, 433);
        assert_eq!(runs[1].job_name, "test-rust-nextest-1");
        assert_eq!(runs[1].root_pipeline_id, 433);

        let bottlenecks = db.ci_job_bottlenecks(2, Some("main"), 10).await?;
        assert_eq!(bottlenecks.len(), 2);
        assert_eq!(bottlenecks[0].job_name, "compile-workspace");
        assert_eq!(bottlenecks[0].runs, 1);
        assert_eq!(bottlenecks[0].latest_duration_secs, Some(12.5));

        Ok(())
    }

    #[tokio::test]
    async fn test_pipeline_tracking_and_retry_records() -> Result<()> {
        let db = setup_db().await?;

        db.upsert_tracked_pipeline(&TrackedPipeline {
            pipeline_id: 10,
            project_id: 42,
            ref_name: "main".into(),
            sha: "abc".into(),
            status: "running".into(),
            updated_at: "2024-01-01T00:00:00Z".into(),
        })
        .await?;

        let active = db.list_active_pipelines_for_ref(42, "main").await?;
        assert_eq!(active.len(), 1);

        db.insert_recovery_decision(42, 99, "abc", "main", "retry_once", "transient network")
            .await?;
        assert_eq!(db.count_recovery_decisions(42, 99).await?, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_release_attempt_lifecycle() -> Result<()> {
        let db = setup_db().await?;

        db.upsert_release_attempt(
            42,
            "main",
            "abc123",
            "ci-abc123",
            Some(88),
            "success",
            "pending",
        )
        .await?;

        let attempt = db
            .get_release_attempt(42, "main", "abc123")
            .await?
            .expect("release attempt should exist");
        assert_eq!(attempt.version, "ci-abc123");
        assert_eq!(attempt.upstream_pipeline_id, Some(88));
        assert_eq!(attempt.canary_status, "pending");

        let claimed = db
            .claim_release_canary(42, "main", "abc123", "ci-abc123", Some(88))
            .await?;
        assert!(claimed);

        let attempt = db
            .get_release_attempt(42, "main", "abc123")
            .await?
            .expect("release attempt should exist");
        assert_eq!(attempt.canary_status, "running");
        assert!(attempt.canary_started_at.is_some());

        db.finish_release_canary(42, "main", "abc123", "passed", Some("ok"))
            .await?;
        let attempt = db
            .get_release_attempt(42, "main", "abc123")
            .await?
            .expect("release attempt should exist");
        assert_eq!(attempt.canary_status, "passed");
        assert_eq!(attempt.canary_note.as_deref(), Some("ok"));
        Ok(())
    }

    #[tokio::test]
    async fn test_release_pipeline_tracking() -> Result<()> {
        let db = setup_db().await?;

        db.upsert_release_attempt(
            42,
            "main",
            "abc123",
            "ci-abc123",
            Some(88),
            "success",
            "pending",
        )
        .await?;
        db.attach_release_pipeline(42, "main", "abc123", 99, "pending")
            .await?;

        let attempt = db
            .release_attempt_by_release_pipeline_id(99)
            .await?
            .expect("release pipeline should be tracked");
        assert_eq!(attempt.release_pipeline_status.as_deref(), Some("pending"));
        assert_eq!(attempt.canary_status, "running");

        let attempt = db
            .update_release_pipeline_status(99, "success")
            .await?
            .expect("release pipeline should still be tracked");
        assert_eq!(attempt.release_pipeline_status.as_deref(), Some("success"));
        assert_eq!(attempt.canary_status, "running");
        assert!(attempt.canary_finished_at.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn test_production_pipeline_tracking() -> Result<()> {
        let db = setup_db().await?;

        db.upsert_release_attempt(
            42,
            "main",
            "abc123",
            "ci-abc123",
            Some(88),
            "success",
            "passed",
        )
        .await?;
        db.attach_production_pipeline(42, "main", "abc123", 199, "created")
            .await?;

        let attempt = db
            .release_attempt_by_production_pipeline_id(199)
            .await?
            .expect("production pipeline should be tracked");
        assert_eq!(
            attempt.production_pipeline_status.as_deref(),
            Some("created")
        );

        let attempt = db
            .update_production_pipeline_status(199, "success")
            .await?
            .expect("production pipeline should still be tracked");
        assert_eq!(
            attempt.production_pipeline_status.as_deref(),
            Some("success")
        );
        assert_eq!(attempt.canary_status, "passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_recent_release_attempts_ordering() -> Result<()> {
        let db = setup_db().await?;

        db.upsert_release_attempt(
            1,
            "main",
            "sha-1",
            "ci-sha-1",
            Some(11),
            "success",
            "failed",
        )
        .await?;
        sleep(Duration::from_millis(5)).await;
        db.upsert_release_attempt(
            1,
            "main",
            "sha-2",
            "ci-sha-2",
            Some(12),
            "success",
            "passed",
        )
        .await?;

        let attempts = db
            .recent_release_attempts(Some(1), Some("main"), 10)
            .await?;
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].sha, "sha-2");
        assert_eq!(attempts[1].sha, "sha-1");

        let latest = db
            .latest_release_attempt_any()
            .await?
            .expect("latest release attempt");
        assert_eq!(latest.sha, "sha-2");
        Ok(())
    }

    #[tokio::test]
    async fn test_cache_metrics_and_pruning() -> Result<()> {
        let db = setup_db().await?;

        // Initially zero
        let m0 = db.get_cache_metrics().await?;
        assert_eq!(m0.total_requests, 0);

        // Record a few requests
        db.record_cache_request(
            "crates.io:443",
            "CONNECT",
            false,
            "intercepted_passthrough",
            1024,
        )
        .await?;
        db.record_cache_request("registry.npmjs.org:443", "GET", true, "hit", 2048)
            .await?;

        let m1 = db.get_cache_metrics().await?;
        assert_eq!(m1.total_requests, 2);
        assert_eq!(m1.hit_count, 1);
        assert_eq!(m1.miss_count, 1);
        assert_eq!(m1.bytes_served, 3072);
        assert_eq!(m1.hit_ratio, 50.0);

        // Test prune_cache_requests
        let pruned = db.prune_cache_requests(7).await?;
        assert_eq!(pruned, 0);

        // All pruned if days=-1 (future)
        let pruned_all = db.prune_cache_requests(-1).await?;
        assert_eq!(pruned_all, 2);

        let m2 = db.get_cache_metrics().await?;
        assert_eq!(m2.total_requests, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_secret_authority_and_release_secret_set_crud() -> Result<()> {
        let db = setup_db().await?;
        db.upsert_secret_authority(&SecretAuthority {
            name: "local-vault".into(),
            kind: "vault".into(),
            address: "http://127.0.0.1:18200".into(),
            status: "ready".into(),
            mount: "secret".into(),
            prefix: "veox".into(),
            token_fingerprint: "abc123...7890".into(),
            metadata_path: "/tmp/vault.env".into(),
            created_at: "2026-04-20T00:00:00Z".into(),
            updated_at: "2026-04-20T00:00:00Z".into(),
        })
        .await?;

        let authority = db
            .get_secret_authority("local-vault")
            .await?
            .expect("secret authority");
        assert_eq!(authority.address, "http://127.0.0.1:18200");

        db.upsert_release_secret_set(&ReleaseSecretSet {
            repo_name: "dougx".into(),
            version: "ci-abcdef123456".into(),
            target: "canary".into(),
            authority_name: "local-vault".into(),
            status: "rotated".into(),
            rendered_deploy_env_path: "/tmp/deploy.env".into(),
            rendered_runtime_env_path: "/tmp/runtime.env".into(),
            audit_path: "/tmp/audit.json".into(),
            bundle_path: Some("/tmp/release-secrets.enc".into()),
            report_path: Some("/tmp/release-handoff.pdf".into()),
            runtime_secret_vault_path: Some(
                "secret/veox/releases/ci-abcdef123456/runtime-secrets".into(),
            ),
            recovery_password_vault_path: Some(
                "secret/veox/releases/ci-abcdef123456/recovery-password".into(),
            ),
            expires_at: Some("2026-04-21T00:00:00Z".into()),
            rotated_at: "2026-04-20T00:00:00Z".into(),
            finalized_at: None,
            updated_at: "2026-04-20T00:00:00Z".into(),
        })
        .await?;

        let secret_set = db
            .get_release_secret_set("dougx", "ci-abcdef123456", "canary")
            .await?
            .expect("release secret set");
        assert_eq!(secret_set.status, "rotated");

        db.mark_release_secret_set_finalized(
            "dougx",
            "ci-abcdef123456",
            "canary",
            "2026-04-20T01:00:00Z",
        )
        .await?;

        let latest = db
            .latest_release_secret_set("dougx")
            .await?
            .expect("latest release secret set");
        assert_eq!(latest.status, "finalized");
        assert_eq!(latest.finalized_at.as_deref(), Some("2026-04-20T01:00:00Z"));
        Ok(())
    }

    #[tokio::test]
    async fn test_secret_audit_events_are_recorded() -> Result<()> {
        let db = setup_db().await?;
        db.insert_secret_audit_event(&SecretAuditEvent {
            id: None,
            repo_name: "dougx".into(),
            version: "ci-abcdef123456".into(),
            target: "canary".into(),
            action: "rotate".into(),
            status: "ok".into(),
            detail: "rotated runtime secret set".into(),
            created_at: "2026-04-20T00:00:00Z".into(),
        })
        .await?;
        let events = db.recent_secret_audit_events("dougx", 5).await?;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "rotate");
        Ok(())
    }

    #[tokio::test]
    async fn test_verdict_cache_round_trip() -> Result<()> {
        let db = setup_db().await?;

        // Store a verdict
        db.store_test_verdict(
            42, // job_id
            "cargo nextest run -E 'test(/pool/)'",
            "obj_hash_abc",
            "inputs_hash_xyz",
            "pass",
            "trusted",
            "unit test passed in 3.2s",
        )
        .await?;

        // Lookup by inputs hash
        let result = db.lookup_test_verdict("inputs_hash_xyz").await?;
        assert!(result.is_some());
        let (verdict, action_key, _created_at) = result.unwrap();
        assert_eq!(verdict, "pass");
        assert_eq!(action_key, "cargo nextest run -E 'test(/pool/)'");

        // Lookup non-existent
        let missing = db.lookup_test_verdict("nonexistent").await?;
        assert!(missing.is_none());

        // Prune with a future date should remove the record
        let pruned = db.prune_test_verdicts("2099-01-01T00:00:00Z").await?;
        assert_eq!(pruned, 1);

        // Confirm it's gone
        let after_prune = db.lookup_test_verdict("inputs_hash_xyz").await?;
        assert!(after_prune.is_none());

        Ok(())
    }
}
