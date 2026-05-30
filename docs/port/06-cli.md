# Port Spec 06 — CLI Taxonomy Subsystem

**Product:** `jeryu` (fused repo `/home/ubuntu/jeryuRUST`)
**Owner of this spec:** CLI/dispatch worker (NOT core/engine — those are Codex-owned).
**Scope:** The top-level `jeryu` binary command taxonomy: clap definitions (`src/cli*.rs`),
the dispatch router (`src/dispatch*.rs`), and the per-command implementations
(`src/commands/**`). This spec re-grounds every command from the GitLab REST backend
onto the `jeryu-*` engine crates, applies the locked renames (MR→PR, pipeline→`ci run`,
pool→`runner`, GitLab terms removed), and defines the acceptance gate.

> LOCKED DECISIONS in force: D1 (zero gitlab/jitforge/JitForge/Nitro literals; only
> `jeryu`/`jeryu-*` survive), D2 (engine crate renames), D3 (keep SQLite+RedlineDB db/,
> HTTP daemons, ratatui TUI, React web; GitLab backend replaced 100% by `jeryu-*` core;
> edition 2024), D4 (MR/merge-request → PullRequest/PR), D5 (runners OCI-first then native).

---

## 1. Source inventory

All paths are read-only sources under `/home/ubuntu/jeryu/src`. One line = one module's purpose.

### 1.1 clap definition layer (pure data; no logic)

| Source file | LOC | Purpose |
|---|---|---|
| `cli.rs` | 59 | Top `Cli` struct (`#[command(name="jeryu", about="Git-compatible version control layer for the AI era")]`), `parse_expanded_path`, `parse_exec_script_path`, `infer_repo_name`; wires all child modules via `#[path=...]`. |
| `cli_defs.rs` | 159 | The root `Commands` enum (`L21-139`): ~35 subcommands incl. `Init/Serve/Install/Remote/Tui/Pool/Job/Pipeline/Cache/Mr/Release/Secrets/Repo/Bug/Node/Mcp/Action/Web/...`. Re-exports child enums. |
| `cli_defs_commands.rs` | 187 | `BugCommands`, `BugProjectCommands`, `AccessCommands`, `AccessKeyCommands`, **`MrCommands`** (`L164-181`, the merge-request create), and re-export of repo enums. |
| `cli_defs_commands_bug.rs` | 35 | `BugAttemptCommands`. |
| `cli_defs_commands_repo.rs` | 200 | `RepoCommands`, `RepoInitCommand`, `RepoAdoptCommand`, `RepoStandardCommand(s)`, `RepoFleetCommand(s)`, `RepoHookCommands`. |
| `cli_defs_node.rs` | 70 | `NodeCommands` (SSH remote Docker nodes). |
| `cli_defs_remote.rs` | 75 | `RemoteCommand` (global flags) + `RemoteActionCommands` (`Install/Refresh(=update)/Doctor/Status/Logs/Restart/Stop/Start/Ssh/Run/Tunnel/Uninstall`). |
| `cli_defs_install.rs` | 52 | `InstallCommand` (global flags) + `InstallActionCommands` (`Guided/Doctor/Smoke/Server/Uninstall/RenderDemo`). |
| `cli_defs_web.rs` | 53 | `WebCommand` (Phase-0 Web Forge BFF stub). |
| `cli_defs_aux.rs` | 64 | `ActionCommands`, **`ExecCommands`** (GitLab Custom-Executor protocol: `Config/Prepare/Run/Cleanup`, `L17-32`), `ServerHookCommands`, `CapabilityCommands`, `McpCommands`. |
| `cli_runtime_commands.rs` | 199 | `JobCommands`, **`PipelineCommands`** (`L31-92`: `List/Explain/Doctor/Jobs/Ingest/Cancel/Bottlenecks`), `CacheCommands`, `LocalCommands`, `AgentCommands`, `SettingsCommands`. |
| `cli_runtime_commands_ext.rs` | 12 | Re-export hub for the ext companions below. |
| `cli_runtime_commands_ext_host.rs` | 39 | `HostCommands`. |
| `cli_runtime_commands_ext_policy.rs` | 12 | `PolicyCommands` (`Audit`). |
| `cli_runtime_commands_ext_release.rs` | 143 | `ReleaseCommands` (`Status/Watch/Reconcile/FullPath/PromoteProd/Preflight/Doctor/Ready/DryRun/Submit/Approve/Rollback`). |
| `cli_runtime_commands_ext_secrets.rs` | 64 | `SecretsCommands` (`Provision/Status/Doctor/Rotate/Finalize/Report/Recover`). |
| `cli_test_commands.rs` | 216 | `TestCommands` (`Run/Plan/Batch/Results/Requeue/Failed/Impact/Choose(=select)/ExplainPlan/SelectExternal/Audit/Learn/CacheStatus`); note `--emit-gitlab` flag at `L120,L152`. |
| `cli_tests.rs` | 435 | Clap parse + **help-snapshot** tests; `cli_help_excludes_removed_git_commands` (`L427-435`) is the no-regression template. |

