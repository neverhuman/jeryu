# Port Spec 04 — Autonomy / Evidence-Gate subsystem → `jeryu-autonomy`

Status: PLAN (to be executed by a later agent).
Product: **jeryu** (fused jeryu × JitForge). Edition 2024.
Source-of-truth product shell to port FROM: `/home/ubuntu/jeryu` (read-only, Rust 2024).
Fused engine repo to port INTO: `/home/ubuntu/jeryuRUST`.

Locked decisions honored here: D1 (zero `gitlab`/`jitforge`/`JitForge`/`Nitro` literals; only `jeryu`/`jeryu-*`), D2 (engine crate renames, incl. `proofcore`→`jeryu-proof`, `forge-core`→`jeryu-core`, `jitforge-api`→`jeryu-api`, `ci-*`→`jeryu-ci-*`), D3 (full fusion: keep jeryu's SQLite/RedlineDB `db/` layer, HTTP daemons, ratatui TUI, React web; GitLab backend replaced 100% by jeryu-* core), D4 (MR / merge-request → **PullRequest / PR**), D5 (runners OCI-first then native sandbox — not load-bearing here).

> **Codex ownership boundary.** Codex owns `jeryu-core` (`forge-core`), `jeryu-proof` (`proofcore`), and `jeryu-ci-scheduler` (`ci-scheduler`). This spec PROPOSES additions to `jeryu-proof` (§4) but the executing agent for THIS task does **not** edit those crates — it writes `jeryu-autonomy` and files a typed dependency request. The split of "port into jeryu-proof vs keep in jeryu-autonomy" is spelled out exhaustively in §4 and §7.

---

## 0. One-paragraph orientation

The Evidence Gate is jeryu's autonomous-delivery control plane: every agent-authored PR is reduced to a signed, SHA-bound **EvidencePack**, reviewed by quorum reviewer agents that each emit a signed **AgentApprovalReceipt**, fused by a *pure* `judge()` into a signed **VibeGateVerdict** (AllowMerge / RequireHuman / Reject), which (if AllowMerge) mints a **MergePassport**. A 40+-entry **hard-stop condition registry** can veto any verdict (veto > approval). Every decision is recorded append-only in a signed **launch_ledger**; a **KillBell** can globally downgrade all verdicts to RequireHuman; a daemon polls open PRs, detects SHA/policy/TTL **drift**, and **auto-rejudges**; **escalation** webhooks fire on RequireHuman; **replay** reconstructs the decision timeline; **shadow** mode dry-runs the gate over git history. Today all inputs come from the host abstraction `GitHost` (GitLab live + GitHub). The port keeps the entire typed object model and policy fusion intact, renames the crate to `jeryu-autonomy`, rewires inputs from the GitLab `GitHost` adapter to the fused **forge PR + jeryu-ci run** surface, and migrates the proof/conditions/quorum core into `jeryu-proof` per §4.

---

## 1. Source inventory

All paths under `/home/ubuntu/jeryu`. Two homes: the library subsystem `src/autonomy/**` and the CLI binary `src/bin/autonomy.rs`. The judge/quorum/rejudge/SHA-bind core lives in sibling modules (`src/agent_review/**`, `src/approval/**`) that the port must move WITH autonomy.

### 1.1 `src/autonomy/**` (library subsystem, ~17.9k LOC incl. tests)

| File | LOC | Purpose (one line) |
|---|---|---|
| `src/autonomy/mod.rs` | 48 | Module root; declares submodules + curated re-exports (`ConditionRegistry`, `EvidencePack`, `SqlLedger`, `VibeGateVerdict`, …). |
| `src/autonomy/types.rs` | 1002 | The **8 canonical typed objects** (IntentCard, CapabilityLease, EvidencePack, AgentApprovalReceipt, VibeGateVerdict, MergePassport, ReleasePassport, LaunchLedgerEntry) + enums (RiskTier R0–R5, ReviewerRole, ReviewDecision, GateDecision, ScanOutcome, LedgerKind) + `SchemaTag<T>` zero-cost `vibegate.*.v1` schema-string wrapper + `CapabilityLease::permits` glob path-deny logic. |
| `src/autonomy/conditions.rs` | 1067 | **The 40+ named hard-stop registry** (`ConditionRegistry`, `HardStop`, `NamedCondition`, `CondFn = fn(&EvidencePack,&[AgentApprovalReceipt])->Option<HardStop>`). Some deterministic (pack-local), some `cond_externally_supplied` (judge/orchestrator injects). Unknown name → `unknown_condition:<name>` fail-closed. **No string-eval / DSL** (decision #3). |
| `src/autonomy/evidence.rs` | 226 | `EvidenceInputs` + `build_evidence_pack` (two-pass sha256 `evidence_digest` over canonical projection, sorts changed_files), `verify_evidence_digest`, `make_gate_receipt`. Re-exported as the public `build_evidence_pack` etc. |
| `src/autonomy/evidence_pack_builder.rs` | 438 | `EvidencePackBuilder` trait + `StandardEvidencePackBuilder` — materializes a fresh **signed** EvidencePack from `GitHost::fetch_pr_diff(repo, mr_iid)` + policy bundle risk classification + ed25519 sign. **Primary GitLab→forge rewire seam.** |
| `src/autonomy/verdict_store.rs` | 577 | `VerdictStore` trait + `SqlVerdictStore` (thin wrapper over `db::autonomy_repo::AutonomyRepo`). `save` idempotent on id + supersedes prior `(repo, merge_request)` rows; `load_latest`, `list_active`, `supersede`. body_json is source of truth. |
| `src/autonomy/ledger.rs` | 541 | `SqlLedger` (append-only, ed25519-only) + `LedgerFilter` + `verdict_issued_entry`, `sign_entry`, `canonical_body_for_signing`, `kind_to_str`/`kind_from_str`. Append refuses stub/HMAC algos. |
| `src/autonomy/kill_bell.rs` | 519 | `KillBell` (SQL-backed, TTL auto-arm), `KillBellState` (Armed/Paused), `BreakGlassReceipt`. `pause`/`resume` append signed `KillBellEngaged`/`KillBellResumed`; `downgrade_if_paused` rewrites GateDecision → RequireHuman. |
| `src/autonomy/auto_rejudge.rs` | 819 | `AutoRejudgeService` — composes pack_builder + orchestrator + `judge()` + verdict_store + ledger into one rejudge cycle (`rejudge(repo, mr_iid, old_verdict)` → `RejudgeOutcome`). Stamps `wave_scope="auto_rejudge"`. |
| `src/autonomy/daemon.rs` | 1537 | Polling daemon: `scan_repo`/`scan_pr` over `GitHost::list_open_prs` + `get_pr_state`; runs drift `check()` (from `agent_review::rejudge`), supersede + signed `MergePassportInvalidated` ledger entry, optional auto-rejudge, escalation. Emits `TickReport`. |
| `src/autonomy/escalation.rs` | 788 | `EscalationConfig`/`WebhookConfig`/`EscalationKind` (Slack/PagerDuty/GenericJson), `EscalationEvent` (RequireHuman / KillBellEngaged), `dispatch_all`, `ReqwestDispatcher` + `FakeDispatcher`. URLs resolved via `llm::secrets` 6-tier chain. |
| `src/autonomy/escalation_loader.rs` | 66 | Loads `escalation:` from `.jeryu/autonomy/autonomy.yml`; missing file/key → disabled default; tolerant of unknown keys. |
| `src/autonomy/escalation_loader_tests.rs` | 165 | Tests for the loader. |
| `src/autonomy/replay.rs` | 811 | `ReplayReport`/`TimelineEvent`/`ReplaySummary` — walk launch_ledger read-only (recorded_at ASC) to reconstruct intent→lease→reviews→verdict→passport→rollback; counts non-ed25519 signatures. |
| `src/autonomy/shadow.rs` | 920 | Shadow mode: replay gate over historical commits, synthesize signed packs, `judge()` with empty receipts, score Match/Disagreement → `agreement_rate`. `ShadowOptions`, `run_shadow`, `render_summary`. |
| `src/autonomy/metrics.rs` | 1177 | Pull-based Prometheus snapshot exporter; reads launch_ledger + kill_bell_state each `collect()`; `render_prometheus` (one HELP/TYPE per name). Zero new deps. |
| `src/autonomy/signing.rs` | 314 | `Signature {key_id, algo, value}`, `default_unsigned`/`stub`, HMAC `SigningKey` (rejected in enforcement), real `EdSigningKey`/`EdVerifier` (ed25519-dalek), `sha256_digest`. |
| `src/autonomy/signing_secrets.rs` | 243 | `resolve_ed25519_signing_key`, `SigningKeyMode` — fail-closed seed resolution from env / signing env file; optional persistent local seed under `~/.jeryu/secrets/signing.env`. |
| `src/autonomy/signing_secrets_helpers.rs` | 130 | Helpers for the above (env/file parsing). |
| `src/autonomy/policy_yaml.rs` | 63 | Strict-typed loaders for `.jeryu/autonomy/policies/*.yml`; `PolicyBundle::from_dir`; canonical keys only, fail-closed on drift. |
| `src/autonomy/policy_yaml_types.rs` | 220 | `RiskPolicy`/`RiskMatcher`/`RiskTierEntry`, `ApprovalsPolicy`/`ApprovalRules`/`QuorumEntry`/`HardStopEntry`, `ReleasePolicy`/`ReleaseBuildRules`/`CanaryRules`/`NightwatchRules`, `ProtectedPathsPolicy`, `FreezePolicy`. All `#[serde(deny_unknown_fields)]`. |
| `src/autonomy/policy_yaml_tests.rs` | 84 | Loader tests. |
| `src/autonomy/risk.rs` | 187 | `RiskClassifier` — walks `risk.yml` tiers in declared order (R5-first), first-match-wins veto semantics; `ClassificationInputs`, `compile_glob`. |
| `src/autonomy/risk_tests.rs` | 104 | Risk classifier tests. |
| `src/autonomy/freeze.rs` | 136 | `FreezeWindows::check(risk, now) -> Option<HardStop>` — calendar freeze enforcement; backs the `freeze_window_active` externally-supplied condition. |
| `src/autonomy/freeze_tests.rs` | 298 | Freeze tests. |
| `src/autonomy/profile.rs` | 162 | `sovereign_plus` profile validator — the "100%-to-prod" on-switch; downgrades to `sovereign` if any Wave 1–4 guardrail missing. Read-only. |
| `src/autonomy/profile_validate.rs` | 212 | Profile validation checks (no side effects). |
| `src/autonomy/profile_tests.rs` | 439 | Profile tests. |
| `src/autonomy/http_server.rs` | 1688 | Hand-rolled HTTP/1.1 server on raw `TcpStream` (zero hyper/axum), `GET` health/metrics + `POST /events` webhook intake (GitHub `pull_request` events today) → `WebhookReceived` ledger entry. `parse_request`/`render_response` pure. |
| `src/autonomy/mcp_tools.rs` | 218 | Read-only MCP tool descriptors (`vibegate.inspect_autonomy_pack`, `vibegate.get_evidence_pack`, `vibegate.get_verdict`, …) for folding into `src/mcp/tools.rs`. |
| `src/autonomy/mcp_tools_tests.rs` | 74 | MCP descriptor tests. |

### 1.2 `src/bin/autonomy.rs` (CLI binary, 2597 LOC)

| Region | Purpose |
|---|---|
| clap `Cmd` tree | Subcommands: `doctor`, `review`, `judge`, `evidence`, `init`, `shadow`, `replay`, `mr validate`, `daemon run`, `kill-bell {pause,resume,status}`, `profile validate`, `metrics dump`, `escalate test`, `freeze check`, `canary …`. |
| `mr validate` (lines ~630–810) | **GitLab live-flow** entrypoint: `provider != "gitlab"` is rejected; builds `GitLabClient::from_jeryu_env_or_repair`, `get_pr_state` + `fetch_pr_diff` → pack → reviewers → `judge()` → verdict → `mint_merge_passport`; writes JSON artifacts; `--emit_status` posts `post_merge_passport_status`. **Primary CLI rewire seam.** |
| `daemon run` (lines ~159–185) | Wraps `autonomy::daemon`; `--fake_git_host`, `--auto_rejudge`, `--repo`, `--tick_once`. |

### 1.3 Sibling modules the port MUST move/depend on (NOT under `src/autonomy/` but inseparable)

| Path | LOC-ish | Purpose | Disposition |
|---|---|---|---|
| `src/agent_review/judge.rs` | ~520 | **Pure policy fusion** `judge(JudgeInputs)->JudgeOutcome`: SHA-bind filter → hard-stop registry + external_hard_stops (ANY → Reject) → quorum → AllowMerge/RequireHuman; `PolicyBundle::quorum_for`. | Move to `jeryu-proof` (§4). |
| `src/agent_review/rejudge.rs` | ~250 | Pure drift `check(verdict, LiveState)->Vec<RejudgeReason>` (NewCommitOnPr, TargetBranchAdvance, PolicyChangeOnTarget, VerdictTtlExpired) + `must_rejudge`. | Move to `jeryu-proof` (§4). |
| `src/agent_review/orchestrator.rs` | — | `ReviewerOrchestrator` trait + `ProductionReviewerOrchestrator` (LLM reviewers). | Keep in `jeryu` shell (LLM/agent surface), depend from `jeryu-autonomy`. |
| `src/agent_review/security.rs`, `nightwatch`, … | — | Per-role reviewer agents. | Keep in `jeryu` shell. |
| `src/approval/quorum.rs` | — | `evaluate_quorum(risk, receipts, approvals, author)->QuorumOutcome` (Met/HumanRequired/Insufficient/Vetoed). | Move to `jeryu-proof` (§4). |
| `src/approval/sha_bind.rs` | — | `verify_sha_binding(pack, receipt)` (Law 4 exact-SHA). | Move to `jeryu-proof` (§4). |
| `src/db/autonomy_repo.rs` | ~600 | `AutonomyRepo` — SQL for `launch_ledger` (append-only triggers, ed25519-only), `verdicts` (body_json + supersede), `kill_bell_state`; `fresh_autonomy_pool` test fixture. | Move to fused `db/` layer (see §4 / Dependency on DB port). |
| `src/db/state.rs::migrate` | — | Installs the append-only `BEFORE UPDATE/DELETE` triggers on `launch_ledger`. | Lives with fused `db/`. |
| `src/git_host/mod.rs` | — | `GitHost` trait, `RepoRef`, `PrSummary`, `PrLiveState`, `PrDiff`, `ChangedFileDiff`, `HostPipeline`, `HostError`. | **Replaced** by forge PR + jeryu-ci surface (see §3, §6). The `gitlab*.rs` adapters are deleted (D1). |

### 1.4 Policy / schema assets (carried verbatim, no rename of `.jeryu/` dir)

- `.jeryu/autonomy/policies/{risk,approvals,release,protected-paths,freeze}.yml` — runtime policy (named conditions referenced by string).
- `.jeryu/autonomy/schemas/*.schema.json` — on-the-wire JSON schemas; CI lints Rust types ↔ schemas together.
- `.jeryu/autonomy/keys/<agent_id>.ed25519.pub`, `.jeryu/autonomy/autonomy.yml` (escalation), `.jeryu/autonomy/prompts/`, `agent/proof-lanes.toml`.
- The string `.jeryu/` is product-correct and stays (it is already `jeryu`, not gitlab/jitforge). Audit: the conditions registry path lists in `conditions.rs` lines 442–470 reference `.gitlab/ci/`, `.gitlab/security-policies`, `.github/workflows/*` as *changed-file glob detectors*. Those are **input-data globs describing a contributor's repo layout**, not jeryu's own backend — see §3 row "scanner/deploy path globs" and §6 R-7 for the D1 decision.

---

## 2. Target layout in `/home/ubuntu/jeryuRUST`

A new workspace crate `crates/jeryu-autonomy` (depends on `jeryu-proof`, `jeryu-core`, the fused `db` crate, and the `jeryu` product shell's LLM/orchestrator surface). The 8 typed objects + persistence + control-plane live here; the **pure decision engine** (conditions, quorum, sha-bind, judge, rejudge drift) moves to `jeryu-proof` (§4).

```
crates/jeryu-autonomy/
  Cargo.toml                       # name = "jeryu-autonomy", edition = "2024"
  src/
    lib.rs                         # <- from src/autonomy/mod.rs (re-exports, renamed)
    types.rs                       # <- src/autonomy/types.rs  (8 objects; schema strings: SEE §6 R-8)
    evidence.rs                    # <- src/autonomy/evidence.rs (build_evidence_pack/verify/digest)
    evidence_pack_builder.rs       # <- src/autonomy/evidence_pack_builder.rs  (REWIRED inputs, §3)
    verdict_store.rs               # <- src/autonomy/verdict_store.rs
    ledger.rs                      # <- src/autonomy/ledger.rs
    kill_bell.rs                   # <- src/autonomy/kill_bell.rs
    auto_rejudge.rs                # <- src/autonomy/auto_rejudge.rs (REWIRED: PrRef + ci run, §3)
    daemon.rs                      # <- src/autonomy/daemon.rs (REWIRED: forge PR poll, §3)
    escalation.rs / escalation_loader.rs
    replay.rs
    shadow.rs                      # (REWIRED: history source from forge, §3)
    metrics.rs
    signing.rs / signing_secrets.rs / signing_secrets_helpers.rs
    policy_yaml.rs / policy_yaml_types.rs
    risk.rs                        # RiskClassifier  (could live in jeryu-proof; SEE §4 note)
    freeze.rs
    profile.rs / profile_validate.rs
    http_server.rs                 # (REWIRED: POST /events PR-webhook payload, §3)
    mcp_tools.rs
    forge_input.rs                 # NEW: adapter trait replacing GitHost (the seam, §3.1)
  tests/
    *                              # ported unit/integration tests (all #[cfg(test)] inline today)

crates/jeryu-proof/                 # OWNED BY CODEX — this task FILES the request in §4, does not edit
  src/
    conditions.rs                  # NEW (ported from src/autonomy/conditions.rs)
    quorum.rs                      # NEW (ported from src/approval/quorum.rs)
    sha_bind.rs                    # NEW (ported from src/approval/sha_bind.rs)
    judge.rs                       # NEW (ported from src/agent_review/judge.rs)
    rejudge.rs                     # NEW (ported from src/agent_review/rejudge.rs)
    evidence_model.rs OR re-export # EvidencePack/Receipt/Verdict types it needs (SEE §4 for the split)
```

CLI binary: fold `src/bin/autonomy.rs` into the fused product CLI as `jeryu autonomy <subcommand>` (the source already notes "Codex can fold this into `cli_defs.rs` later"). Keep the same subcommand names; rename `mr validate` → `pr validate` (D4) keeping `mr` as a hidden alias for one release.

Workspace `Cargo.toml`: add `crates/jeryu-autonomy` to `members`. No `gitlab`/`jitforge`/`Nitro` literals anywhere in the new crate (D1) — `rg -i 'gitlab|jitforge|nitro'` over `crates/jeryu-autonomy/` MUST return 0 (excluding the changed-file glob strings discussed in §6 R-7, which are renamed/removed there).

---

## 3. Rewire map (GitLab `GitHost` → forge PR + jeryu-ci run)

The whole subsystem touches the host through ONE trait, `GitHost` (and its value types `RepoRef`/`PrSummary`/`PrLiveState`/`PrDiff`/`HostPipeline`). The port replaces this with a fused adapter `ForgePrSource` (new `forge_input.rs`) backed by `jeryu-core` PR types + `jeryu-ci` runs. **No autonomy logic changes** — only the data source and the names.

### 3.1 The seam: define `ForgePrSource` to mirror the 5 `GitHost` calls autonomy actually uses

Autonomy uses exactly five `GitHost` methods: `list_open_prs`, `get_pr_state`, `fetch_pr_diff`, `fetch_target_policy_sha`, and (CLI-only) `post_merge_passport_status`. Re-express as:

```rust
// crates/jeryu-autonomy/src/forge_input.rs
#[async_trait]
pub trait ForgePrSource: Send + Sync {
    async fn list_open_prs(&self, repo: &RepoId) -> Result<Vec<PrSummary>>;
    async fn get_pr_state(&self, repo: &RepoId, pr: &PullRequestId) -> Result<PrLiveState>;
    async fn fetch_pr_diff(&self, repo: &RepoId, pr: &PullRequestId) -> Result<PrDiff>;
    async fn fetch_target_policy_sha(&self, repo: &RepoId, target: &str) -> Result<Option<String>>;
    async fn post_gate_status(&self, repo: &RepoId, head_sha: &str, status: GateStatus, summary: &str) -> Result<()>;
}
```
Backed by `jeryu-core::phase7::{RepoId, PullRequestId, PullRequest, ChangedPath}` for identity/diff and by a `jeryu-ci` run query for pipeline/CI status (replacing `HostPipeline`). A `FakeForgePrSource` mirrors today's `FakeGitHost` for tests.

### 3.2 Symbol-level rewire table

| Source symbol / data | Current (GitLab) source | Target jeryu-* type / API |
|---|---|---|
| `GitHost` trait | `src/git_host/mod.rs:542` | `jeryu_autonomy::forge_input::ForgePrSource` (5-method seam, §3.1) |
| `GitLabClient::from_jeryu_env_or_repair` | `src/git_host/gitlab.rs` (used in bin `mr validate` line 648) | Removed (D1). `pr validate` builds a `ForgePrSource` over the in-process fused forge; no external GitLab. |
| `RepoRef` / `repo.slug()` | `src/git_host/mod.rs:66,82` | `jeryu_core::phase7::RepoId` (`RepoId::new`, `.as_str()`). EvidencePack `repo: String` field carries the slug as before. |
| `mr_iid: &str` everywhere | autonomy + git_host | `jeryu_core::phase7::PullRequestId` at the seam; the wire field `EvidencePack`/`VibeGateVerdict.merge_request` is **renamed `pull_request`** (D4) — `mr_iid` local vars → `pr_id`. |
| `PrSummary` / `list_open_prs` | `src/git_host/mod.rs:139` + `gitlab.rs:130` | `ForgePrSource::list_open_prs` over forge open-PR query (jeryu-core / forge HTTP daemon). |
| `PrLiveState {head_sha, target_branch, target_branch_sha, target_policy_sha}` / `get_pr_state` | `src/git_host/mod.rs:157` + `gitlab.rs` | `ForgePrSource::get_pr_state`; `head_sha`/`base_sha`/`target_branch` come from `jeryu_core::phase7::PullRequest`; `target_branch_sha` + `target_policy_sha` from the forge git layer (`jeryu-gitd`). |
| `PrDiff` / `ChangedFileDiff` / `fetch_pr_diff` | `src/git_host/mod.rs:176,194` + `gitlab.rs:163` | `ForgePrSource::fetch_pr_diff`; map to `EvidencePack.changed_files: Vec<ChangedFile>`. `ChangedPath{path,sensitive}` (jeryu-core) → `ChangedFile{path,risk_tags,lines_added,lines_removed}` (line counts from forge diff). |
| `fetch_target_policy_sha` (Law 3) | `GitHost` default + `GitHubClient` override | `ForgePrSource::fetch_target_policy_sha` reading `.jeryu/autonomy/policies/*.yml` off the **protected target branch** via `jeryu-gitd`. |
| **CI pipeline status** `HostPipeline {status: "success"/...}` | `src/git_host/mod.rs:461`, GitLab pipeline API | **`jeryu-ci` run** status (`jeryu-ci-scheduler` / `jeryu-ci-*`). Feeds `EvidencePack.security.*` scan outcomes + `gate_receipts` (a green required jeryu-ci run → `ScanOutcome::Passed`; missing/failed → `Failed`/`Missing`, which trips `sast_failed`/`secret_scan_failed`/`dependency_scan_failed`). |
| `post_merge_passport_status` + `CheckStatus::{Success,Failure,ActionRequired}` (bin lines 768–795) | GitLab commit-status API | `ForgePrSource::post_gate_status` posting a forge check on `head_sha`; AllowMerge→Success, Reject→Failure, RequireHuman→ActionRequired. Forge-native check, no GitLab status. |
| `StandardEvidencePackBuilder.git_host: Arc<dyn GitHost>` | `evidence_pack_builder.rs:66` | `pr_source: Arc<dyn ForgePrSource>`. `build(repo: &RepoId, pr: &PullRequestId)` (rename of `mr_iid`). All else unchanged. |
| `AutoRejudgeService::rejudge(repo: &RepoRef, mr_iid, …)` | `auto_rejudge.rs:93` + `crate::git_host::RepoRef` | `rejudge(repo: &RepoId, pr: &PullRequestId, …)`. `repo.slug()` → `repo.as_str()`. |
| daemon `scan_repo`/`scan_pr` over `git_host.list_open_prs/get_pr_state` | `daemon.rs:160,202` | same flow over `ForgePrSource`; `LiveState` for drift `check()` filled from `PrLiveState`. |
| `EscalationEvent::summary` `"[jeryu] RequireHuman on {repo} @ {head}"` | `escalation.rs:101` | unchanged (already `jeryu`). |
| `LedgerKind::WebhookReceived` "GitHub `pull_request` events on POST /events" | `types.rs:595`, `http_server.rs` | Keep variant; payload source = forge PR webhook (jeryu HTTP daemon), not GitHub. Doc comment de-vendored. |
| `mr validate` CLI guard `if provider != "gitlab" { … "owned by GitLab" }` | `src/bin/autonomy.rs:640` | Deleted. `pr validate` always targets the in-process forge; no `--provider` GitLab branch. |
| `source_branch: "gitlab-mr-{mr}"` synthetic (bin 692) / `"auto-rejudge/{mr_iid}"` (builder 163) | bin + builder | `"jeryu-pr-{pr}"` / `"auto-rejudge/{pr}"` — drop the `gitlab` literal (D1). |
| changed-file glob detectors `".gitlab/ci/"`, `".gitlab/security-policies"` in `conditions.rs:438,452` | conditions registry input globs | **NOT a backend reference** — they describe a *contributor repo's* CI layout. Per D1 (zero gitlab literals in the fused repo): drop GitLab-specific globs, keep `.github/workflows/*` + generic `ops/ci/`, `deploy/`, `infra/`, `k8s/`, `helm/`, `terraform/`, and add a jeryu-native `.jeryu/ci/` prefix. See §6 R-7. |

### 3.3 Concept renames (D4) applied repo-wide in the new crate

`MergeRequest`/`merge_request`/`MR`/`mr_iid` → `PullRequest`/`pull_request`/`PR`/`pr_id`; `mint_merge_passport`/`MergePassport` keep the word "merge" only where it means the git merge act (a `MergePassport` is the post-decision passport to *perform a merge*; rename to **`MergePassport`** is retained — it is not a GitLab MR). Audit the wire-schema field `VibeGateVerdict.merge_request` / `MergePassport.merge_request` → `pull_request`; **bump schema strings** `vibegate.*.v1` only if the field rename is breaking, else keep `.v1` and rename the Rust field with `#[serde(rename = "pull_request")]` for a non-breaking wire change. (Decision needed in §6 R-8.)

---

## 4. `jeryu-proof` vs `jeryu-autonomy` split — what MUST be ported into the (currently-stub) proof crate

`jeryu-proof` today (`crates/proofcore/`) is a **path→owner→lane mapper + lane-evidence verifier** (`ProofEngine::{plan,verify}` → `ProofWitness`/`Receipt` from `jeryu-core::phase7`). It has **none** of the Evidence-Gate decision machinery: no `EvidencePack`, no `RiskTier`, no conditions registry, no quorum, no `judge()`, no SHA-binding, no verdict. The task says "map the 40+ hard-stops + EvidencePack + verdict ledger + kill-bell + auto-rejudge + escalation onto jeryu-proof (conditions/quorum) + jeryu-core PR + forge Receipt." Concretely:

### 4.1 PORT INTO `jeryu-proof` (the pure, side-effect-free decision engine)

| Move | From | Rationale |
|---|---|---|
| `ConditionRegistry` + 40+ `HardStop` conditions + `cond_externally_supplied` | `src/autonomy/conditions.rs` | This IS the "conditions" the task names. Pure `fn(&EvidencePack,&[Receipt])->Option<HardStop>`; no IO, no DB, no LLM. Belongs with the proof engine alongside `ProofBlocker`. |
| `evaluate_quorum` + `QuorumDecision` | `src/approval/quorum.rs` | This IS the "quorum" the task names. Pure over `(risk, receipts, ApprovalsPolicy, author)`. |
| `verify_sha_binding` | `src/approval/sha_bind.rs` | Pure Law-4 check; the proof engine's exact-SHA invariant lines up with `ProofWitness.head_sha`. |
| `judge(JudgeInputs)->JudgeOutcome` | `src/agent_review/judge.rs` | Pure fusion (no LLM — judge "never reads code"). Sits naturally beside `ProofEngine::verify`. |
| `check(verdict, LiveState)->Vec<RejudgeReason>` + `must_rejudge` | `src/agent_review/rejudge.rs` | Pure drift detection over SHAs/policy/TTL; the proof crate already owns SHA-bound witnesses. |
| `RiskClassifier` (optional, recommended) | `src/autonomy/risk.rs` | Pure path-glob tier classifier; reuses `jeryu-proof::matcher::PathPattern`. Could stay in autonomy, but it is pure and policy-driven → cleaner in proof. **Flag as a soft preference; if Codex prefers, leave in jeryu-autonomy.** |

### 4.2 Typed objects `jeryu-proof` needs (decision required)

`judge`, `quorum`, `conditions`, `sha_bind` all reference `EvidencePack`, `AgentApprovalReceipt`, `VibeGateVerdict`, `RiskTier`, `ReviewerRole`, `GateDecision`, `HardStop`, the policy types (`ApprovalsPolicy`/`PolicyBundle`/`QuorumEntry`). Two options:

- **Option A (recommended):** put the *pure data model* (`types.rs`'s 8 objects + enums + `SchemaTag`, and `policy_yaml_types.rs`) in a thin shared crate `jeryu-evidence-model` that BOTH `jeryu-proof` and `jeryu-autonomy` depend on. Keeps `jeryu-proof` free of any DB/LLM/forge dep. The serde-only objects have no IO.
- **Option B:** put the model directly in `jeryu-proof` and have `jeryu-autonomy` re-export from it. Simpler dependency graph, but `jeryu-proof` (Codex-owned) gains the whole vibegate object model.

This spec assumes **Option A**; the executing agent files a request to Codex naming the exact symbols. Until then, `jeryu-autonomy` can hold the model and `jeryu-proof` re-export it — but that inverts the intended layering, so confirm with Codex.

### 4.3 Bridge `jeryu-proof::ProofWitness`/`Receipt` ↔ EvidenceGate

The existing `ProofEngine::verify` mints a `forge_core::phase7::Receipt {kind: ProofWitness, repo, subject(pr), sha, …}`. Map it into the Evidence Gate as a **`GateReceipt`** inside `EvidencePack.gate_receipts` (one per required proof lane): `Receipt.summary`→`detail`, lane name→`id`, pass/fail→`status`, log digest→`evidence`. This is the concrete "onto forge Receipt" mapping: a jeryu-ci/proof lane pass becomes an EvidencePack `gate_receipt`, and a missing/failed required lane becomes a `ScanOutcome` that trips the matching hard-stop. `make_gate_receipt` (`evidence.rs:111`) is the existing helper; add `From<&forge_core::phase7::Receipt> for GateReceipt`.

### 4.4 KEEP IN `jeryu-autonomy` (everything stateful / IO / forge-coupled)

EvidencePack **builder** (touches `ForgePrSource`), `SqlLedger`/`SqlVerdictStore`/`KillBell` (DB), `AutoRejudgeService` (composition + IO), `daemon` (polling + forge), `escalation` (HTTP/secrets), `replay`/`shadow`/`metrics`/`profile` (DB reads), `http_server` (sockets), `signing`/`signing_secrets` (vault), `policy_yaml` loaders (filesystem), `mcp_tools`. The pure `types.rs`/`policy_yaml_types.rs` data model goes to the shared crate per §4.2.

> **One-line summary of the split:** *pure decision* (conditions, quorum, sha-bind, judge, rejudge[, risk]) → `jeryu-proof`; *typed model* → shared `jeryu-evidence-model`; *everything with a side effect or a forge/DB/LLM dependency* → `jeryu-autonomy`.

---

## 5. Dependencies & ordering

Hard prerequisites (this crate cannot compile until these exist in `/home/ubuntu/jeryuRUST`):

1. **`jeryu-core` rename complete** (`forge-core`→`jeryu-core`) with `phase7::{RepoId, PullRequestId, PullRequest, ChangedPath, Receipt, ReceiptKind, ProofWitness}` public. *Blocks §3 seam + §4.3 bridge.* (Codex.)
2. **`jeryu-proof` rename complete** (`proofcore`→`jeryu-proof`) AND the §4.1 decision-engine modules accepted (Codex request filed by this task). *Blocks judge/quorum/conditions imports.*
3. **Fused `db/` layer ported** — jeryu's SQLite + RedlineDB `db/` (D3 keeps it), including `db::AnyPool`, `db::autonomy_repo::AutonomyRepo`, `db::state::migrate` (append-only triggers on `launch_ledger`), `db::raw_query`, `fresh_autonomy_pool`. *Blocks `ledger.rs`, `verdict_store.rs`, `kill_bell.rs`, `metrics.rs`, `replay.rs`, `profile.rs`.* This is a separate port spec (DB layer); `jeryu-autonomy` consumes it.
4. **`ForgePrSource` adapter** over the forge open-PR/diff/git surface (`jeryu-gitd` for branch SHAs + target-policy SHA) and **`jeryu-ci` run-status query** (`jeryu-ci-scheduler`/`jeryu-ci-*`) for the pipeline→scan-outcome mapping. *Blocks `evidence_pack_builder.rs`, `daemon.rs`, `auto_rejudge.rs`, `pr validate` CLI.* (Forge/CI port specs must land the query APIs first.)
5. **LLM/agent reviewer surface** (`agent_review::orchestrator::ReviewerOrchestrator` + `ProductionReviewerOrchestrator`, provider chains, `BudgetLedger`) ported into the `jeryu` product shell. *Blocks `auto_rejudge.rs` and `pr validate`.* `jeryu-autonomy` depends on the shell for these (the judge stays pure in `jeryu-proof`; the reviewers stay in the shell).
6. **Secrets/vault** (`llm::secrets` 6-tier chain) for escalation webhook URLs + ed25519 seed resolution. *Blocks `escalation.rs`, `signing_secrets.rs`.*

Suggested ordering: (1)+(2) jeryu-core/jeryu-proof renames → (3) db port → (4) shared `jeryu-evidence-model` + §4 proof modules → (5) `jeryu-autonomy` types/signing/policy_yaml/ledger/verdict_store/kill_bell (no forge dep) → (6) `ForgePrSource` + evidence_pack_builder/daemon/auto_rejudge → (7) escalation/replay/shadow/metrics/profile/http_server/mcp → (8) CLI fold-in (`jeryu autonomy …`).

What BLOCKS / is blocked: nothing in `jeryu-autonomy` blocks Codex's core crates (one-directional dep). `jeryu-autonomy` is blocked by all six above. The §4 proof-engine move is the critical-path coordination item with Codex.

---

## 6. Risks & hardest seams

- **R-1 (hardest): the `jeryu-proof` split crosses a Codex ownership boundary.** The judge/quorum/conditions move (§4) requires Codex to accept ~1.6k LOC into a crate they own, plus the shared-model crate decision (§4.2). If Codex declines, fall back to keeping the pure engine in `jeryu-autonomy` and have `jeryu-proof` *call into* it — but that inverts intended layering. **Resolve with Codex before coding.**
- **R-2: CI pipeline → scan-outcome mapping is semantically lossy.** GitLab `HostPipeline.status` is a single string; the EvidencePack wants three independent `ScanOutcome`s (sast/dependency/secret) + `gate_receipts`. The forge's `jeryu-ci` runs are per-lane (§4.3) which is actually *richer* — but the mapping from "named jeryu-ci lane" → "which `SecuritySection` field / which hard-stop" must be specified (proposed: a `proof-lanes.toml` → ScanOutcome map). Getting this wrong silently flips a fail-closed scan to "Passed."
- **R-3: `fetch_target_policy_sha` (Law 3) must read the PROTECTED target branch, not the contributor branch.** Today only `GitHubClient` overrides it; the GitLab path returned `None` (degrades to "policy unknown, assume no drift"). The forge `jeryu-gitd` adapter MUST implement this correctly or the `PolicyChangeOnTarget` rejudge trigger and `policy_sha_drift` hard-stop go silent (a real autonomy-safety hole).
- **R-4: append-only ledger triggers + ed25519-only enforcement must survive the DB port.** `SqlLedger::append` refuses stub/HMAC algos and the SQL `BEFORE UPDATE/DELETE` triggers are load-bearing invariants (`ledger.rs:265` test). If RedlineDB/SQLite trigger semantics differ in the fused `db/`, the immutability guarantee breaks. Acceptance test `append_only_trigger_blocks_update` must stay green.
- **R-5: KillBell TTL auto-arm is a brick-prevention invariant.** A forgotten pause must auto-arm at `expires_at` (`kill_bell.rs:107`). Any clock/timezone drift in the ported `chrono` usage could either brick the control plane forever or release a pause early. Keep `current(now)` deterministic and test both edges.
- **R-6: verdict supersede idempotency under concurrency.** `SqlVerdictStore::save` supersedes prior `(repo, pull_request)` rows then `INSERT OR IGNORE`. The 4-task concurrency test (`verdict_store.rs:509`) must pass on the fused pool, or the daemon can issue two live verdicts for one PR.
- **R-7 (D1 literal scrub vs. real detectors): the conditions registry contains `.gitlab/...` path globs** (`conditions.rs:438,452`) used to detect when a *contributor* edits CI/security config. These are input-data strings, not backend coupling, but D1 says ZERO `gitlab` literals in the fused repo. Decision: **remove the `.gitlab/*` globs, keep `.github/workflows/*` (still a real external host contributors use) + generic infra prefixes, and add `.jeryu/ci/`**. Update `conditions.rs` tests accordingly (e.g. `changes_release_or_deploy_policy_fires_on_deploy_path` already keys on `deploy/`, which survives). Verify no test asserts a `.gitlab` path fires.
- **R-8: wire-schema stability.** `merge_request` field rename → `pull_request` (D4) and the `vibegate.*.v1` schema strings. If any persisted verdict/passport JSON or `.jeryu/autonomy/schemas/*.schema.json` is loaded across the rename, choose `#[serde(rename)]` (non-breaking) over a `.v2` bump. The `SchemaTag<T>` deserializer is strict (`types.rs:719`) — a mismatched schema string hard-fails parse, so coordinate the schema JSON files in the same change.
- **R-9: hand-rolled HTTP server on raw `TcpStream`** (`http_server.rs`, zero-dep mandate). The fused repo's HTTP daemons (D3) may want this folded into the shared HTTP stack; if so, preserve the pure `parse_request`/`render_response` contract and the `POST /events` → `WebhookReceived` ledger path (now fed by forge PR webhooks, not GitHub).
- **R-10: `EvidencePack` is built from a diff with NO source-branch / sometimes-blank target-branch** (`evidence_pack_builder.rs:163-167`). The judge keys off `head_sha`/`base_sha`, not branch names, but the forge diff payload must at least carry `head_sha` + `base_sha` reliably or `verify_sha_binding` drops every receipt → permanent RequireHuman.

---

## 7. Tests / acceptance gate

### 7.1 Exact commands (run from `/home/ubuntu/jeryuRUST`)

```bash
# 1. Build the new crate + proof additions.
cargo build -p jeryu-autonomy -p jeryu-proof

# 2. Unit + integration tests (port every #[cfg(test)] block from §1).
cargo nextest run -p jeryu-autonomy
cargo nextest run -p jeryu-proof
# or: cargo test -p jeryu-autonomy -p jeryu-proof

# 3. Workspace must still build/test green (no regression in Codex-owned crates).
cargo build --workspace
cargo nextest run --workspace

# 4. Lint clean (the bin had targeted #[allow]s; carry only those).
cargo clippy -p jeryu-autonomy -p jeryu-proof -- -D warnings
```

### 7.2 Invariant assertions that MUST hold (ported tests + new)

- **8-object serde round-trip** (`types.rs` tests): every canonical object round-trips losslessly; `SchemaTag` rejects schema mismatch; `LedgerKind::WebhookReceived` disjoint from `HumanDecisionRecorded`.
- **40+ conditions registry** (`conditions.rs` tests, now in `jeryu-proof`): unknown name → `unknown_condition:` fail-closed; clean signed pack → only `evidence_signature_invalid` when unsigned; each deterministic detector fires/holds at its threshold; the 4 Wave-3 release conditions register but stay no-op locally (externally supplied).
- **Pure judge** (`judge.rs` tests): hard-stop hit → Reject (veto > approval); quorum Met → AllowMerge; Insufficient/HumanRequired → RequireHuman; SHA-bind drift drops receipts.
- **Drift `check`** (`rejudge.rs` tests): NewCommitOnPr / PolicyChangeOnTarget / VerdictTtlExpired fire; unknown live values do not.
- **Ledger immutability** (`ledger.rs`): `append_only_trigger_blocks_update` (raw UPDATE/DELETE error), `append_refuses_stub_signature`, `append_refuses_hmac_signature`, idempotent-on-id, ed25519 verify after DB round-trip, malformed-payload → `Err` not panic, 4-task concurrent append → exactly 20 rows.
- **Verdict store** (`verdict_store.rs`): save idempotent + supersede; `list_active` excludes expired/rejected/superseded, orders by created_at ASC; body_json source-of-truth round-trip; 4-task concurrency → 20 active rows; unsigned verdict accepted (replay use-case).
- **KillBell**: pause/resume append signed ledger entries; TTL auto-arm at `expires_at`; `downgrade_if_paused` → RequireHuman.
- **Auto-rejudge**: one cycle = one signed pack + one judge + one `VerdictIssued` ledger entry + one save/supersede; orchestrator error degrades to "no receipts" (→ RequireHuman) not abort; pack-builder error bubbles up.
- **Evidence digest**: `verify_evidence_digest` round-trips; tamper breaks it; digest stable under changed_file order.

### 7.3 No-regression product checks (D3 surfaces) — run as applicable to what the change touches

- **tuiwright** (ratatui TUI): the autonomy/"Needs You" TUI views still render after the crate move — run the existing tuiwright suite over the autonomy panels.
- **Playwright** (React web): the web Evidence-Gate / verdict views unchanged — run the existing Playwright e2e over the autonomy pages.
- **MCP tools-call**: `mcp_tools::descriptors()` still serialize; an MCP `tools/call` against `vibegate.get_verdict` / `vibegate.get_evidence_pack` / `vibegate.inspect_autonomy_pack` returns the same shapes (rename `merge_request`→`pull_request` if R-8 picks a wire change — update the descriptor + Playwright/MCP fixtures together).
- **verdict-replay**: `autonomy replay --subject <verdict_id>` reconstructs the identical timeline (intent→lease→reviews→verdict→passport→rollback) from the ported ledger; `non_ed25519_signature_count == 0` for a clean trail.
- **shadow agreement**: `autonomy shadow` over a known git window reproduces the same `agreement_rate` as the pre-port baseline (forge history source, §3).

### 7.4 Zero-evidence gate (D1) — MUST be 0 hits

```bash
# No backend literals anywhere in the new crate (allow only the §6 R-7 changed-file
# detector strings, which are removed/renamed there — so this is a HARD zero):
rg -i --hidden -g '!*.lock' 'gitlab|jitforge|jitForge|nitro' crates/jeryu-autonomy/   # → 0
rg -i 'gitlab|jitforge|jitForge|nitro' crates/jeryu-proof/src/{conditions,quorum,sha_bind,judge,rejudge}.rs  # → 0
# And the GitLab git-host adapters are gone:
ls crates/jeryu-autonomy/src/ | rg -i 'gitlab' ; test $? -ne 0   # no gitlab*.rs
```
Also assert the conditions registry no longer matches a `.gitlab/...` changed-file path (R-7): a unit test feeding a `ChangedFile{path:".gitlab/ci/x.yml"}` must NOT fire `changes_release_or_deploy_policy` after the glob scrub (the `.github`/`deploy/` cases still fire).

---

## 8. Execution checklist (for the later agent)

1. File the §4 request to Codex (proof-engine modules + shared model crate). Get the layering decision (Option A vs B).
2. Create `crates/jeryu-evidence-model` (if Option A) with `types.rs` + `policy_yaml_types.rs` (pure serde).
3. Move conditions/quorum/sha_bind/judge/rejudge[/risk] into `jeryu-proof`; add `From<&Receipt> for GateReceipt` bridge (§4.3).
4. Scaffold `crates/jeryu-autonomy`; port the stateful modules; introduce `forge_input.rs::ForgePrSource` and delete every `GitHost`/`gitlab*` reference.
5. Apply D4 renames (`mr_iid`→`pr_id`, `merge_request`→`pull_request`) and the R-7 glob scrub + R-8 schema decision.
6. Wire `ForgePrSource` over forge PR/gitd + jeryu-ci run status (§3.1, R-2/R-3).
7. Fold the CLI into `jeryu autonomy …` (`pr validate` replacing `mr validate`).
8. Run §7.1 commands; satisfy §7.2/§7.3/§7.4.
