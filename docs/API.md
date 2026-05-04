# API.md — JeRyu / jeryu Complete Agent and Control Surface Reference

> Version: 5.0.1
> Last updated: 2026-05-04
> Verified against: `src/cli.rs`, `src/dispatch.rs`, `src/engine.rs`, `src/capability.rs`, `src/state.rs`, `src/release.rs`, `src/gitlab_client.rs`, `src/settings.rs`, `src/tui/action_registry.rs`
> Audience: External agents, reviewers, and contributors requiring complete API context.

This file is the single technical reference for every public, internal, and agent-facing control surface exposed by JeRyu's `jeryu` binary. It covers the CLI, hidden hook commands, custom executor commands, the Unix-socket capability API, webhook HTTP API, GitLab client wrapper, state API, release controls, test-intelligence controls, secrets controls, action registry, settings API, and operational hooks available to humans or agents.

The repository root is `/home/ubuntu/JeRyu`. The binary is the Rust crate/package `jeryu`.

---

## 1. API Surface Summary

| Surface | Transport | Primary consumer | Entry point |
| --- | --- | --- | --- |
| CLI | Process invocation | humans, agents, shell scripts | `src/cli.rs`, `src/dispatch.rs` |
| TUI | Terminal UI | humans, supervising agents | `jeryu tui`, `src/tui/` |
| Webhook server | HTTP | GitLab webhooks | `src/engine.rs` |
| Capability API | Unix domain socket JSON | local supervised agents | `src/capability.rs` |
| MCP adapter | Stdio JSON-RPC + Streamable HTTP | external coding agents | `src/mcp.rs` |
| Git server hook | stdin/stdout process hook | GitLab Gitaly/server hook | `jeryu server-hook pre-receive` |
| Custom executor | GitLab Runner custom executor protocol | GitLab Runner | `jeryu exec ...` |
| Action registry | Static registry, CLI list, TUI palette | agents, TUI, CLI | `src/tui/action_registry.rs` |
| State API | Postgres-primary/sqlx methods with SQLite fallback | internal modules and tests | `src/state.rs` |
| Settings API | JSON file + singleton | all modules | `src/settings.rs` |
| GitLab REST client | reqwest HTTP wrapper | internal modules | `src/gitlab_client.rs` |
| Release API | CLI + internal orchestration | engine, humans, agents | `src/release.rs` |
| Secrets API | CLI + Vault HTTP | release operations | `src/secrets.rs` |

---

## 2. Constants, Defaults, and Ports

| Constant | Default Value | Meaning |
| --- | --- | --- |
| `GITLAB_IMAGE` | `gitlab/gitlab-ce:17.9.2-ce.0` | GitLab CE Docker image |
| `GITLAB_RUNNER_IMAGE` | `gitlab/gitlab-runner:v17.9.2` | Runner manager image |
| `GITLAB_HOSTNAME` | `gitlab.local` | Internal GitLab hostname |
| `GITLAB_HTTP_PORT` | `8929` | Host HTTP port for GitLab |
| `GITLAB_SSH_PORT` | `2224` | Host SSH port for GitLab |
| `WEBHOOK_LISTEN_PORT` | `9777` | Engine HTTP port (bind: `127.0.0.1:9777`) |
| `VAULT_IMAGE` | `hashicorp/vault:1.17.5` | Vault image |
| `VAULT_CONTAINER_NAME` | `jeryu-vault` | Vault container |
| `VAULT_HTTP_PORT` | `18200` | Host Vault port |
| `POSTGRES_IMAGE` | `postgres:16-alpine` | State database image |
| `POSTGRES_PORT` | `15432` | Host Postgres port bound to `127.0.0.1` |
| `VAULT_DEFAULT_MOUNT` | `secret` | Vault KV mount |
| `VAULT_DEFAULT_PREFIX` | `veox` | Vault path prefix |
| `CACHE_PROXY_PORT` | `19800` | SmartCache proxy/gateway port |
| `CACHE_REGISTRY_PORT` | `19801` | Local OCI registry mirror port |
| `DEFAULT_RELEASE_PROJECT_ID` | `2` | Default release project |
| Default release repo root | `/home/ubuntu/dougx` | Overridable via `JERYU_RELEASE_REPO_ROOT` or `settings.release.repo_root` |

All constants are configurable through `~/.jeryu/settings.json` — see the Settings API section.

Configuration paths are rooted under `crate::config::data_dir()`:
- `jeryu.env`: primary jeryu environment/secrets file
- `postgres/`: Postgres data directory for the bootstrap-managed state database
- `jeryu.db`: SQLite fallback state database when `JERYU_DATABASE_URL` is unset
- `runners/`: runner manager config directories
- `cache/`: manager cache, crate cache, CAS, and SmartCache state
- Vault config/storage/env/bootstrap files

---

## 3. Default Runner Pools

| Pool | Tags | Executor | min_warm | max_managers | Trust tier |
| --- | --- | --- | ---: | ---: | --- |
| `default` | `default,rust,test` | `docker` | 2 | 4 | `trusted` |
| `build` | `build,docker-build,x86-64,docker,dind` | `docker` | 2 | 4 | `privileged` |
| `untrusted` | `untrusted,sandbox,mr` | `custom` | 1 | 2 | `untrusted` |

Agents should prefer tags inferred by `jeryu test plan` or VTI planners instead of hardcoding runner tags.

---

## 4. CLI Reference

Clap subcommands are kebab-case on the command line. Hidden/internal commands are documented because they are part of the agent/hook/control surface.

### 4.1 Lifecycle

#### `jeryu init`

Alias: hidden `jeryu bootstrap`. Bootstraps the full local control plane (secrets → compose → GitLab → DB → pools → smoke).

#### `jeryu serve`

Starts the operational daemon (loads client → state DB → Docker → compose → hook → cache → pools → engine → shadow → Ctrl-C).

#### `jeryu down`

Drains every pool and stops GitLab via Docker Compose.

#### `jeryu status`

Shows GitLab readiness, Vault status, pool state, managed Docker containers, recent job events, latest release, and latest release secret set.

#### `jeryu tui [--once] [--capture --tab <name> --output <png>]`

Launches the Ratatui dashboard. `--once` renders a single frame with optional GitLab auth and exits. `--capture` renders a deterministic Ratatui frame to a PNG file without entering an interactive terminal; accepted tabs are `mission`, `release`, `jobs`, `agents`, `tests`, `pools`, `cache`, `evidence`, and `secrets`. See `docs/JERYU_TUI.md`.

