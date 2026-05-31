# Changelog

## Unreleased

## v4.0.0 - 2026-05-31

- GitHub-compatible REST edge mounted on `jeryu-api`: `github_forward` plus the `github/mod.rs` `handle()` dispatch table and `github/pulls.rs`, with in-process `routes::Response.headers` (`json_response_with_headers`) returning `route_to_existing` and the `X-Jeryu-Reused-PR` header on PR reuse.
- `X-Jeryu` steering middleware and `GET /.jeryu/capabilities` capability advertisement.
- WebSocket event spine for live activity, with the live assembler projecting pool/health state.
- Operator surfaces over the spine: TUI Pools/Health views and the Web `/fleet` page driven by the live read-model.
- Autonomy FULL-AUTO loader (`FullAutoProfile`: R0-R4 auto, R5 fail-closed) plus pre-merge CI-check gating (`EvidencePack.ci_status`, `required_ci_lanes`, and `missing_/failed_required_ci_check` hard-stops in `conditions/ci.rs`) merged into the judge veto walk.
- PR-overlap engine (`crates/jeryu-core/src/overlap.rs`) with route-to-existing reuse wiring.
- Real MCP backend wired through `jeryu-mcp`.
- GitLab/MR vocabulary purge across the workspace in favor of GitHub-native PR vocabulary.
- CI parity hardening from the Codex engine: `--full` and `github-vanilla` local lanes, the drift guard, and the `ops/ci` manifest landed on main.
- 18-repo fleet preparation: ownership/test maps extended, workspace versioned at 4.0.0, and the canonical Apache-2.0 LICENSE added at the repo root.
- Jankurai full audit: clean-code raw score 89, but the headline lands at 70 — held below the 85 floor by a known false-positive dead-language cap plus same-name/same-body duplication warnings (pre-existing on `main`; the cap fix belongs in jankurai, tracked as a pre-v4 follow-up — and a reason v4.0.0 is staged-but-untagged). Tool adoption is artifact-verified across the applicable catalog tools.
- Runner sandbox now has a live Docker-backed escape matrix covering no-new-privileges, default seccomp, cgroup memory/pid pressure, network denial, host socket absence, and workspace-only writable file isolation.
- Local CI lanes default to 40 workers through `ops/ci/common.sh`.
- Added the first local live runtime: SQLite-backed `ForgeCore::open_sqlite`, `jeryu-api --features web` Axum routes/WebSocket, `jeryu-tui --once --source api`, affected fast-CI planning, and explicit local Git import registration.
