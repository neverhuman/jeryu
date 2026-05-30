# Port Spec 03 — MCP Transport + Tools Subsystem

**Status:** authored for execution by a later agent.
**Owner crate (new):** `crates/jeryu-mcp`.
**Backend it wires onto:** `crates/jeryu-agentbridge` (renamed from `agentbridge`) + `crates/jeryu-proof` (renamed from `proofcore`), with `crates/jeryu-core` (renamed from `forge-core`) types.
**Product invariant (D1):** ZERO `gitlab`/`jitforge`/`JitForge`/`Nitro` literals survive in any file this spec produces. Only `jeryu`/`jeryu-*`.
**Concept invariant (D4):** MR / merge-request → PullRequest/PR; `mr_iid` → `pr_number`; `pipeline` → CI run; GitLab project_id → `RepoId`.

This spec ports the MATURE MCP shell at `/home/ubuntu/jeryu/src/mcp/**` + `/home/ubuntu/jeryu/src/autonomy/mcp_tools.rs` (which currently sit over a GitLab-backed `capability` layer) onto the jit engine, which today has **no MCP layer at all** (recon §1.1–1.4: "JitForge has no MCP layer yet; agentbridge lacks JSON-RPC stdio protocol").

---

## 1. Source inventory

Every source file/module to study, with a one-line purpose. All paths are read-only sources under `/home/ubuntu/jeryu`.

### 1a. MCP transport + protocol + tools (`/home/ubuntu/jeryu/src/mcp/`)

| Source file | Purpose (one line) |
|---|---|
| `src/mcp.rs` (L1–19) | Module root; declares `core`/`http`/`tools`, re-exports `start_mcp_stdio`, `start_mcp_http`, `tool_manifest`; consts `MCP_PROTOCOL_VERSION = "2025-11-25"` (L17), `TOOL_PREFIX = "jeryu."` (L18). |
| `src/mcp/core.rs` (L1–58) | Declares `McpCore { client }` (L37–45) and `McpSessionState { initialized, client_actor }` (L22–34); `#[path]`-includes the dispatch/io/jsonrpc/protocol/tools companions. **This is the only file holding the GitLab handle** (`crate::gitlab_client::GitlabClient`). |
| `src/mcp/core_io.rs` (L1–39) | stdio JSON-RPC server: line-buffered stdin loop, batches → array reply, `start_mcp_stdio(client)`. |
| `src/mcp/core_protocol.rs` (L1–51) | serde types: `Implementation`, `InitializeRequestParams`, `ListToolsRequestParams`, `CallToolRequestParams` (L27–32), `JsonRpcRequest` (L34–42), untagged `IncomingMessage` = Request \| Batch \| Raw (L44–50). |
| `src/mcp/core_dispatch.rs` (L1–176) | JSON-RPC routing: `handle_line` (L5), `handle_request` (L33) → `initialize`/`ping`/`tools/list`/`tools/call`; `handle_notification` (L65) for `notifications/initialized`; `handle_initialize` (L81) enforces `protocolVersion` match, emits `serverInfo.name = "jeryu"` (L130). |
| `src/mcp/core_tools.rs` (L1–60) | `handle_tools_call`: resolves tool by name (strips `TOOL_PREFIX`, L34), `build_intent`, then `crate::capability::execute_intent(intent, &ctx, &core.client)` (L48). Builds `CapabilityContext::mcp(...)` (L43). **This is the seam that must repoint from `capability::execute_intent` to `jeryu-agentbridge`.** |
| `src/mcp/core_jsonrpc.rs` (L1–37) | `ensure_initialized` (L6), `jsonrpc_result` (L14), `jsonrpc_error` (L22). Pure JSON-RPC 2.0 envelope helpers. |
| `src/mcp/http.rs` (L1–114) | Axum HTTP transport: `McpHttpState { core, sessions: HashMap<String, McpSessionState> }` (L38–51), `mcp_router` (L53), `start_mcp_http(client, bind)` **loopback-only bind enforcement** (L64–79), `handle_mcp_get` → 405 (L81), `handle_mcp_delete` session teardown (L90). |
| `src/mcp/http_post.rs` (L1–123) | `handle_mcp_post`: header validation, `Mcp-Method`/`Mcp-Name` must match body (L37–46, L92–105), `initialize` mints a UUID `Mcp-Session-Id` (L48–73), subsequent calls require session + `MCP-Protocol-Version` header (L75–116). No batch over HTTP (L25). |
| `src/mcp/http_support.rs` (L1–97) | Loopback/Origin enforcement `is_loopback_origin` (L47), `validate_mcp_http_headers` (L11), `http_jsonrpc_error`/`http_jsonrpc_response`/`http_error`, always stamps `mcp-protocol-version` response header (L77–80). |
| `src/mcp/tools.rs` (L1–451) | **The 16-tool catalog.** `ToolKind` enum (L10–28), `ToolDefinition` (L30–38), `build_intent` maps args→`AgentIntent` (L52–167), `tool_manifest()` filters `action_registry::REGISTRY` by `Surface::Capability` (L170–178), `tool_definition(action_id)` (L180–301) holds titles/descriptions/annotations, `tool_input_schema` (L311–450) holds per-tool JSON Schema. **`request_merge` carries `mr_iid` (L106, L404, L408) — the field to rename to `pr_number`.** |
| `src/mcp/tools_schema.rs` (L1–84) | Schema/parse helpers: `tool_annotations` (L3), `object_schema`/`string_schema`/`integer_schema`/`array_schema`/`enum_schema`, `parse_modifications` → `FileModification` (L58), `parse_hypotheses` → `HypothesisPatch` (L71). |
| `src/mcp/tests.rs` (L1–346) | Conformance suite: `manifest_includes_capability_tools` (L91), `manifest_covers_all_capability_actions` (L116), `loopback_origin_validation_is_strict` (L140), stdio init+list (L149), HTTP init→list→call→delete (L189), malformed-JSON → -32700 (L239), unknown tool → -32601 (L258), non-loopback Origin → 403 (L284), unknown session → 404 (L308), GET → 405 (L332). **Port these verbatim as the acceptance gate.** |