### 4.2 Install and Remote Provisioning

#### `jeryu install [--color auto|always|never] [--interactive auto|always|never] [--path-mode advise|update|skip] [--verbose]`

Guided local installer for Linux and macOS. Installs the running `jeryu` binary into `~/.jeryu/bin/jeryu` by default, renders a step-by-step plan, prompts before mutation unless `--yes`, verifies `jeryu --version`, and prints shell-specific PATH advice when the prefix is not already on `PATH`.

#### `jeryu install server [--install-deps --allow-sudo]`

Server bootstrap path. Verifies Docker first, then runs `jeryu init`. On Linux, Docker package installation is only attempted when `--install-deps --allow-sudo` is present. On macOS, missing Docker is explained rather than auto-installed.

#### `jeryu remote install <target> [--alias <alias>] [--setup-key] [--service-mode auto|user|manual] [--verbose]`

Guided SSH provisioning for a remote host. The install plan covers local SSH prerequisites, remote OS and Docker/systemd probes, binary upload, remote `--version` verification, service setup, and metadata persistence. `--dry-run --json` emits the full plan without any network mutation.

### 4.3 Pool Management

#### `jeryu pool list`

Prints pools with paused state, executor, min warm count, active managers, max managers, and runner id.

#### `jeryu pool scale <name> <count>`

Scales a pool exactly to `count` manager containers.

#### `jeryu pool pause <name>`

Pauses a runner pool in GitLab and local DB.

#### `jeryu pool resume <name>`

Resumes a paused runner pool.

#### `jeryu pool drain <name>`

Gracefully drains a pool.

#### `jeryu pool delete <name>`

Drains, deletes local pool record, unregisters GitLab runner.

#### `jeryu pool rotate-token <name>`

Resets GitLab runner auth token, updates DB, rolls manager configuration.

### 4.4 Job Management

#### `jeryu job list <project_id> [--status running,pending]`

Lists jobs from GitLab for comma-separated scopes.

#### `jeryu job trace <project_id> <job_id>`

Fetches and prints GitLab job trace output.

#### `jeryu job play <project_id> <job_id>`

Starts a manual job.

#### `jeryu job cancel <project_id> <job_id>`

Cancels a job.

#### `jeryu job retry <project_id> <job_id>`

Retries a failed job.

#### `jeryu job explain <project_id> <job_id>`

Reads the latest structured evidence capsule for the job from the state database and prints: pipeline id, stage, ref, commit, failure kind, classification, retry advice, latest retry record, log snippet.

#### `jeryu job clear`

Clears local job and pipeline history from the state database.

### 4.5 Pipeline Inspection and Control

#### `jeryu pipeline explain <pipeline_id> [--project-id 2] [--json]`

Builds a blocking/non-blocking release eligibility report for a pipeline. Groups jobs into release-critical, extended, research, and release-execution categories.

#### `jeryu pipeline doctor <pipeline_id> [--project-id 2] [--json]`

Diagnoses active jobs, runner assignment, stale trace symptoms, and likely pipeline health issues.

#### `jeryu pipeline jobs <pipeline_id> [--project-id 2] [--ingest] [--json]`

Lists all GitLab jobs for a pipeline with timing fields. With `--ingest`, stores job timing rows into `ci_job_runs`.

#### `jeryu pipeline ingest <pipeline_id> [--project-id 2]`

Fetches and stores all current GitLab job timings for the pipeline.

#### `jeryu pipeline cancel <pipeline_id> [--project-id 2]`

Cancels a pipeline in GitLab.

#### `jeryu pipeline bottlenecks [--project-id 2] [--ref-name <ref>] [--limit 25] [--json]`

Reports historical slow CI jobs from `ci_job_runs`.

### 4.6 Cache Management

#### `jeryu cache enable`

Configures Docker for the SmartCache registry mirror and local cache services.

#### `jeryu cache doctor`

Health-checks proxy, registry, Docker mirror, and cache prerequisites.

#### `jeryu cache status [--json]`

Shows cache objects, hot bandwidth, hit/miss counts, proxy/registry state, manager cache state, Cargo target/sccache usage, and disk usage. JSON output also includes local and pool Cargo cache byte totals plus per-target cache entries, including nested nextest extract scratch trees that still live under a target cache.

#### `jeryu cache gc [--dry-run] [--json] [--keep-active-managers[=true|false]] [--older-than <age>] [--max-cache-gb <gb>]`

Runs cache garbage collection. Options: `--dry-run` (preview), `--keep-active-managers=false` (allow active cache eviction), `--older-than 12h|2d` (age threshold), `--max-cache-gb` (budget forcing). Active Cargo target leases are preserved, pool Cargo caches stay inside their pool namespace, and stale nested `target/nextest/extract/*` scratch trees can now be reclaimed without manual path deletion.

### 4.6 Local Cargo Wrappers

#### `jeryu local cargo --repo <path> -- <cargo args...>`

Runs Cargo from the given repository root with jeryu-owned local cache roots. Target reuse lives under `~/.jeryu/cache/local-cargo/targets/<repo-key>/<rustc-key>/<host-triple>/target` and sccache lives under `~/.jeryu/cache/local-cargo/sccache`. `CARGO_INCREMENTAL` defaults to `0`; set `JERYU_CARGO_INCREMENTAL` to override it for the local wrapper.

#### `jeryu local cargo-env --repo <path> [--json]`

Prints the computed Cargo environment for the repository. Shell output is a list of `export ...` lines; `--json` prints the full layout object. The environment includes `CARGO_TARGET_DIR`, `SCCACHE_DIR`, `RUSTC_WRAPPER=sccache`, and the jeryu cache metadata keys.

Runner jobs also honor `JERYU_CARGO_CACHE=0` to skip target-dir injection and `JERYU_CARGO_TARGET_ISOLATE=job` to append job identity to the target root when a pool needs extra separation.

### 4.7 Manager Logs

#### `jeryu logs <manager_id> [-n|--lines 50]`

Tails Docker logs for a runner manager container.

### 4.8 Autonomous Agent Operations

#### `jeryu agent spawn <project_id> --task <text>`

Creates an autonomous agent task: creates branch, creates/updates task artifacts, opens GitLab issue/MR, returns project+branch+issue+task.

#### `jeryu agent list <project_id>`

