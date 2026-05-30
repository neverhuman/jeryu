# Port Spec 07 — Jankurai Proof Governance + Bugtracker (P07)

Status: ready-to-execute · Owner: this worker · Product: **jeryu** · Edition: 2024

Scope: (a) retarget the Jankurai foundation (owner-map / test-map / proof-lanes /
generated-zones) to the fused `jeryu-*` layout and wire the `check-*.py` scripts;
(b) port jeryu's RedlineDB bug domain + `bug_*` MCP tools into a new
`crates/jeryu-bugtracker` (NOT GitLab issues); (c) map jeryu's autonomy
conditions onto the `jeryu-proof` `ConditionRegistry`.

Hard constraints from LOCKED DECISIONS: **D1** zero `gitlab`/`jitforge`/`JitForge`/
`Nitro`/`CrateVault` literals survive; only `jeryu`/`jeryu-*`. **D2** engine
crates are renamed (`proofcore -> jeryu-proof`, `forge-core -> jeryu-core`,
`cratevault* -> jeryu-cache*`, `cratevault-service -> jeryu-cache-service`, etc).
**D4** MR/merge-request -> PullRequest/PR everywhere. **D3** keep jeryu's
SQLite/RedlineDB `db/` layer + daemons + ratatui TUI + React web; GitLab backend
is replaced 100% by `jeryu-*` core.

> Codex owns the core/engine crates (`jeryu-proof` engine internals,
> `jeryu-core`, `jeryu-ci-scheduler`). This worker's writes are confined to:
> `crates/jeryu-bugtracker/**`, `agent/**` map files (data, not engine),
> `scripts/check-*.py`, `docs/port/**`. Where this spec needs a change inside a
> Codex-owned crate (e.g. registering a `bug_*`-adjacent condition or adding the
> Jankurai-from-jeryu-maps loader), it is flagged **[hand-off to Codex]**.

---

## 1. Source inventory

### 1.1 Jankurai foundation — `/home/ubuntu/jeryu/agent/` (read-only)

| Source file | Purpose (one line) |
|---|---|
| `/home/ubuntu/jeryu/agent/owner-map.json` | Flat `{ "owners": { "<path-or-glob>": "<owner-team>" }, "workspace": "jankurai" }` mapping every repo path to an owning team (`workspace`, `agent`, `ops`, `data`, `contracts`, `evidence-gate`, `tools`, `apps`, `ai`, `standard`). 159 entries. |
| `/home/ubuntu/jeryu/agent/test-map.json` | Flat `{ "tests": { "<path>": { "command": "...", "purpose": "..." } }, "workspace": "jankurai" }` — per-path proof command (mostly `just score`, plus targeted `cargo test -p jeryu --lib ...`). |
| `/home/ubuntu/jeryu/agent/proof-lanes.toml` | `[[lane]]` array: `name`, `command`, `purpose`. Lanes: `fast`, `audit`, `security`, `release`, `runtime-sqlite-kafka`, `runtime-redlinedb-jansu`, `release-control-plane`. |
| `/home/ubuntu/jeryu/agent/generated-zones.toml` | `[[zone]]` array: `command`, `path`, `read_only`, `source`, `generator`, `owner`, `regenerate`. Declares machine-generated artifacts (`agent/repo-score.{json,md}`, `schemas/web-api.openapi.json`, `schemas/websocket-events.schema.json`, `contracts/generated`). |
| `/home/ubuntu/jeryu/agent/audit-policy.toml` | `advisory_on`/`fail_on` severity gates, `minimum_score = 85`, `[history]` dedupe/rotation, and a large `[scan].excluded_paths` list with per-path rationale (lines 30–262). |
| `/home/ubuntu/jeryu/agent/boundaries.toml` | Architecture boundary contract: `[db].root_paths`, `[rust].forbidden_domain_imports`, `[queues]`, `[public_api].paths`, `[cross_runtime].paths`, stack id `rust-ts-vite-react-redline-jansu-bounded-python`. |
| `/home/ubuntu/jeryu/agent/jankurai-install.toml` | Install manifest: `schema_version`, `standard_version = 0.9.0`, `target_stack_id`, `[[templates]]` with merge policies. |
| `/home/ubuntu/jeryu/agent/JANKURAI_STANDARD.md` | Bootstrap doc (lines 1–10): tells agents to read the maps before editing; **lines 7–9 carry GitLab auth/SSH-remote instructions that must be rewritten (D1).** |
| `/home/ubuntu/jeryu/agent/guard-policy.toml`, `security-policy.toml`, `tool-adoption.toml`, `ux-qa.toml`, `standard-version.toml`, `coverage-sources.toml` | Auxiliary policy inputs (guard rules, secret/dep policy, tool-adoption tracking, UX-QA thresholds, version pin). Carry forward as data, scrub literals. |
| `/home/ubuntu/jeryu/agent/repo-score.{json,md}`, `score-history.{csv,jsonl}` | Generated audit artifacts. **Do not port content; regenerate.** Listed as generated zones / scan exclusions. |

