# JeRyu: A Proof-Carrying, Agent-First Git Control Plane

Draft version: V3.01 working paper.

This Markdown file is the Markdown-friendly companion to `main.tex` and `main.pdf`. It mirrors the white paper's argument without IEEE formatting noise so executives, engineers, and agents can quickly retrieve the design, contracts, and roadmap.

## Executive Reading Path

- `executive-brief.pdf`: one-page CTO/CIO summary focused on business value, differentiation, and adoption rationale.
- `main.pdf`: full IEEE-style technical white paper with architecture, VTI, capability/MCP, merge/release gates, security, TUI, roadmap, and limitations.
- `paper.md`: this Markdown version for source review, agent retrieval, and fast internal circulation.

The executive thesis is simple: JeRyu is not another coding assistant. It is the proof-carrying delivery control plane around coding assistants. It lets agents move quickly while keeping validation, runner trust, cache policy, merge authority, release authority, and audit evidence under local, inspectable control.

## Abstract

Modern software agents can edit code faster than conventional review, CI, and release systems can safely absorb. GitHub Copilot cloud agent, GitLab Duo Agent Platform, OpenHands, SWE-agent, Aider, and Codex-style systems all improve authoring and task execution, but public surfaces generally stop short of owning local GitLab runner authority, cache trust, proof-scoped merge gates, and release evidence. JeRyu is the missing control layer: an agent-first delivery plane that treats intent, validation, CI execution, runner trust, cache use, failure evidence, merge gates, and release gates as one proof loop.

## Core Thesis

JeRyu assumes agents can already write code. Its purpose is to decide what agents are allowed to do and what evidence is required before their work can merge or release.

The V3.01 direction is:

- Intent first: every privileged action is tied to actor, task, grant, request, and idempotency identity.
- Policy before side effects: path scope, branch scope, risk tier, trust tier, TTL, and budget are evaluated before mutation.
- Validation as evidence: VTI can emit receipts; merge proof blocks smart-skipped validation when receipts are missing.
- Shared gates: CLI, TUI, webhooks, and capability API call the same merge/release gate logic.
- Durable memory: blockers, proof receipts, selector misses, cache taints, and screenshots are artifacts.

## Public Capability Baseline

Public agent products are strong at authoring and interaction. GitHub Copilot cloud agent can work in the background on implementation tasks and pull requests, but public documentation describes branch, PR, and hosting constraints. GitLab Duo Agent Platform brings agentic automation to GitLab issues, merge requests, pipelines, planning, and security workflows. OpenHands, SWE-agent, Aider, and Codex-style systems provide terminal/code autonomy.

JeRyu is deliberately different. It is a local-first GitLab-compatible authority layer. It does not compete on code generation. It controls validation, runner trust, cache policy, merge evidence, release evidence, and auditability.

## Why Git Must Change

Agent-first and vibe-coding workflows make code generation cheap and validation scarce. The right response is not fewer tests. It is more explicit validation: selected tests, skipped-test receipts, cache provenance, runner trust, and merge proofs that can be rechecked.

Legacy Git forges were built around human-paced pull requests, broad human credentials, and CI as a background reaction to commits. Agent-first development needs Git to understand task identity, capability grants, validation receipts, cache trust, runner isolation, proof-carrying merge decisions, and cleanup of speculative work. JeRyu keeps GitLab compatibility where it helps, but moves the agent proof loop into a local, open-source Rust control plane.

## Architecture

JeRyu is a Rust workspace with the `jeryu` binary and proof-scoped support crates.

Main surfaces:

- CLI and TUI for humans and supervising agents.
- Unix-socket capability API for local agents.
- GitLab webhook ingestion and reconciliation.
- Postgres-primary state with SQLite fallback for jobs, pools, evidence, VTI plans, selector misses, releases, cache, and audit events.
- Runner pool management across trust classes.
- Custom executor hooks for sandbox, cache, honeypot, and failure evidence.
- VTI for smart validation selection.
- Release gates for canary and production promotion.

Proof loop:

1. Intent: agent asks for an action.
2. Policy: control plane evaluates grants and scope.
3. Plan: VTI selects validation or falls back.
4. Execute: runner pool, sandbox, cache namespace, and CI job run.
5. Evidence: logs, capsules, receipts, and cache verdicts are recorded.
6. Decide: merge/release gate allows, denies, or blocks.

## Proof-Scoped Workspace

The support crates exist to make the repo easier for agents to reason about:

- `cargo-witness`: indexes public API signatures and builds witness graphs.
- `cargo-vrc`: maps changed paths to validation rings.
- `cargo-aer`: audits structural exceptions.
- `witness-rt`: emits structured repair packets from runtime expectations.
- `arc-bench`: benchmarks proof-scoped design tradeoffs.

