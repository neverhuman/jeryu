# JeRyu V2.0.1 Upgrade Plan

Version target: `2.0.1`

Scope: convert JeRyu from an ambitious agent-aware GitLab control plane into a proof-carrying, agent-first delivery control plane with truthful capabilities, hardened authority boundaries, robust VTI, and publishable evidence.

## Current Findings

Local review confirms the main v2 upgrade risks captured in the retained upgrade review corpus and current implementation:

- `docs/API.md` previously said `ProposePatch`, `RacePatches`, and `RequestMerge` were deferred, while `src/capability.rs` actively handled them. V2.0.1 docs now mark them active and grant-required.
- `src/capability.rs::RequestMerge` now returns a versioned advisory merge-gate proof over selector/cache evidence, but is not yet a full MR/pipeline/approval merge gate.
- `src/admission.rs` now evaluates pre-receive lines into versioned allow/audit/deny records, persists hook decisions, and can allow enforced agent refs when they match active capability grants. GitLab commit API writes now bind grants to the returned commit SHA. Signed grants, peer identity, path scopes, and SHA binding for non-commit Git write paths remain to be built.
- `src/capability.rs` and `src/test_runner.rs` still build GitLab CI YAML with string interpolation; safe typed CI generation remains a post-cleanup hardening item.
- `src/test_runner.rs` creates scratch branches from `main` instead of an immutable requested SHA/ref.
- `src/impact.rs` now treats unknown files as full-validation fallback instead of docs-only, closing the unsafe non-code shortcut called out by the V2 review notes.
- VTI is conservative but still path/glob driven; it lacks coverage, history, flake, confidence calibration, and proof receipts.
- The state layer now supports RedlineDB as the primary backend for concurrent agent workloads and keeps RedlineDB as the embedded fallback. Core state operations now have a disposable-container Redline proof lane; remaining state work is to move every direct SQL caller behind `Db` methods and promote that lane into CI.
- TUI docs now describe the nine-tab model and deterministic PNG capture path.
- The action registry now drives capability `ListAllowedActions`; generated docs/schema parity remains to be added.

## V2 Tip Disposition

The raw review corpus is retained as release evidence. The repeated recommendations resolve into four buckets:

- **Implemented in the V2.0.1 cleanup:** version tracking, RedlineDB-primary state with RedlineDB fallback, disposable Redline proof, unknown-file full fallback, recursive VTI subsystem globs, canonical action listing for capability discovery, advisory merge proof records, branch-write capability grants with commit-SHA binding for GitLab commit API writes, persisted admission decisions, TUI screenshot capture, warning-clean proof lanes, and the IEEE/Markdown paper package.
- **Partially implemented but not yet release-complete:** intent ledger, admission enforcement, merge readiness, VTI proof receipts, cache taint decisions, and TUI proof panes. These exist as working surfaces or advisory records but need stricter schemas, scoped evidence, and caller-independent enforcement before being marketed as complete.
- **Deliberately deferred because they are high-risk or broad:** typed CI YAML generation, peer-credential authentication on the capability socket, path-scoped grants, signed grants, full GitLab MR/pipeline/approval merge gate, event streaming, semantic log intelligence, patch-race tournament completion, hermetic sandbox replay, and generic multi-repo onboarding.
- **Documented as limitations rather than claims:** strict sandboxing is not yet a production isolation boundary, dynamic CI generation is not yet injection-hardened, VTI is not yet coverage/history/flake aware, and merge decisions remain advisory until wired to complete GitLab evidence.

## Release Control

- Set workspace package version to `2.0.1` for the V2.0.1 development line.
- Add `VERSION` with `2.0.1` and a `version.json` checkable against Cargo metadata.
- Add `CHANGELOG.md` section `2.0.1 - Agent Proof Control Plane`.
- Tag policy: `v2.0.1-rc.1` after check/unit/integration lanes pass, `v2.0.1` after release and security lanes pass.
- Require `cargo run -p jeryu -- repo render-agent-index --check` before any release tag.

## Milestone 1: Contract Truth

Goal: one canonical action/capability contract used by docs, CLI, TUI, and capability API.

Implementation:

- Replace hardcoded `ListAllowedActions` with serialization from `src/tui/action_registry.rs`.
- Add action status: `available`, `experimental`, `dry_run_only`, `disabled`, `deprecated`.
- Add side effect classes: `read`, `test`, `branch_write`, `mr_write`, `merge`, `release`, `secret`, `runner`.
- Add generated `paper/generated/action-surface.md` and `docs/API.generated.md`.
- Add contract tests checking `AgentIntent`, action registry, docs table, and CLI JSON output agree.

Tests:

- Unit: action registry serialization and risk tier mapping.
- Snapshot: capability `ListAllowedActions` equals `jeryu action list --json`.
- CI: API docs generated check.

## Milestone 1.5: Concurrent State Backend

Goal: make JeRyu's durable memory suitable for many agents, webhooks, runners, and TUI/API clients operating at the same time.

Implemented:

- Add `JERYU_DATABASE_URL` selection with `redline://` and explicit `redline:` URL support.
- Keep RedlineDB as the no-config fallback and in-memory test backend.
- Add bootstrap-managed `jeryu-redline` Docker Compose service and fresh-env Redline URL generation.
- Move shared upserts away from RedlineDB-only `INSERT OR REPLACE` / `INSERT OR IGNORE` syntax to portable `ON CONFLICT` statements.
- Add optional `JERYU_TEST_REDLINE_URL` integration smoke coverage for core state operations, VTI ledgers, cache verdicts, epoch bumps, taint propagation, CacheBrain hit/deny decisions, capability grants, and admission decisions.
- Add `just state-proof` to run the Redline smoke against a disposable runtime container.

Next:

- Add a required CI Redline service lane before the final `v2.0.1` tag, using the disposable proof target as the local equivalent.
- Convert remaining direct pool users into `Db` methods when their query shape becomes cross-module behavior.
- Decide whether embedded high-write local caches should use a Rust-native store such as `redb` while keeping RedlineDB as the source of truth for operational state.

## Milestone 2: Capability Security and Intent Ledger

Goal: all agent actions become typed, scoped, auditable intents.

Implementation:

- Add `capability_intents`, `capability_grants`, and `admission_decisions` tables. **Initial V2.0.1 rows now exist for branch-writing capability intents and hook decisions.**
- Add request envelope: `protocol`, `request_id`, `agent_id`, `task_id`, `grant_id`, `intent`, `payload`, `idempotency_key`.
- Harden Unix socket: mode `0600`, Linux peer credentials via `SO_PEERCRED`, redacted logging, framed request reads, max payload per intent.
- Add policy hooks: allowed project IDs, branch prefix, path scopes, TTL, test budget, mutation class.
- Require every mutating intent to create an `approved` ledger entry before side effects. **Initial implementation records grants after successful branch writes and binds commit API writes to the returned SHA; next step is pre-authorization and rollback-aware status updates.**

Tests:

- Unit: grant expiry, path allow/deny, idempotency replay.
- Integration: unauthorized peer/request denied, denied attempts audited.
- Security lane: no plaintext patch/secret payloads in logs.

## Milestone 3: Safe CI and Validation Engine

Goal: agents ask for validation; JeRyu returns durable proof handles.

Implementation:

- Replace raw YAML strings with typed CI document structs serialized by `serde_yaml`.
- Validate job names, image names, tags, stages, and scripts.
- Replace raw shell command path with a validation command registry.
- Add high-risk `trusted_shell` mode for explicit human-granted arbitrary shell.
- Anchor test runs to exact target SHA/ref; never silently use `main`.
- Return `validation_id`, branch, commit SHA, pipeline ID, selected tests, skipped tests, event stream path, cleanup status.
- Reuse one internal validation service from CLI and capability API.

Tests:

- Unit: YAML serialization rejects multiline/suspicious identifiers.
- Integration with fake GitLab client: branch created from requested ref, pipeline selected by branch and SHA.
- Regression: trigger pipeline errors are returned, not swallowed.

## Milestone 4: Proof-Carrying Merge Gate

Goal: merge decisions become verifiable evidence objects.

Implementation:

- Create `MergeGate::evaluate(project_id, mr_iid, actor)` shared by CLI, TUI, capability, and webhook handlers.
- Check MR existence, source/target branch, exact head SHA, latest pipeline, required jobs, pending jobs, approvals, conflicts, branch protection, CODEOWNERS equivalent, VTI plan confidence, selector misses scoped to project/ref/SHA, cache taints scoped to artifacts, secret/security lanes, release-impacting files.
- Rename current advisory path to `EvaluateMergeReadiness` or upgrade `RequestMerge` to call the real gate.
- Persist `merge_gate_decisions` with policy version and evidence refs before any merge call.