Lists active agent MRs/issues by labels and titles.

#### `jeryu agent merge <project_id> <mr_iid> [--trust-tier trusted|untrusted|privileged]`

Runs the risk gate before accepting a merge request.

### 4.9 Shadow Sync

#### `jeryu shadow add --source <dir> --project-id <id> --branch <branch> [--enable]`

Registers a local source directory for periodic shadow sync.

#### `jeryu shadow enable --source <dir>`

Enables an existing shadow sync config.

#### `jeryu shadow disable --source <dir>`

Disables an existing shadow sync config.

#### `jeryu shadow remove --source <dir>`

Deletes shadow sync configuration.

#### `jeryu shadow sync-now --source <dir>`

Triggers an immediate sync by requesting it through the DB (`request_shadow_sync`). The shadow worker picks it up within ~2 seconds.

#### `jeryu shadow status [--source <dir>]`

Shows one shadow sync config or all configs.

### 4.9 Shadow Remote

#### `jeryu shadow-remote status [--repo <path>] [--name shadow]`

Shows current git remotes and whether the target shadow remote exists.

#### `jeryu shadow-remote ensure [--repo <path>] [--name shadow] --url <url>`

Creates or updates a repo-local remote.

#### `jeryu shadow-remote push [--repo <path>] [--name shadow] [--branch <branch>] [--mirror]`

Pushes current HEAD or mirrors the repo to the shadow remote.

### 4.10 Test Runner and VTI Controls

#### `jeryu test run --command <cmd> [--project-id 2] [--image rust:1.92.0] [--tags a,b] [--timeout 600] [--force]`

Runs one test command through an ephemeral GitLab CI branch and dynamic `.gitlab-ci.yml`. Infers tags/risk/timeout unless tags are supplied. Checks local test cache unless `--force`. Creates scratch branch, commits CI config, waits, prints result, records execution, cleans up.

#### `jeryu test plan --command <cmd> [--project-id 2] [--image rust:1.92.0] [--tags a,b] [--timeout 600]`

Prints inferred risk class, tags, timeout, and rationale without running.

#### `jeryu test batch --command <cmd> ... [--max-parallel 3] [--force]`

Runs multiple test commands in parallel through separate CI pipelines.

#### `jeryu test results <pipeline_id> [--project-id 2]`

Shows pass/fail/skipped/running summary for a pipeline's jobs.

#### `jeryu test retry <pipeline_id> <job_name> [--project-id 2]`

Retries a failed job selected by name.

#### `jeryu test failed <pipeline_id> [--project-id 2]`

Prints only failed jobs and recent trace tails.

#### `jeryu test impact --base <ref> --head <ref> [--repo-root /home/ubuntu/dougx] [--json]`

Delegates to `veox-testctl ci-impact` in the target repo and prints release-impacting/full-build/jobs/rules.

#### `jeryu test select [--base origin/main] [--head HEAD] [--repo-root <path>] [--explain] [--json] [--emit-gitlab <path>] [--emit-plan <path>]`

Runs built-in VTI smart test selection from changed files. Can emit a GitLab child pipeline YAML and JSON plan.

#### `jeryu test explain-plan <plan_path>`

Reads a JSON VTI plan and prints a human explanation.

#### `jeryu test select-external --workspace <path> [--base origin/main] [--head HEAD] [--explain] [--json] [--emit-gitlab <path>] [--emit-plan <path>] [--emit-skipped <path>]`

Loads external `.jeryu/testmap.toml`, computes selected/skipped CI job plan, can emit child pipeline YAML, plan JSON, and skipped-job metadata.

#### `jeryu test audit --changed <csv> --all-tests <csv> [--failed <csv>] [--sha HEAD] [--json] [--workspace <path>]`

Audits VTI selector accuracy. Persists selector misses to the state database.

#### `jeryu test learn --changed <csv> --all-tests <csv> [--failed <csv>] [--sha HEAD] [--json] [--workspace <path>]`

Runs selector audit and prints learning suggestions / flagged subsystems.

#### `jeryu test cache-status [--base HEAD~1] [--head HEAD] [--json]`

Computes selected tests and deterministic cache keys from changed files, `Cargo.lock`, rustc version, and epoch.

### 4.11 Release Management

#### `jeryu release status [--project-id 2] [--ref-name main] [--sha <sha>] [--limit 5] [--json]`

Shows recent release attempts with upstream/release-execution/production pipelines, canary status, gate files, release identity state, public canary URL.

#### `jeryu release watch [--project-id 2] [--ref-name main] [--sha <sha>] [--limit 5] [--interval-secs 5] [--json]`

Continuously refreshes release status.

#### `jeryu release reconcile [--project-id 2] [--ref-name main] [--json]`

Reconciles release attempts against latest successful upstream pipeline. Also run by engine reconciliation loop.

#### `jeryu release promote-prod [--project-id 2] [--ref-name main] [--version <version>]`

Triggers production promotion when canary/E2E-passed and handoff/validation artifacts exist. Variables: `CI_PIPELINE_PRODUCT=production-promotion`, `JERYU_PROD_APPROVED=1`, `JERYU_RELEASE_SHA`, `JERYU_RELEASE_VERSION`.

#### `jeryu release preflight [--ssh-host <host>] [--json]`

Pre-launch canary check: verifies SSH connectivity, Vault health, Docker registry, and disk availability. Returns structured pass/fail with blocker codes and recommended actions. Exits non-zero on failure.

#### `jeryu release doctor [--version <version>] [--preflight true] [--json]`

Diagnoses what is blocking canary or production for a release version. Reports: `next_action`, `canary_complete`, `prod_complete`, `safe_to_reconcile`, gate artifact presence (C handoff, C validation), and blockers. Optionally runs live preflight checks. If `--version` is omitted, uses the latest known release version.

### 4.12 Secrets and Vault

#### `jeryu secrets init`

Bootstraps the jeryu-managed Vault, configures KV mount/prefix and records authority.

#### `jeryu secrets status [--json]`

Shows Vault health and latest release secret-set tracking.

#### `jeryu secrets rotate [--repo dougx] --version <version> --target <target>`

Rotates release-scoped secrets, writes deploy/runtime env handoff files, stores references in Vault, records `release_secret_set`. Only `--repo dougx` is supported.

#### `jeryu secrets finalize [--repo dougx] --version <version> --target <target>`

Marks a secret set finalized after promotion succeeds.