### 1.2 jit proof engine — `/home/ubuntu/jeryu_rust/jit/crates/proofcore/` (→ `jeryu-proof`; **Codex-owned**)

| Source file | Purpose |
|---|---|
| `crates/proofcore/src/lib.rs` | Re-exports `engine`, `matcher`, `policy`. |
| `crates/proofcore/src/policy.rs` | Types: `OwnerRule`, `TestRule`, `GeneratedZone`, `ProofLane`, `ChangeSet { repo, pr: PullRequestId, head_sha, paths, agent_authored }`, `ProofPlan`, `ProofEvidence`, `ProofBlocker { OwnerlessPath, UnmappedProofLane, GeneratedZoneEditDenied, MissingEvidence, FailedLane }`. **Already PR-shaped** (uses `PullRequestId`). |
| `crates/proofcore/src/engine.rs` | `ProofEngine::{new, plan, verify}` + `default_phase7_engine()` (lines 161–210) which hardcodes owner/test/lane rules using **banned literals** `forge-core`, `proofcore`, `ci-scheduler`, `agentbridge`, `phase7-cli`. `verify()` mints a `ProofWitness` via `Receipt::new(ReceiptKind::ProofWitness, …)` from `forge_core`. |
| `crates/proofcore/src/matcher.rs` | `PathPattern` — exact / `prefix*` / `prefix/**` glob matcher. |
| `crates/proofcore/Cargo.toml` | `name = "proofcore"`, dep `forge-core`. **description line 3 contains "JitForge Nitro" (banned, D1).** |
| `crates/forge-core/src/phase7.rs` | `ChangedPath { path, sensitive }`, `ProofWitness { id, repo, pr: PullRequestId, head_sha, changed_paths, lanes, owners, receipt }`. |
| `crates/forge-core/src/receipt.rs` | `Receipt`, `ReceiptKind::ProofWitness`. |

### 1.3 jeryu autonomy condition registry — `/home/ubuntu/jeryu/src/autonomy/conditions.rs`

`ConditionRegistry::default()` (lines 36–216) is the **named hard-stop registry**.
`HardStop { name, reason, details }`; `CondFn = fn(&EvidencePack, &[AgentApprovalReceipt]) -> Option<HardStop>`; `NamedCondition { name, func }`. `evaluate(requested, pack, receipts)` fails closed on unknown names (`unknown_condition:<name>`).
Locally-evaluated detectors: `evidence_missing`, `evidence_signature_invalid` (rejects `stub`/`sha256-hmac-stub`/unsigned; requires `ed25519`), `secret_scan_failed`, `secret_scan_missing`, `sast_failed`, `dependency_scan_failed`, `reviewer_blocked`, `reviewer_abstained_required`, `lockfile_only_change`, `prompt_injection_suspected`, `coverage_threshold_lowered`, `snapshot_mass_replacement`, `changes_security_scanner_config`, `changes_release_or_deploy_policy`, `changes_agent_prompts_or_judge_policy`, `touches_secret_handling`, `removes_or_weakens_tests`, `introduces_new_external_code_source`, `lockfile_diff_without_manifest_diff`.
Externally-supplied (orchestrator injects): `sha_drift`, `policy_sha_drift`, `missing_required_review_role`, `missing_evidence_pack`, `codeowners_not_satisfied`, `freeze_window_active`, `budget_exceeded`, `training_use_required_but_disallowed`, `judge_signature_invalid`, `destructive_database_change`, `dependency_count_delta_gte_5`, `all_files_have_targeted_tests`, `release_artifact_unsigned`, `release_sbom_missing`, `release_provenance_missing`, `rollback_drill_failed`.
**Banned-literal seams inside this file:** path constants at lines 442–468 reference `.jeryu/autonomy/policies/...` (OK), `proof-lanes.toml`/`agent/proof-lanes.toml` (OK), and **`.gitlab/security-policies`, `.gitlab/ci/`** (lines 438, 459 — must be dropped/rewritten under D1).

### 1.4 jeryu bug domain — `/home/ubuntu/jeryu/src/bugtracker/` + `/home/ubuntu/jeryu/src/db/`