### 1.2 dispatch router layer (CLI cmd → domain fn; no business logic)

| Source file | LOC | Purpose |
|---|---|---|
| `dispatch.rs` | 266 | Entry `run(cli) -> Result<i32>` (`L62`); `load_client()` builds `GitlabClient` from `gitlab_auth::resolve_or_repair_default()` (`L33-40`); `load_pool_service()`; handles hot path commands then `=> dispatch_back::run(other)`. |
| `dispatch_back.rs` | 137 | Second-tier match: `Cache/Local/Logs/Agent/Access/Mr/Test/Settings/Release/Secrets/Progress/Repo/Policy/Host/Node/Exec/ServerHook/Action/Capability/Mcp/Next/ExplainBlocker`. |
| `dispatch_back_ops.rs` | 204 | `run_serve`, `run_cache`, `run_local`, `run_logs`, `run_agent`, `run_progress` impl bodies. |
| `dispatch_back_late.rs` | 96 | `run_action`, `run_capability`, `run_mcp`, `run_next`, `run_explain_blocker`. |
| `dispatch_inspect.rs` | 209 | `run_next`/`run_explain_blocker` deep logic (blocker explanation across pipeline/release entities). |
| `dispatch_support.rs` | 55 | `fetch_ci_job_runs(client, project_id, pipeline_id)` — pulls job timings from GitLab pipeline jobs. |

### 1.3 command implementation layer (`src/commands/**`)

| Source file | LOC | Purpose |
|---|---|---|
| `commands/mod.rs` | — | `resolve_project_id`, module declarations. |
| `commands/job.rs` | 63 | `execute_job_commands`: `list_jobs/job_trace/play_job/cancel_job/requeue_job` via `GitlabClient`; `Explain` reads `db.latest_evidence_for_job`. |
| `commands/pipeline.rs` | 186 | `execute_pipeline_commands`: `list_pipelines/get_pipeline/cancel_pipeline`, `db.upsert_tracked_pipeline`, `db.upsert_ci_job_runs`, `db.ci_job_bottlenecks`. |
| `commands/pool.rs` | 176 | `PoolCommands` enum + `execute_pool_commands` over `pool_service::PoolService` (`list/doctor/repair/scale/pause/resume/drain/delete/rotate_token`); prints `gitlab_runner_id`. |
| `commands/mr.rs` | 159 | `execute_mr_commands`/`create_merge_request`: builds `GitlabClient`, `client.create_merge_request(...)`, prints `iid: !N`; refuses non-local-GitLab checkouts. |
| `commands/remote.rs` | 116 | `execute_remote_command`/`map_remote_command`: maps clap → `remote::RemoteAction`/`RemoteOperation`. SSH bootstrap; no GitLab coupling. |
| `commands/install.rs` | 55 | `execute_install_command` → `install::run_{local,guided,doctor,smoke,server,uninstall}` + render-demo. |
| `commands/system.rs` | 151 | `execute_down` (drain pools + `docker compose down`), `execute_status` (git status), `execute_system_status` (GitLab health probe, Vault, pools, containers, release). |
| `commands/release.rs` / `release_ops.rs` / `release_render.rs` | 7.8K/6.2K/1.7K | Release orchestration (canary/promote/rollback) + report rendering. |
| `commands/secrets.rs` | 8.0K | Vault provision/rotate/finalize/report/recover. |
| `commands/test.rs` / `test_back.rs` / `test_back_choose.rs` / `test_back_support.rs` / `test_intel_commands.rs` / `test_pipeline_commands.rs` | 3.8K..7.3K | `execute_test_commands`: submit test commands as CI pipelines, VTI smart-select, audit/learn; emits GitLab child-pipeline YAML. |
| `commands/agent_submit.rs` | 6.4K | Agent-first GitHub PR submission (proof + evidence capsule + draft PR). |
| `commands/access.rs` | 9.4K | GitLab access doctor/repair/project resolution + remote-key metadata. |
| `commands/repo.rs` | 8.5K | `execute_repo_commands`: init/adopt/standard/fleet/hooks/audit. |
| `commands/node.rs` | 13.5K | SSH remote Docker node management. |
| `commands/bug.rs` / `bug_support.rs` | 6.7K/6.4K | Bug tracker project/triage/attempt commands. |
| `commands/host.rs` | 1.4K | Host capability inspection. |
| `commands/settings.rs` | 2.3K | `~/.jeryu/settings.json` repair/reset. |
| `commands/git.rs` | 1.8K | `git`/`save`/`sync`/`undo` passthrough wrappers. |
| `commands/health.rs` | 1.8K | `execute_health_command(json, ci)`. |
| `commands/exec.rs` | 568B | `validate_script_path`; backs the GitLab Custom-Executor `exec` subtree. |

