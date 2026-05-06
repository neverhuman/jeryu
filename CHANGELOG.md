# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Interactive Ratatui Rust TUI for God-Mode control dashboard.
- GitHub templates and OSS documentation structure.
- Initial GitLab Omnibus bootstrap logic and execution engine.

## [3.0.1] - 2026-04-27
### Added
- V3.01 capability request envelope with request id, actor, nonce, expiry, optional budget, optional grant proof, and length-prefixed JSON framing support while retaining  `AgentIntent` compatibility.
- VTI proof receipts for internal plans plus `jeryu test select --emit-receipt`.
- Merge-gate VTI receipt enforcement so smart-skipped validation cannot satisfy the default policy without a proof receipt.
- Explicit strict sandbox backend reporting with fail-closed behavior when strict isolation is requested but no `bwrap` or `unshare` backend is available.
- Real `agent list` support through GitLab issue label queries.
- Deterministic `jeryu tui --capture` PNG export for paper, review, and agent evidence workflows.
- Action-first TUI Mission Control landing view with Top Signal, Attention Queue, Proof Stack, metric tiles, next actions, and compact sparkline context.
- Agent Cockpit TUI view with session phase, progress, branch/SHA, grants, timeline, and action guidance.
- Command palette preview pane backed by the action registry, showing risk, side effects, required grants, dry-run availability, disabled reasons, and execution guidance.
- IEEE-style V3.01 working paper sources, agent-friendly Markdown, bibliography, and generated TUI screenshots.
- Version control files: `VERSION` and `version.json`.
- Postgres-primary state backend with SQLite fallback, bootstrap-managed Postgres Compose service, optional `JERYU_TEST_POSTGRES_URL` smoke coverage, and a disposable `just postgres-state-proof` harness.
- Backend-neutral state SQL placeholder handling for core Postgres operations across pools, managers, job/event tracking, VTI records, capability grants, and admission decisions.
- Backend-aware cache control managers for epoch invalidation, taint propagation, and CacheBrain decisions; executor cache writes now go through `Db` methods.

### Changed
- `RunTests` capability requests now return pipeline trigger errors instead of silently succeeding after branch creation.
- Dynamic CI YAML for capability-triggered test branches now uses a typed serializer and a fixed scope allowlist.
- GitLab TLS certificate validation is secure by default; insecure cert acceptance requires explicit `JERYU_GITLAB_INSECURE_TLS`.
- Group webhook creation now enables push events so engine supersedence and VTI planning receive the events they depend on.
- Capability action listing now derives from the canonical action registry.
- The TUI now opens on Mission instead of Jobs so operators see blockers, missing proof, and next actions first.
- VTI subsystem ownership patterns now include nested TUI, gateway, and test-intelligence modules.
- Unknown file changes now conservatively select full validation instead of docs-only validation.
- API and TUI docs now describe the current nine-tab TUI and screenshot capture path.
- Shared state upserts now use portable `ON CONFLICT` SQL instead of SQLite-only `INSERT OR REPLACE` forms.