| Source file | Purpose |
|---|---|
| `src/bugtracker/mod.rs` | Module root. Re-exports `ops::{branch_name, generate_bug_id, parse_report_json, ranking_key, validate_transition}` and all `types`. Header invariant: "RedlineDB is the only durable backend". |
| `src/bugtracker/types.rs` | `CanonicalBugReport` (validated bug intake) + `validate()` → `BugStatus` (lines 46–67): requires non-empty text fields, `no_secrets_confirmed == true`, `difficulty ∈ 1..=5`; empty repro+evidence ⇒ `NeedsInfo`, else `NeedsTriage`. |
| `src/bugtracker/types_enums.rs` | `BugSeverity {S0..S4}`, `BugPriority {P0..P4}`, `BugStatus {NeedsTriage, NeedsInfo, Accepted, Ready, InProgress, Blocked, FixProposed, Reviewing, Verifying, Done, Duplicate, Invalid, CannotReproduce, WontDo}` (+ `is_terminal`, `as_str`, `parse`), `BugSort {Rank, Severity, Priority, Difficulty, Ready, Updated, Attempts}`, `AttemptStatus {Pending, Started, Failed, FixProposed, Verified, Abandoned}`. |
| `src/bugtracker/types_records.rs` | `BugEvidenceInput`, `BugProjectInput`, `BugProject`, `BugRecord` (carries `body: CanonicalBugReport`, `attempt_count`, `failed_attempt_count`), `BugEvent`, `BugAttemptInput`, `BugAttempt`, `BugDetail {bug, events, attempts}`. **`BugAttemptInput`/`BugAttempt` carry `pr_url: Option<String>`** (already PR-named). |
| `src/bugtracker/ops.rs` | Pure logic: `generate_bug_id` (SHA-256 of target/source/title/ts → `bug-<10hex>`), `branch_name` → `bug/<id>-<slug>`, `validate_transition` (terminal status cannot reopen), `ranking_key`, `parse_report_json`. Has unit tests (lines 75–128). |
| `src/bugtracker/render.rs` | `canonical_markdown(&CanonicalBugReport) -> String` — renders the canonical bug template. |
| `src/db/bugtracker_repo.rs` | `BugTrackerRepo { pool: AnyPool }` over **RedlineDB/SQLite** (`sqlx::AnyPool`). Methods: `open_default` (via `crate::state::Db::open`), `install_schema`, `add_project`, `project`, `list_projects`, `link_projects`, `submit_bug` (idempotency-keyed), `list_bugs`, `ready_bugs` (filters `failed_attempt_count < 3`), `show_bug`, `update_bug` (validates transition), `record_attempt`, private `append_event`, `attach_attempt_counts`, `by_idempotency_key`. |
| `src/db/bugtracker_repo_schema.rs` | `bugtracker_schema_ddl()` — DDL for `bug_projects`, `bug_project_edges`, `bugs`, `bug_events`, `bug_attempts`, `bug_links`, `bug_external_refs`, `bug_evidence` + indexes. `bug_attempts.pr_url`, `bug_external_refs.provider/external_id`. |
| `src/db/bugtracker_repo_decode.rs` | Row→struct decode + `base_select_with`, `sort_bugs`. |
| `src/db/bugtracker_repo_tests.rs` | Integration tests against a fresh sqlite pool (`fresh_bugtracker_pool`). |
| `src/capability.rs` | `AgentIntent` enum — bug variants at lines 69–95: `BugSubmit{report, idempotency_key}`, `BugList{project,status,sort}`, `BugShow{bug_id}`, `BugReady{project}`, `BugUpdate{bug_id,status,severity,priority,component,owner}`, `BugRecordAttempt{bug_id, attempt}`. |
| `src/mcp/tools.rs` | MCP tool surface. `ToolKind::{BugSubmit,BugList,BugShow,BugReady,BugUpdate,BugRecordAttempt}`; arg-parse (lines 110–166), `tool_definition` titles/descriptions (lines 242–277 — **descriptions say "RedlineDB tracker"; keep**), `tool_input_schema` (lines 412–448). |

---

## 2. Target layout in `/home/ubuntu/jeryuRUST`

### 2.1 New crate `crates/jeryu-bugtracker`

```
crates/jeryu-bugtracker/
  Cargo.toml                 # name = "jeryu-bugtracker", edition = "2024" (D3)
  src/
    lib.rs                   # pub mod domain; pub mod repo; pub mod render; pub mod mcp; re-exports
    domain/
      mod.rs                 # re-export types + ops  (← src/bugtracker/{mod,types*}.rs)
      types.rs               # CanonicalBugReport + validate()   (← types.rs)
      enums.rs               # BugSeverity/Priority/Status/Sort/AttemptStatus  (← types_enums.rs)
      records.rs             # Bug* records + BugDetail           (← types_records.rs)
      ops.rs                 # generate_bug_id/branch_name/validate_transition/ranking_key  (← ops.rs)
    repo/
      mod.rs                 # BugTrackerRepo (sqlx::AnyPool)      (← db/bugtracker_repo.rs)
      schema.rs              # bugtracker_schema_ddl()             (← db/bugtracker_repo_schema.rs)
      decode.rs              # row decode + base_select_with/sort  (← db/bugtracker_repo_decode.rs)
    render.rs                # canonical_markdown                  (← bugtracker/render.rs)
    mcp.rs                   # bug_* ToolKind/arg-parse/schema → BugIntent (split out of src/mcp/tools.rs)
  tests/
    bugtracker_repo.rs       # ← db/bugtracker_repo_tests.rs (sqlite/RedlineDB integration)
    bug_mcp_tools_call.rs    # MCP tools/call round-trip for the 6 bug_* tools
```