---

## 2. Target layout in `/home/ubuntu/jeryuRUST`

The fused repo is a Cargo workspace (`Cargo.toml` `members = [...]`). The product shell
(this CLI) becomes a **new binary crate** that consumes the renamed `jeryu-*` engine crates.

### 2.1 New crate

```
crates/jeryu-cli/                      # NEW — the `jeryu` binary (clap + dispatch + commands)
  Cargo.toml                           # name = "jeryu-cli"; [[bin]] name = "jeryu"; edition = "2024"
  src/
    main.rs                            # parse Cli, init tracing, call dispatch::run, exit(code)
    cli/                               # was src/cli*.rs  (pure clap data, no logic)
      mod.rs                           # Cli root struct + Commands enum (was cli.rs + cli_defs.rs)
      forge.rs                         # ForgeCommands  (NEW: repo/issue/pr — absorbs MrCommands)
      ci.rs                            # CiCommands     (was PipelineCommands + Job + test bits)
      runner.rs                        # RunnerCommands (was PoolCommands)
      remote.rs                        # RemoteCommand  (was cli_defs_remote.rs, verbatim shape)
      install.rs                       # InstallCommand (was cli_defs_install.rs)
      proof.rs                         # ProofCommands  (NEW: verify/explain)
      release.rs                       # ReleaseCommands (signrail-backed)
      cache.rs                         # CacheCommands  (adds `self-test`)
      repo_admin.rs                    # RepoCommands/Node/Bug/Access/Settings/Host/Policy
      aux.rs                           # Action/ServerHook/Capability/Mcp/Web (exec subtree REMOVED)
    dispatch/                          # was src/dispatch*.rs (router only)
      mod.rs                           # run(Cli) -> Result<i32>; load_core(), no GitlabClient
      back.rs, ops.rs, late.rs, inspect.rs, support.rs
    commands/                          # was src/commands/** (thin adapters onto jeryu-* APIs)
      forge.rs, ci.rs, runner.rs, remote.rs, install.rs, system.rs,
      release.rs, secrets.rs, test.rs, agent_submit.rs, repo.rs, node.rs,
      bug.rs, host.rs, settings.rs, git.rs, health.rs, proof.rs, cache.rs
    tests/cli_snapshots.rs             # was cli_tests.rs (help-snapshot acceptance gate)
```

`crates/jeryu-cli/Cargo.toml` depends on the renamed engine crates (see §4):
`jeryu-core` (forge-core), `jeryu-api` (jitforge-api), `jeryu-gitd` (gitd),
`jeryu-ci-compiler`/`jeryu-ci-scheduler`/`jeryu-ci-ir`, `jeryu-runnerd` + `jeryu-runner-*`,
`jeryu-proof` (proofcore), `jeryu-signrail`, `jeryu-cache*` (cratevault*), plus the
preserved db/HTTP/TUI modules.

### 2.2 Preserved product-shell modules (D3)

The jeryu library modules the CLI calls (`state::Db` SQLite+RedlineDB, `docker`, `tui`,
`remote`, `install`, `secrets`, `release`, `cache`, `settings`, `mcp`, `capability`,
`web`) are PORTED INTO the fused repo as the `jeryu` library (separate spec). This CLI
spec assumes those exist; the only client swap is **`gitlab_client::GitlabClient` →
`jeryu_core` / `jeryu_api` handles** (see §3).

### 2.3 Workspace registration

Add `"crates/jeryu-cli"` to `Cargo.toml` `members`. The legacy stub bins
(`bins/jit-ci`, `bins/jit-phase11`) are out of scope for this spec; the canonical user
entrypoint becomes `jeryu`.

---

## 3. Rewire map

Top-level taxonomy renames (root `Commands` enum):

