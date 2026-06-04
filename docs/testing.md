# Testing

Local CI is the source of truth. Hosted CI mirrors these commands, but it must
not replace them or make a local gate silently green.

Default worker count is 40. CI scripts source `ops/ci/common.sh` or
`ops/ci/ci-env.sh`, which set `JERYU_CI_JOBS=40` and `CARGO_BUILD_JOBS=40`
unless the caller explicitly overrides them. Local Jeryu runners default to
`native-rust-hot`; GitHub-hosted clean-profile runs `native-rust-clean` on ordinary
Ubuntu runners. Docker/OCI is opt-in for jobs that require container isolation.

Local fast CI keeps Rust tests inline by default. `just fast` and
`bash ci-fast-push.sh --no-push` use `JERYU_CI_RUST_TEST_MODE=inline` unless the
caller explicitly overrides it. Hosted `ops/ci/ci-fast.sh` selects
`JERYU_CI_RUST_TEST_MODE=sharded`, so the aggregate affected lane still runs
format, environment, drift, check, clippy, web, DB, audit, and proof steps while
recording the generic Rust test step as covered by the external shard matrix.
The `rust-test-shards` job in `.github/workflows/ci-fast.yml` fans out shards
`0..39`; each shard runs
`bash ops/ci/shard.sh "$JERYU_CI_SHARD_INDEX" "$JERYU_CI_SHARD_TOTAL"` with
`JERYU_CI_SHARD_TOTAL=40`, `JERYU_CI_SHARD_JOBS=2`,
`JERYU_RUNNER_EXECUTOR=native`, `JERYU_RUNNER_CLASS=native-rust-clean`, and
`JERYU_CI_DOCKER=0`. The shard driver also accepts
`bash ops/ci/shard.sh <index> <total>` locally for targeted reproduction and
fails closed if the runner is Docker-backed or not a native Rust runner class.

Primary lanes:
- `bash ci-fast-push.sh --no-push`: canonical local/hosted fast gate for branch
  and PR checks.
- `bash ci-fast-push.sh --full --no-push`: local proof of the full hosted-lane
  union from `agent/ci-lanes.toml`, including GitHub clean profile proof,
  security toolchain verification, retired-listener/process rejection, and all
  full workflow lanes.
- `npm --workspace @jeryu/web run test:e2e`: Playwright lane for critical web
  flows, including the rendered README and repository browsing paths.
- `npm --workspace @jeryu/web run ux-qa`: rendered UX QA lane for screenshots,
  accessibility checks, and the visual contract for the web surface.
- `bash ci-fast-push.sh`: local publish path after gates pass; it pushes the
  current branch and opens or reports a PR. Direct `HEAD:main` push requires
  explicit `--push-main` or `JERYU_CI_PUSH_MAIN=1`.
- `bash ops/ci/publish-readme-score.sh --verify`: local README publish helper
  that reads `target/jankurai/repo-score.{json,md}`, posts the managed score
  block through the local API, and writes
  `target/jankurai/readme-publish-receipt.json`. Use `--dry-run --verify` to
  validate the block render without mutating the worktree.
- `just fast`: deterministic fast lane for agent iteration.
- `just ci`: per-phase gate aggregator with explicit PASS, FAIL, and PENDING states.
- `just full`: workspace foundation gate with fmt, check, tests, clippy, zero-evidence, docs, release, score, and doctor checks.
- `just security`: cache adversary, poisoning matrix, zero-evidence, and secret scan.
- `just audit`: Jankurai audit plus dependency-audit integration when the tool is installed.
- `cargo test -p jeryu-signrail --test release_witness`,
  `cargo test -p jeryu-signrail --jobs 40 verify_release`, and
  `cargo clippy -p jeryu-signrail --all-targets -- -D warnings`: SignRail
  release signing, verification, provenance, witness, and stage-receipt proof
  lane.
- `cargo test -p jeryu-wsversion --jobs 40` plus
  `cargo run -q -p jeryu-wsversion -- inherit-guard`: workspace version source
  and changelog roll-forward proof lane. Add
  `cargo run -q -p jeryu-wsversion -- decide --range origin/main..HEAD --json`
  for release-candidate evidence.

## Workcells