Decision: the bug **domain + repo move into a workspace crate** (not a leaf in the
monolith `src/`) so `jeryu-proof`/daemons/TUI/web can all depend on it without a
cycle. The `db/` layer is **kept** (D3): `BugTrackerRepo` stays `sqlx::AnyPool` over
SQLite/RedlineDB; `open_default()` still routes through `jeryu`'s `state::Db`. The
monolith keeps a thin re-export shim `src/bugtracker/mod.rs -> pub use jeryu_bugtracker::*;`
and `src/db/bugtracker_repo.rs -> pub use jeryu_bugtracker::repo::*;` so existing
call-sites (`capability_actions.rs`, `tui`, `web`) compile unchanged during cutover.

`Cargo.toml` (target):
```toml
[package]
name = "jeryu-bugtracker"
description = "jeryu RedlineDB bug domain, repo, and bug_* MCP tools"  # no banned literals
version.workspace = true
edition = "2024"                 # D3
[dependencies]
anyhow = { workspace = true }
chrono = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }
sqlx = { workspace = true, features = ["any", "sqlite", "runtime-tokio"] }
jeryu-core = { path = "../jeryu-core" }   # for PullRequestId/RepoId if domain references them
[dev-dependencies]
tempfile = { workspace = true }
tokio = { workspace = true, features = ["macros", "rt"] }
```
Add `"crates/jeryu-bugtracker"` to root `Cargo.toml` `members`.

### 2.2 Jankurai foundation data — `agent/` in the fused repo

The fused repo's `scripts/check-*.py` (see §1, `/home/ubuntu/jeryuRUST/scripts/`)
expect the **jit-shaped** structured map, NOT jeryu's flat map. Two distinct
schemas are in play and they MUST be reconciled (this is the hardest seam — §6):

- jeryu `owner-map.json`: `{ "owners": { "<path>": "<owner>" } }` (string→string).
- jit `check-owner-test-map.py` / `check-agent-maps.py` expect:
  `{ "owners": [ { "paths": [...], "owners": [...], "required_reviews": N } ] }`
  and `{ "routes": [ { "paths": [...], "commands": [...], "proof_lane": "..." } ] }`.

Target shape under `agent/` (authored by this worker, consumed by check scripts and
by the **[hand-off to Codex]** `jeryu-proof` loader that replaces `default_phase7_engine`):

```
agent/
  owner-map.json        # jit-shaped: owners[] entries, paths point at jeryu-* crates
  test-map.json         # jit-shaped: routes[] entries, proof_lane ∈ proof-lanes.toml
  proof-lanes.toml      # lanes retargeted to jeryu-* (see §3)
  generated-zones.toml  # zones retargeted; generator strings scrubbed of banned literals
  audit-policy.toml     # carry forward; scrub excluded_paths of src/git_host/gitlab_stub.rs etc.
  boundaries.toml       # carry forward; scrub gitlab paths
  JANKURAI_STANDARD.md  # rewrite lines 7-9 (GitLab auth) → jeryu-core auth surface
```

Required-root coverage (both check scripts assert these top-level roots are owned
AND test-routed): `agent`, `bins`, `config`, `configs`, `crates`, `docs`,
`examples`, `fixtures`, `ops`, `policies`, `scripts`, `tests`. The retargeted maps
MUST include an entry for each.

### 2.3 Condition registry target — `jeryu-proof` (**Codex-owned**, hand-off)

The condition registry stays in the engine. Today it lives at
`/home/ubuntu/jeryu/src/autonomy/conditions.rs` (monolith) and there is a separate
`ProofEngine` in `proofcore`. P07 consolidates the **named-condition catalog** into
`crates/jeryu-proof` (renamed `proofcore`) as `jeryu_proof::conditions::ConditionRegistry`,
keeping the exact 36 condition names, fail-closed semantics, and `ed25519`-only
signature rule. Provide it to Codex as the port input below (§3.3). The monolith's
`src/autonomy/conditions.rs` becomes a re-export of `jeryu_proof::conditions`.

---

## 3. Rewire map