| jeryu (source) | jeryu (fused target) | Note |
|---|---|---|
| `Mr(MrCommands)` | `Forge(ForgeCommands)` w/ `forge pr create` | D4: MR→PR. `forge repo`, `forge issue`, `forge pr` subtree. |
| `Pipeline(PipelineCommands)` | `Ci(CiCommands)` w/ `ci run/status/explain/compile` | "pipeline → ci run". |
| `Pool(PoolCommands)` | `Runner(RunnerCommands)` w/ `enroll/list/drain/rotate` | pool→runner; "pool manages GitLab runners" removed. |
| `Job(JobCommands)` | folded into `Ci` (`ci job …`) | jobs are CI artifacts. |
| `Exec(ExecCommands)` | **REMOVED** | GitLab Custom-Executor protocol; no replacement (D1). |
| (new) | `Proof(ProofCommands)` (`verify/explain`) | surfaces `jeryu-proof`. |
| `Cache(CacheCommands)` | `Cache` + `cache self-test` | adds RBAC/integrity self-test. |
| `Release(ReleaseCommands)` | `Release` backed by `jeryu-signrail` | replaces GitLab pipeline release path. |

### 3.1 Forge — repo / issue / pr

| source symbol/data | current (GitLab) source | target jeryu-* type/API |
|---|---|---|
| `MrCommands::Create{source,target,title,draft,push,json}` | `gitlab_client::create_merge_request(project,src,tgt,title,body)` (`mr.rs:113`) | `jeryu_core::create_pull_request(repo, CreatePullRequestRequest{head,base,title,draft,...})` (`forge-core/core.rs:440`); returns `PullRequest` (`forge-core/model.rs:241`). |
| `mr.iid` / printed `!{iid}` | GitLab MR IID | `PullRequest.number: u64` (GitHub-style PR number); print `#{number}`. |
| `MrCreateReport.project_path` | GitLab project path | `RepoKey{owner,name}` (`forge-core/model.rs:7`). |
| `access::repo_is_local_gitlab` gate | refuses non-local-GitLab | replace gate with `jeryu_gitd` repo presence check; drop GitLab SSH-origin requirement. |
| (new) `forge repo create/list` | `gitlab_client_projects` (create/list project) | `jeryu_core::create_repository(CreateRepositoryRequest)` (`forge-core/core.rs:155`), `Repository` (`model.rs:74`). |
| (new) `forge issue create/list/show` | `gitlab_client_issues` (get/list/create) | `jeryu_core::create_issue(CreateIssueRequest)` (`core.rs:249`), `Issue`/`IssueState` (`model.rs:101,129`). |
| (new) `forge pr merge` | `gitlab_client::accept_merge_request` | `jeryu_core::merge_pull_request(...)` (`core.rs:781`); `PullRequestState` (`model.rs:202`). |
| `agent merge --mr-iid` (`dispatch_back_ops.rs:177`) | `agent::merge_agent_mr(client, project, mr_iid, tier)` | `forge pr merge --pr <number> --trust-tier`; same risk-gate, `merge_pull_request`. |

### 3.2 CI — compile / run / status / explain (was pipeline + job)

| source symbol/data | current (GitLab) source | target jeryu-* type/API |
|---|---|---|
| `PipelineCommands::List` | `client.list_pipelines(project, ref)` (`pipeline.rs:16`) | `ci status --repo --ref`: `jeryu_ci_scheduler` queue/schedule summary + `db.list_tracked_pipelines`. |
| `PipelineCommands::Explain` | `release::build_pipeline_explain_report(client,...)` | `ci explain <run-id>`: drive from `jeryu_ci_scheduler::QueueSummary`/`ValidationOutcome` (`ci-scheduler/merge_queue.rs:24,56`) + proof verdict; NO GitLab fetch. |
| `PipelineCommands::Doctor/Jobs/Ingest` | `fetch_ci_job_runs` (`dispatch_support.rs`) | `ci run --file <ci.yml|ci.toml>`: `jeryu_ci_compiler::Compiler::compile(input, CiKind::{GitHubActions,NativeToml}, CompileContext)` (`ci-compiler/lib.rs:66`) → `Pipeline` IR → `jeryu_ci_scheduler::MergeQueue`/`LeaseBook` (`leases.rs:90`). |
| `--kind gitlab` / `emit_gitlab` (test.rs `L120,L152`) | GitLab YAML child pipeline | **REMOVED**. `CiKind::GitHubActions` + `CiKind::NativeToml` only (`ci-compiler/lib.rs:11`). `--emit-plan`/`--emit-receipt` retained. |
| `client.cancel_pipeline` | GitLab cancel | `jeryu_ci_scheduler::MergeQueue` cancel/drop + `jeryu_runnerd` lease release. |
| `db.upsert_ci_job_runs` / `ci_job_bottlenecks` | local SQLite ledger (already jeryu-native) | KEEP verbatim (D3 db layer preserved). |
| `JobCommands::{List,Trace,Play,Cancel,Retry}` | `client.{list_jobs,job_trace,play_job,cancel_job,requeue_job}` | `ci job …`: `jeryu_runnerd::DispatchEngine::dispatch(JobRequest, DispatchMode)` (`runnerd/dispatch.rs:24`) for play/retry; `jeryu_ci_scheduler::LeaseBook::{fail,complete}` for cancel; trace from runner job-file/log store. |
| `JobCommands::Explain` | `db.latest_evidence_for_job` (jeryu-native) | KEEP; optionally enrich with `jeryu_proof` verdict. |
| `PipelineRef`/`PipelineBridge`/downstream | GitLab bridges | `jeryu_ci_ir::Pipeline.edges` (DAG `Dependency{from,to}`) — bridges become IR edges. |

