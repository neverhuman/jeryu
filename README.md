# Jeryu

Jeryu is a local, GitHub-compatible forge implemented as a Rust-first workspace.
Agents should treat it as the local GitHub-shaped control plane: repositories,
issues, PRs, checks, Actions-compatible workflows, release receipts, and bounded
agent automation all live behind one workspace root.

The product does not depend on hosted GitHub, external forge source, or external
forge assets. Compatibility work is self-authored and fixture-driven. Retired
provider and retired review-request vocabulary are intentionally absent from
code, docs, fixtures, tests, generated artifacts, and operator scripts.

## Agent Start Here

Read these files before editing:

- `AGENTS.md`
- `agent/owner-map.json`
- `agent/test-map.json`
- `agent/generated-zones.toml`
- `agent/proof-lanes.toml`
- `agent/exceptions.toml`
- `agent/boundaries.toml`
- `agent/tool-adoption.toml`
- `docs/architecture.md`
- `docs/testing.md`
- `docs/errors.md`
- `docs/boundaries.md`
- `docs/generated-zones.md`
- `docs/release.md`
- `docs/release-process.md`
- `docs/audit-rubric.md`
- `docs/agent-native-standard.md`
- Local `AGENTS.md` files under changed paths, for example `docs/AGENTS.md`
  and `crates/jeryu-api/AGENTS.md`
- `CI_TRACKER.md`

Public and agent-facing review objects are PRs. Do not add aliases, flags,
fixtures, fields, docs, screenshots, or compatibility layers for retired review
request terminology.

## Current State (2026-06-01)

`origin/main` contains the local v+1.0.0 platform baseline: durable
`ForgeCore::open_sqlite` storage, the local Axum API, API-backed TUI capture,
local repository import, affected fast CI, and guided GitHub-compatible
REST/GraphQL repair responses, and gitd-backed local import materialization.
The latest closeout full lanes pass with 1175 workspace nextest tests in both
local-native and GitHub-clean profiles, all phase gates at
PASS=7/PENDING=0/FAIL=0, the full manifest lane union green, proof-evidence
Jankurai full scan at score 92/caps 0, and changed-file Jankurai checks at hard
0/caps 0. Publishing is PR-first by default; direct `main` pushes require
explicit opt-in.

This checkpoint is local-first. The default operator path binds loopback and
uses `~/.local/share/jeryu`; public/LAN access, token rotation, production
signer adapters, and deeper daemon hardening remain explicit follow-up work.
The runner fabric now includes an epoch-fenced registry plus the deterministic
`xbabe0..xbabe3` 40-slot dogfood fleet. The autonomy bridge records
`jeryu/autonomy` verdict check-runs as advisory release evidence only; it does
not merge PRs until the safety rework is proven and re-enabled.

## Implemented Surfaces

| Area | Crates / binaries |
| --- | --- |
| Git service and imports | `jeryu-gitd`, `jeryu-mirror`, `jeryu-mirror-cli` |
| Forge/domain/API facade | `jeryu-core`, `jeryu-domain`, `jeryu-api`, `jeryu-cli` |
| Agent, review, MCP, and read models | `jeryu-mcp`, `jeryu-agentbridge`, `jeryu-autonomy`, `jeryu-review`, `jeryu-bugtracker`, `jeryu-readmodel`, `jeryu-tui` |
| CI IR, scheduler, cache/artifact planning | `jeryu-ci-ir`, `jeryu-ci-compiler`, `jeryu-ci-scheduler`, `jeryu-cache-policy`, `jeryu-artifact-metadata`, `jeryu-ci-bin` |
| Runner fabric | `jeryu-runner-core`, `jeryu-runner-native`, `jeryu-runner-microvm`, `jeryu-runner-oci`, `jeryu-runner-protocol`, `jeryu-runner-registry`, `jeryu-runnerd` |
| Rust CI acceleration | `jeryu-rustjet`, `jeryu-rustjet-cli` |
| JeryuCache cache/CAS | `jeryu-cache-core`, `jeryu-cache-service`, `jeryu-cache-cli`, `jeryu-cache-adversary`, `jeryu-cache` |
| Proof, governance, and repo gates | `jeryu-proof`, `jeryu-mapcheck`, `jeryu-repogate`, `jeryu-evidence` |
| Release provenance and compliance | `jeryu-signrail`, `jeryu-signing`, `jeryu-compliance-export`, `jeryu-lifecycle` |
| Benchmark, observability, and operations | `jeryu-bench`, `jeryu-obs`, `jeryu-ops`, `jeryu-phase7-cli` |
| Enterprise/operations layer | `jeryu-enterprise`, `phase11-*`, `jeryu-kernel`, `jeryu-tenant`, `jeryu-replay-verifier`, `jeryu-phase11-bin` |