### 1b. Autonomy / Evidence-Gate MCP descriptors (`/home/ubuntu/jeryu/src/autonomy/mcp_tools.rs`, L1–219)

7 descriptor-only tools (recon §4.7: "✅ PARTIAL"). These are *not yet wired* into `tool_manifest()`; the module comment (L4–7) says they fold in "when Codex's Phase 9 wires the MCP server."

| Tool name | Category | Lease | Backend target |
|---|---|---|---|
| `vibegate.inspect_autonomy_pack` (L42) | ReadOnly | no | parse `.jeryu/autonomy` PolicyBundle (autonomy port, spec 04) |
| `vibegate.get_evidence_pack` (L54) | ReadOnly | no | `jeryu-agentbridge::receipt(...)` / EvidencePack store |
| `vibegate.get_verdict` (L65) | ReadOnly | no | verdict store (autonomy port) |
| `vibegate.list_receipts` (L76) | ReadOnly | no | `jeryu-agentbridge` receipt index by pack id |
| `vibegate.get_agent_health` (L87) | ReadOnly | no | agent run telemetry (`jeryu-obs`) |
| `vibegate.doctor` (L100) | ReadOnly | no | provider sweep (live; autonomy port) |
| `vibegate.run_review` (L107) | Mutating | yes | reviewer orchestration (agent-review port) |
| `vibegate.approve_mr` (L122) | Mutating | yes | `jeryu-agentbridge::propose_fix`/quorum; **`mr_iid` field (L129/L131) → `pr_number`** |
| `vibegate.propose_autonomy_edit` (L139) | Mutating | yes | opens a PR against `.jeryu/autonomy/` |

`ToolDescriptor::to_mcp_json()` (L175–192) and `manifest_jsons()` (L212) already emit the exact MCP manifest shape (`{name,title,description,inputSchema,outputSchema,annotations}`) — reuse this conversion verbatim so the autonomy descriptors and the 16-tool catalog produce one unified manifest.

### 1c. Current GitLab-backed backend the tools call into (PORT AWAY FROM)

These are the symbols `core_tools.rs` reaches through today. They must be **replaced**, not ported as-is.

| Source file | Purpose |
|---|---|
| `src/capability.rs` (L20–96) | `AgentIntent` tagged enum (16 live variants + `ListAllowedActions`); `CapabilityContext` (L141), `CapabilityResponse { success, message, data }` (L191), `FileModification`/`HypothesisPatch` (L179–189). |
| `src/capability_actions.rs` (L10–107) | `execute_intent` — the big match that dispatches each `AgentIntent` to its backend fn; bug_* handlers (L113–265). |
| `src/capability_execute.rs` (L5–92) | `propose_patch` → `client.create_branch` + `commit_actions_with_sha` + `create_merge_request` (GitLab). |
| `src/capability_execute_support.rs` (L4–193) | `fetch_capsule` (L4), `run_tests` (L19; commits `.gitlab-ci.yml`, `trigger_pipeline`), `race_patches` (L87), `request_merge` (L176; calls `client.accept_merge_request(project_id, mr_iid)`). |
| `src/capability_inspect_read.rs` (L4–134) | `explain_blockers` (L4), `get_ci_bottlenecks` (L66), `plan_validation` (L94). |
| `src/capability_inspect_snapshot.rs` (L4–101) | `get_system_snapshot` (L4; reads `gitlab_ready`, L52), `get_pipeline_jobs` (L65; `list_pipeline_jobs_with_downstream`). |
| `src/gitlab_client_merge_requests.rs` (L20–99) | `create_merge_request` (L20), `accept_merge_request(project_id, mr_iid)` (L79). **GitLab REST — fully replaced by jeryu-* core.** |
| `src/gitlab_client_{branches,projects,pipelines,core}.rs` | `create_branch`, `commit_actions_with_sha`, `trigger_pipeline`, `list_pipeline_jobs_with_downstream`, `is_ready` — all GitLab REST, all replaced. |
| `src/bugtracker/types.rs` (L15–40) | `CanonicalBugReport`; `BugAttemptInput`, `BugStatus`/`BugSeverity`/`BugSort` (`types_enums.rs`). RedlineDB-backed — **KEEP per D3** (SQLite+RedlineDB db/ layer survives). bug_* tools keep their existing repo backend. |