#### `jeryu secrets report [--repo dougx] --version <version>`

Regenerates the release handoff report from current artifacts.

#### `jeryu secrets recover [--repo dougx] --version <version>`

Prints recovery instructions for a release bundle.

### 4.13 Progress

#### `jeryu progress [--project-id 2] [--ref-name main] [--json]`

Builds a lane-aware CI/release progress report for a ref. Combines tracked pipeline state, GitLab job state, release lane classification, blockers, and release execution status.

### 4.14 Next Action

#### `jeryu next [--project-id 2] [--ref-name main]`

Shows the highest-priority recommended action for the current branch. Checks in order: recent job failures, active pipelines, release gate state, selector misses (7-day window). Prints specific suggested `jeryu` commands.

### 4.15 Explain Blocker

#### `jeryu explain-blocker <entity_type> <entity_id>`

Deep-diagnoses why a specific entity is blocked.

| entity_type | behavior |
| --- | --- |
| `job` | Shows failure capsule: kind, classification, exit_code, retry advice, repro script, log snippet, supersedence status |
| `release` | Shows release attempt state: upstream/canary/release/prod pipeline statuses, identifies specific blockers |
| `merge` | Shows selector miss count (30-day window), pipeline/approval guidance |

### 4.16 Action List

#### `jeryu action list [--json]`

Lists all registered jeryu actions with risk tier, key hint, surfaces, and descriptions. Machine-readable with `--json`. See the Action Registry section for the full list.

### 4.17 Repo Agent Surface

#### `jeryu repo render-agent-index [--check]`

Generates or checks the machine-readable agent routing index for the repo.

#### `jeryu repo audit-agent-surface [--json]`

Audits routing docs, generated index freshness, and agent-facing surfaces.

### 4.18 Host Operations

#### `jeryu host storage-audit`

Performs a host storage audit.

#### `jeryu host doctor [--json]`

Checks host, GitLab, Docker, registry/cache, and runner-cache health. Exits non-zero if unhealthy.

#### `jeryu host reclaim --mode aggressive (--plan|--apply)`

Runs aggressive host reclaim. Only `--mode aggressive` is currently accepted.

### 4.19 Hidden: Custom Executor

These are invoked by GitLab Runner when using the `custom` executor. Agents should not call them directly except for driver testing.

#### `jeryu exec config`

Prints custom executor driver config JSON to stdout: `builds_dir=/builds`, `cache_dir=/cache`, `builds_dir_is_shared=false`, driver name/version.

#### `jeryu exec prepare`

Prepares sandbox: reads `CUSTOM_ENV_CI_JOB_ID`/`CUSTOM_ENV_CI_PROJECT_DIR`, creates reflink sandbox worktree, seeds honeypot tripwires.

#### `jeryu exec run <script_path> <stage>`

Runs a custom executor stage. Bootstraps tools, initializes DB/epoch/taint/cache brain, computes witnesses, decides cache hits, captures failure capsules, quarantines on tripwire evidence.

#### `jeryu exec cleanup`

Cleans up custom executor state.

### 4.20 Hidden: Server Hook

#### `jeryu server-hook pre-receive`

Runs the admission controller as a Git pre-receive hook. Reads refs from stdin. V3.01 admission evaluates each update into a versioned decision record (`allow`, `audit`, or `deny`) and persists the result in `admission_decisions` when the state database is available. Agent refs (`refs/heads/agent/*`, `refs/heads/agents/*`, `refs/heads/jeryu/*`) are allowed when they match an active `capability_grants` row for the full ref and optional new SHA. Without a matching grant they are audit-only by default; set `JERYU_ADMISSION_ENFORCE=1` to deny agent refs without ledger proof.

### 4.21 Hidden: Capability Server

#### `jeryu capability serve <socket_path>`

Starts the local Unix domain socket capability API for supervised agents.

#### `jeryu mcp serve`

Starts the MCP adapter over stdio. This is a thin JSON-RPC transport wrapper around the capability policy engine.

#### `jeryu mcp serve-http`

Starts the MCP adapter over Streamable HTTP on the configured loopback bind. The server is local-only, validates loopback `Origin` headers, and assigns ephemeral `Mcp-Session-Id` values at initialization time.

#### `jeryu mcp tools [--json]`

Prints the canonical MCP tool manifest. Tool names are namespaced as `jeryu.<action_id>` and each tool reuses the same grant, evidence, and merge/release gates as the capability API.

---

## 5. Webhook Server API

The engine server binds `0.0.0.0:9777` (configurable via `settings.webhook.bind`):

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| `GET` | `/health` | none | Returns `ok` |
| `POST` | `/hooks` | `X-Gitlab-Token` = `JERYU_WEBHOOK_SECRET` | GitLab webhook ingestion |
| `GET` | `/cache/summary` | none | JSON cache summary |

### 5.1 `POST /hooks`

Required headers: `X-Gitlab-Token`, `X-Gitlab-Event`.

| Event | Behavior |
| --- | --- |
| `Job Hook` | Upserts job event, maybe auto-retries failed job, maybe scales up on pending/created jobs |
| `Pipeline Hook` | Tracks pipeline status, triggers canary on green main, updates release/prod pipelines, triggers prod promotion after release-execution gates |
| `Push Hook` | Normalizes ref, skips `jeryu-test-*`, cancels superseded pipelines, computes impact plan, records VTI plan |
| `Merge Request Hook` | Logged; no active behavior yet |
| other | Logged as unhandled |

### 5.2 Job Hook Payload Fields

Consumed: `build_id`, `project_id`, `pipeline_id`, `build_status`, `build_name`, `build_queued_duration`, `tag`, `ref`, `runner.description`.

### 5.3 Pipeline Hook Behavior

For `ref == main` and `status == success`:
1. If tracked production-promotion pipeline → update production status
2. If tracked release-execution pipeline → update release status → `maybe_trigger_production_promotion`
3. Otherwise → `launch_canary_for_green_pipeline`

For `ref == main` and `status == failed|canceled`: update release/prod pipeline status accordingly.

### 5.4 Push Hook Behavior

1. Normalize `refs/heads/<branch>` to `<branch>`
2. Skip `jeryu-test-*` branches
3. Cancel superseded pending/running/created pipelines for older SHAs on same ref
4. Append `pipeline_superseded` and `pipeline_cancel_requested` events
5. Run impact analysis
6. Append `impact_decision`
7. If changed paths exist, run VTI planner and persist `test_plans`