## Local Live Runtime

The first live target is local-only. `jeryu-api` can run an Axum server backed
by durable SQLite under the Rust data dir:

```bash
cargo run -p jeryu-api --features web -- web serve \
  --bind 127.0.0.1:8787 \
  --spa-dir web/dist \
  --data-dir ~/.local/share/jeryu
```

The server exposes `/health`, `/api/v1/bootstrap`, `/api/v1/bootstrap.tui`,
`/api/v1/repos`, `/api/v1/repos/{id}`, repo refs/tree/blob/raw/readme routes,
`/api/v1/markdown/render`, `/api/v1/ws`, and the guided GitHub-compatible
`/user` and `/graphql` routes.
`~/.local/share/jeryu` is intentionally separate from the retired
`~/.jeryu` config/secrets tree.

`jeryu-tui` has a deterministic capture path for fixture or live API data:

```bash
cargo run -p jeryu-tui -- --once --source api \
  --api-url http://127.0.0.1:8787 --tab mission --width 120 --height 40
```

Local Git directories can be registered into the SQLite forge store and a
host-local manifest under the data dir. The same import also materializes a
gitd-managed bare mirror at `~/.local/share/jeryu/git/repos/OWNER/REPO.git` so
local clone/fetch smoke tests use the Jeryu Git storage path, not only metadata:

```bash
cargo run -p jeryu-mirror-cli -- import-local \
  --data-dir ~/.local/share/jeryu /path/to/repo-or-bare.git
```

## GitHub-Compatible API

The REST edge is a guided GitHub subset for common local `gh` and agent flows:
`/user`, repository list/view/create, pull request list/view/create/merge,
issues and issue comments, statuses, check runs, branch protection, releases,
hooks, and `/api/v1/version`. Unknown REST routes return GitHub-shaped `404`
objects with `jeryu_repair_hint`, MCP tool ids, and closest Jeryu REST route
alternatives.

`POST /graphql` is intentionally narrow. It supports read-only `__typename`,
`viewer`, and simple `repository(owner, name)` probes; other GraphQL operations
return `501` with `jeryu_repair_hint`, MCP tool ids, REST alternatives, and the
local rerun command expected for extending support.

## Web and TUI

The React web app lives under `web/` and is part of the local product surface.
The current recorded web proof lane includes typecheck, 28 Vitest tests, build,
lint, and Playwright end-to-end coverage against the local BFF/API contract.

The TUI exposes `jeryu-tui --once` for deterministic tests and captures. It can
render fixture data or the live `/api/v1/bootstrap.tui` read model, including
all 18 tabs used by the local control-plane views.

## Local CI

Local CI is the source of truth. The default worker count is 40, and
`ci-fast-push.sh` is the local PR gate: it builds an affected plan from
tracked and untracked changes, escalates shared roots to full CI, runs mapped
Rust/web/API/TUI/db lanes, then runs Jankurai diff and repository audits before
publishing. By default, a non-`--no-push` run pushes the current branch and
opens or reports a GitHub PR; direct `HEAD:main` publishing requires explicit
`--push-main` or `JERYU_CI_PUSH_MAIN=1`. The Jankurai changed-list is
regenerated from the affected plan plus committed, staged, unstaged, and
untracked Git files; the gate then fails closed if source files remain
untracked because GitHub and Jankurai changed-fast proof can only validate
staged or tracked paths. `ops/ci/ci-env.sh` detects the local or GitHub profile,
keeps dockerless native Rust as the default executor, uses `sccache` when it is
available, and falls back to ordinary Cargo on GitHub-hosted runners.