### 3.1 Proof engine (`proofcore` → `jeryu-proof`)

| Source symbol / data | Current (GitLab / jit) source | Target jeryu-* type / API |
|---|---|---|
| crate `proofcore` | `crates/proofcore` | `crates/jeryu-proof`; `Cargo.toml` description drops "JitForge Nitro" → "jeryu proof planning and witness engine" |
| dep `forge-core` | `proofcore/Cargo.toml:10` | `jeryu-core` |
| `ProofEngine`, `ChangeSet.pr: PullRequestId`, `ProofBlocker`, `ProofWitness` | `policy.rs`, `engine.rs`, `forge-core/phase7.rs` | unchanged shape (already PR-named per D4); only crate/dep renames |
| `default_phase7_engine()` owner rules | `engine.rs:162-176` (`crates/forge-core/**`, `crates/proofcore/**`, `crates/ci-scheduler/**`, `crates/agentbridge/**`, `crates/phase7-cli/**`) | **delete the hardcoded fn**; load rules from `agent/owner-map.json`. If a default is kept, rename paths → `crates/jeryu-core/**`, `crates/jeryu-proof/**`, `crates/jeryu-ci-scheduler/**`, `crates/jeryu-agentbridge/**`, `crates/jeryu-cli/**` **[hand-off to Codex]** |
| `default_phase7_engine()` test/lanes | `engine.rs:177-208` (`cargo test -p proofcore`, `-p ci-scheduler`, `-p agentbridge`) | `cargo test -p jeryu-proof`, `-p jeryu-ci-scheduler`, `-p jeryu-agentbridge` **[hand-off to Codex]** |
| lane `merge-queue-sim` semantics | "merge queue" wording | acceptable (queue of PRs); keep name `merge-queue-sim` but doc-comment says "PR merge queue" (D4) |
| `Receipt::new(ReceiptKind::ProofWitness, …)` | `forge_core::receipt` | `jeryu_core::receipt` |

### 3.2 Jankurai maps (jeryu flat → jit structured, jeryu-* paths)

| Source data | Current source | Target |
|---|---|---|
| owner `"src/merge/": "workspace"`, `"src/repos/"`, `"src/repo_browser/"` | `owner-map.json:138-140` | keep paths; these are jeryu's **PR/repo** surfaces (already not GitLab) |
| owner `".gitlab/issue_templates/bug.md": "workspace"` | `owner-map.json:10` | **drop** (D1); bug intake is `CanonicalBugReport`, not a GitLab template |
| owner `".gitlab-ci.yml": "ops"` | `owner-map.json:124` | **drop**; CI is `jeryu-ci-*` |
| test routes for `crates/**` | jit `default_phase7_engine` lanes | route `crates/jeryu-bugtracker/**` → lane `bug-domain` (`cargo test -p jeryu-bugtracker`) |
| proof-lane `release-control-plane` cmd | `proof-lanes.toml:32-34` (`cargo test -p jeryu ...`) | keep (`-p jeryu` is the product crate); ensure referenced source files still exist post-fusion |
| proof-lane `runtime-redlinedb-jansu` | `proof-lanes.toml:26-29` | keep; RedlineDB is the kept backend (D3) |
| generated zone `agent/repo-score.{json,md}` generator `jankurai audit` | `generated-zones.toml:1-17` | keep binary name `jankurai` (it is the jeryu auditor, not a banned literal) |
| generated zone `schemas/web-api.openapi.json` generator `jeryu_export_schemas` | `generated-zones.toml:19-26` | keep (`jeryu_*` bins survive) |
| `check-generated-zones.py` required zone `receipts/generated/** → "cratevault-service"` | `/home/ubuntu/jeryuRUST/scripts/check-generated-zones.py:14-16` | **rewrite generator string** `cratevault-service` → `jeryu-cache-service` (D2) **[this worker edits the script]** |
| `check-generated-zones.py` `docs/generated/** → scripts/render-policy-docs.sh` | same | keep |
| `check-docs.py` required markers `["JitForge","CrateVault","Phase 12"]` etc. | `/home/ubuntu/jeryuRUST/scripts/check-docs.py:4-9` | **rewrite** banned markers → `["jeryu", ...]`; update the required-doc set to jeryu docs **[this worker edits the script]** |
| `JANKURAI_STANDARD.md` lines 7-9 (`~/.jeryu/jeryu.env`, `gitlab_auth::resolve_or_repair_default()`, `GitLabClient::from_jeryu_env_or_repair()`, `ssh://git@127.0.0.1:2224`) | `agent/JANKURAI_STANDARD.md` | rewrite to jeryu-core auth/host API; remove `glab`/GitLab-PAT prose (D1) |

### 3.3 Autonomy conditions (jeryu monolith → `jeryu-proof::conditions`)