- `cargo test -p jeryu-runnerd workcell --jobs 40`: workcell lifecycle, epoch fencing, tar safety, and frozen CI repair helper proof lane.
- `cargo test -p jeryu-readmodel --jobs 40 && cd web && npm run typecheck`: read-model dashboard and generated contract proof lane for the workcells snapshot.
- `cargo test -p jeryu-api --features web --jobs 40`: required when the bootstrap payload or web feature flags change, including the `workcells` flag.
- `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop`: the integrated R5 proof lane. It claims a live workcell, rebases it onto `origin/main`, runs a jailed edit inside the checkout, exports a namespaced branch with `changed_files`, opens the pull request, and verifies CI evidence for the resulting head sha.
- `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent`: route-level proof for `POST /api/v1/workcells/{id}/run_agent`. It claims a repo-root slice, proves an out-of-root program returns typed `workcell_run_path_denied`, then runs a staged in-root program and verifies structured stdout/stderr/finish events when the host sandbox is available.
- `cargo test -p jeryu-api --features web --jobs 40 agent_runs`: high-level proof for `POST /api/v1/agent-runs`. It launches from a held or repairing failed-CI workcell, validates epoch and path fencing, verifies live PTY events/control recording, proves cursor-safe `/events` resume reads, and proves unsupported/finished controls plus unfinished export attempts return typed repair bodies.
- `cargo test -p jeryu-agent-auth -p jeryu-agent-stream --jobs 40`: portable native CLI auth receipts and broker-compatible TTY/control stream contracts.
- `cargo test -p jeryu-mcp -p jeryu-cli --jobs 40`: `agent_work.start/status/control/events/export_pr` MCP catalog/memory fallback and `jeryu agent ...` CLI grammar/live URL dispatch.
- `cargo test -p jeryu-agent-auth -p jeryu-agent-stream -p jeryu-cli -p jeryu-mcp --jobs 40`: companion proof for portable native CLI auth receipts, broker-compatible TTY/control event schemas, `jeryu agent ...`, and MCP `agent_work.*` subscription/control tools.
- `cargo test -p jeryu-readmodel -p jeryu-tui --jobs 40`: read-model/TUI proof for agent runs, live TTY status, held failed-CI workcells, repair/export state, and codegraph/oracle evidence.
- `cargo test -p jeryu-sandbox-linux --jobs 40 pty` and `cargo test -p jeryu-agentbridge --test pty_driver --jobs 40`: PTY launch, TTY ioctl policy, process-group signaling, resize/control, and final-drain proof.
- `cargo run -p jeryu-sandbox-linux --example jail_demo`: the live folder-jail demo (Rung 1, see `docs/workcell.md`). Drives the production launch path against a throwaway checkout and exits non-zero unless write-inside is ALLOWED and write-outside, `/etc/shadow` read, and an `AF_INET` socket are each DENIED (or a host-absent primitive is honestly skipped). Run it on a fleet node where Landlock + seccomp are present.
- `cargo test -p jeryu-runnerd jailgun`: the jailgun tar round-trip (Rung 2). A clean subtree imports/exports while adversarial tar entries (parent traversal, absolute path, symlink, character device, a smuggled traversal, and an out-of-root export) are each rejected with `workcell_tar_path_denied`.
- `cargo test -p jeryu-agentbridge`: the in-cell agent driver (Rung 4, see `docs/workcell.md`). The `driver_in_cell` integration tests prove a jailed edit-bot writes only inside the cell, an out-of-cell write is DENIED by Landlock (honestly skipped if the host lacks Landlock), the watchdog kills a runaway, and an exceeded output/token budget kills the child.
- `cargo test -p jeryu-egress`: the allowlist egress proxy. Unit tests cover `egress_decision` (allowed host/suffix, non-allowlisted denial, budget-kill denies even allowlisted hosts, case-insensitive match, and the `crates.io.attacker.com` substring-attack denial); the integration tests prove a non-allowlisted CONNECT returns 403 before any upstream connect.
- `cargo test -p jeryu-sandbox-linux` (`cgroup_confinement`) and `cargo test -p jeryu-agentbridge` (`cgroup_fail_closed`): resource confinement, fail-closed. They prove a `require_cgroup` plan refuses to launch without a delegated cgroup-v2 subtree, that the `LandlockRule.execute` bit permits/denies exec correctly on ABI ≥ 2, and (honest-skip on hosts without cgroup-v2 delegation) that a runaway is contained under an enforced cgroup.

Workcell regression suite (added to keep the north-star guarantees from silently
regressing — each test asserts a discriminating signal, not a tautology):