---

## 2. Target layout in `/home/ubuntu/jeryuRUST`

New crate: **`crates/jeryu-mcp`** (workspace member; add to `Cargo.toml [workspace] members`). The transport is deliberately a separate crate (recon: "Build MCP transport as separate crate") so it can be a thin adapter over `jeryu-agentbridge` without pulling Axum into the engine crates.

```
crates/jeryu-mcp/
  Cargo.toml                # deps: jeryu-agentbridge, jeryu-proof, jeryu-core,
                            #       tokio, axum, serde, serde_json, anyhow, uuid, tracing
  src/
    lib.rs                  # mod root; consts MCP_PROTOCOL_VERSION, TOOL_PREFIX = "jeryu.";
                            # pub use start_mcp_stdio, start_mcp_http, tool_manifest
    core/
      mod.rs                # McpCore { bridge: AgentBridgeHandle }, McpSessionState
      io.rs                 # <- port of core_io.rs (stdio loop)
      dispatch.rs           # <- port of core_dispatch.rs (initialize/ping/tools.list/tools.call)
      protocol.rs           # <- port of core_protocol.rs (JSON-RPC serde types) [verbatim]
      jsonrpc.rs            # <- port of core_jsonrpc.rs (envelope helpers) [verbatim]
      tools_call.rs         # <- port of core_tools.rs; calls backend::dispatch(intent) NOT capability::execute_intent
    http/
      mod.rs                # <- port of http.rs (McpHttpState, mcp_router, start_mcp_http, loopback bind)
      post.rs               # <- port of http_post.rs (session mint, header binding)
      support.rs            # <- port of http_support.rs (loopback origin) [near-verbatim]
    tools/
      mod.rs                # ToolKind, ToolDefinition, tool_manifest(), tool_definition()
      schema.rs             # <- port of tools_schema.rs [near-verbatim]
      catalog.rs            # the 16 tool descriptors (titles/desc/annotations/input_schema)
    backend/
      mod.rs                # ProductIntent enum (renamed AgentIntent), dispatch(intent, ctx, &bridge) -> ToolResponse
      patch.rs              # propose_patch / race_patches / run_tests -> jeryu-agentbridge
      merge.rs              # request_merge -> jeryu-agentbridge mergeability/propose_fix (pr_number)
      inspect.rs            # get_*, explain_blockers, plan_validation, fetch_capsule
      bugs.rs               # bug_* -> KEEP RedlineDB repo (db/ layer, D3)
  tests/
    mcp_conformance.rs      # <- port of src/mcp/tests.rs (stdio + HTTP + manifest invariants)
```

Notes:
- The original `#[path = "..."]` include-trick (e.g. `core.rs` pulling in `core_io.rs`) should be flattened into ordinary `mod`/sub-`mod` files — it was an artifact of the single-crate product layout, not load-bearing.
- `ToolResponse` replaces `CapabilityResponse { success, message, data }` but **keeps the same JSON shape** so `core_tools.rs`’s `structuredContent`/`content[0].text`/`isError` block (`core_tools.rs` L52–59) ports unchanged and existing MCP clients see no contract drift.
- `McpCore` no longer holds a `GitlabClient`; it holds a handle to `jeryu_agentbridge::AgentBridge` (or `Arc<Mutex<AgentBridge>>`, since some bridge methods take `&mut self` — see §6).

If a later decision prefers fewer crates, the alternative is a `jeryu-mcp` *module* inside `jeryu-agentbridge`; this spec assumes a standalone crate to keep Axum/tokio out of the engine.

---

## 3. Rewire map

`project_id: i64` → `RepoId` everywhere (jit identity, `jeryu-core::ids::RepoId`). `mr_iid: i64` → `pr_number` (carried as the numeric part of `PullRequestId`, jit type `jeryu-core::ids::PullRequestId`). `pipeline` → CI run (jit `ci-scheduler` / `jeryu-ci-*`).