This is procedural memory for agents. It tells an agent not only what code changed, but how to prove the change.

## VTI: Validation Test Intelligence

VTI is not just test skipping. It is proof-scoped validation planning.

V3.01 VTI rules:

- Unknown files force full validation.
- Docs-only is an allowlist, not a fallback.
- Nested module paths must match recursive subsystem ownership patterns.
- Internal plans can emit proof receipts with mode, confidence, changed paths, selected tests, skipped subsystems, fallback reason, and base/head identity.
- The default merge gate blocks when a required VTI receipt is missing or bound to a different head SHA.
- Selector misses are scoped by project, ref, SHA, and plan.
- Historical failures, dependency graphs, flake scores, and sentinel sampling should improve rules over time.

Target VTI output:

- changed paths
- affected subsystems
- selected lanes
- selected tests
- skipped tests
- fallback reasons
- confidence
- cache key inputs
- selector-miss status

## Capability and Admission Model

Current useful intents include:

- `RunTests`
- `FetchCapsule`
- `ProposePatch`
- `RacePatches`
- `RequestMerge`
- `ExplainBlockers`
- `GetSystemSnapshot`
- `ListAllowedActions`
- `PlanValidation`

V3.01 makes the action registry canonical so API, CLI, and TUI surfaces do not drift. It also adds a V3 capability request envelope with request id, actor, nonce, expiry, optional budget, optional grant proof, and framed JSON transport while keeping  `AgentIntent` clients working. The next step is schema generation, peer-credential actor binding, signed grant verification, and contract tests.

The intended authority model is an intent ledger:

1. Agent requests an action with actor/task/grant/request identity.
2. Policy evaluates path, branch, risk, trust, TTL, and budget.
3. Ledger records allow, deny, or escalate.
4. Domain service performs the side effect.
5. Evidence and cleanup records attach to the ledger entry.

Admission hooks are the Git boundary. Agent pushes should be rejected if branch, SHA, project, path scope, or task identity do not match an approved intent.

## Proof-Carrying Merge Gates

A merge gate must verify evidence, not trust the agent.

Gate inputs:

- MR project, IID, source branch, target branch, and head SHA.
- Latest pipeline for that exact SHA.
- Required jobs present, terminal, and passing.
- No pending/running required jobs.
- VTI plan and receipts for the same SHA.
- Selector misses scoped to the same project/ref/SHA/plan.
- Cache taints for artifacts used in validation.
- Security/release lanes for sensitive paths.
- Branch protection, approvals, conflicts, and stale-base state.
- Failure capsules and logs retained for blockers.

The output should be a durable merge decision record with blockers and evidence references.

## Runner, Cache, and Sandbox Boundaries

Fast agents increase runner and cache pressure. JeRyu treats execution context as evidence:

- Trusted branch, untrusted MR, release, and detonation lanes should not share assumptions.
- Cache hits must be gated by trust tier, taint, and epoch.
- BuildKit state should be namespace-scoped.
- Secrets must never leak through logs or capsules.
- Honeypot and taint infrastructure should record supply-chain detonations.
- Isolation can use techniques inspired by gVisor, Firecracker, and Bubblewrap depending on lane risk.

## Agent Workflows

Repair loop:

1. Agent fetches a failing job capsule.
2. Capsule identifies project, pipeline, job, owner area, and likely validation lanes.
3. Agent proposes a patch.
4. VTI plans validation.
5. Runner executes in the correct trust/cache namespace.
6. Merge gate checks exact SHA and receipts.

Patch-race loop:

1. Agent proposes multiple hypothesis branches.
2. Control plane launches validation in parallel.
3. Fastest branch is not automatically winner.
4. Winner must satisfy the same merge gate.
5. Losing branches and cache records are cleaned up.

## Threat Model

JeRyu assumes agents are useful but not inherently trusted.

Possible failures:

- Agent hallucinates that tests ran.
- Agent overfits to a narrow validation plan.
- Agent reuses stale evidence.
- Agent pushes outside task scope.
- Agent leaks secrets through logs.
- Agent uses tainted cache artifacts.
- Agent races patches but leaves unsafe cleanup state.

Defense layers:

- identity and intent
- policy
- execution isolation
- VTI receipts
- merge/release gates
- audit ledger

## TUI and Screenshots

The TUI is a Ratatui operations surface with nine tabs:

1. Mission
2. Release
3. Jobs
4. Agents
5. Tests
6. Pools
7. Cache
8. Evidence
9. Secrets