- `cargo test -p jeryu-sandbox-linux secret_paths_denied`: the north-star promise, asserted negatively. A workspace-only Landlock jail must DENY reads of decoy `~/.ssh` key material, `~/.jeryu` CI secrets, and an UNCLAIMED sibling repo, while still permitting an in-workspace read. The decoys are world-readable (`chmod 0644`) so a denial is provably Landlock, not DAC; honest-skip without Landlock, and a denial MUST be `Blocked` (never a false skip) when Landlock is present.
- `cargo test -p jeryu-sandbox-linux memory_oom_kill`: proves a runaway allocator is OOM-killed by its cgroup `memory.max` (swap disabled to force OOM over swap). Complements the `pids.max` fork-bomb escape; honest-skip where no cgroup-v2 subtree is delegated.
- `cargo test -p jeryu-egress`: adds the plain-HTTP forward path (GET/POST to a non-allowlisted host → 403), empty-allowlist deny-all, an unparseable request line → 400, and unit coverage for CONNECT-target / absolute-URI authority winning over the `Host` header, userinfo stripping, and the fact that a non-numeric port falls back to the default (it is NOT a 400).
- `cargo test -p jeryu-runnerd workcell`: adds branch-budget exhaustion (through a held cell), two distinct claims with stale-epoch fencing, a stale-epoch release that is fenced WITHOUT transitioning the cell, and hardlink/fifo/socket tar entries rejected by kind.
- `cargo test -p jeryu-api --features web workcell_surface_tests`: the cell-surface REST + error-path lane for the previously-untested handlers — `list`/`status`/`claim`/`heartbeat` plus typed 404 (`not_found`), 409 epoch-fence (`workcell_epoch_fenced`), 422 malformed body (`workcell_invalid_request`), and 400 id-mismatch (`workcell_id_mismatch`).
- `cargo test -p jeryu-api --features web autonomy_bridge`: the record-only auto-merge 7-probe adversarial harness. Every probe proves the bridge never merges; probes 4/5/7 assert the R5-floor and red-CI hard stops (they fail if those guards are removed), while probes 1/2/3/6 document the known AllowMerge gaps (vacuous CI, synthetic quorum, no author gate) as tripwires that must flip red when the safety rework lands.
- `bash ops/ci/coverage.sh`: line + mutation coverage, now extended with a per-crate src-coverage ratchet (`ops/ci/coverage-baseline.json`) over `jeryu-api`, `jeryu-egress`, and `jeryu-codegraph`. Coverage may not drop below the recorded floor (minus a small jitter epsilon) and ratchets UP only — regenerate with `JERYU_COVERAGE_UPDATE_BASELINE=1 bash ops/ci/coverage.sh`. `jeryu-sandbox-linux` and `jeryu-agentbridge` are deliberately NOT ratcheted: their security tests honest-skip when a host primitive is absent, so a coverage percentage would be host-dependent; they are protected by their own escape/refute suites instead. (Namespace classification and the output-budget/timeout kill paths are already covered by `capability.rs` and `driver_in_cell.rs` unit tests, so they are not duplicated here.)

## Codegraph Oracle

- `cargo test -p jeryu-codegraph --jobs 40`: schema-v3 storage, symbol
  references, impact, reverse dependencies, and oracle pack construction.
- `cargo test -p jeryu-mcp --test mcp_conformance --jobs 40`: MCP catalog
  proof for `code.symbols.search`, `code.definition`, `code.impact`,
  `code.crate.reverse_deps`, `code.references`, and `codegraph.query`.
- `cargo test -p jeryu-api --features web --jobs 40 codegraph`: REST facade
  proof for `POST /api/v1/repos/{id}/codegraph/query` and typed repair
  guidance.
- `bash ops/ci/codegraph-oracle.sh`: the composed schema-v3 API/MCP contract
  lane.

## Codegraph Tool-Build Insights

- `cargo test -p jeryu-codegraph --jobs 40 tool_build`: fast normalized-window
  cluster scan, persistence, and ignore feedback.
- `cargo test -p jeryu-mcp --test mcp_conformance --jobs 40`: MCP catalog
  proof for `codegraph.tool_build.status`, `codegraph.tool_build.clusters`, and
  `codegraph.tool_build.feedback`.
- `cargo test -p jeryu-api --features web --jobs 40 tool_build`: REST facade
  proof for status, ranked clusters, and typed feedback repair bodies.
- `bash ops/ci/codegraph-tool-build.sh`: composed scanner/MCP/API/CLI smoke
  lane. It writes `target/jankurai/codegraph-tool-build-{scan,clusters}.json`.

## JMCP Control Plane