### 3.3 Runner — enroll / list / drain / rotate (was pool)

| source symbol/data | current (GitLab) source | target jeryu-* type/API |
|---|---|---|
| `PoolCommands::List` + `gitlab_runner_id` column | `PoolService::list()` → `PoolListRow` (`pool.rs:116-132`) | `runner list`: `jeryu_runnerd` node report; drop the `RUNNER`/`gitlab_runner_id` column; show `jeryu` runner id + executor (OCI/native, D5). |
| `PoolCommands::Scale{name,count}` | `service.scale` (spawns GitLab runner managers) | `runner enroll <node> [--count]`: register node with `jeryu_runnerd`; OCI-first executor (D5). |
| `PoolCommands::Pause/Resume` | `service.pause/resume` | `runner pause/resume` (lease admission toggle in `jeryu_ci_scheduler::LeaseBook`). |
| `PoolCommands::Drain{name}` | `service.drain` (waits, stops managers) | `runner drain <node>`: stop accepting leases, await in-flight via `LeaseBook::state` (`leases.rs:225`), then stop. |
| `PoolCommands::Remove`(`delete`) | `service.delete` + GitLab runner dereg | `runner delete <node>`: deregister from `jeryu_runnerd` only (no GitLab). |
| `PoolCommands::RotateToken{name}` | `service.rotate_token` (GitLab runner auth token) | `runner rotate <node>`: rotate runner enrollment credential in `jeryu_runnerd` registry. |
| `PoolCommands::Doctor/Repair` + `--prune-outdated` ("Delete outdated standard GitLab runner registrations…") | `pool::PoolDoctorReport` | `runner doctor/repair`: topology/drift over `jeryu_runnerd` node set; **rewrite all "GitLab runner" doc strings** → "jeryu runner". |
| `runner_policy::enforce_pool_runner_policy(client,pools)` (`dispatch.rs:107`) | normalizes GitLab runners | `jeryu_runnerd` policy normalization; the `client` arg is dropped. |

### 3.4 Remote — install / tunnel / status (no rewire of backend; client swap only)

| source symbol/data | current source | target jeryu-* type/API |
|---|---|---|
| `RemoteActionCommands::{Install,Refresh(=update),Doctor,Status,Logs,Restart,Stop,Start,Ssh,Run,Tunnel,Uninstall}` | `remote::execute_remote(RemoteAction, RemoteCommonOptions)` (`remote.rs:80`) | KEEP shape verbatim (SSH bootstrap, no GitLab coupling). Only the bootstrapped daemon it installs changes from GitLab-runner to `jeryu_runnerd`. |
| `RemoteCommand` global flags (`--service-mode`, `--color`, `--interactive`) | `jeryu::remote::ServiceMode` etc. | unchanged (ported product-shell module). |

### 3.5 Proof — verify / explain (NEW top-level surface)

| source symbol/data | current source | target jeryu-* type/API |
|---|---|---|
| (folded into `release ready`/`agent submit` today) | `decision::*`, evidence capsules | `proof verify --changeset <path>`: `jeryu_proof::ProofEngine::verify(&ChangeSet) -> Result<ProofPlan, ProofBlocker>` (`proofcore/engine.rs:106`); `jeryu_proof::default_phase7_engine()` (`engine.rs:165`). |
| `ExplainBlocker{entity_type,entity_id}` (`dispatch_inspect.rs`) | jeryu blocker explanation | `proof explain <id>`: render `ProofBlocker` + matcher (`proofcore/matcher.rs`, `policy.rs`). |

### 3.6 Release — signrail-backed

| source symbol/data | current source | target jeryu-* type/API |
|---|---|---|
| `ReleaseCommands::{DryRun,Submit,Ready,Rollback,...}` | `release::*` + GitLab `release.yml` trigger | `jeryu_signrail`: `validate_release`/`ReleasePolicy` (`signrail/policy.rs`), `Release` (`release.rs`), `RollbackMetadata` (`rollback.rs`), `ReleaseWitness` (`witness.rs`), `Artifact`/`ArtifactStore` (`artifact.rs`,`store.rs`). `release submit` writes a signed release + checksums via signrail instead of triggering GitLab CI. |
| `release ready --pr <num>` | GitHub Check Run (gh) | KEEP gh path; compose gate from `jeryu_signrail::validate_release` + `jeryu_proof` verdict. |