The V3.01 TUI upgrade takes the strongest theme from the v3 TUI notes: the
console must be a decision cockpit, not a passive dashboard. Mission now opens
by default and shows a Top Signal, Attention Queue, Proof Stack, metric tiles,
next actions, and compact activity graphics. Agents now has an Agent Cockpit
with phase, progress, branch/SHA, grants, timeline, and action guidance. The
command palette now previews risk, side effects, required grants, dry-run
availability, disabled reasons, and execution guidance from the canonical action
registry.

Screenshot command:

```bash
./scripts/capture-tui-screenshots.sh
just tui-screenshot-smoke
jeryu tui --capture --tab jobs --output paper/assets/jeryu-tui-jobs-flow.png
jeryu tui --capture --tab tests --output paper/assets/jeryu-tui-tests-vti.png
jeryu tui --capture --tab evidence --output paper/assets/jeryu-tui-evidence.png
```

The publication screenshot path uses `tui-capture`, which runs `jeryu tui --screenshot` in a real PTY, parses the terminal state with `vt100`, and rasterizes the grid with pinned DejaVu Sans Mono, fixed geometry, a lifted dark background, and brightened colors. This avoids missing-glyph boxes from browser, SVG, ANSI-converter, or window-manager capture paths.

Generated figures:

![TUI Jobs/Flow capture](assets/jeryu-tui-jobs-flow.png)

![TUI Tests/VTI capture](assets/jeryu-tui-tests-vti.png)

![TUI Evidence capture](assets/jeryu-tui-evidence.png)

![TUI Release capture](assets/jeryu-tui-release.png)

![TUI Mission capture](assets/jeryu-tui-mission.png)

![TUI Agents capture](assets/jeryu-tui-agents.png)

## Implementation Contracts

V3.01 should turn implicit behavior into contracts:

- Capability contract: stable action IDs, risk tiers, surfaces, grants, side-effect classes, request/response schemas.
- Validation contract: VTI plan IDs, changed paths, SHAs, selected tests, skipped tests, fallback reasons, receipts.
- Merge contract: decision record, blockers, policy version, evidence references, timestamp.
- Release contract: version, commit, canary status, production status, secret handoff, preflights, rollback path.
- Audit contract: JSON schemas for machines and TUI/screenshot surfaces for humans.

## Public Pain Points

The paper now cites public evidence that  Git/CI surfaces are under pressure:

- GitHub documents Copilot cloud agent as a GitHub-centered pull-request task flow with documented task/PR constraints.
- GitHub community reports describe Actions workflows stuck queued for long periods.
- GitLab support documents excessive pending-job queuing with Docker Autoscaler runners.
- GitLab Runner docs describe long-polling issues that can leave workers idle.
- GitLab support documents external status checks stuck pending and blocking merge requests.
- GitLab's Duo Workflow design notes that ordinary CI pipelines are not a complete runtime substrate for agentic workflows.

JeRyu addresses these classes of problem by making runner authority, validation proof, status evidence, cache trust, and merge/release decisions local and explicit.

## Architecture Appendix

The LaTeX paper includes a C4-style TikZ appendix diagram grounded in `docs/ARCHITECTURE.md`. It uses two panels:

- system topology: interfaces, the five `jeryu` planes, GitLab/Docker/Vault runtime substrate, and proof-scoped workspace crates
- agent proof flow: intent, policy/admission, VTI planning, execution, evidence, merge/release decision, and TUI/API feedback

The figure intentionally uses orthogonal rails, numbered flow markers, and separate control/proof/trust arrow styles so the architecture is readable in the IEEE PDF.

## Agent API Appendix

The LaTeX paper now includes a full-page single-column API appendix. It documents every current agent-relevant `jeryu` control surface:

- CLI commands for lifecycle, pools, jobs, pipelines, cache, agents, git, VTI/tests, release, secrets, host, repo, and action discovery
- exact CLI grammar from `src/cli.rs`, including all current flag-bearing forms and hidden protocol calls
- hidden protocol entrypoints: custom executor, server hook, and capability server
- Unix-socket capability protocol: V3.01 envelope,  `AgentIntent`, response shape, nonce/expiry behavior, and grant metadata
- capability intents: `ProposePatch`, `RacePatches`, `RunTests`, `FetchCapsule`, `RequestMerge`, `ExplainBlockers`, `GetSystemSnapshot`, `ListAllowedActions`, and `PlanValidation`
- native MCP server: stdio JSON-RPC and loopback Streamable HTTP `initialize`, `notifications/initialized`, `ping`, `tools/list`, and `tools/call` routed through the same capability policy
- HTTP engine routes: `GET /health`, `POST /hooks`, and `GET /cache/summary`
- action registry contract: risk tiers, side effects, required grants, surfaces, and dry-run metadata