### 5.5 `GET /cache/summary`

Returns JSON: `{"bytes_served": N, "hits": N, "objects": N, "status": "healthy"}`.

---

## 6. Capability API for Agents

Unix domain socket server. One JSON payload → one JSON response. 64 KiB max.

Start: `jeryu capability serve /tmp/jeryu-capability.sock`

The V3.01 transport accepts either the legacy tagged `AgentIntent` JSON body or an `AgentActionRequest` envelope. New agents should use the envelope, optionally length-prefixed by a 4-byte big-endian frame length:

```rust
pub struct AgentActionRequest {
    pub protocol_version: String, // "v3.01"
    pub request_id: String,
    pub actor: String,
    pub nonce: String,
    pub expires_at: Option<String>,
    pub project_id: Option<i64>,
    pub base_ref: Option<String>,
    pub base_sha: Option<String>,
    pub idempotency_key: Option<String>,
    pub budget: Option<ActionBudget>,
    pub grant: Option<CapabilityGrantProof>,
    pub intent: AgentIntent,
}
```

Legacy request enum (tagged serde):

```rust
#[serde(tag = "intent", content = "payload")]
pub enum AgentIntent {
    ProposePatch { project_id, branch_name, base_ref, commit_message, modifications, mr_title },
    RacePatches { base_branch, commit_message, hypotheses },
    RunTests { project_id, target_ref, test_scope },
    FetchCapsule { job_id },
    RequestMerge { project_id, mr_iid, source_branch, target_branch },
    ExplainBlockers { entity_type, entity_id },
    GetSystemSnapshot,
    ListAllowedActions,
    PlanValidation { project_id, test_ids, ref_name },
}
```

Response: `CapabilityResponse { success: bool, message: String, data: Option<Value> }`.

### 6.1 `FetchCapsule` (Active)

```json
{"intent": "FetchCapsule", "payload": {"job_id": 14445}}
```

Opens the configured state database, scans latest 500 event log rows for matching `failure_capsule`, returns deserialized JSON.

### 6.2 `RunTests` (Active)

```json
{"intent": "RunTests", "payload": {"project_id": 2, "target_ref": "main", "test_scope": "unit"}}
```

Creates branch `<ref>-ci-<timestamp>`, generates dynamic `.gitlab-ci.yml`, triggers pipeline.

| Scope | Generated script |
| --- | --- |
| `unit` | `cargo test --lib --benches` |
| `integration` | `cargo test --test '*'` |
| `lint` | `cargo clippy --all-targets --all-features -- -D warnings` + `cargo fmt -- --check` |
| `full` | `cargo test` |

Unknown scopes are rejected. Pipeline trigger failures are returned to the caller instead of being swallowed after branch creation.

### 6.3 Mutating Intents and Proof Contracts

`ProposePatch`, `RacePatches`, and `RunTests` perform GitLab side effects and must be treated as grant-required agent actions. Successful branch-writing capability intents now create durable rows in `capability_intents` and 24-hour `capability_grants` rows scoped to `refs/heads/<branch>`. For GitLab commit API writes, the grant records the returned commit SHA so admission can bind the grant to the exact post-update object. The admission hook consults those grants for agent ref updates and records every hook decision in `admission_decisions`.

`RequestMerge` returns a V3.01 merge-gate proof record with policy version, blockers, selector-miss count, cache-taint count, branch identity, VTI receipt status, and decision. The current capability merge proof is intentionally conservative: without a VTI receipt and complete GitLab MR/pipeline evidence it blocks rather than manufacturing success.

### 6.4 `ListAllowedActions` Contract

`ListAllowedActions` is generated from `src/tui/action_registry.rs`. Each capability action includes: `id`, `label`, `risk`, `risk_tier`, `side_effect_class`, `required_grant`, `dry_run`, `status`, `surfaces`, and `description`.

### 6.5 MCP Adapter

The MCP adapter in `src/mcp.rs` is a transport layer over the same intent and grant engine used by the capability socket. It supports the MCP `2025-11-25` `initialize`, `notifications/initialized`, `ping`, `tools/list`, and `tools/call` messages over both stdio and Streamable HTTP.

`tools/list` returns the capability-backed tool set from the canonical action registry. `tools/call` maps the tool name back to the matching `AgentIntent` and executes it through the same policy checks, capability grants, nonce handling where applicable, durable evidence, and merge/release gate logic.

The HTTP transport is loopback-only, rejects non-local `Origin` headers, requires `MCP-Protocol-Version` on non-initialization requests, and uses ephemeral `Mcp-Session-Id` values to keep attribution and session scope explicit without introducing a second policy model.

The adapter does not invent a second policy model. It is a transport shim for local agents that already speak MCP and still need the same authority boundary.

---

## 7. Action Registry

Single source of truth in `src/tui/action_registry.rs`. Each action has: id, label, key_hint, risk_tier, surfaces, dry_run, description, side_effect_class, and required_grant.

**Risk Tiers**: `ReadOnly` (green), `Low` (yellow), `High` (light red), `Production` (red).

**Surfaces**: `Cli`, `Tui`, `Capability`.

| id | label | risk | key | surfaces | description |
| --- | --- | --- | --- | --- | --- |
| `open_logs` | Open job logs | read-only | Enter | TUI | Open live log view for selected job |
| `retry_job` | Retry job | low | r | CLI+TUI | Retry selected failed/canceled job |
| `delete_record` | Forget local record | low | d | TUI | Remove selected job from local DB |
| `pause_pool` | Pause/resume pool | low | p | CLI+TUI | Toggle pause on selected runner pool |
| `explain_blockers` | Explain blockers | read-only | — | ALL | Show why job/release/merge is blocked |
| `get_system_snapshot` | System snapshot | read-only | — | Cap+CLI | Full system state summary |
| `propose_patch` | Propose patch | high | — | ALL | Create branch, apply patch, open MR |
| `race_patches` | Race patches | high | — | Cap+CLI | Run multiple hypotheses, keep first green |
| `request_merge` | Request merge | production | — | ALL | Merge through risk gate |
| `plan_validation` | Plan validation | read-only | — | Cap+CLI | Validate plan against selector misses |
| `run_tests` | Run tests | low | — | ALL | Trigger targeted test pipeline |
| `next_action` | Show next action | read-only | — | CLI+TUI | Highest-priority recommended action |
| `tab_mission` | Go to Mission tab | read-only | 1 | TUI | System health overview |
| `tab_release` | Go to Release tab | read-only | 2 | TUI | Release gate matrix |
| `tab_jobs` | Go to Jobs tab | read-only | 3 | TUI | Jobs & Flow board |
| `tab_agents` | Go to Agents tab | read-only | 4 | TUI | Agent task dashboard |
| `tab_tests` | Go to Tests tab | read-only | 5 | TUI | Test Intelligence |
| `tab_pools` | Go to Pools tab | read-only | 6 | TUI | Runner Pools |
| `tab_cache` | Go to Cache tab | read-only | 7 | TUI | SmartCache metrics |
| `tab_evidence` | Go to Evidence tab | read-only | 8 | TUI | Evidence & Audit ledger |
| `tab_secrets` | Go to Secrets tab | read-only | 9 | TUI | Vault lifecycle |
| `toggle_audit_ledger` | Toggle audit ledger | read-only | a | TUI | Toggle capsule/event view in Evidence tab |
| `quit` | Quit jeryu TUI | read-only | q | TUI | Exit TUI |