### 3.7 Cache — adds self-test

| source symbol/data | current source | target jeryu-* type/API |
|---|---|---|
| `CacheCommands::{Enable,Doctor,Status,Gc}` | `cache::SmartCache` (Docker registry mirror) | `jeryu_cache*` (cratevault): `Cache`/`service` (`cratevault/cache.rs,service.rs`), `policy` (`policy.rs`), `verify` (`cratevault-core/verify.rs`), `quarantine`. |
| (new) `cache self-test` | — | `jeryu_cache` integrity/false-hit self-test (`cratevault/false_hit.rs`, `cratevault-core/verify.rs`) + report. Mirrors the `/api/phase10/rbac/self-test` route philosophy in `jeryu-api`. |

### 3.8 Auth / client construction (dispatch core)

| source symbol/data | current source | target |
|---|---|---|
| `load_client() -> (GitlabClient, secret)` (`dispatch.rs:33`) | `gitlab_auth::resolve_or_repair_default()` + `GitlabClient::new(url, token)` | `load_core() -> jeryu_core handle` + `jeryu_gitd` repo handle; no HTTP URL/PAT; `JERYU_WEBHOOK_SECRET` retained for `jeryu-api` webhook verify. |
| `config::GITLAB_HTTP_PORT` (`dispatch.rs:49`, `system.rs:42`) | GitLab loopback port | REMOVE; `jeryu-api` BFF bind (web spec) or in-process core; rename const → `JERYU_API_PORT`. |
| `docker_ctl.compose_up()` "Ensure GitLab is running" (`dispatch.rs:77`) | GitLab compose | `jeryu_gitd`/`jeryu-api` daemon start (HTTP daemons preserved, D3). |
| `system.rs` "GitLab: running ({url})" line (`L46-55`) | GitLab health probe | "Forge: …" via `jeryu_api::Router` ready route (`/api/phase10/ready` style) or core handle. |
| `JERYU_SYSTEM_GIT` / `JERYU_GITLAB_INSECURE_TLS` env | GitLab TLS | drop `*_GITLAB_*` env names; standard `jeryu_gitd` TLS config. |

---

## 4. Dependencies & ordering

This subsystem is **downstream of nearly everything**. Hard ordering:

1. **D2 crate renames must land first (Codex-owned).** This CLI crate's `Cargo.toml`
   imports the renamed crates by name. Required before any compile:
   `forge-core→jeryu-core`, `jitforge-api→jeryu-api`, `gitd→jeryu-gitd`,
   `ci-compiler→jeryu-ci-compiler`, `ci-ir→jeryu-ci-ir`, `ci-scheduler→jeryu-ci-scheduler`,
   `runnerd→jeryu-runnerd`, `runner-*→jeryu-runner-*`, `proofcore→jeryu-proof`,
   `signrail→jeryu-signrail`, `cratevault*→jeryu-cache*`.
2. **Persistence layer ported (D3).** `state::Db` (SQLite+RedlineDB) must be available as
   the `jeryu` library, because `ci`/`job explain`/`bottlenecks`/`system` read it directly
   (`pipeline.rs:73`, `job.rs:41`, `system.rs:38`). Spec'd separately; **blocks** the
   `Ci`, `Job-Explain`, and `System` dispatch arms.
3. **Product-shell modules ported (D3).** `remote`, `install`, `secrets`, `release`,
   `cache` (SmartCache shell), `tui`, `mcp`, `capability`, `web`, `settings`. The clap
   shape of `Remote`/`Install`/`Secrets` is unchanged; only their backends rewire.
4. **`jeryu-core` public API parity.** Needs `create_repository`/`create_issue`/
   `create_pull_request`/`merge_pull_request` (all already present, `forge-core/core.rs`).
   No core changes requested from this worker (Codex owns it) — if a method is missing the
   CLI files a gap, does not edit core.
5. **`jeryu-runnerd` node-registry API.** `runner enroll/list/drain/rotate` need a node
   registry surface. Today `runnerd` is a STUB (`DispatchEngine`/`DispatchMode` only,
   `dispatch.rs:24`). **This is the single biggest external blocker** — the CLI can scaffold
   `RunnerCommands` + adapters but `enroll/rotate` bodies stay `todo!()`/return "not
   implemented" until runnerd grows enrollment. Flag to Codex.