Use `bash ci-fast-push.sh --full --no-push` when a change must prove the full
hosted-lane union locally. Full mode forces the workspace gate, verifies the
GitHub fallback profile with `JERYU_CI_PROFILE=github` and
`JERYU_CI_USE_SCCACHE=0`, installs/verifies the open security toolchain, then
runs every full lane declared in `agent/ci-lanes.toml`. Full release validation
also fails closed when retired `~/.jeryu`, old `/home/ubuntu/jeryu`, local
`:2224`, or retired-provider runner/process state is still active. `jeryu-repogate
ci-lanes-check` is the drift guard: every workflow under `.github/workflows/`
must be declared in the manifest, every substantive `run:` command must match a
local lane, and setup-only commands are explicitly allowlisted.

The fast gate also verifies this repository before testing: it builds and
checks the repo-local `jeryu` binary, ignores any retired `~/.jeryu/bin/jeryu`
binary on `PATH`, and accepts only the canonical GitHub remote when a remote is
configured.

```bash
just fast
just ci
just full
just security
just audit
```

The tracker in `CI_TRACKER.md` records the latest passing counts, Jankurai
score, and phase-gate status. Do not make a missing capability look green; keep
it PENDING with evidence until the runtime exists.

Hosted workflows are thin wrappers around local scripts. Jankurai is pinned to
the `neverhuman/jankurai` `v1.6.10-deadlang-precision` tag and must report
`jankurai 1.6.10` before CI uses it. The hosted `ci-fast` workflow runs the
same `ci-fast-push.sh --no-push` path used locally; the difference is profile
detection, not a separate test plan. The hosted security workflow calls
`ops/ci/security-tools.sh` for pinned open-source tool installation and then
delegates to `ops/ci/security.sh`; no hosted-only security command is allowed.
Cosign keyless signing is opt-in with `JERYU_COSIGN_KEYLESS=1`, while default
local CI writes an honest transcript/instructions artifact instead of blocking
on an interactive OIDC flow.

## Cache Laws

The Phase 12 JeryuCache implementation is designed around these cache laws:

- fork and public/untrusted jobs read source caches only and cannot consume trusted compiled caches;
- trusted compiled cache writes require T1 protected-internal policy and a green protected decision;
- cache key material is verified before writes/restores and must match the request trust tier;
- cross-project compiled reads are denied unless explicitly allowlisted;
- release lanes ignore mutable compiled caches and use L6 hermetic snapshots;
- promotion from quarantine indexes the promoted artifact for later restore and emits receipts;
- restore without an explainable fingerprint is blocked;
- all cache events emit deterministic JSON receipts;
- adversarial false-hit checks compare key material and object digests before accepting reuse.

## Useful Commands

```bash
just fast
just full
./scripts/ci-doctor.sh
cargo run -p jeryu-cache-cli -- self-test
cargo run -p jeryu-cache-cli -- key --material examples/cache-key-material.json
cargo run -p jeryu-cache-cli -- policy --request examples/fork-pr-write-request.json
```

The archive includes validation scripts that are safe to run even when macOS
AppleDouble `._*` files are present from tar extraction.

## Implementation Boundary

The engineering plan remains broader than this workspace checkpoint. The
GitHub-compatible REST subset, guided GraphQL endpoint, React web surface, CLI,
proof lanes, tool-adoption evidence, DB migration evidence, and live runner
escape matrix are present. Complete Git protocol parity beyond current
oracle/import smoke coverage, authenticated public/LAN deployment, production
signer adapters, benchmark lab execution adapters, multi-node runner control
plane, migration waves, and deeper daemon hardening remain post-v1 work.
Checked-in tests and local gates must reflect what is actually present, not
aspirational behavior.