| Source symbol / data | Current source | Target |
|---|---|---|
| `ConditionRegistry`, `HardStop`, `CondFn`, `NamedCondition` | `src/autonomy/conditions.rs:13-253` | `jeryu_proof::conditions::*` (verbatim semantics) **[hand-off to Codex]** |
| `EvidencePack`, `AgentApprovalReceipt`, `ReviewDecision`, `ScanOutcome`, `ChangedFile` deps | `crate::autonomy::types` | stay in monolith `jeryu` autonomy types OR move to `jeryu-core`; registry takes them by ref either way |
| const `SECURITY_SCANNER_PATH_PREFIXES` entry `".gitlab/security-policies"` | `conditions.rs:438` | **drop** (D1) |
| const `RELEASE_DEPLOY_PATH_PREFIXES` entry `".gitlab/ci/"` | `conditions.rs:459` | **drop** (D1); keep `ops/ci/`, `deploy/`, `infra/`, `k8s/`, `helm/`, `terraform/` |
| condition `changes_release_or_deploy_policy` path `agent/proof-lanes.toml` | `conditions.rs:446` | keep (jeryu path) |
| fail-closed `unknown_condition:<name>`; `ed25519`-only signature rule | `conditions.rs:230-252,272-296` | keep verbatim |
| 4 Wave-3 release conditions (`release_artifact_unsigned`, `release_sbom_missing`, `release_provenance_missing`, `rollback_drill_failed`) | `conditions.rs:178-212` | keep; remain `cond_externally_supplied` |

### 3.4 Bugtracker (jeryu monolith → `jeryu-bugtracker`) — NO GitLab issues

| Source symbol / data | Current source | Target jeryu-* type / API |
|---|---|---|
| `BugTrackerRepo` (RedlineDB/SQLite) | `src/db/bugtracker_repo.rs` | `jeryu_bugtracker::repo::BugTrackerRepo` (unchanged `sqlx::AnyPool`) |
| `bugtracker_schema_ddl()` tables | `bugtracker_repo_schema.rs` | `jeryu_bugtracker::repo::schema::bugtracker_schema_ddl` (DDL unchanged) |
| `BugAttempt.pr_url`, `BugAttemptInput.pr_url` | `types_records.rs:78,93` | **already PR-named** (D4) — keep; no `mr_url` anywhere |
| `bug_external_refs.provider` | `schema.rs:74-83` | keep generic; `provider` value MUST NOT default to `gitlab`. Default `sync_status='local'` (no remote issue tracker). Acceptable provider values: `"local"`, `"jeryu"`, `"github"` |
| `BugProjectInput.provider_kind` | `types_records.rs:22` | keep field; values map to jeryu hosts, never `gitlab` |
| `AgentIntent::Bug*` variants | `src/capability.rs:69-95` | `jeryu_bugtracker::mcp::BugIntent` (or keep in monolith `AgentIntent`; bug arm dispatches into `jeryu-bugtracker`) |
| `ToolKind::Bug*` + tool defs `bug_submit/bug_list/bug_show/bug_ready/bug_update/bug_record_attempt` | `src/mcp/tools.rs:242-277,412-448` | `jeryu_bugtracker::mcp` tool defs; descriptions keep "RedlineDB tracker" wording (RedlineDB is kept, D3); **NOT** GitLab issue tools |
| `propose_patch` description "open an MR" | `src/mcp/tools.rs:226-228` | rewrite to "open a PR" (D4) — *adjacent file; flag if owned elsewhere* |
| `request_merge` / `ToolKind::RequestMerge` "merge an MR" | `src/mcp/tools.rs:236-241` | rewrite MR→PR wording (D4) |

---

## 4. Dependencies & ordering

Strict order (later steps blocked by earlier):

1. **[Codex] crate renames + workspace wiring exist first.** `jeryu-core`
   (`forge-core`) and `jeryu-proof` (`proofcore`) must be renamed and building,
   exporting `PullRequestId`, `RepoId`, `ChangedPath`, `ProofWitness`, `Receipt`,
   `ReceiptKind`. Without `jeryu-core`, `jeryu-bugtracker` cannot compile its
   optional `PullRequestId` reference, and `jeryu-proof` cannot resolve `Receipt`.
2. **[Codex] persistence layer / `state::Db` is reachable.** `BugTrackerRepo::open_default()`
   calls `crate::state::Db::open()`. In the fused repo this is jeryu's kept SQLite/
   RedlineDB `db/` layer (D3). `jeryu-bugtracker` either depends on a `jeryu-db`
   crate exposing `Db`/`AnyPool`, or `open_default` stays behind a feature and the
   monolith provides the pool. Decide before porting the repo. Blocks §2.1 `repo/`.