6. **`ci-scheduler` is Codex-owned and explicitly off-limits to edit**, but the CLI
   *consumes* `MergeQueue`/`LeaseBook`/`QueueSummary` for `ci run/status/explain`.

**Blocks downstream of this spec:** TUI lenses that mirror CLI verbs (runners lens,
pipeline lens), MCP tool manifest (must not advertise gitlab/pool/mr tool names), and the
Web BFF route names.

**Within-subsystem ordering:** (a) port pure clap defs + the snapshot test harness; (b)
wire dispatch router with `load_core()`; (c) port command adapters cheapest-first
(`remote`, `install`, `system`, `cache`), then `forge`, `ci`, then `runner` last (blocked
on runnerd).

---

## 5. Tests / acceptance gate

### 5.1 Exact commands

```bash
# Build the new CLI crate (edition 2024 workspace)
cargo build -p jeryu-cli

# Unit/parse + help-snapshot suite (ported from cli_tests.rs)
cargo test -p jeryu-cli

# Render the full help tree and assert ZERO-EVIDENCE (D1). Must exit non-zero on any hit.
target/debug/jeryu --help > /tmp/jeryu_help.txt
for sub in forge ci runner remote proof release cache install secrets repo node bug system; do
  target/debug/jeryu "$sub" --help >> /tmp/jeryu_help.txt 2>&1 || true
done
! grep -Eiaq 'gitlab|jitforge|jit-forge|nitro|merge[ -]?request|--kind[ =]gitlab|emit[-_]gitlab|\bmr\b' /tmp/jeryu_help.txt

# Sanity: required new verbs are present
grep -q 'forge'  /tmp/jeryu_help.txt
target/debug/jeryu forge --help  | grep -Eq 'repo|issue|pr'
target/debug/jeryu ci --help     | grep -Eq 'compile|run|status|explain'
target/debug/jeryu runner --help | grep -Eq 'enroll|list|drain|rotate'
target/debug/jeryu cache --help  | grep -q 'self-test'
target/debug/jeryu proof --help  | grep -Eq 'verify|explain'

# Repo-wide literal ban for this subsystem's source (D1)
! grep -REnia 'gitlab|jitforge|JitForge|Nitro' crates/jeryu-cli/src crates/jeryu-cli/tests
```

### 5.2 Snapshot / invariant tests (in `crates/jeryu-cli/tests/cli_snapshots.rs`)

Port and extend the `cli_tests.rs` template (`CommandFactory` introspection,
`L427-435`):

- `cli_help_contains_no_legacy_terms`: walk `Cli::command()` recursively over
  `get_subcommands()`, collect every name + `get_about()`/`get_long_about()` + every
  arg long flag + arg help; assert none match
  `(?i)(gitlab|jitforge|nitro|merge.?request|^mr$|kind.?gitlab|emit.?gitlab)`.