- `cargo test -p jeryu-api --features web --jobs 40 control_plane`: REST and
  pure aggregation proof for `/api/v1/control-plane/status`, priorities,
  repo-graph clusters, artifact absence states, local runner capacity,
  read-only mirror degradation, camelCase contracts, and
  `/api/v1/agent-runs` listing.
- `cargo test -p jeryu-mcp --jobs 40`: MCP catalog and memory fallback proof
  for read-only control-plane catalog entries.
  Catalog changes are reviewed against `agent/tool-adoption.toml` and the
  pinned `ops/ci/security-tools.sh` transcript; untrusted tool output remains
  evidence only and never becomes trusted policy input.
- `cargo test -p jeryu-cli --jobs 40`: CLI grammar and fail-closed live API URL
  dispatch for status, priorities, repo-graph, artifact lookup, and runner
  status subcommands.
- `npm --workspace @jeryu/web run typecheck`,
  `npm --workspace @jeryu/web run test`, and
  `npm --workspace @jeryu/web run build && JERYU_PLAYWRIGHT_API_URL=http://127.0.0.1:8790 npm --workspace @jeryu/web run test:e2e -- 12-intelligence.spec.ts`:
  web contract, selector/render, and critical route smoke for `/intelligence`
  without reusing a stale local BFF on the default port.

PENDING is only allowed for a capability that is not built yet and must be
printed as PENDING, not PASS. The current phase gates report PASS=10,
PENDING=0, FAIL=0; if a future live capability is missing, mark only that gate
PENDING with evidence.

CI parity checks:
- `ops/ci/verify-jeryu-env.sh --build-local` builds the repo-local `jeryu`
  binary, accepts the canonical GitHub remote or the loopback local Jeryu
  remote on `127.0.0.1:8787`, and ensures CI does not select the retired
  `~/.jeryu/bin/jeryu` binary.
- `ops/ci/verify-jeryu-env.sh --build-local --release-guard` is wired into
  full release validation and fails while retired-provider runners, stale
  `~/.jeryu` binaries, old `/home/ubuntu/jeryu`, local `:2224`, or other
  monitored listeners are still active. A local `~/.jeryu/bin/jeryu-api`
  process is accepted only when it matches the repo-built API binary.
  Additional source-root retired-CI sweeps run only when
  `JERYU_CI_SOURCE_ROOTS` is set.
- `ops/ci/ensure-jankurai.sh` is the single local/hosted bootstrap for pinned
  Jankurai 1.6.10.
- `agent/ci-lanes.toml` is the committed CI lane manifest. `cargo run -q -p
  jeryu-repogate -- ci-lanes-check` fails if a workflow adds hosted-only `run:`
  commands or stops calling the manifest-declared local lane.
- Hosted `ci-fast` fetches `origin/main` and runs `ci-fast-push.sh --no-push`
  so affected planning, Jankurai diff audit, and local push behavior match.
- Hosted security installs pinned open-source tools through
  `ops/ci/security-tools.sh` and then runs `ops/ci/security.sh`; local full mode
  uses the same two scripts before claiming security parity.
- The SBOM lane always writes a cosign transcript. Keyless signing is opt-in via
  `JERYU_COSIGN_KEYLESS=1`; default local CI records signing instructions so it
  cannot hang waiting for an OIDC/browser flow.

Repair evidence:
- Every failed lane must print the exact rerun command and the local artifact path when one exists.
- Common fixes are routed through `agent/test-map.json`; use the narrowest lane for the changed path before running `just full`.
- Typed repair surfaces must name `purpose`, `reason`, common fixes, `docs_url`,
  and `repair_hint` so the next rerun is local and agent-readable.
- Structured repair receipts should point at the lane transcript, the local
  artifact path, and the owning doc or proof lane for the rerun. For release
  and provenance failures, link back to `docs/release.md` and
  `docs/release-process.md` so the commit, rollback target, and gate evidence
  stay explicit.