---

## 8. Agent-Control Hooks and Permissions

| Control | Command/API | Safety gate |
| --- | --- | --- |
| Spawn autonomous task | `jeryu agent spawn` | branch/MR workflow |
| List agent work | `jeryu agent list` | project-scoped |
| Merge MR | `jeryu agent merge` | risk gate |
| Run one test | `jeryu test run` | ephemeral branch, optional cache skip |
| Plan tests | `jeryu test plan`, `test select` | read-only |
| Select external CI jobs | `jeryu test select-external` | `.jeryu/testmap.toml` |
| Fetch job logs | `jeryu job trace`, TUI log pane | GitLab auth |
| Retry job | `jeryu job retry`, TUI `r` | GitLab auth; TUI only failed jobs |
| Cancel job/pipeline | `jeryu job cancel`, `pipeline cancel` | GitLab auth |
| Inspect failure evidence | `jeryu job explain`, capability `FetchCapsule` | DB evidence |
| Explain blocker | `jeryu explain-blocker` | DB + GitLab reads |
| Show next action | `jeryu next` | DB reads |
| List all actions | `jeryu action list` | static registry |
| Trigger canary reconciliation | `jeryu release reconcile` | release gates |
| Trigger prod promotion | `jeryu release promote-prod` | canary e2e + handoff validation |
| Run release preflight | `jeryu release preflight` | infrastructure checks |
| Diagnose release | `jeryu release doctor` | DB + optional preflight |
| Rotate release secrets | `jeryu secrets rotate` | Vault authority |
| Finalize release secrets | `jeryu secrets finalize` | explicit CLI action |
| Pause/drain/scale pools | `jeryu pool ...` | GitLab runner auth + DB |
| Reclaim host storage | `jeryu host reclaim` | requires `--plan` or `--apply` |

Critical guardrails:
- Production promotion is not an ad hoc command — it's a GitLab pipeline with `CI_PIPELINE_PRODUCT=production-promotion`.
- Custom executor commands are GitLab Runner protocol hooks.
- Server hook reads refs from stdin.
- Secrets commands support only `--repo dougx`.
- Capability API has partial routing — not all intents perform the named action.

---

## 9. GitLab REST Client Surface

`GitlabClient` wraps GitLab REST operations:

| Method | Purpose |
| --- | --- |
| `new(base_url, pat)` | Construct client |
| `pat_value_for_clone()` | Return PAT for clone URLs |
| `is_ready()` | Readiness check |
| `create_runner(...)` | Register runner |
| `set_runner_paused(runner_id, paused)` | Pause/resume |
| `list_runner_managers(runner_id)` | List runner managers |
| `delete_runner(runner_id)` | Delete runner |
| `reset_runner_token(runner_id)` | Rotate runner token |
| `list_jobs(project_id, scopes)` | List jobs by scopes |
| `job_trace(project_id, job_id)` | Fetch raw trace |
| `job_artifact_file(...)` | Fetch artifact file |
| `play_job`, `cancel_job`, `retry_job` | Job controls |
| `create_group_webhook(...)` | Register webhook |
| `list_projects`, `get_project`, `create_project` | Project management |
| `create_project_bot(...)` | Bot identity |
| `create_file`, `update_file`, `commit_file`, `update_files`, `commit_actions` | Repository file commits |
| `create_issue`, `update_issue_labels`, `comment_on_issue` | Issue operations |
| `create_merge_request`, `get_merge_request`, `accept_merge_request` | MR operations |
| `create_branch`, `delete_branch` | Branch operations |
| `trigger_pipeline`, `list_pipelines`, `get_pipeline`, `cancel_pipeline` | Pipeline operations |
| `list_pipeline_jobs`, `list_pipeline_bridges`, `list_pipeline_jobs_with_downstream` | Pipeline job graph |
| `get_job_log_snippet` | Tail/snippet helper |

---

## 10. State API

State is owned by `Db` in `src/state.rs`. Postgres is the preferred backend for concurrent agent fleets and is selected with `JERYU_DATABASE_URL=postgres://...` or `postgresql://...`. SQLite remains the embedded fallback when `JERYU_DATABASE_URL` is absent, and explicit `sqlite:` URLs are supported for tests and development. Callers use `Db` methods — never raw SQL except in state-owned migrations or narrowly scoped backend-neutral helpers.

### 10.1 Core Tables

| Table | Purpose |
| --- | --- |
| `pools` | Runner pool definitions, auth tokens, trust tiers |
| `managers` | Runner manager containers and lifecycle state |
| `job_events` | Latest job status events from webhooks/GitLab |
| `ci_job_runs` | Historical pipeline job timing ledger |
| `events` | Append-only event ledger |
| `tracked_pipelines` | Latest known pipeline state by pipeline id |
| `release_attempts` | Canary/release/prod promotion attempts |
| `evidence_capsules` | Structured failure records |
| `retry_decisions` | Retry decisions for failed jobs |
| `shadow_sync_configs` | Shadow sync configuration/status |
| `secret_authorities` | Vault authority metadata |
| `release_secret_sets` | Release secret rotations and artifact paths |
| `secret_audit_events` | Secret lifecycle audit events |
| `cache_objects` | Cache object metadata |
| `cache_requests` | Cache request ledger |
| `hot_cache_entries` | Hot cache tracking |
| `build_signatures` | Build input/environment signatures |
| `image_signatures` | Image signature metadata |
| `force_refresh_rules` | Cache force-refresh policy |
| `resolved_refs` | Resolved git refs |
| `cache_taints` | Active cache taint/quarantine rules |
| `cache_leases` | Cache lease records |
| `cache_verdicts` | Cache hit/miss/verdict history |
| `cache_promotions` | Cache promotion records |
| `material_objects` | Materialized object metadata |
| `material_aliases` | Material aliases |
| `action_cache` | Action cache records |
| `cache_epochs` | Epoch invalidation state |
| `toolchain_fingerprints` | Toolchain identity records |
| `test_executions` | Test execution cache/history |
| `test_plans` | VTI plans for pushes |
| `test_plan_items` | VTI plan selected/skipped items |
| `selector_misses` | VTI miss audit records |