Tests:

- Table tests for allow/deny/escalate.
- Fake GitLab integration tests for stale pipeline, missing required job, pending job, failed job, wrong SHA, approval missing.
- Regression: global selector miss no longer blocks unrelated project without scope match.

## Milestone 5: Admission Enforcement

Goal: Git server pre-receive hook rejects unauthorized agent writes.

Implementation:

- Parse push metadata and actor identity from GitLab hook environment where available.
- For agent branch patterns, require approved intent/grant matching new SHA, branch, project, and path scope. **Initial implementation matches active grants by fully-qualified ref and optional SHA.**
- Reject direct agent pushes to protected refs.
- Emit machine-readable hook denial messages.
- Write admission allow/deny events. **Implemented in `admission_decisions`.**

Tests:

- Unit: parse pre-receive lines.
- Integration: approved intent accepted, unknown intent rejected, expired intent rejected, branch mismatch rejected.

## Milestone 6: VTI 2.0.1 Robustness

Goal: VTI becomes a proof selector, not only a glob router.

Implementation:

- Fix unsafe `impact.rs` fallback: unknown files force full validation.
- Update subsystem globs to recursive patterns where needed, especially `src/tui/**`, `src/test_intel/**`, `src/gateway/**`.
- Add schema validation for external `.jeryu/testmap.toml`; align docs and parser.
- Add VTI proof receipts: selected, skipped, docs-only, full fallback, cache hit, confidence, invalidators, evidence refs.
- Add selector-miss scoping by project/ref/SHA/plan.
- Add sentinels for docs-only and low-confidence selected plans.
- Start a test evidence graph with file -> subsystem -> tests -> jobs -> historical failures.

Tests:

- Unit: recursive glob coverage, unknown file full fallback, docs-only allowlist only.
- Golden plans for representative changes.
- Oracle tests: simulated skipped failure records selector miss and widens future plans.
- Integration: generated GitLab child YAML uses documented schema.

## Milestone 7: TUI Agent Operations Console

Goal: TUI mirrors the API and shows the proof loop.

Implementation:

- Update `docs/JERYU_TUI.md` to the current nine-tab model.
- Add an Agent Runs pane: task state, branch, MR, validation ID, current blocker, next action.
- Add Merge Gate pane: required jobs, VTI confidence, taints, selector misses, approvals, decision.
- Add VTI Plan inspector: changed files, selected tests, skipped tests, fallback reasons, proof receipt.
- Add Patch Race pane: hypotheses, pipeline IDs, winner, loser cleanup, evidence.
- Add event stream status once the event bus exists.
- Add deterministic PNG capture command: `jeryu tui --capture --tab <tab> --output paper/assets/<name>.png`.

Tests:

- Existing smoke render for every tab.
- Snapshot tests for agent, merge gate, VTI, evidence, and patch race empty/populated states.
- Screenshot command writes valid PNG and is stable for empty state.

## Milestone 8: Paper and Public Evidence

Goal: publish an 8-10 page IEEE-style paper backed by screenshots, examples, and limitations.

Implementation:

- Maintain `paper/main.tex`, `paper/paper.md`, `paper/references.bib`.
- Generate screenshots into `paper/assets/` through the TUI capture command.
- Add architecture figures: intent-ledger loop, VTI proof selector, merge gate, evidence graph.
- Add benchmarks: VTI latency/savings, false-skip oracle, cache hit safety, patch-race throughput, TUI render stability.
- Keep an honest status matrix: implemented, experimental, planned.

Validation:

- `latexmk -pdf paper/main.tex` or documented fallback.
- Link check for references.
- `cargo check --workspace --message-format=json`.
- `cargo nextest run -p jeryu --lib --profile ci`.
- `cargo test -p jeryu --test '*' -- --test-threads=1`.

## Execution Order

1. Fix compile/check failures if any exist in the current dirty tree.
2. Contract truth and action registry unification.
3. Capability intent ledger and security envelope.
4. Safe validation engine and typed CI YAML.
5. Merge gate and admission enforcement.
6. VTI 2.0.1 robustness.
7. TUI capture and agent console upgrades.
8. Paper figures, benchmarks, release candidate.