3. **This worker — `jeryu-bugtracker` crate (§2.1).** Port domain (pure, no deps on
   1–2 except optional `PullRequestId`), then repo (needs 2), then `mcp.rs`. Add to
   workspace members. Add monolith re-export shims so call-sites compile.
4. **This worker — `agent/` maps retarget + check scripts (§2.2, §3.2).** Can proceed
   in parallel with 3, but the **structured-schema reconciliation** (jeryu flat →
   jit `owners[]`/`routes[]`) must land before the check scripts pass. The
   `jeryu-proof` loader that replaces `default_phase7_engine()` is **[Codex]** and
   consumes these files — coordinate the JSON schema with Codex first.
5. **[Codex] `jeryu-proof::conditions` consolidation (§3.3).** Independent of 3/4 but
   blocked by 1. Provide the scrubbed condition list as input.

Blocks / external: the ci-scheduler is Codex-owned; any lane named
`merge-queue-sim` whose command references `-p jeryu-ci-scheduler` depends on that
rename landing. The web/TUI bug surfaces depend on step 3's re-export shims.

---

## 5. Tests / acceptance gate

Run from `/home/ubuntu/jeryuRUST`. All commands must pass; invariants are hard gates.

### 5.1 Build + unit

```bash
cargo build --workspace
cargo test -p jeryu-bugtracker            # domain + repo + render unit/integration
cargo test -p jeryu-proof                 # ProofEngine plan/verify + conditions registry
cargo test -p jeryu --lib bugtracker      # re-export shim compiles; existing call-sites green
cargo test -p jeryu --lib db::bugtracker_repo
cargo test -p jeryu --lib autonomy::      # condition registry (if still surfaced in monolith)
```

Invariants:
- `jeryu-bugtracker` tests reproduce the ported cases: `validation_requires_no_secrets_confirmation`,
  `validation_lands_missing_repro_in_needs_info`, `generated_ids_use_bug_prefix_and_hash_length`
  (`bug-` prefix, id len 14), `terminal_status_blocks_reopen`, `markdown_contains_canonical_sections`.
- `jeryu-proof`: `ownerless_path_blocks_merge`, `unmapped_proof_lane_blocks_merge`,
  `generated_zone_blocks_agent_edit`, `proof_witness_is_minted_when_required_lanes_pass`.
- conditions: `unknown_condition_fail_closes`, `secret_scan_failed_triggers`,
  `wave3_release_conditions_are_registered`, `wave3_release_conditions_are_externally_supplied`,
  `clean_pack_no_hard_stops` (asserts `evidence_signature_invalid` fires on unsigned — `ed25519` rule preserved).

### 5.2 Jankurai map / zone / docs gate (the `check-*.py` scripts)

```bash
python3 scripts/check-agent-maps.py        # required roots owned + test-routed
python3 scripts/check-owner-test-map.py     # owners[]/routes[] well-formed, no missing roots
python3 scripts/check-generated-zones.py    # docs/generated/** + receipts/generated/** zones (jeryu-cache-service)
python3 scripts/check-docs.py               # required docs + markers (jeryu markers, no banned)
python3 scripts/check-fixtures.py           # tests/fixtures/*.json parse
```

Invariants:
- `check-agent-maps.py` and `check-owner-test-map.py` print `ok` and exit 0 — proves
  the flat→structured schema reconciliation (§2.2) succeeded for all 12 required roots.
- `check-generated-zones.py` proves the `cratevault-service`→`jeryu-cache-service`
  generator rewrite landed (zero `cratevault` literal).
- `check-docs.py` proves no `JitForge`/`CrateVault` markers are required by the gate.

### 5.3 No-regression: MCP / TUI / web / verdict-replay

```bash
cargo test -p jeryu --test bug_mcp_tools_call        # MCP tools/call for 6 bug_* tools round-trips
# tuiwright (ratatui golden snapshots) — bug list/detail panels still render:
just tui-snapshots   ||  cargo test -p jeryu --lib tui::  -- bug
# Playwright (React web) — bug surfaces unchanged:
cd apps/web && npx playwright test         # or: just web-e2e
# verdict-replay (autonomy ledger determinism) — condition evaluation reproducible:
cargo test -p jeryu --lib autonomy::replay
```

Invariants:
- MCP `tools/list` exposes exactly `bug_submit, bug_list, bug_show, bug_ready,
  bug_update, bug_record_attempt`; `tools/call` on each returns the canonical
  `{success, message, data}` output schema.
- tuiwright/Playwright snapshots byte-identical to pre-port (the move is mechanical;
  rendering must not change).
- verdict-replay: replaying a recorded EvidencePack through `ConditionRegistry::evaluate`
  yields the identical ordered `HardStop` list (determinism preserved).

