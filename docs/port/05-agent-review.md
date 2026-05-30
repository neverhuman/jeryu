# Port Spec 05 — Multi-Reviewer Orchestrator + Approval Subsystem

> Target crate: **`crates/jeryu-review`** (new). Product = **jeryu**. Edition 2024.
> Owns: reviewer orchestrator, LLM reviewers (Claude/GPT/Gemini via OpenAI-compatible
> router), prompt_sha replay, judge (policy fusion), quorum + SHA-bind, re-judge
> triggers, capability/intent protocol, and the agent task lifecycle (ephemeral bot
> identity, branch naming, 2-day tokens).
>
> This is one of the two **CRITICAL missing-in-jit** subsystems (the other is the MCP
> transport layer, spec'd separately). jit's `jeryu-agentbridge` + `jeryu-proof` have a
> *typed-but-empty* surface for proofs/mergeability and **zero** reviewer/LLM/quorum/
> judge logic. This spec ports ~3,300 LOC of reviewer/judge/approval/capability/agent
> logic and rewires every GitLab seam (MR→PR, pipeline→ci-run, `GitlabClient`→
> `jeryu-gitd`/`jeryu-api`) to the renamed jeryu-* core.
>
> Naming law (D1): the new code MUST contain ZERO `gitlab` / `jitforge` / `JitForge` /
> `Nitro` / `forge-core` / `proofcore` / `agentbridge` / `signrail` literals. Only
> `jeryu` / `jeryu-*` survive. All crate paths below use the **post-rename (D2)** names.

---

## 0. Crate-rename context (D2) used throughout this spec

| Old (jit, still-on-disk) | New (D2) | Used here as |
|---|---|---|
| `forge-core` | `jeryu-core` | `PullRequest`, `Receipt`, `ReceiptKind`, `ProofWitness`, `AgentScope`, `AgentId`, `PullRequestId`, `RepoId`, `ReceiptId` |
| `proofcore` | `jeryu-proof` | `ProofEngine`, `ChangeSet`, `ProofPlan`, `ProofEvidence`, `ProofBlocker`, policy types |
| `agentbridge` | `jeryu-agentbridge` | `AgentBridge`, `AgentContext`, `Mergeability`, `DryRunPatchRequest` |
| `signrail` | `jeryu-signrail` | `Signer`, `Signature`, `HmacSha256Signer`, `Receipt` (release receipts) |
| `gitd` | `jeryu-gitd` | branch create/delete, ref resolution |
| `jitforge-api` | `jeryu-api` | GitHub-style REST edge (PR/issue/run endpoints) |
| `ci-scheduler` | `jeryu-ci-scheduler` | `MergeQueue`, queue-entry outcome (Codex-owned; read-only consumer) |
| `runnerd` | `jeryu-runnerd` | reviewer LLM-call sandboxing target (later phase) |

**Codex owns** `jeryu-core`, `jeryu-proof`, `jeryu-ci-scheduler`, `jeryu-gitd`. This
crate DEPENDS on them; it must NOT edit them. Where this spec needs a new type in a
Codex crate (e.g. `ReceiptKind::AgentReview`), it is flagged as a **cross-crate ask**
in §4.

---

## 1. Source inventory

All paths under `/home/ubuntu/jeryu` (read-only mature shell). One line each.

### 1.1 `src/agent_review/**` — reviewer orchestration (12 files, ~3.3k LOC)

| File (`src/agent_review/`) | LOC | Purpose |
|---|---|---|
| `mod.rs` | 31 | Module root; re-exports `judge`, `runner`, per-role reviewers, `prompt_builder`, `rejudge`. Documents the 4 cross-cutting invariants (diff wrapped untrusted; strict-schema parse→abstain; judge never reads code; every receipt records `prompt_sha`/`model`/`provider`/`raw_response_sha`). |
| `orchestrator.rs` | 1073 | `ReviewerOrchestrator` trait + `ProductionReviewerOrchestrator` (spawns one tokio task per required `ReviewerRole`, budget-gated via `BudgetLedger::would_exceed`, signs synthesized abstains with ed25519) + `FakeReviewerOrchestrator` test double. One reviewer failing → `Abstain` receipt, never aborts batch. Loads role prompt from `autonomy_dir/prompts/<role>.md`. |
| `runner.rs` | 355 | Generic reviewer dispatch: `ReviewerRoleId` (Security/TestIntegrity/Runtime/Lockfile/Nightwatch) → `chain_key()`/`agent_id()`/`prompt_path()`/`to_receipt_role()`; `run_review()` = scrub→build messages→`prompt_sha`→`router.dispatch(chain_key)`→`extract_receipt_json`→map/abstain→sign. |
| `security.rs` | 335 | Security reviewer (standalone copy of `run_review` flow; `SecurityReviewInputs`, `ReviewerCallError`). Pre-flight `scrub_diff` fail-closed; SQLi/secret test fixtures. |
| `test_integrity.rs` | 43 | Thin wrapper → `run_review(ReviewerRoleId::TestIntegrity)`. Catches tests being silently weakened. |
| `runtime.rs` | 43 | Thin wrapper → `run_review(ReviewerRoleId::Runtime)`. Production-behavior risk (perf, memory, migrations, blast radius). |
| `lockfile.rs` | 45 | Thin wrapper → `run_review(ReviewerRoleId::Lockfile)`. Supply-chain/lockfile tiebreaker. |
| `nightwatch.rs` | 298 | Canary telemetry reviewer (no diff; `telemetry_summary` wrapped in `<telemetry release_id=… ring_percent=…>` then in the outer untrusted `<diff>` envelope). Owns `NightwatchReviewInputs`. |
| `prompt_builder.rs` | 136 | `build_reviewer_messages` → `[system, user]`; user wraps diff in `<diff>…UNTRUSTED INPUT…</diff>`; `prompt_sha()` = sha256 of canonicalized prompt (strips `# (no-hash)` lines so editorial comments don't change the hash). **Replay anchor.** |
| `parse.rs` | 153 | `extract_receipt_json` — robust first-valid-`{…}` extraction (strips ```json fences, scans balanced braces respecting string literals). `ParsedReceiptFields { role, decision, reason, findings }`. |
| `judge.rs` | 544 | Pure policy fusion. `judge(JudgeInputs)`→`JudgeOutcome{ verdict: VibeGateVerdict, dropped_receipts }`. Order: (1) SHA-bind filter, (2) hard-stops (registry-computed + external) → any hit = Reject, (3) quorum, (4) HumanRequired vs AllowMerge. Judge **never reads code**. Mints 30-char `vgv_` verdict id. |
| `rejudge.rs` | 291 | `RejudgeReason` (NewCommitOnPr/TargetBranchAdvance/PolicyChangeOnTarget/VerdictTtlExpired) + `check(verdict, LiveState)`/`must_rejudge()`. Pure drift detector. Stable order: head, policy, ttl. |

### 1.2 `src/approval/**` — quorum + SHA binding (3 files, ~485 LOC)

| File (`src/approval/`) | LOC | Purpose |
|---|---|---|
| `mod.rs` | 12 | Re-exports; documents invariants: no self-approval, exact-SHA binding, veto > approval count. |
| `quorum.rs` | 332 | `evaluate_quorum(risk, receipts, ApprovalsPolicy, author_agent)` → `QuorumOutcome{ decision: Met/Insufficient/Vetoed/HumanRequired, … }`. Filters author (no self-approval), dedupes by `agent_id` (distinct identities), any `Block`→Vetoed, missing required role→Insufficient, `human_required`→HumanRequired. Missing quorum policy → fail closed (HumanRequired). |
| `sha_bind.rs` | 141 | `verify_sha_binding(pack, receipt)` → `Result<(), ShaBindError>` (PackIdMismatch / HeadMismatch / PolicyMismatch). Tip1 Law 4: a receipt is valid only for one `(evidence_pack_id, head_sha, policy_sha)` tuple. |

### 1.3 `src/capability*.rs` — capability/intent protocol (10 files, ~470 LOC)

| File | LOC | Purpose |
|---|---|---|
| `capability.rs` | 267 | `AgentIntent` tagged enum (17 variants; 14 distinct intent *families* per recon — patch/race/tests/capsule/merge/blockers/snapshot/pipeline-jobs/bottlenecks/list-actions/plan-validation + 6 bug-* ); `AgentActionRequest` envelope (protocol_version `v3.01`, request_id, actor, nonce, expires_at, budget, grant, intent); `CapabilityGrantProof`+`CapabilityGrantScope`; `ActionBudget`; `FileModification`/`HypothesisPatch`; `start_capability_server` (UDS, 0o600, 1 MiB frame cap). |
| `capability_request.rs` | 73 | `parse_capability_request` (length-prefixed frame OR bare JSON) + `validate_capability_request` (protocol_version check, nonce replay cache `SEEN_NONCES`, `expires_at` RFC3339 expiry). |
| `capability_actions.rs` | 297 | `execute_intent(intent, ctx, client)` dispatch table — one arm per `AgentIntent` variant → `inspect`/`execute`/`bug_*`. |
| `capability_execute.rs` / `_support.rs` | 92 / 193 | Mutation intents (propose_patch, race_patches, request_merge, run_tests) against `GitlabClient`. |
| `capability_inspect.rs` / `_read.rs` / `_snapshot.rs` | 22 / 134 / 100 | Read intents (snapshot, pipeline jobs, bottlenecks, blockers). |
| `capability_request.rs` (envelope) | — | (see above) |
| `capability_ci.rs` | 41 | CI-specific helpers used by capability dispatch. |
| `capability_tests.rs` | 62 | Capability parser/dispatch tests. |

### 1.4 `src/agent.rs` / `src/agent_ops.rs` — agent task lifecycle (~371 LOC)

| File | LOC | Purpose |
|---|---|---|
| `agent.rs` | 167 | `AgentTask { project_id, task_description, branch_name, target_branch, issue_iid, bot_user_id, bot_token }`; `compute_slug` (≤4 words, dash, lowercase); `format_bot_name(slug,ts)` (`@agent-<slug>-<rev4>`); `provision_agent_identity` (ephemeral project bot, **2-day token expiry**, Developer/least-privilege, scopes `["api","write_repository"]`); `create_tracking_issue_for_agent`; `create_agent_branch_with_master_attempt` (try `main` then `master`); `build_agent_task`. Branch name = `agent/<slug>-<YYYYMMDD-HHMMSS>`. |
| `agent_ops.rs` | 204 | `AgentOutcome { Pending, Success, Failed{capsules} }`; `check_agent_pipeline` (list jobs for branch, detect race via multiple refs or `-hypo-`, pick winner / purge losers / collect `FailureCapsule`s); `complete_agent` (label issue `agent:done`/`agent:failed`, comment); `list_agents` (labels `agent:running`/`agent:pending`). |

### 1.5 Cross-subsystem dependencies pulled in from `src/autonomy/**` and `src/llm/**`

These are **inputs** the reviewer/judge code consumes. They are owned by the autonomy
port (separate spec) but their types are load-bearing here and listed so this crate's
dependency edges are explicit:

| File | Type(s) this crate needs | Notes |
|---|---|---|
| `src/autonomy/types.rs` (31.6k) | `AgentApprovalReceipt`, `EvidencePack`, `VibeGateVerdict`, `VerdictReceiptRef`, `ReviewerRole`, `ReviewDecision`, `RiskTier`, `GateDecision`, `Finding`, `Severity`, `TokenCounts`, `ScanOutcome`, `TestsSection`, `SecuritySection`, `SchemaTag<T>` | The receipt/verdict/pack wire schema. Ported to `jeryu-proof` (see §4). |
| `src/autonomy/signing.rs` (10.8k) | `EdSigningKey` (`from_seed`/`generate`/`sign_raw`/`public_key_hex`/`verifier`), `EdVerifier`, `Signature` (`default_unsigned`/`stub`, fields `key_id`/`algo`/`value`), `sha256_digest` | Receipt signing primitives. Maps onto `jeryu-signrail::Signer`. |
| `src/autonomy/conditions.rs` (39.3k) | `ConditionRegistry::default()`, `ConditionRegistry::evaluate(&[String], &EvidencePack, &[AgentApprovalReceipt]) -> Vec<HardStop>`, `HardStop{ name, reason, details }` | 58 named hard-stop functions. Ported to `jeryu-proof` (autonomy spec). Judge calls `evaluate`. |
| `src/autonomy/policy_yaml*.rs` | `PolicyBundle::from_dir`, `ApprovalsPolicy`, `ApprovalRules`, `QuorumEntry{ approvals_needed, roles, human_required, fail_closed, fail_closed_without_human }`, `HardStopEntry`, `verdict_ttl_minutes`, `re_judge_on` | Policy loaded from target branch (Law 3). |
| `src/llm/mod.rs`+`router.rs`+`budget.rs`+`scrub.rs` | `LlmRouter` (`dispatch(role, &[ChatMessage]) -> CallResponse`), `RoleChain`/`RoleChainEntry`, `LlmProvider` trait, `ChatMessage::{system,user}`, `CallResponse{ provider, model, content, prompt_tokens, completion_tokens, raw_response_sha, latency_ms }`, `LlmError`, `Budget`, `BudgetLedger`, `TokenUsage`, `scrub_diff` | LLM transport. Ported into this crate's `llm/` module (see §2) since it is the reviewer-call engine; `provider_chains` selects Claude/GPT/Gemini-class OpenAI-compatible endpoints per role. |
| `.jeryu/autonomy/prompts/*.md` (5 files) | `reviewer-security.md`, `reviewer-test-integrity.md`, `reviewer-runtime.md`, `lockfile-scout.md`, `reviewer-nightwatch.md` | Role system prompts; `prompt_sha` is computed over canonicalized bytes. Ship verbatim under `jeryu-review/assets/prompts/`. |
| `.jeryu/autonomy/policies/*.yml` | `approvals.yml`, `risk.yml`, `protected-paths.yml`, `freeze.yml`, `release.yml` | Quorum/hard-stop/risk policy. |

---

## 2. Target layout in `/home/ubuntu/jeryuRUST`

New crate `crates/jeryu-review`. Module structure:

```
crates/jeryu-review/
├── Cargo.toml                 # edition 2024; deps: jeryu-core, jeryu-proof,
│                              #   jeryu-signrail, jeryu-gitd, tokio, async-trait,
│                              #   serde, serde_json, chrono, thiserror, sha2,
│                              #   ed25519-dalek (or via jeryu-signrail), reqwest
├── assets/
│   ├── prompts/               # reviewer-security.md, reviewer-test-integrity.md,
│   │                          #   reviewer-runtime.md, lockfile-scout.md,
│   │                          #   reviewer-nightwatch.md  (verbatim copies)
│   └── policies/              # approvals.yml, risk.yml, protected-paths.yml, …
├── src/
│   ├── lib.rs                 # re-exports; crate-level invariant doc
│   ├── schema.rs              # AgentApprovalReceipt, EvidencePack, VibeGateVerdict,
│   │                          #   ReviewerRole, ReviewDecision, RiskTier, GateDecision,
│   │                          #   Finding, Severity, TokenCounts, VerdictReceiptRef,
│   │                          #   SchemaTag<T>   (see §4: may live in jeryu-proof
│   │                          #   instead and be re-exported here)
│   ├── signing.rs             # EdSigningKey/EdVerifier/Signature shim over
│   │                          #   jeryu-signrail::Signer (see §3 + §4)
│   ├── llm/
│   │   ├── mod.rs             # ChatMessage, CallResponse, LlmError, LlmProvider, DataUse
│   │   ├── router.rs          # LlmRouter, RoleChain, RoleChainEntry
│   │   ├── budget.rs          # Budget, BudgetLedger, TokenUsage
│   │   ├── scrub.rs           # scrub_diff fail-closed secret scan
│   │   ├── provider_chains.rs # per-role chains (Claude/GPT/Gemini OpenAI-compat)
│   │   └── openai_compatible.rs
│   ├── prompt_builder.rs      # build_reviewer_messages, prompt_sha (REPLAY anchor)
│   ├── parse.rs               # extract_receipt_json, ParsedReceiptFields
│   ├── reviewers/
│   │   ├── mod.rs
│   │   ├── runner.rs          # ReviewerRoleId, run_review (shared flow)
│   │   ├── security.rs
│   │   ├── test_integrity.rs
│   │   ├── runtime.rs
│   │   ├── lockfile.rs
│   │   └── nightwatch.rs
│   ├── orchestrator.rs        # ReviewerOrchestrator trait, Production/Fake impls
│   ├── judge.rs               # judge(), JudgeInputs, JudgeOutcome
│   ├── rejudge.rs             # RejudgeReason, check, must_rejudge, LiveState
│   ├── approval/
│   │   ├── mod.rs
│   │   ├── quorum.rs          # evaluate_quorum, QuorumOutcome, QuorumDecision
│   │   └── sha_bind.rs        # verify_sha_binding, ShaBindError
│   ├── capability/
│   │   ├── mod.rs             # AgentIntent, AgentActionRequest, grant/scope, server
│   │   ├── request.rs         # parse/validate (frame, nonce, expiry)
│   │   └── actions.rs         # execute_intent dispatch → jeryu-agentbridge + jeryu-gitd
│   └── agent/
│       ├── mod.rs             # AgentTask, identity provisioning, branch naming
│       └── ops.rs             # AgentOutcome, check_agent_pipeline, complete_agent
└── tests/
    ├── orchestrator.rs        # ports orchestrator.rs #[cfg(test)] (14 cases)
    ├── judge.rs               # ports judge.rs tests (8 cases)
    ├── quorum.rs              # ports quorum.rs tests (8 cases)
    ├── sha_bind.rs            # ports sha_bind.rs tests
    ├── rejudge.rs             # ports rejudge.rs tests (8 cases)
    ├── prompt_replay.rs       # NEW: prompt_sha stability + verdict-replay gate
    └── reviewers.rs           # security/nightwatch parse+scrub fixtures
```

**Placement decision (default):** keep the wire schema (`AgentApprovalReceipt`,
`EvidencePack`, `VibeGateVerdict`, `ReviewerRole`, `ReviewDecision`, `RiskTier`,
`GateDecision`) and `ConditionRegistry` in **`jeryu-proof`** (Codex-owned, autonomy
port), and have `jeryu-review` re-export them. This crate owns the *orchestration,
LLM, quorum, judge fusion, capability, and agent-lifecycle* logic. If the autonomy
spec lands the schema in `jeryu-proof` first, `schema.rs`/`signing.rs` here become
thin `pub use` re-export shims. If it does not land in time, this crate temporarily
hosts them and the autonomy port absorbs them later (flagged in §4 ordering).

---

## 3. Rewire map

`MR → PullRequest/PR` (D4); `pipeline → ci run`; `GitlabClient → jeryu-gitd + jeryu-api`;
`project_id (i64) → RepoId`; `mr_iid (i64) → PullRequestId`. Reviewer/judge/quorum/
SHA-bind logic is GitLab-free already — only the *envelope and lifecycle* seams change.

### 3.1 Reviewer / judge / approval core (no GitLab; rename only)

| Source symbol / data | Current source | Target jeryu-* type / API |
|---|---|---|
| `ReviewerOrchestrator` trait, `ProductionReviewerOrchestrator`, `FakeReviewerOrchestrator` | `agent_review/orchestrator.rs` | `jeryu_review::orchestrator::*` (unchanged shape) |
| `ReviewerRoleId`, `run_review` | `agent_review/runner.rs` | `jeryu_review::reviewers::runner::*` |
| `run_security_review` etc. | `agent_review/{security,test_integrity,runtime,lockfile,nightwatch}.rs` | `jeryu_review::reviewers::<role>::run_*_review` |
| `build_reviewer_messages`, `prompt_sha` | `agent_review/prompt_builder.rs` | `jeryu_review::prompt_builder::*` (byte-identical; sha must match for replay) |
| `extract_receipt_json`, `ParsedReceiptFields` | `agent_review/parse.rs` | `jeryu_review::parse::*` |
| `judge`, `JudgeInputs`, `JudgeOutcome` | `agent_review/judge.rs` | `jeryu_review::judge::*` |
| `evaluate_quorum`, `QuorumOutcome`, `QuorumDecision` | `approval/quorum.rs` | `jeryu_review::approval::quorum::*` |
| `verify_sha_binding`, `ShaBindError` | `approval/sha_bind.rs` | `jeryu_review::approval::sha_bind::*` |
| `RejudgeReason`, `check`, `must_rejudge`, `LiveState` | `agent_review/rejudge.rs` | `jeryu_review::rejudge::*` |
| `AgentApprovalReceipt`, `EvidencePack`, `VibeGateVerdict`, `VerdictReceiptRef`, `ReviewerRole`, `ReviewDecision`, `RiskTier`, `GateDecision`, `Finding`, `Severity`, `TokenCounts`, `SchemaTag<T>` | `autonomy/types.rs` | `jeryu_proof::schema::*` (Codex/autonomy), re-exported by `jeryu_review::schema` |
| `ConditionRegistry`, `HardStop` (58 conditions) | `autonomy/conditions.rs` | `jeryu_proof::conditions::*` (autonomy port) |
| `PolicyBundle`, `ApprovalsPolicy`, `QuorumEntry` | `autonomy/policy_yaml*.rs` | `jeryu_proof::policy::*`, loaded from **target branch** via `jeryu-gitd` |

### 3.2 Signing — `autonomy::signing` → `jeryu-signrail`

| Source symbol | Current | Target |
|---|---|---|
| `EdSigningKey::sign_raw(&[u8]) -> Signature` | `autonomy/signing.rs` (ed25519) | `jeryu_signrail::Signer::sign(&[u8]) -> Result<Signature>`; introduce an **ed25519 `Signer` impl** in `jeryu-signrail` (today it ships `HmacSha256Signer`/`UnavailableSigner`). |
| `Signature{ key_id, algo, value }` (`algo`=`"ed25519"`/`"stub"`) | `autonomy/signing.rs` | `jeryu_signrail::Signature{ algorithm, key_id, value_hex }`. **Field rename**: `algo`→`algorithm`, `value`→`value_hex`. Receipt canonical-JSON signing must serialize with the jeryu field names; keep a stable canonicalization (sign over the receipt JSON with the signature stubbed, exactly as `sign_canonical`/`sign_receipt` do today). |
| `Signature::stub()` / `default_unsigned()` (`algo == "stub"`) | `autonomy/signing.rs` | `jeryu_signrail::UnavailableSigner` output OR a `Signature` with `algorithm:"stub"`. The orchestrator's re-sign check `if receipt.signature.algo == "stub"` becomes `if receipt.signature.algorithm == "stub"`. |
| `EdVerifier::verify` | `autonomy/signing.rs` | `jeryu_signrail::Signer::verify(&[u8], &Signature)` |
| `sha256_digest` (`sha256:<hex>`) | `autonomy/signing.rs` | reuse `jeryu_signrail::checksum::sha256_hex` (NB jeryu emits bare hex; prompt_sha/raw_response_sha format is `sha256:<hex>` — keep the `sha256:` prefix wrapper in `prompt_builder.rs` to preserve replay hashes). |

### 3.3 Capability / intent — GitLab payloads → PR/run model

| Source symbol / field | Current (GitLab) | Target jeryu-* type / API |
|---|---|---|
| `AgentIntent::ProposePatch{ project_id, branch_name, base_ref, modifications, mr_title }` | `capability.rs` | keep variant; `project_id:i64 → repo: RepoId`; `mr_title → pr_title`; dispatch → `jeryu_agentbridge::AgentBridge::dry_run_patch(DryRunPatchRequest)` then branch via `jeryu-gitd`. |
| `AgentIntent::RacePatches{ project_id, base_branch, hypotheses }` | `capability.rs` | `repo: RepoId`; each hypothesis → a PR branch + dry-run; winner selection moves to `agent/ops.rs` (§3.5). |
| `AgentIntent::RequestMerge{ project_id, mr_iid, source_branch, target_branch }` | `capability.rs` | `RequestMerge{ repo: RepoId, pr: PullRequestId, … }`; dispatch → `jeryu_agentbridge::mergeability(pr)` + `jeryu-ci-scheduler` enqueue. **D4 rename of the variant payload.** |
| `AgentIntent::FetchCapsule{ job_id }` | `capability.rs` | `job_id` → ci-run/job id from `jeryu-core`; capsule fetch via `jeryu-api`/db layer (jeryu SQLite kept, D3). |
| `AgentIntent::GetPipelineJobs{ project_id, pipeline_id }` | `capability.rs` | `GetCiRunJobs{ repo: RepoId, run_id }` — **pipeline→ci run** rename. |
| `AgentIntent::GetCiBottlenecks{ project_id, ref_name, limit }` | `capability.rs` | `repo: RepoId`; reads `jeryu-ci-scheduler`/obs metrics. |
| `AgentIntent::ExplainBlockers{ entity_type, entity_id }` | `capability.rs` | `entity_type` enum gains `pr`/`ci_run`/`release` (was `"job"|"release"|"merge"`); → `jeryu_agentbridge::mergeability().blockers`. |
| `AgentIntent::PlanValidation{ project_id, test_ids, ref_name }` | `capability.rs` | → `jeryu_agentbridge::proof_plan(ProofPlanRequest)` → `jeryu_proof::ProofPlan`. |
| `AgentIntent::Bug*` (6 variants) | `capability.rs` | unchanged shape; `project: Option<String>` stays repo slug; dispatch → bugtracker port (separate). |
| `AgentActionRequest{ protocol_version, request_id, actor, nonce, expires_at, budget, grant, intent }` | `capability.rs` | unchanged; `project_id: Option<i64>` → `repo: Option<RepoId>`. Protocol version stays `v3.01`. |
| `CapabilityGrantProof{ grant_id, actor, action_id, scope, signature }` + `CapabilityGrantScope{ project_id, refs, paths, head_sha }` | `capability.rs` | grant verified via `jeryu-signrail::Signer::verify`; `scope` maps onto `jeryu_core::AgentScope{ agent, repo, allowed_paths, max_paths }` — `paths`→`allowed_paths`, `project_id`→`repo`. Emit a `jeryu_core::Receipt{ kind: AgentDryRunPatch/AgentProposedFix, agent, sha, … }` per granted action. |
| `start_capability_server(socket_path, GitlabClient)` | `capability.rs` | `start_capability_server(socket_path, AgentBridge)` — UDS server unchanged (0o600, 1 MiB frame); client arg becomes `jeryu_agentbridge::AgentBridge`. |
| `parse/validate_capability_request` (nonce replay, expiry) | `capability_request.rs` | unchanged logic; pure. |

### 3.4 Agent task lifecycle — GitLab issues/bots → jeryu-core PR/issue + jeryu-gitd

| Source symbol / field | Current (GitLab) | Target jeryu-* type / API |
|---|---|---|
| `AgentTask{ project_id, branch_name, target_branch, issue_iid, bot_user_id, bot_token }` | `agent.rs` | `AgentTask{ repo: RepoId, branch_name, target_branch, issue: Option<IssueId>, agent: AgentId, token: Option<EphemeralToken> }`. `project_id:i64→RepoId`; `issue_iid→IssueId`; `bot_user_id/bot_token→AgentId + ephemeral token`. |
| `provision_agent_identity` → `GitlabClient::create_project_bot(scopes=["api","write_repository"], expires_at=+2d, access=30)` | `agent.rs` | `jeryu_core::AgentId` + ephemeral token minted by `jeryu-api`/auth; **keep 2-day expiry + least-privilege** (Developer-equivalent write scope on one repo). Branch name rule **unchanged**: `agent/<slug>-<YYYYMMDD-HHMMSS>`. `format_bot_name` unchanged. |
| `create_tracking_issue_for_agent` → `GitlabClient::create_issue(labels=["agent:pending"…])` | `agent.rs` | `jeryu-core` issue create (jeryu keeps issue/label model, D3) via `jeryu-api`. Labels `agent:pending/running/done/failed` survive. |
| `create_agent_branch_with_master_attempt` (try `main` then `master`) | `agent.rs` | `jeryu_gitd::create_branch(repo, branch, base)`; keep `main`→`master` fallback. |
| `check_agent_pipeline` → `GitlabClient::list_jobs/get_job_log_snippet/delete_branch` | `agent_ops.rs` | branch jobs from `jeryu-ci-scheduler`/`jeryu-core` ci-run model; race detection (multiple refs or `-hypo-`) unchanged; loser-branch purge via `jeryu_gitd::delete_branch`. `FailureCapsule` from jeryu db layer (kept). **MR-pipeline → PR ci-run.** |
| `complete_agent` → `GitlabClient::update_issue_labels/comment_on_issue` | `agent_ops.rs` | `jeryu-api` issue label update + comment; labels unchanged. |
| `list_agents` → `GitlabClient::list_issues_by_labels` | `agent_ops.rs` | `jeryu-api` issue list by label. |
| `AgentOutcome{ Pending, Success, Failed{capsules: Vec<FailureCapsule>} }` | `agent_ops.rs` | unchanged enum; `FailureCapsule` from jeryu db (D3). |

### 3.5 Verdict → merge-queue admission (consumer of Codex crate)

| Source flow | Current | Target |
|---|---|---|
| `judge()` → `VibeGateVerdict{ decision: AllowMerge }` then merge | `judge.rs` + GitLab merge | `VibeGateVerdict` AllowMerge → mint a `jeryu_core::ProofWitness` / `Receipt{ kind: ProofWitness }` and hand to `jeryu_ci_scheduler::MergeQueue` as admission proof. **Cross-crate ask** (§4): scheduler must accept an externally-minted witness, or expose an admission hook. |

---

## 4. Dependencies & ordering

### 4.1 Hard prerequisites (must exist before `jeryu-review` compiles)

1. **D2 crate renames landed** for every crate in §0. Until `forge-core`/`proofcore`/
   `agentbridge`/`signrail` are renamed, this crate cannot name its deps without
   violating D1 (zero `jitforge`/`forge-core`/`proofcore` literals). **BLOCKS everything.**
2. **`jeryu-proof` schema + conditions** (autonomy port). This crate's judge/quorum/
   sha-bind operate on `AgentApprovalReceipt`/`EvidencePack`/`VibeGateVerdict`/
   `ReviewerRole`/`ReviewDecision`/`RiskTier`/`GateDecision` and call
   `ConditionRegistry::evaluate`. These are **not in jit today** (jit's `proofcore` has
   only `OwnerRule`/`TestRule`/`ProofPlan`/`ProofWitness` and ZERO hard-stop conditions
   or reviewer-receipt schema). Either:
   - (a) autonomy port lands them in `jeryu-proof` first (preferred), **or**
   - (b) this crate temporarily hosts `schema.rs`/`conditions` and the autonomy port
     absorbs them later. Pick (a) if ordering allows; (b) is the unblock fallback.
3. **`jeryu-signrail` ed25519 `Signer`.** Today signrail ships `HmacSha256Signer` +
   `UnavailableSigner` only. Reviewer receipts are ed25519-signed and the judge's
   `evidence_signature_invalid` condition + the orchestrator re-sign path
   (`algo == "stub"`) depend on ed25519. **Cross-crate ask**: add an ed25519 `Signer`
   impl to `jeryu-signrail`, or keep `EdSigningKey` local in this crate's `signing.rs`
   and only *route receipt persistence* through signrail's `Receipt`. Decision: keep
   ed25519 keygen/sign local (`signing.rs`) but emit/store the receipt envelope via
   `jeryu_signrail::Receipt` so signatures are auditable in one place.
4. **`jeryu-core` `ReceiptKind` extension.** Add `ReceiptKind::AgentReview` (per-reviewer
   receipt) and `ReceiptKind::Verdict` (judge verdict) so verdicts/receipts persist in
   the existing receipt log. **Cross-crate ask to Codex.** Until then, store the native
   `AgentApprovalReceipt`/`VibeGateVerdict` in jeryu's SQLite verdict_store (D3) and only
   project a summary `Receipt` of an existing kind.
5. **LLM transport.** Port `src/llm/**` (router, budget, scrub, provider_chains,
   openai_compatible) into `jeryu-review::llm`. No jit equivalent exists. This is
   self-contained (no GitLab) — only the `JERYU_LLM_SCRUB_SKIP` env var name and
   per-role chain keys (`reviewer-security`, …) must be preserved for prompt replay.

### 4.2 Build order within this crate

`schema/signing` (or re-export) → `llm/*` → `prompt_builder` + `parse` →
`reviewers/runner` → per-role reviewers → `orchestrator` → `approval/{quorum,sha_bind}` →
`judge` → `rejudge` → `capability/*` → `agent/*`. Tests last.

### 4.3 What this crate blocks (downstream)

- MCP tool catalog (`propose_patch`, `request_merge`, `plan_validation`, `explain_blockers`)
  needs `capability::AgentIntent` + `execute_intent`.
- The merge-gate / autonomy daemon needs `judge` + `rejudge` + `evaluate_quorum`.
- Agent-first workflows need `agent::AgentTask` lifecycle.

### 4.4 Explicitly out of scope (other specs)

`auto_rejudge` daemon, `kill_bell`, `escalation` webhooks, `ledger`, `evidence_pack_builder`,
the full `ConditionRegistry` bodies, bugtracker domain, MCP transport. This crate provides
the *pure* `rejudge::check` that the daemon polls, and consumes the conditions registry.

---

## 5. Tests / acceptance gate

### 5.1 Exact commands

```bash
# Build + unit/integration tests for the new crate
cargo test -p jeryu-review                # ports all #[cfg(test)] modules below

# Workspace must still build (no-regression on Codex crates)
cargo build --workspace
cargo nextest run -p jeryu-review

# Lint / format gate (mirrors jeryu scripts/pre-pr.sh; single-threaded for env-mutation tests)
cargo clippy -p jeryu-review -- -D warnings
cargo test -p jeryu-review -- --test-threads=1
```

### 5.2 Ported test cases (must all pass; counts from source)

- **orchestrator** (14 cases, from `orchestrator.rs:577-1073`): canned receipts;
  records each required role; `error_on`→abstain for that role only; unknown role→default
  Pass; one receipt per required role; empty roles→empty Vec; **concurrent reviewers run
  in parallel (4×50 ms < 200 ms)**; exhausted budget→all abstain with `"budget exhausted"`
  reason; construct with required fields; abstain carries correct role; abstain signature
  verifies under ed25519; receipt `evidence_pack_id`/`head_sha`/`policy_sha` match pack;
  `not_author == true` on all synthesized receipts.
- **judge** (8 cases, `judge.rs:215-544`): AllowMerge when quorum met & no hard stops;
  one Block→Reject via `reviewer_blocked`; `secret_scan_failed`→Reject even with unanimous
  approval; SHA drift drops receipt→RequireHuman; unsigned pack→Reject via
  `evidence_signature_invalid`; injected `codeowners_not_satisfied`→Reject; R4 protected
  path→RequireHuman; `mint_verdict_id` is 30 chars + `vgv_` prefix.
- **quorum** (8 cases, `quorum.rs:141-332`): met when required roles pass; one Block
  vetoes regardless of count; missing required role→Insufficient; author self-approval
  doesn't count; `human_required` lands separately; missing quorum policy→fail closed;
  author identity overrides a lying `not_author=true`; duplicate `agent_id`s collapse.
- **sha_bind** (3 cases): matching SHA passes; head drift rejects; policy drift rejects.
- **rejudge** (8 cases): fresh→no triggers; head drift→`new_commit_on_pr`; policy
  drift→`policy_change_on_target`; ttl expiry→`verdict_ttl_expired`; multiple→all reported;
  missing live fields→no trigger; `rebind_on_train=false` skips target advance; documented
  order (head, policy, ttl) deterministic.
- **prompt_builder/parse**: diff wrapped with UNTRUSTED marker; `# (no-hash)` stripped
  before sha; sha changes on real edit; injection text lands in user not system;
  bare/fenced/preamble/braces-in-strings JSON all parse; invalid→error.
- **reviewers**: security parses Block + finding + `prompt_sha` present; abstains on
  malformed; **fail-closes on secret in diff** (`ReviewerCallError::SecretScrubFailed`);
  nightwatch wraps telemetry, sanitizes attr breakers, routes to nightwatch chain,
  fail-closes on secret in telemetry.

### 5.3 Invariants (no-regression gate)

- **Verdict-replay**: a receipt's `(prompt_sha, model, provider, raw_response_sha,
  head_sha, policy_sha)` fully determines its admissibility. `prompt_sha` over the
  canonicalized prompt MUST be byte-stable across the port (NEW test `prompt_replay.rs`:
  load each ported `assets/prompts/*.md`, assert `prompt_sha` equals the value recorded
  from the jeryu source so replayed verdicts still bind).
- **SHA-bind / Law 4**: any drift in `evidence_pack_id`/`head_sha`/`policy_sha` drops the
  receipt; the judge then fails open to RequireHuman (never AllowMerge).
- **Veto > approval**: any single `Block` or hard-stop → Reject, regardless of quorum.
- **Fail-closed**: missing quorum policy, unsigned pack, secret-in-diff, exhausted budget,
  missing prompt → abstain/RequireHuman/Reject, never AllowMerge.
- **One-receipt-per-role** and **concurrent** reviewer execution preserved.
- **No-regression on existing harnesses** (run if/when present in fused repo):
  TUI `tuiwright` snapshots, web `Playwright` flows, and `MCP tools/call` round-trips
  must still pass after capability/agent rewire — these surfaces consume `AgentIntent`
  and the agent lifecycle, so this crate's PR must not break them.
- **Zero-evidence (D1)**: the gate MUST fail if any forbidden literal appears in the new
  crate:
  ```bash
  ! grep -rniE 'gitlab|jitforge|nitro|forge-core|proofcore|agentbridge|signrail|mr_iid|merge.?request' \
      crates/jeryu-review/src crates/jeryu-review/assets crates/jeryu-review/tests
  ```
  (Exception: the literal string `pr` and `pull_request` are fine; `merge_request`/`mr_iid`
  must be gone per D4. Prompt `.md` files referencing "merge request" in prose must be
  rewritten to "pull request".)

---

## 6. Risks & hardest seams

1. **prompt_sha replay fidelity (HIGHEST).** Verdicts are bound to `prompt_sha`. The
   canonicalizer strips `# (no-hash)` lines and trailing whitespace
   (`prompt_builder.rs:38-49`). Any change to the prompt `.md` files (including the D1
   rename of "merge request"→"pull request" *inside* a prompt) **changes the hash** and
   invalidates every historical verdict. Seam: edit prompts only on non-hashed lines, or
   accept a one-time prompt_sha rotation and document it. The `prompt_replay.rs` test
   pins this.
2. **Signature schema field rename (`algo`→`algorithm`, `value`→`value_hex`).** The
   receipt is signed over its own canonical JSON with the signature stubbed
   (`sign_canonical`, `orchestrator.rs:499-509`; `sign_receipt`, `runner.rs:143-153`).
   If the serialized field names change, every signature recomputes and old receipts fail
   verification. Seam: keep a stable internal canonical form for signing even if the
   public/persisted struct uses jeryu-signrail field names; or freeze the canonical-JSON
   shape in `signing.rs` independent of the wire struct.
3. **`jeryu-signrail` lacks ed25519.** It ships HMAC-SHA256 only. The whole reviewer/judge
   trust model is ed25519 (`evidence_signature_invalid` checks `algo=="ed25519"`,
   `judge.rs` test at `:266-275`). Either add an ed25519 `Signer` to signrail (cross-crate
   ask) or keep `EdSigningKey` local. Mismatch here silently degrades every verdict to
   Reject.
4. **`jeryu-proof` is empty of reviewer schema + conditions.** jit's `proofcore` has none
   of `AgentApprovalReceipt`/`EvidencePack`/`VibeGateVerdict` nor any of the 58 hard-stop
   conditions. The judge cannot run without `ConditionRegistry::evaluate`. Ordering risk:
   if the autonomy port slips, this crate is blocked or must temporarily host the schema
   and later donate it.
5. **MR→PR semantic gap in lifecycle.** GitLab MR-IIDs are per-project monotone integers;
   jeryu/`jeryu-core` `PullRequestId` may differ. `check_agent_pipeline` keys race
   detection off branch-ref naming (`-hypo-`) and job `ref_name.starts_with(branch_name)`
   (`agent_ops.rs:55-79`). The jeryu ci-run/job model must expose `ref_name` (or branch)
   per run or this detection breaks. Confirm `jeryu-ci-scheduler`/`jeryu-core` ci-run model
   carries branch/ref before porting `agent_ops.rs`.
6. **Verdict→MergeQueue admission.** `jeryu-ci-scheduler::MergeQueue` (Codex-owned) admits
   on a `ProofWitness`. The judge mints a `VibeGateVerdict`, not a `ProofWitness`. Need a
   bridge: either map AllowMerge verdict → synthesized `ProofWitness`/`Receipt`, or get an
   admission hook from the scheduler. Cross-crate coordination required; do not edit the
   scheduler.
7. **LLM transport import is large and provider-coupled.** `provider_chains.rs` (31.6k)
   carries free-tier OpenAI-compatible model ids (`openai/gpt-oss-120b:free`, etc.) and
   the per-role chain config. Claude/GPT/Gemini are all reached via the OpenAI-compatible
   client + `JekkoKeyPool`; "Claude/GPT/Gemini" is a *config* concern, not separate code
   paths. Risk is mostly volume + secret-resolver wiring (`JERYU_*` env names) and the
   `DataUse::NoTrain` invariant, not logic. Keep the role chain keys verbatim for replay.
8. **Capability UDS server client type.** `start_capability_server` currently takes a
   `GitlabClient` by value and clones per-connection (`capability.rs:213-263`). Swapping
   to `jeryu_agentbridge::AgentBridge` requires `AgentBridge: Clone` (it is `#[derive(Clone)]`
   today) — confirm it stays `Clone` after the rename, or wrap in `Arc`.
9. **Nonce replay cache is process-local** (`SEEN_NONCES` static, clears at 4096,
   `capability_request.rs:51-61`). Fine for single-process, but in a multi-daemon fused
   deployment this must move to the shared jeryu db (D3) or nonce replay protection is
   per-process only. Flag for the daemon spec.

---

## Summary (5 lines)

1. Specs new crate `crates/jeryu-review` porting ~3,300 LOC: reviewer orchestrator (concurrent, budget-gated, fail-to-abstain), 5 LLM reviewers, prompt_sha-replay prompt builder + robust JSON parse, pure judge (SHA-bind→hard-stops→quorum), quorum/sha-bind approval, rejudge drift triggers, capability/intent protocol, and the agent task lifecycle (ephemeral 2-day bot, `agent/<slug>-<ts>` branches).
2. Rewire is GitLab-free in the reviewer/judge/quorum core (rename only); the seams are capability intents + agent lifecycle: MR→PR (D4), pipeline→ci-run, `GitlabClient`→`jeryu-gitd`/`jeryu-api`, `project_id`→`RepoId`, grant-scope→`jeryu_core::AgentScope`, verdict→`jeryu-ci-scheduler` admission.
3. Hard prereqs: D2 renames landed; `jeryu-proof` must first host the receipt/verdict/pack schema + 58-condition `ConditionRegistry` (not in jit today); `jeryu-signrail` needs an ed25519 `Signer` (HMAC-only today); `jeryu-core::ReceiptKind` gains `AgentReview`/`Verdict`; LLM transport ported wholesale into `jeryu-review::llm`.
4. Acceptance gate: `cargo test -p jeryu-review` (14 orchestrator + 8 judge + 8 quorum + 8 rejudge + sha-bind/parse/reviewer cases) + `cargo build --workspace`, with invariants verdict-replay (prompt_sha byte-stable), Law-4 SHA-bind, veto>approval, fail-closed, plus a zero-evidence grep (no gitlab/jitforge/nitro/merge_request) and no-regression on tuiwright/Playwright/MCP-tools-call.
5. Hardest seams: prompt_sha replay fidelity (prompt edits rotate every historical verdict), signature field rename (`algo`→`algorithm`) recomputing receipt sigs, signrail lacking ed25519, empty `jeryu-proof` schema/conditions blocking the judge, and bridging AllowMerge verdict→`ProofWitness` for the Codex-owned merge queue.

File: /home/ubuntu/jeryuRUST/docs/port/05-agent-review.md