### 3a. Tool → backend call map (the 16-tool catalog)

| Tool (MCP name `jeryu.*`) | Source intent + args (`tools.rs`) | Current GitLab backend | Target jeryu-* type / API |
|---|---|---|---|
| `fetch_capsule` | `FetchCapsule { job_id }` (tools.rs L56) | `capability_execute_support::fetch_capsule` → `CapabilityRepo::latest_evidence_by_job_id` | `jeryu-agentbridge` evidence/receipt read keyed by CI-run/job id; backed by KEPT `db/` evidence store (D3). Returns `ToolResponse.data = EvidencePack`. |
| `get_system_snapshot` | `GetSystemSnapshot` (L59) | `capability_inspect_snapshot::get_system_snapshot` → `repo.system_snapshot()` + `client.is_ready()` (`gitlab_ready` L52) | `jeryu-agentbridge` snapshot over `jeryu-core` repo state + `ci-scheduler` queue; replace `gitlab_ready` field with `engine_ready` (no `gitlab` literal). |
| `get_pipeline_jobs` | `GetPipelineJobs { project_id, pipeline_id }` (L60) | `client.list_pipeline_jobs_with_downstream` (GitLab) | **rename tool → `get_ci_run_jobs`**, args `{ repo, ci_run_id }`; backend = `jeryu-ci-scheduler` run inspection. `pipeline_id` → `ci_run_id`. |
| `get_ci_bottlenecks` | `GetCiBottlenecks { project_id, ref_name, limit }` (L64) | `CapabilityRepo::ci_job_bottlenecks` | `jeryu-agentbridge` analytics over KEPT `db/` CI history; args `{ repo, ref_name?, limit? }`. |
| `explain_blockers` | `ExplainBlockers { entity_type, entity_id }` (L72) | `capability_inspect_read::explain_blockers` (job/release/**merge**) | `jeryu-agentbridge::mergeability(pr)` → `Mergeability { mergeable, blockers }` (api.rs L167–189). `entity_type="merge"` → PR blockers; entity_id → `pr_number`. job/release branches read evidence/release store. |
| `plan_validation` | `PlanValidation { project_id, test_ids, ref_name }` (L76) | `capability_inspect_read::plan_validation` (selector-miss count) | `jeryu-proof::ProofEngine::plan(&ChangeSet)` (engine.rs L41) — turn `test_ids`/paths into a `ChangeSet`, return the resulting `ProofPlan.lanes` or first `ProofBlocker`. This is a strictly *better* typed validation than the selector-miss heuristic. |
| `run_tests` | `RunTests { project_id, target_ref, test_scope }` (L81) | `run_tests` → create branch + commit `.gitlab-ci.yml` + `trigger_pipeline` | `jeryu-ci-scheduler` run trigger over `jeryu-runnerd` (D5: OCI-first then native). `test_scope` enum `unit|integration|lint|full` → proof lanes. Returns `ci_run_id` not `pipeline_id`. |
| `propose_patch` | `ProposePatch { project_id, branch_name, base_ref, commit_message, modifications, mr_title }` (L86) | `capability_execute::propose_patch` → branch+commit+`create_merge_request` (GitLab) | `jeryu-agentbridge::dry_run_patch` (api.rs L192) → `DryRunPatchRequest { scope, pr, base_sha, patches }`, then `propose_fix` (api.rs L276). `modifications` (`{file_path, content}`) → `FilePatch { path, patch }`. `mr_title` → PR title. Emits `Receipt(AgentProposedFix)`. |
| `race_patches` | `RacePatches { project_id, base_branch, commit_message, hypotheses }` (L97) | `race_patches` → N branches + N pipelines | N× `jeryu-agentbridge::dry_run_patch` (one per `HypothesisPatch.branch_suffix`) + N CI runs via `jeryu-ci-scheduler`; "keep first green" = poll `MergeQueue`/run outcomes. Returns `ci_run_id` per hypothesis. |
| `request_merge` | `RequestMerge { project_id, mr_iid, source_branch, target_branch }` (L103) | `request_merge` → `client.accept_merge_request(project_id, mr_iid)` (GitLab) | **`mr_iid` → `pr_number`.** Backend = `jeryu-ci-scheduler::MergeQueue::enqueue(pr, Some(witness))` then `process_all(&DeterministicValidator)` (merge_queue.rs L86/L141); admission requires a `ProofWitness` from `jeryu-proof`. `source_branch`/`target_branch` carried on the `PullRequest` (`jeryu-core::phase7::PullRequest`). Direct "accept" is replaced by queue admission (no direct merge). |
| `bug_submit` | `BugSubmit { report, idempotency_key }` (L109) | `BugTrackerRepo::submit_bug` (RedlineDB) | **KEEP** (D3): `jeryu` RedlineDB repo unchanged; `report: CanonicalBugReport`. |
| `bug_list` | `BugList { project, status, sort }` (L116) | `BugTrackerRepo::list_bugs` | **KEEP** RedlineDB. |
| `bug_show` | `BugShow { bug_id }` (L130) | `BugTrackerRepo::show_bug` | **KEEP** RedlineDB. |
| `bug_ready` | `BugReady { project }` (L133) | `BugTrackerRepo::ready_bugs` | **KEEP** RedlineDB. |
| `bug_update` | `BugUpdate { bug_id, status, severity, priority, component, owner }` (L139) | `BugTrackerRepo::update_bug` | **KEEP** RedlineDB. |
| `bug_record_attempt` | `BugRecordAttempt { bug_id, attempt }` (L162) | `BugTrackerRepo::record_attempt` | **KEEP** RedlineDB. |

> Catalog count check: `tool_manifest()` enumerates exactly the `ToolKind` variants `fetch_capsule, get_system_snapshot, get_pipeline_jobs(→get_ci_run_jobs), get_ci_bottlenecks, explain_blockers, plan_validation, run_tests, propose_patch, race_patches, request_merge, bug_submit, bug_list, bug_show, bug_ready, bug_update, bug_record_attempt` = **16 tools**. The 9 `vibegate.*` autonomy descriptors (§1b) fold in via `manifest_jsons()` to form the consolidated catalog.

### 3b. Type / symbol rewire map (transport + protocol + concepts)

| Source symbol / data | Current (GitLab) source | Target jeryu-* type / API |
|---|---|---|
| `McpCore { client: GitlabClient }` | `mcp/core.rs` L37 | `McpCore { bridge: Arc<Mutex<jeryu_agentbridge::AgentBridge>> }` |
| `core::start_mcp_stdio(client)` | `mcp.rs` L13 / `core_io.rs` L9 | `jeryu_mcp::start_mcp_stdio(bridge)` |
| `http::start_mcp_http(client, bind)` | `mcp.rs` L14 / `http.rs` L64 | `jeryu_mcp::start_mcp_http(bridge, bind)` (loopback-only bind preserved verbatim, http.rs L68–69) |
| `crate::capability::execute_intent(intent, &ctx, &core.client)` | `core_tools.rs` L48 | `jeryu_mcp::backend::dispatch(intent, &ctx, &bridge)` — repointed dispatcher |
| `AgentIntent` (tagged enum, 16 variants) | `capability.rs` L20–96 | `jeryu_mcp::backend::ProductIntent` (same serde tag layout; `mr_iid`→`pr_number`, `project_id`→`repo`, `pipeline_id`→`ci_run_id`) |
| `CapabilityResponse { success, message, data }` | `capability.rs` L191 | `jeryu_mcp::ToolResponse` (identical JSON shape; keeps `content`/`structuredContent`/`isError` wrapping in tools_call.rs) |
| `CapabilityContext::mcp(request_id, actor, protocol_version)` | `capability.rs` L158 / `core_tools.rs` L43 | `jeryu_mcp::McpCallContext` (actor + request id; feeds `AgentId`/`AgentScope` for `jeryu-agentbridge`) |
| `FileModification { file_path, content }` | `capability.rs` L179 | `jeryu_agentbridge::FilePatch { path, patch }` (api.rs L42) |
| `HypothesisPatch { branch_suffix, modifications }` | `capability.rs` L185 | `Vec<FilePatch>` per hypothesis branch (no engine type needed) |
| MR object (`mr.iid`, `mr.web_url`) | `gitlab_client_merge_requests.rs` L20 | `jeryu_core::phase7::PullRequest` / `PullRequestId` (phase7.rs L36); `web_url`→PR URL string |
| `accept_merge_request(project_id, mr_iid)` | `gitlab_client_merge_requests.rs` L79 | `jeryu_ci_scheduler::MergeQueue::{enqueue, process_all}` (merge_queue.rs L86/L141) gated by `ProofWitness` |
| `trigger_pipeline` / `pipeline_id` | `gitlab_client_pipelines.rs` L5 | `jeryu-ci-scheduler` run + `jeryu-runnerd` exec; id field `ci_run_id` |
| `MCP_PROTOCOL_VERSION = "2025-11-25"` | `mcp.rs` L17 | unchanged (MCP spec version, not a brand literal) |
| `TOOL_PREFIX = "jeryu."` | `mcp.rs` L18 | unchanged (already `jeryu.`) |
| `serverInfo.name = "jeryu"` | `core_dispatch.rs` L130 | unchanged (already `jeryu`) — but **audit** the `description` strings (L131, L133) for stray brand words. |
| `tool_manifest()` source-of-truth = `action_registry::REGISTRY` filtered by `Surface::Capability` | `tools.rs` L170–178 | jit has no `action_registry`; replace with a static `CATALOG: &[&str]` of the 16 tool ids in `tools/catalog.rs` (the TUI action registry is a separate port; do not depend on it here). |
| `vibegate.approve_mr` arg `mr_iid` | `autonomy/mcp_tools.rs` L129/L131 | `pr_number` (pattern stays string but rename key) |
| `gitlab_ready` snapshot field | `capability_inspect_snapshot.rs` L52 | `engine_ready` (no `gitlab` literal) |
| `.gitlab-ci.yml` commit (run_tests) | `capability_execute_support.rs` L46 | replaced — CI is triggered through `jeryu-ci-scheduler`, no `.gitlab-ci.yml` file write |

---

## 4. Dependencies & ordering

This crate is a **leaf adapter**; it blocks on the engine renames and on the agentbridge surface being complete. Build order:

1. **D2 renames must land first** (Codex owns these; do NOT edit them here). `jeryu-mcp` depends by path on:
   - `jeryu-core` (was `forge-core`) — `phase7::{PullRequest, PullRequestId, RepoId, ProofWitness, Receipt, AgentId, AgentScope, ChangedPath, JitForgeError→rename, JitForgeResult→rename}`. **Note:** `JitForgeError`/`JitForgeResult` (api.rs L4, error.rs) are brand literals and MUST be renamed by the core port (e.g. `JeryuError`/`JeryuResult`) before this crate compiles clean under D1.
   - `jeryu-agentbridge` (was `agentbridge`) — `AgentBridge`, `context`, `mergeability`, `dry_run_patch`, `proof_plan`, `run_proof`, `propose_fix`, `hotfix`, `receipt` (api.rs L142–373).
   - `jeryu-proof` (was `proofcore`) — `ProofEngine::{plan, verify}`, `ChangeSet`, `ProofPlan`, `ProofEvidence`, `ProofBlocker`, `default_phase7_engine` (engine.rs).
   - `jeryu-ci-scheduler` (Codex-owned) — `MergeQueue`, `DeterministicValidator`, `QueueSummary` (merge_queue.rs).

2. **Persistence layer (KEEP, D3)** — `bug_*` tools depend on the jeryu RedlineDB/SQLite `db/` layer being ported into the fused repo (`BugTrackerRepo`, `CanonicalBugReport`, `BugAttemptInput`). This is the **db/ port** (separate spec). Until that lands, the 6 bug_* tools must be feature-gated or stubbed to `ToolResponse::error("bug tracker unavailable")` so the other 10 tools can ship.

3. **agentbridge surface gaps that block full fidelity** (recon §"JitForge's Agent APIs"):
   - `request_merge` needs queue admission to be reachable from a tool call → depends on `jeryu-agentbridge` exposing a "submit PR to merge queue with witness" method, OR `jeryu-mcp::backend::merge.rs` orchestrating `mergeability` → `proof_plan` → `run_proof` → `MergeQueue::enqueue` directly. Decide before coding `merge.rs`.
   - `AgentBridge::dry_run_patch`/`propose_fix`/`hotfix` take `&mut self` (api.rs L192/L276/L319) while `context`/`mergeability`/`receipt` take `&self`. The `McpCore.bridge` handle therefore needs interior mutability (`Arc<Mutex<AgentBridge>>`) — **this is a hard ordering constraint on the `McpCore` field type (§6).**
   - Tools needing analytics/telemetry (`get_ci_bottlenecks`, `get_agent_health`) depend on `jeryu-obs` (was `jitforge-obs`) being renamed and exposing read APIs.

4. **What blocks this spec's execution:** the D2 rename PR (esp. `forge-core`→`jeryu-core` incl. `JitForgeError`→`JeryuError`) and the db/ port. `jeryu-mcp` can be **scaffolded** (transport + protocol + manifest, the brandless/db-free 80%) before the db/ port; the bug_* backend wiring is the only piece that must wait.

5. **Workspace registration:** add `crates/jeryu-mcp` to `Cargo.toml [workspace].members`, and set `edition = "2024"` (D3) in the crate manifest — note the workspace currently pins `edition = "2021"` / `rust-version = "1.95"`; the edition-2024 bump is a workspace-wide decision owned by the fusion lead, not this crate alone.

---

## 5. Tests / acceptance gate

### 5a. Exact commands

```bash
# Build + lint the new crate (and the renamed deps it pulls in)
cargo check -p jeryu-mcp --message-format=short
cargo clippy -p jeryu-mcp -- -D warnings

# Unit + conformance suite (ported from src/mcp/tests.rs)
cargo test -p jeryu-mcp

# Workspace must still build/test green (no-regression)
cargo test --workspace

# Zero-evidence brand scan (D1) — MUST return ZERO matches in the crate + this doc
rg -i -n 'gitlab|jitforge|nitro' crates/jeryu-mcp/ docs/port/03-mcp.md

# MR/merge-request concept scan (D4) — MUST be empty (mr_iid is gone; only pr_number/PullRequest)
rg -n 'mr_iid|merge_request|MergeRequest' crates/jeryu-mcp/
```

### 5b. Conformance invariants (port these tests 1:1 from `src/mcp/tests.rs`)

1. **manifest completeness** — `tool_manifest()` contains all 16 tool names (`jeryu.fetch_capsule`, `jeryu.run_tests`, `jeryu.request_merge`, `jeryu.propose_patch`, the 6 `jeryu.bug_*`, etc.). Port `manifest_includes_capability_tools` (tests.rs L91) and `manifest_covers_all_capability_actions` (L116) — the latter now asserts against the static `CATALOG` slice rather than `action_registry::REGISTRY`.
2. **stdio init+list** — `initialize` returns `protocolVersion == MCP_PROTOCOL_VERSION`; `tools/list` returns an array (tests.rs L149).
3. **MCP tools/call round-trip** — HTTP `initialize` → `tools/list` → `tools/call` (e.g. `jeryu.explain_blockers {entity_type:"merge", entity_id:1}`) → `result.content` is an array (tests.rs L189–224). This is the "MCP tools-call" no-regression gate.
4. **session teardown** — `DELETE /mcp` with valid `Mcp-Session-Id` → 204; unknown → 404 (tests.rs L226, L308).
5. **loopback enforcement** — `is_loopback_origin` strict set (tests.rs L140); non-loopback `Origin` → 403 (L284); `start_mcp_http` rejects non-loopback bind (http.rs L68). Plus `GET /mcp` → 405 (L332).
6. **JSON-RPC error codes** — malformed JSON → -32700 (L239), unknown tool → -32601 (L258), uninitialized `tools/list`/`tools/call` → -32002, bad params → -32602. Port `jsonrpc_error` semantics (core_jsonrpc.rs L22).
7. **header binding** — `Mcp-Method` must match body method, `Mcp-Name` must match `params.name` (http_post.rs L37–46, L92–105).
8. **verdict-replay / proof determinism** — for `request_merge` and `plan_validation`, assert the `jeryu-proof::ProofEngine::plan`+`verify` path is deterministic: same `ChangeSet` → same `ProofPlan.lanes` → same `ProofWitness` lanes (mirror proofcore engine.rs tests L257–331). This replaces jeryu's selector-miss heuristic and is the "verdict-replay" invariant for this subsystem.
9. **PR-rename guard** — a dedicated test asserts the `request_merge` tool's `inputSchema.required` contains `pr_number` and does NOT contain `mr_iid`.

### 5c. No-regression matrix (whole-repo gates that must stay green)

| Gate | Command | Applies here? |
|---|---|---|
| MCP tools/call | `cargo test -p jeryu-mcp mcp_conformance` | YES (primary) |
| verdict-replay | proof plan/verify determinism test (5b#8) | YES |
| tuiwright (TUI) | TUI snapshot suite | indirect — only if MCP manifest feeds a TUI lens; otherwise N/A |
| Playwright (web) | web e2e | N/A for MCP transport |
| zero-evidence | `rg -i 'gitlab\|jitforge\|nitro'` → 0 | YES (D1, blocking) |

---

## 6. Risks & hardest seams

1. **`&mut self` on mutating bridge methods (HIGHEST).** `dry_run_patch`/`propose_fix`/`hotfix` take `&mut AgentBridge` (api.rs L192/L276/L319), but `McpCore` is `Clone` and shared across concurrent HTTP sessions (`McpHttpState.sessions`, http.rs L41). The original `McpCore` held a cheaply-cloneable `GitlabClient`. Porting requires `Arc<Mutex<AgentBridge>>` (or making the bridge methods `&self` with internal locking). This serializes all mutating tool calls — acceptable for a loopback dev tool, but it is a real behavioral change vs the stateless GitLab client. **Decide the handle type before writing `core/mod.rs`.**

2. **`request_merge` has no 1:1 replacement.** GitLab path was a single `accept_merge_request` REST call (capability_execute_support.rs L183). The jit path is a multi-step *proof-gated queue admission*: `mergeability` → (if blocked) report blockers; else `proof_plan` → `run_proof` (needs `ProofEvidence` — where does it come from in a tool call?) → `MergeQueue::enqueue(pr, witness)` → `process_all`. The tool can no longer "just merge"; it can at most *enqueue with proof*. The evidence-acquisition step is the seam: a tool caller does not naturally carry `ProofEvidence`. Likely answer: `request_merge` returns `Mergeability` blockers (read-only), and actual admission requires a prior `run_proof`-bearing receipt. **Confirm the intended semantics with the agentbridge owner.**

3. **AgentBridge state is in-memory (`BTreeMap`, state.rs L7).** The original capability backend persisted via `CapabilityRepo`/SQLite. If the MCP server restarts, all PRs/receipts vanish. For parity with jeryu's durable capability grants, the bridge needs DB backing (state.rs comment L6: "Production would back this with Postgres"). This crate must NOT silently rely on volatile state for `propose_patch`/`request_merge`. Flag: persistence is a `jeryu-agentbridge` concern, but it blocks meaningful `propose_patch`→`request_merge` flows over MCP.

4. **`tool_manifest()` source-of-truth swap.** jeryu derives the catalog from `tui::action_registry::REGISTRY` (tools.rs L171) — a cross-subsystem coupling that does not exist in jit. Replacing it with a static `CATALOG` slice is straightforward, but the original test `manifest_covers_all_capability_actions` (tests.rs L116) was a *guardrail against drift between TUI and MCP*. That guardrail is lost; the TUI port (separate spec) must add its own cross-check, or the catalog must live in a shared module both consume.

5. **Schema field renames vs. existing MCP clients.** Renaming `mr_iid`→`pr_number`, `project_id`→`repo`, `pipeline_id`→`ci_run_id` in `inputSchema` (tools.rs L313–449) is a **breaking change to the tool contract**. Any external agent prompt/config referencing the old field names breaks. This is mandated by D4/D1, but downstream agent configs and the autonomy `vibegate.approve_mr` descriptor (mcp_tools.rs L129) must be updated in lockstep.

6. **Brand literals hiding in strings, not just identifiers.** Easy to miss: `serverInfo.description` (core_dispatch.rs L131 "MCP adapter over jeryu capability policy" — OK), `gitlab_ready` JSON key (snapshot L52), `.gitlab-ci.yml` literal (run_tests L46), and the `JitForgeError`/`JitForgeResult` types threaded through agentbridge (api.rs L4). The zero-evidence scan (§5a) must cover JSON string values and schema keys, not only Rust idents.

7. **`vibegate.*` namespace.** The 7+2 autonomy descriptors use the `vibegate.` prefix (mcp_tools.rs L43) while the 16 core tools use `jeryu.` (TOOL_PREFIX). Consolidating into one manifest means two prefixes coexist; confirm whether autonomy tools should be re-prefixed to `jeryu.` for consistency, or kept namespaced. No brand literal issue (`vibegate` is allowed), but it is a catalog-coherence decision.

---

### 5-line summary

1. Spec'd a new leaf crate `crates/jeryu-mcp` (stdio + loopback-HTTP JSON-RPC) ported from `/home/ubuntu/jeryu/src/mcp/**`, repointing the one backend seam (`core_tools.rs` L48 `capability::execute_intent`) onto `jeryu-agentbridge`/`jeryu-proof`/`jeryu-ci-scheduler`.
2. Inventoried all 13 MCP source files + the 7/9 `vibegate.*` autonomy descriptors, and mapped the full **16-tool catalog** (`fetch_capsule`, `propose_patch`, `race_patches`, `request_merge`, `run_tests`, `get_*`, `explain_blockers`, `plan_validation`, `bug_*`) tool-by-tool to its jeryu-* backend call.
3. Locked the concept renames: `mr_iid`→`pr_number` (in `request_merge` + `vibegate.approve_mr`), `project_id`→`RepoId`, `pipeline_id`→`ci_run_id`, GitLab `accept_merge_request`→proof-gated `MergeQueue::enqueue/process_all`; bug_* stays on the KEPT RedlineDB `db/` layer (D3).
4. Ordering: blocks on the D2 engine renames (esp. `forge-core`→`jeryu-core` incl. `JitForgeError`→`JeryuError`) + the db/ port; the brand/db-free 80% (transport+protocol+manifest) can be scaffolded first, bug_* wiring waits.
5. Hardest seams flagged: `&mut self` bridge methods forcing `Arc<Mutex<AgentBridge>>`, `request_merge` having no 1:1 (now proof-gated queue admission needing evidence), in-memory AgentBridge state, loss of the TUI↔MCP drift guardrail, and breaking schema field renames; acceptance gate = ported conformance suite + proof-determinism replay + `rg -i 'gitlab|jitforge|nitro'` → 0.

File written to: **/home/ubuntu/jeryuRUST/docs/port/03-mcp.md**