### 5.4 Zero-evidence scan (D1 — banned literals)

```bash
# Must return ZERO hits across the fused source (excluding /target, /.git, generated score artifacts):
grep -rniE 'gitlab|jitforge|nitro|cratevault|merge[_-]?request|\bMR\b' \
  /home/ubuntu/jeryuRUST/crates/jeryu-bugtracker \
  /home/ubuntu/jeryuRUST/agent \
  /home/ubuntu/jeryuRUST/scripts/check-*.py \
  /home/ubuntu/jeryuRUST/docs/port/07-proof-and-bugtracker.md
# Targeted: no banned literal survives in the renamed proof crate manifest/engine:
grep -rniE 'jitforge|nitro|proofcore|forge-core' /home/ubuntu/jeryuRUST/crates/jeryu-proof
```

Invariants:
- Zero matches for `gitlab`, `jitforge`, `JitForge`, `Nitro`, `CrateVault`,
  `merge_request`/`merge-request`, standalone `MR`. (The condition-name token
  `merge-queue-sim` is allowed — it is a PR merge queue; document the carve-out.)
- `bug_external_refs.provider` default is `local`, never `gitlab`.

---

## 6. Risks & hardest seams

1. **Two incompatible Jankurai map schemas (highest risk).** jeryu's `owner-map.json`/
   `test-map.json` are **flat string maps** (`{path: owner}`), but the fused repo's
   `scripts/check-owner-test-map.py:23-24` and `check-agent-maps.py:25-30` expect
   **structured arrays** (`owners[].{paths,owners,required_reviews}`,
   `routes[].{paths,commands,proof_lane}`). A naive copy of jeryu's files fails the
   gate. The maps must be **rewritten into the structured schema** with `proof_lane`
   values that exist in `proof-lanes.toml`, and `required_reviews >= 1` on every
   owner entry. This schema is also the input contract for the **[Codex]**
   `jeryu-proof` loader replacing `default_phase7_engine()` — agree the JSON shape
   with Codex before authoring, or the engine and the check scripts will diverge.

2. **`default_phase7_engine()` is hardcoded with banned crate names.**
   `engine.rs:162-208` literally names `forge-core`, `proofcore`, `ci-scheduler`,
   `agentbridge`, `phase7-cli`. It is Codex-owned. If Codex keeps a default, the
   safest fusion is to **delete it and load from `agent/*`**; otherwise every name
   must be rewritten to `jeryu-*`. The proofcore tests reference
   `crates/proofcore/src/lib.rs` as an owned+lane'd path (`engine.rs:299`) and will
   break on rename — Codex must update fixtures.

3. **`state::Db` coupling for the bug repo.** `BugTrackerRepo::open_default()` reaches
   into `crate::state::Db::open()` (monolith). Moving the repo into a workspace crate
   risks a dependency cycle (`jeryu` → `jeryu-bugtracker` → `jeryu`). Break it by
   either (a) exposing the SQLite/RedlineDB pool from a lower `jeryu-db` crate, or
   (b) feature-gating `open_default` so the monolith injects the pool. Decide in
   step 4.2 before porting `repo/`.

4. **Banned literals embedded in the check scripts and conditions, not just docs.**
   `check-generated-zones.py:16` hardcodes `cratevault-service`; `check-docs.py:6-7`
   requires `JitForge`/`CrateVault` markers; `conditions.rs:438,459` carry `.gitlab/...`
   path prefixes. These are **executable gates** — scrubbing them is mandatory or the
   zero-evidence scan and the doc gate contradict each other.

5. **`audit-policy.toml` excluded_paths reference GitLab-era source files.** Lines
   177–181 exclude `src/git_host/gitlab_stub.rs`, `src/git_host/github.rs`,
   `src/git_host/mod.rs`. Once the GitLab backend is removed (D3) these paths vanish;
   the exclusions become dead and the audit may re-flag whatever replaces them. The
   excluded_paths list must be re-derived against the fused tree, not copied.

6. **Snapshot/golden drift.** The bug-domain move is mechanical, but tuiwright and
   Playwright snapshots can drift if any string (e.g. tool description, panel label)
   is touched while scrubbing literals. Keep bug_* tool descriptions ("RedlineDB
   tracker") byte-identical; only MR→PR wording in `propose_patch`/`request_merge`
   may legitimately change a snapshot, and that snapshot must be regenerated and
   reviewed.

7. **`merge-queue-sim` / "merge queue" vocabulary vs the standalone-`MR` ban.** The
   PR merge queue is a legitimate concept; the lane name and `cargo test ... concurrent`
   wording stay. Document this carve-out so the §5.4 grep's `\bMR\b` pattern is not
   widened to catch "merge" and produce false D1 failures.