- Observability-related failures should use the same `AgentRepairHint` contract
  documented in [docs/errors.md#missing-receipt](errors.md#missing-receipt).
  For Jankurai, make the failure payload name the lane, the exact rerun
  command, the local artifact path, and the dashboard owner before the next
  audit starts:
  ```rust
  AgentRepairHint {
      purpose: "repair observability lane evidence",
      reason: "jankurai-audit failed and the repo-score artifact is stale or missing",
      common_fixes: [
          "rerun `JERYU_JANKURAI_FULL=1 bash ops/ci/jankurai.sh`",
          "rerun `jankurai diff-audit --base-ref origin/main .` for a path-scoped repair",
          "inspect `target/jankurai/raw-repo-score.{json,md}` and `.jankurai/repo-score.{json,md}`",
      ],
      docs_url: "errors.md#missing-receipt",
      repair_hint: "rerun the pinned wrapper, then compare the emitted score against `.jankurai/repo-score.md`",
  }
  ```
- SignRail artifact-support failures also link
  `docs/signrail-release-signing.md` and preserve generated
  `target/artifact-support/signrail` receipt paths.
- Public read-only API additions, including `/api/v1/ecosystem` and
  `/api/v1/ci/runs/{id}/evidence`, require route tests that prove live data
  sourcing, camelCase response contracts, digest-verifiable payloads, and typed
  404 repair guidance. Rerun
  `cargo test -p jeryu-api --features web --jobs 40` plus the matching clippy
  lane before release evidence is recorded.
- CI evidence digest changes must preserve canonicalization tests and fail
  visibly on impossible serialization errors; release proof includes the route
  test, clippy transcript, and Jankurai score artifact.
- Workcell export changes must prove changed-file evidence is derived from the
  frozen git diff, not caller input, before a PR is created. Rerun
  `cargo test -p jeryu-api --features web --jobs 40 workcell_export_slice` and
  attach the typed `workcell_export_slice_denied` no-PR evidence for restrictive
  leases.
- Workcell run-agent route changes must prove stale/out-of-root requests remain
  typed repair failures and successful runs emit structured events. Rerun
  `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent` and
  attach either the stdout/stderr/finished event evidence or the honest
  `workcell_run_sandbox_unavailable` skip evidence for hosts without the
  required sandbox.
- Agent-run route changes must prove stale epochs, state denials, path denials,
  unsupported controls, and finished-run controls remain typed repair failures.
  Rerun `cargo test -p jeryu-api --features web --jobs 40 agent_runs` and attach
  live PTY event/control evidence or the sandbox-unavailable repair evidence.
- README publish failures should rerun
  `bash ops/ci/publish-readme-score.sh --verify` after regenerating
  `target/jankurai/repo-score.json` and `target/jankurai/repo-score.md` from
  `bash ops/ci/proof-evidence.sh`.
- Repair hint: if a Jankurai finding names a path, first run `jankurai diff-audit --base-ref origin/main .`, then the mapped proof command for that path.
- Unsupported GitHub-compatible REST or GraphQL requests must return a
  `jeryu_repair_hint` with route/tool alternatives and a local rerun command;
  widen the subset only with `jeryu-api` conformance tests.

Budget and stop conditions:
- Default local CI uses 40 workers and should finish quickly on this workspace; if a lane exceeds 20 minutes, stop and split it into a narrower proof lane.
- Do not keep retrying a flaky or missing live-capability gate. Mark it PENDING with evidence until the runtime exists.
- Paid or networked tools must be opt-in and must have an explicit environment variable gate plus a documented stop condition.
- Networked or paid agent/tool execution is disabled unless
  `JERYU_ALLOW_NETWORK_TOOLS=1` or a narrower lane-specific opt-in is present.
- Any paid tool lane must publish a budget receipt naming the request budget,
  consumed units, remaining quota, and operator who opted in. Missing budget
  receipt is a failed lane, not a warning.
- Stop a paid or unbounded lane when it reaches 80 percent of the declared
  budget, when no progress artifact changes for two consecutive attempts, or
  when the same failure repeats twice.
- Kill switch: unset the opt-in variable and create
  `target/jeryu-ci/STOP_NETWORK_TOOLS` to make networked local CI lanes
  fail closed before launching work.

Launch-gate evidence:
- Release candidates require artifact-backed evidence for security, backups, monitoring, rollback, and abuse controls before signing.
- Full launch gate evidence includes security scan receipts, backup receipts, monitoring receipts, rollback receipts, abuse controls receipts, and CI or script evidence from `just ci`, `just security`, and `just release`.
- Security: `just security` must pass and record secret-scan, dependency-scan,
  zero-evidence, and cache-poisoning results before a release candidate is
  signed.
- Backups: release candidates must include a restore receipt or dry-run restore
  log for repository metadata, artifacts, and service state.
- Monitoring: operators must attach the metrics/log receipt for the release
  candidate and the rollback alert route before rollout.
- Rollback: `docs/release.md` is the rollback control surface; each release
  receipt must name the previous signed artifact and checksum.
- Abuse controls: agent, runner, and token-scope gates must pass before any
  hosted or remote deployment path is enabled.