### 10.2 Important `Db` Methods

**Pool/manager:**
`insert_pool`, `list_pools`, `get_pool`, `update_pool_paused`, `update_pool_token`, `delete_pool`, `insert_manager`, `list_managers`, `get_manager`, `update_manager_state`, `update_manager_system_id`, `delete_manager`, `count_active_managers`.

**Jobs/pipelines:**
`upsert_job_event`, `recent_job_events`, `upsert_ci_job_run`, `upsert_ci_job_runs`, `list_ci_job_runs`, `ci_job_bottlenecks`, `count_pending_jobs`, `clear_history`, `delete_pipeline`, `delete_job_event`, `upsert_tracked_pipeline`, `list_active_pipelines_for_ref`, `list_tracked_pipelines`.

**Release:**
`upsert_release_attempt`, `get_release_attempt`, `latest_release_attempt`, `latest_release_attempt_any`, `recent_release_attempts`, `claim_release_canary`, `finish_release_canary`, `attach_release_pipeline`, `update_release_pipeline_status`, `release_attempt_by_release_pipeline_id`, `attach_production_pipeline`, `update_production_pipeline_status`, `release_attempt_by_production_pipeline_id`.

**Evidence/retry/event ledger:**
`insert_evidence_capsule`, `latest_evidence_for_job`, `latest_evidence_by_job_id`, `list_evidence_for_ref`, `insert_retry_decision`, `count_retry_decisions`, `latest_retry_decision`, `append_event`, `get_events`.

**Shadow:**
`list_shadow_sync_configs`, `get_shadow_sync_config`, `upsert_shadow_sync_config`, `set_shadow_sync_enabled`, `delete_shadow_sync_config`, `request_shadow_sync`.

**Secrets:**
`upsert_secret_authority`, `get_secret_authority`, `upsert_release_secret_set`, `get_release_secret_set`, `latest_release_secret_set`, `mark_release_secret_set_finalized`, `insert_secret_audit_event`, `recent_secret_audit_events`.

**Tests/VTI:**
`record_test_execution`, `latest_successful_test_execution`, `get_test_bottlenecks`, `get_test_history`, `record_test_plan`, `record_test_plan_item`, `record_selector_miss`, `count_selector_misses_since`, `latest_test_plan`, `store_test_verdict`, `lookup_test_verdict`, `prune_test_verdicts`.

**Cache:**
`record_cache_request`, `get_cache_metrics`, `prune_cache_requests`.

---

## 11. Settings API

`src/settings.rs` manages `~/.jeryu/settings.json`. Process-wide singleton loaded at startup.

### 11.1 Schema

```json
{
  "gitlab": {
    "image": "gitlab/gitlab-ce:17.9.2-ce.0",
    "runner_image": "gitlab/gitlab-runner:v17.9.2",
    "hostname": "gitlab.local",
    "http_port": 8929,
    "ssh_port": 2224
  },
  "vault": {
    "image": "hashicorp/vault:1.17.5",
    "container_name": "jeryu-vault",
    "http_port": 18200,
    "mount": "secret",
    "prefix": "veox"
  },
  "webhook": { "bind": "127.0.0.1:9777" },
  "mcp": { "bind": "127.0.0.1:9778" },
  "cache": { "proxy_port": 19800, "registry_port": 19801, "manager_budget_gib": 400.0 },
  "sccache": { "enabled": true, "cache_size": "10G", "binary_version": "v0.9.1" },
  "release": { "repo_root": null, "default_project_id": 2 },
  "shadow": { "upstream_url": null },
  "sandbox": { "strict_network_isolation": false },
  "tui": { "sync_interval_ms": 5000, "recent_jobs_limit": 50, "recent_evidence_limit": 100, "audit_events_limit": 50 }
}
```

### 11.2 API

- `settings::init()` — load once at startup
- `settings::get()` — access process-wide singleton
- `settings::load()` — load from disk (creates defaults if absent)
- Forward-compatible: unknown keys are ignored
- Backward-compatible: missing keys use defaults

---

## 12. Decision and Risk API

`src/decision.rs` types:

| Type | Values |
| --- | --- |
| `SupersedenceAction` | Cancel, Preserve, Degrade, Ignore |
| `ImpactLane` | Full, Unit, Integration, DocsOnly |
| `FailureClassification` | Infrastructure, Transient, Regression, Unknown |
| `RetryDecision` | RetryOnce, DoNotRetry, Quarantine, Escalate |
| `TrustTier` | Untrusted, Trusted, Privileged |
| `RiskGateDecision` | Allow, Deny, Escalate |

Key functions: `classify_failure(capsule)`, `recommend_retry(capsule)`, `evaluate_risk_gate(trust_tier, successful_jobs, pending_jobs, failed_jobs, policy)`.

---

## 13. Release Automation API

Key functions: `build_release_status_report`, `render_release_status_text`, `watch_release_status`, `build_progress_report`, `render_progress_text`, `build_pipeline_explain_report`, `render_pipeline_explain_text`, `build_pipeline_doctor_report`, `render_pipeline_doctor_text`, `reconcile_release_for_ref`, `launch_canary_for_green_pipeline`, `trigger_production_promotion`, `maybe_trigger_production_promotion`, `release_preflight`, `release_doctor`.

### 13.1 Canary Launch Gate

`launch_canary_for_green_pipeline` acts when: ref is `main`, upstream is latest successful for ref, not already release-execution, explain is release-eligible, extended lane is green/absent, change is release-impacting, release attempt can be claimed atomically.

### 13.2 Production Promotion Gate