- `cli_help_excludes_removed_commands`: assert `mr`, `pool`, `pipeline`, `exec` are **not**
  top-level subcommand names (replaces the source's `ship`/`mirror` assertion).
- `cli_help_includes_renamed_commands`: assert `forge`, `ci`, `runner`, `proof` exist;
  `forge` has `repo|issue|pr`; `ci` has `compile|run|status|explain`; `runner` has
  `enroll|list|drain|rotate`.
- Parse-shape carryovers (keep these from `cli_tests.rs`): `remote install` alias/setup-key
  (`L157`), remote service/ui flags (`L217`), install action flags (`L82-145`), release
  `--ref`/`--ref-name` alias (`L9-28`), release `full-path` flags (`L31`).
- `forge_pr_create_parses`: `jeryu forge pr create --head f --base main --title T --draft`
  → `ForgeCommands::Pr(PrCommands::Create{ head, base, draft:true, .. })` (replaces the MR
  parse test; note `--source/--target` → `--head/--base`).
- `ci_run_rejects_gitlab_kind`: `jeryu ci run --kind gitlab` must `try_parse` **err** (flag
  removed); `--kind native|github` ok.

### 5.3 No-regression gates (run if/when those harnesses are wired; otherwise document N/A)

- **tuiwright** (TUI snapshot harness): TUI lens labels must track CLI renames — runners
  lens shows `jeryu runner` verbs, pipeline lens shows `ci`. Re-snapshot after rename;
  assert no `gitlab/pool/MR` strings in captured frames.
- **Playwright** (React web): BFF/route labels mirroring CLI (`forge`/`ci`/`runner`) — only
  relevant once Web BFF binds (web spec); assert page text has no `GitLab/Merge Request`.
- **MCP tools-call**: `jeryu mcp tools --json` manifest must not advertise tool names
  containing `gitlab/pool/mr/pipeline`; tools rename to `forge_*`/`ci_*`/`runner_*`.
- **verdict-replay** (`jeryu-api` `/benchmarks/replay`, `replay-verifier`): `proof verify`
  output must be replay-stable — same `ChangeSet` ⇒ identical `ProofPlan` hash (matches
  `ci-compiler` determinism contract, `ci-compiler/lib.rs:805` test pattern).

### 5.4 Zero-evidence invariant (D1, the binding acceptance criterion)

> CLI help snapshots and `crates/jeryu-cli/src|tests` contain **no** case-insensitive match
> for `gitlab`, `jitforge`, `JitForge`, `nitro`, nor the `mr`/`merge request`/`--kind gitlab`/
> `--emit-gitlab` tokens. The `grep -REnia` and help-walk tests above are the gate.

---

## 6. Risks & hardest seams

1. **`runnerd` enrollment gap (highest risk).** `runner enroll/rotate` have no backing API
   — `runnerd` is a STUB (`DispatchEngine`/`DispatchMode` only). The whole `runner` subtree's
   semantics (drain-wait, token rotation) currently live in jeryu's `PoolService` against
   GitLab. Mitigation: scaffold `RunnerCommands` + adapters, leave `enroll/rotate` returning
   a typed "not yet wired" error, and file a core gap to Codex. `list/drain/pause/resume` can
   land first via `ci-scheduler::LeaseBook`.

2. **GitLab IID ↔ PR number (D4).** Everything prints/accepts GitLab IID (`!{iid}`, `--mr-iid`,
   `mr.iid: Option<i64>`). `forge-core::PullRequest.number: u64` is GitHub-style. Off-by-model
   bugs are easy (IID is per-project; PR number is per-repo). Every `--mr-iid`/`iid`/`!N`
   surface (in `mr.rs`, `agent merge` `dispatch_back_ops.rs:177`) must convert to PR `number`
   and print `#N`. Snapshot the renderer.

3. **pipeline→ci-run model mismatch.** Source `pipeline` commands assume a *running* remote
   GitLab pipeline you poll (`list_pipelines`, `cancel_pipeline`, bridges/downstream). The
   `jeryu-*` model is **compile→IR→schedule** (`ci-compiler` → `ci-ir::Pipeline` →
   `ci-scheduler::MergeQueue`/`LeaseBook`). `ci run` therefore needs a *file/ref* input
   (`--file`/`--repo --ref`), not a `pipeline_id`. Downstream/bridge pipelines collapse to IR
   `edges`. `--emit-gitlab` is deleted, not renamed. This is a behavioral re-shape, not a
   rename — the riskiest correctness seam.

4. **`exec` Custom-Executor protocol removal.** `ExecCommands{Config,Prepare,Run,Cleanup}`
   (`cli_defs_aux.rs:17`) is the GitLab Runner Custom-Executor contract and is special-cased
   in `main`/`dispatch_back.rs:100` (`unreachable!("handled in main")`). Removing it (D1) means
   deleting the `main.rs` early-dispatch branch too, or the binary panics. Replacement is
   `jeryu-runner-oci`/`jeryu-runner-native` (D5) invoked by `jeryu-runnerd`, not a CLI verb.

5. **db-layer reads bound to GitLab job shape.** `fetch_ci_job_runs` (`dispatch_support.rs`)
   and `upsert_tracked_pipeline_from_gitlab` (`pipeline.rs:169`) read GitLab job/pipeline
   payloads then persist to the (kept) SQLite ledger. The persistence is jeryu-native and
   stays; the *ingest source* must switch to `ci-scheduler`/`runnerd` job records. Column
   `runner_pool` in `ci_job_bottlenecks` should be relabeled (no "pool" leak in JSON output —
   check `--json` snapshots, not just `--help`).

6. **Pervasive doc-string leakage.** Many `///` help strings hard-code "GitLab" beyond the
   obvious commands: `AccessCommands` doctor/repair/project (`cli_defs_commands.rs:117-148`),
   `pool repair --prune-outdated` (`pool.rs:21`), `secrets` Vault (ok), `mr.rs` module header.
   The help-walk test must cover `get_long_about` and **arg help** text, not just subcommand
   names — the original snapshot test only checked names (`cli_tests.rs:428`) and would miss
   these. This is the most likely cause of an acceptance miss.

7. **`access`/`policy audit` are GitLab-shaped end-to-end.** `policy audit` prints
   `local-gitlab` target + main-relay actor (`dispatch_back.rs:67-87`); `access` doctors
   "local GitLab access". Decide per locked decisions whether these become `jeryu access`
   (repo hygiene) or are dropped; either way every literal must go. Coordinate with the
   repo-admin port spec to avoid double-owning these files.