The native MCP server in V3.01 exposes the canonical capability-backed tool surface through stdio JSON-RPC and loopback Streamable HTTP. It supports `jeryu mcp serve`, `jeryu mcp serve-http`, and `jeryu mcp tools --json`; negotiates MCP protocol version `2025-11-25`; implements `initialize`, `notifications/initialized`, `ping`, `tools/list`, and `tools/call`; and maps each tool invocation back into the existing `AgentIntent` execution path.

The MCP tool catalog is schema-bearing and registry-derived. It exposes `jeryu.get_system_snapshot`, `jeryu.explain_blockers`, `jeryu.plan_validation`, `jeryu.fetch_capsule`, `jeryu.run_tests`, `jeryu.propose_patch`, `jeryu.race_patches`, and `jeryu.request_merge`, with input schemas, output schemas, and MCP annotations for read-only, destructive, idempotent, and open-world behavior. Every tool call still traverses the same grant, evidence, GitLab, and merge/release gates as the Unix-socket capability API.

The Streamable HTTP transport is local by construction: it rejects non-loopback binds, rejects non-local `Origin` headers, requires `MCP-Protocol-Version` after initialization, validates `Mcp-Method` and `Mcp-Name`, issues ephemeral `Mcp-Session-Id` values, and supports `DELETE /mcp` session teardown. The server returns human-readable MCP `content` plus native `structuredContent`, so chat agents and structured orchestrators can consume the same policy result without losing audit detail.

## Reproducibility

Minimum proof bundle:

- workspace version `3.0.1`
- `VERSION`
- `version.json`
- `paper/executive-brief.tex`
- `paper/executive-brief.pdf`
- `paper/main.tex`
- `paper/main.pdf`
- `paper/paper.md`
- `paper/references.bib`
- `paper/IEEEtran.cls`
- `paper/IEEEtran.bst`
- generated screenshots under `paper/assets`
- `cargo check --workspace`
- targeted VTI, impact, and TUI tests
- `cargo-witness build`
- `cargo-vrc plan`

Negative results should also be reproducible. Unknown file fallback, stale pipeline blockers, selector misses, and denied capability requests should be durable records.

## V3.01 Roadmap

Stage 1: truthfulness.

- Version numbers and changelog entries.
- Registry-backed action list.
- Conservative VTI fallback.
- Recursive subsystem patterns.
- Deterministic screenshots.

Stage 2: authority.

- V3 capability request envelope.
- Durable branch-write grants.
- Admission enforcement.
- Structured dynamic CI emitter.
- Scope-aware side-effect classes.

Stage 3: proof.

- VTI receipts.
- Merge-gate decision schema.
- Release-gate decision schema.
- Selector-miss learning.

Stage 4: operations.

- Agent Runs pane.
- Merge Gate pane.
- VTI Plan inspector.
- Patch Race pane.
- Event-stream health.
- Screenshot CI.

## Evaluation Plan

Report:

- VTI selection latency.
- Test-count reduction.
- Selector-miss rate under oracle runs.
- Unknown-file fallback coverage.
- Merge-gate false-allow rate.
- Capability contract conformance.
- Admission hook rejection coverage.
- TUI render determinism.
- Screenshot determinism.
- Patch-race cleanup success.

Speedup is not enough. The paper must report where JeRyu widened validation because proof was missing.

## Current Limitations

The current repository is not yet the complete V3.01 system. Known gaps:

Implemented in this tranche: V3 capability envelopes, nonce/expiry checks, durable branch-write grants, persisted admission decisions, ledger-aware agent-ref enforcement, commit-SHA binding for GitLab commit API writes, typed dynamic CI YAML, strict-sandbox fail-closed behavior, secure-by-default GitLab TLS, VTI receipt emission, default merge-gate receipt enforcement, action-first Mission Control, Agent Cockpit, command-palette blast-radius previews, and regenerated deterministic TUI screenshots.

- generated capability schema
- signed grant verification
- peer-credential actor binding
- path-scoped grants and SHA binding for non-commit Git write paths
- GitLab-backed merge-gate completeness
- persisted VTI receipts and GitLab job binding
- selector-miss learning
- richer event stream
- dedicated agent-run lifecycle state
- stronger merge/release fixtures

## References

The LaTeX version uses `references.bib`, currently 55+ entries spanning public agent products, open-source agents, empirical software-agent studies, regression test selection, CI/CD, supply-chain proof systems, sandboxing, platform pain points, and project dependencies.