`trigger_production_promotion` requires: release attempt exists, `canary_state == "e2e-passed"`, C handoff/validation artifacts exist, no existing production pipeline for same SHA/ref. `maybe_trigger_production_promotion` is the automatic path after release-execution success.

---

## 14. Secrets API

Key types: `SecretTarget`, `VaultStatusReport`, `RotateSecretOutcome`, `SecretAuthority`, `ReleaseSecretSet`, `SecretAuditEvent`.

Key functions: `run_secrets_init`, `vault_status`, `rotate_release_secrets`, `finalize_release_secrets`, `build_release_secret_report`, `recover_release_secrets`, `default_release_paths`.

Environment variables: `JERYU_VAULT_ADDR`, `JERYU_VAULT_TOKEN`, `JERYU_VAULT_MOUNT`, `JERYU_VAULT_PREFIX`, `JERYU_RELEASE_REPO_ROOT`.

---

## 15. SmartCache and Execution Cache API

Important surfaces: `SmartCache::start`, `SmartCache::enable`, `SmartCache::doctor`, `SmartCache::status_with_options`, `SmartCache::gc_with_options`, `SmartCache::host_doctor_report`, `SmartCache::status_report`, `CacheManager::gc_disk_cache`, `CacheManager::gc_disk_cache_with_pressure`, `CacheBrain::new(...)`, `EpochManager`, `TaintManager`, `WitnessBuilder`, `BuildKitManager`, gateway modules under `src/gateway/`.

---

## 16. TUI API Summary

Launch: `jeryu tui`. Screenshot capture: `jeryu tui --capture --tab jobs --output paper/assets/jeryu-tui-jobs-flow.png`. Key actions are defined in the action registry (section 7). Additional TUI-specific controls:

- `Tab`, arrows: focus/select within pane
- `Enter` on jobs: open real-time log view
- `G`/`End`: follow latest logs
- `Esc`: close overlay / go back

Logs are polled every 650ms, syntax-highlighted, and tail-following. The Flow Board retains non-empty snapshots. Full details in `docs/JERYU_TUI.md`.

---

## 17. Environment Variables and Hook Inputs

**Common:** `GITLAB_PAT`, `JERYU_WEBHOOK_SECRET`, `GITLAB_ROOT_PASSWORD`, `JERYU_RELEASE_REPO_ROOT`, `JERYU_DATABASE_URL`, `JERYU_GITLAB_INSECURE_TLS`.

`JERYU_GITLAB_INSECURE_TLS=1` allows invalid GitLab TLS certificates for development-only self-signed HTTPS setups. The default is certificate validation enabled.

**State test override:** `JERYU_TEST_POSTGRES_URL` enables optional Postgres integration smoke tests; normal test runs use in-memory SQLite and skip the Postgres smoke when unset. `jeryu repo postgres-state-proof` starts a disposable `postgres:16-alpine` container, sets the URL, runs the core state/cache smoke, and removes the container unless `JERYU_KEEP_POSTGRES_PROOF=1`.

**Vault:** `JERYU_VAULT_ADDR`, `JERYU_VAULT_TOKEN`, `JERYU_VAULT_MOUNT`, `JERYU_VAULT_PREFIX`.

**Release pipeline variables set by jeryu:** `CI_PIPELINE_PRODUCT`, `JERYU_CANARY_APPROVED`, `JERYU_PROD_APPROVED`, `JERYU_UPSTREAM_PIPELINE_ID`, `JERYU_UPSTREAM_BUILD_JOB_ID`, `JERYU_RELEASE_SHA`, `JERYU_RELEASE_VERSION`, `VEOX_PUBLISH_ENCLAVE_REF`.

**Custom executor inputs from GitLab Runner:** `CUSTOM_ENV_CI_JOB_ID`, `CUSTOM_ENV_CI_PROJECT_ID`, `CUSTOM_ENV_CI_PROJECT_DIR`, `CUSTOM_ENV_JERYU_FORCE_REFRESH`.

**Webhook headers:** `X-Gitlab-Token`, `X-Gitlab-Event`.

**Git server hook:** `server-hook pre-receive` reads standard pre-receive lines from stdin.

---

## 18. Proof-Scoped Workspace Tools

| Crate | Command | Purpose |
| --- | --- | --- |
| `cargo-witness` | `cargo run -p cargo-witness -- build` | Build `.witness/witness-graph.json` from public API signatures |
| `cargo-witness` | `cargo run -p cargo-witness -- diff <old.json> <new.json>` | Classify API/signature changes |
| `cargo-witness` | `cargo run -p cargo-witness -- diagnose` | Route compiler errors to likely owning modules |
| `cargo-vrc` | `cargo run -p cargo-vrc -- map --output-dir .` | Generate `agent-map.json` and `test-map.json` |
| `cargo-vrc` | `cargo run -p cargo-vrc -- plan <changed-paths> --output vrc-plan.json` | Select minimal validation for changed files |
| `cargo-aer` | `cargo run -p cargo-aer -- scan --output aer-findings.json` | Audit structural exceptions, mega-files |
| `arc-bench` | `cargo run -p arc-bench -- run psd-mechanics` | Benchmark ARC/VRC design tradeoffs |
| `witness-rt` | library macros | `agent_ensure!`, `agent_bail!`, `agent_expect!` + structured repair packets |

---

## 19. Error and Output Conventions

- Most modules use `anyhow::Result`; security and domain modules define typed errors (`ReleaseError`, `SecretError`, `CacheError`, `ExecError`).
- JSON flags produce pretty JSON with `serde_json::to_string_pretty`.
- Hook/custom executor protocols write machine-readable output where the caller expects it.
- Webhook auth failure returns HTTP 401.
- `host doctor` exits non-zero when unhealthy.
- `release preflight` exits non-zero when blockers exist.

---

## 20. Validation Commands

Documentation-only changes:
```bash
cargo check -p jeryu --message-format=json
```

API/control-surface changes:
```bash
cargo check -p jeryu --message-format=json
cargo test -p jeryu -- state release::tests engine test_runner -- --nocapture
cargo run -p jeryu -- repo audit-agent-surface --json
```

TUI changes:
```bash
cargo test -p jeryu -- tui -- --nocapture
cargo run -p jeryu -- tui --once
```

Release automation changes:
```bash
cargo test -p jeryu -- state::tests::test_production_pipeline_tracking release::tests -- --nocapture
```
