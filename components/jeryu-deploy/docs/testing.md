# Testing

Run `bash scripts/ci-local.sh` without arguments for the existing
`just fast` then `just check` loop. Exactly
`bash scripts/ci-local.sh required` runs `ops/ci/pr-ci.sh` from this
component root and preserves its exit status. Unknown or extra arguments fail
before any lane runs. Required dispatch does not itself qualify every retained
proof; the full wrapper and separate capability lanes must actually pass.

The full PR wrapper also runs `ops/ci/check.sh`, preserving the
metadata, agent-map, shell syntax, phase-dispatch and coverage-evidence
regressions formerly reachable only through the quick recipes. Its fast recipe
already delegates to that same check, so no unique quick assertion is dropped.

The PR lock guard asks Cargo for the API member's workspace manifest and binds
it to the physical Git root. A standalone export keeps its own root lock;
the exact `components/jeryu-deploy` monorepo placement uses the shared root
lock. Missing files, failed discovery, foreign roots and linked inputs fail.
The guard compares source-root identity, member/workspace manifests and lock
identity/content at both existing recheck points. It never restores or discards
changed inputs. `bash scripts/test-workspace-lock.sh` exercises synthetic
standalone and monorepo layouts without compiling or fetching dependencies.

The web gate and generated Deploy split CI share `ops/ci/web-build.sh`.
In the monorepo it builds the checked, clean root with `npm ci` and
`npm run build`. A generated split fetches the exact public monorepo commit
from `.jeryu-source.json` and verifies its original Deploy component tree
before building the Web source there. Exported Deploy Cargo commands still
compile their own package and clear `JERYU_TEST_BINARY`; a root or installed
binary cannot substitute for that independent process proof.

The helper inspects the existing physical dist before npm/Vite can clear it,
rejects linked/nonregular bundle entries, and validates the complete resulting asset
set and local index references. Source HEAD/tree, tracked bytes and custody,
provenance and the bundle are checked again after Cargo. Failed or changed
scratch remains available for diagnosis; success cleanup checks its identity,
mounts and internal-only links. Caller `JERYU_WEB_DIST` is not proof authority.
Historical standalone trees without split provenance may use only an unchanged
Git-committed vendored bundle; the sibling `stage-web-dist.sh` recipe does not
establish CI provenance. Generated public exports contain no vendored bundle.

`bash scripts/test-web-build.sh` exercises synthetic layout, provenance,
asset, failure-status and cleanup cases without a product build or runtime
claim. Real web qualification retains the API browser/JSON routing unit test
and runs all three existing CLI standalone process tests, including embedded
assets/notices, auth, Git operations and restart persistence. The routing unit
uses a temporary page; it is not itself production-bundle evidence.

Repository scripts define the reproducible commands. Local runs are developer
evidence; the protected hosted `jeryu-deploy/required` result at the exact head
is merge authority. Neither side may replace a failed command with a silent
green.

Default compiler worker count is 40 and aggregate test-process concurrency is
8. CI scripts source `ops/ci/common.sh` or `ops/ci/ci-env.sh`, which set
`JERYU_CI_JOBS=40`, `CARGO_BUILD_JOBS=40`, and `JERYU_CI_TEST_THREADS=8`
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
- `bash ops/ci/web.sh`: builds or admits the exact-source/committed bundle
  described above, retains the API route regression and runs the three real
  CLI process tests. Web typechecking, Playwright and rendered UX proof remain
  required in their owning Web and root lanes.
- `bash ci-fast-push.sh`: local publish path after gates pass; it pushes the
  current branch and opens or reports a PR. Direct `HEAD:main` push requires
  explicit `--push-main` or `JERYU_CI_PUSH_MAIN=1`.
- `bash ops/ci/publish-readme-score.sh --verify`: local README publish helper
  that reads `target/jankurai/repo-score.{json,md}`, posts the managed score
  block through the local API, and writes
  `target/jankurai/readme-publish-receipt.json`. Use `--dry-run --verify` to
  validate the block render without mutating the worktree.
- `just fast`: deterministic fast lane for agent iteration.
- `just agent-runs`: narrow REST/MCP/session/TTY regression loop for changes
  under `crates/jeryu-api/src/web/agent_runs/`.
- `just request-id`: focused hostile-input and correlation-header checks for
  the shared HTTP/MCP request-id middleware.
- `just ci`: per-phase gate aggregator with explicit PASS, FAIL, and PENDING states.
- `just full`: workspace foundation gate with fmt, check, tests, Clippy,
  repository proof evidence, workflow parity, release-receipt contract, score,
  security, and doctor checks.
- `just security`: Deploy-owned secret, workflow, environment-file, and locked
  Cargo metadata checks through the canonical `tools/security-lane.sh`.
- `just security-network`: the same lane with Cargo audit/deny, hosted Git
  dependency-source verification, and npm critical-advisory checks enabled.
  Comprehensive and pull-request validation require this network profile;
  cache implementation gates remain in their owning repository.
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
- `cargo test --locked -p jeryu-api --features web --lib reviewed_main_`:
  real Git fixtures reject direct main pushes, require an exact-head review and
  check, and verify that the protected merge plus repeated push callback keeps
  the reviewed head, tree, version, changelog and existing tag. Both ordinary
  features and explicitly prepared version changes are covered without a
  `[skip-version]` marker. Fixture reviews and checks stay in memory; this test
  does not publish release evidence or replace the sealed required lane.

This checkout has source for only `jeryu-api`, `jeryu-cli`, and
`jeryu-split-tool`. Commands below that name another package exercise a pinned
dependency only when Cargo can resolve it; its authoritative source tests and
changes belong in that component's standalone repository.

## Workcells

- `cargo test -p jeryu-runnerd workcell --jobs 40`: workcell lifecycle, epoch fencing, tar safety, and frozen CI repair helper proof lane.
- `cargo test -p jeryu-readmodel --jobs 40 workcells` plus `bash ops/ci/web.sh`:
  pinned read-model projection and Deploy's immutable web-bundle integration.
- `cargo test -p jeryu-api --features web --jobs 40`: required when the bootstrap payload or web feature flags change, including the `workcells` flag.
- `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop`: the integrated R5 proof lane. It claims a live workcell, rebases it onto `origin/main`, runs a jailed edit inside the checkout, exports a namespaced branch with `changed_files`, opens the pull request, and verifies CI evidence for the resulting head sha.
- `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent`: route-level proof for `POST /api/v1/workcells/{id}/run_agent`. It claims a repo-root slice, proves an out-of-root program returns typed `workcell_run_path_denied`, then runs a staged in-root program and verifies structured stdout/stderr/finish events when the host sandbox is available.
- `cargo test -p jeryu-api --features web --jobs 40 agent_runs`: high-level proof for `POST /api/v1/agent-runs`. It launches from a held or repairing failed-CI workcell, validates epoch and path fencing, verifies live PTY events/control recording, proves cursor-safe `/events` resume reads, and proves unsupported/finished controls plus unfinished export attempts return typed repair bodies.
- `cargo test -p jeryu-api --features web --lib web::sessions::tests -- --test-threads=4`: session-route concurrency proof. Routine hermetic fixtures suppress the automatic two-hour companion shell; a dedicated proof enables it and terminates it through the public control surface so the suite cannot leak live PTYs.
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
- `bash ops/ci/coverage.sh`: line coverage over the three owned crates, an API
  source ratchet in `ops/ci/coverage-baseline.tsv`, mutation testing scoped to
  `jeryu-split-tool`, and the Jankurai coverage audit. The API floor was
  rebound once to `0.8044` because exact protected main measured `0.7981` and
  this candidate measured `0.8044`; the pre-split `0.8411` value was not
  reproducible here. Future baseline updates are upward-only.
- `bash ops/ci/proof-evidence.sh`: the single fail-closed Jankurai evidence
  implementation. It runs Deploy's actual source-security wrapper, verifies a
  protected-main provenance-bound ratchet baseline, and emits changed-surface,
  proofmark, copy-code, and Rust witness artifacts. The underscore and
  `jankurai.sh` spellings only delegate to it. It does not invent UX,
  migration, vibe, or coverage artifacts; the immutable bundle and dedicated
  coverage lane own the applicable proofs. Its protected-base proof plan binds
  additions, modifications, renames, type changes, and deletions. Installed
  Jankurai 1.6.11 proofbind receives only the exact extant subset because it
  cannot classify deleted bytes; both generated path sets are checked against
  independently sorted Git inventories, and a delete-only proofbind scope
  fails closed.

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
- `bash ops/ci/web.sh`: immutable bundle and API-serving integration for the
  staged `/intelligence` surface. Typecheck, unit, build, and Playwright source
  gates belong to `jeryu-web` and must be green before its bundle is staged.

PENDING is only allowed for a capability that is not built yet and must be
printed as PENDING, not PASS. A fully green phase run reports `PASS=10`,
`PENDING=0`, `FAIL=0`; if a future live capability is missing, mark only that
gate PENDING with evidence.

CI parity checks:

Jankurai identity failures are diagnosed before any score is trusted. Run
`bash ops/ci/test-governed-jankurai.sh`; a missing/non-physical file, symlink,
wrong version, digest mismatch, or missing/mismatched installation receipt is a
hard lane failure. A hostile initial PATH is not described as rejected: the
test proves it initially resolves the substitute and that governed prepending
then neutralizes it before execution. The embedded API bridge also rejects
hardlinks and incomplete or wrong-authority receipts. The verifier prints the
rejected path and observed identity. Audit evidence belongs in `.jankurai/` and
`target/jankurai/`; never repair an identity failure by editing a score or
relabeling `agent/baselines/historical/` evidence.

For a governed identity rotation, run the external projection invariant before
the full lanes:

```bash
cargo test --locked --offline -p jeryu-api --features web --test jankurai_governance
bash ops/ci/test-governed-jankurai.sh
just score
jankurai diff-audit --base-ref origin/main .
```

The integration test closes the tag/revision/tree/archive/binary projection,
the image receipt and manifest authority, retired-identity absence, and
release-broker custody behavior. It also proves the committed release graph
contains no sibling path dependency, every internal package is lock-bound to
an immutable Git source, and Core/proof plus Rustjet each resolve exactly once
at reviewed split.3/split.1. `agent/test-map.json` owns the rerun route.
The monorepo-only `jeryu-mapcheck docs` marker check is not a standalone
Deploy proof lane. The protected hosted `jeryu-deploy/required` context is
merge authority; local executions are developer evidence and may not replace
that hosted result.

- `ops/ci/verify-jeryu-env.sh --build-local` builds the repo-local `jeryu`
  binary, requires the exact `git.neverhuman.org` fetch origin with no alternate
  push destination, and ensures CI does not select the installed predecessor
  `~/.jeryu/bin/jeryu` binary.
- `ops/ci/verify-jeryu-env.sh --build-local --release-guard` is wired into
  full release validation and fails while retired-provider runners, old
  `/home/ubuntu/jeryu` source roots, local `:2224`, or other retired
  experimental listeners are still active. The accepted predecessor runtime
  may remain live during candidate validation; every test command selects the
  freshly built repository binary, and the dependency-source gate separately
  proves that Cargo contacts only the hosted Git authority.
  Additional source-root retired-CI sweeps run only when
  `JERYU_CI_SOURCE_ROOTS` is set.
- `ops/ci/ensure-jankurai.sh` is the single local/hosted bootstrap for pinned
  governed Jankurai 1.6.11. It is a verifier, not an installer.
- `agent/ci-lanes.toml` is the committed CI lane manifest. `cargo run --locked
  -q -p jeryu-split-tool --bin jeryu-split -- ci-lanes-check` fails if a
  workflow adds hosted-only `run:` commands or stops calling the
  manifest-declared local lane.
- Hosted `ci-fast` fetches `origin/main` and runs `ci-fast-push.sh --no-push`
  so affected planning, Jankurai diff audit, and local push behavior match.
- Hosted security installs pinned open-source tools through
  `ops/ci/security-tools.sh` and then runs `ops/ci/security.sh`; local full mode
  uses the same two scripts before claiming security parity.
- `bash ops/ci/dependency-sources.sh` keeps Cargo Deny at `unknown-git=deny`,
  validates the exact immutable Git-source allowlist, and proves historical
  source spellings resolve through the active validated Git config only to
  `git.neverhuman.org`. Release CI may supply its own sealed global Git config;
  the repository overlay never overrides it, and the gate binds the active
  config hash in its receipt. The default overlay contains only exact mappings
  plus the dedicated hosted credential helper and the host-scoped smart-HTTP
  compatibility setting; it includes no ambient user or system config. All CI roots source
  `ops/ci/hosted-git-env.sh` before Cargo because Cargo does not apply its own
  `[env]` table to the Git child that performs dependency fetches. The same
  gate checks `.cargo/hosted-pin-refs.tsv` against the lock and the live hosted
  immutable tag plus preservation ref, preventing a warm cache from hiding an
  exact-SHA fetch that the hosted backend can no longer serve.
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
- Security: `just security` must pass and record pinned gitleaks, actionlint,
  committed-environment-file denial, and locked Cargo metadata. `just audit`
  owns dependency advisories/licenses, and the component/release lanes own
  cache-poisoning, SBOM, vulnerability, provenance, and signing evidence before
  a release candidate is signed.
- Backups: release candidates must include a restore receipt or dry-run restore
  log for repository metadata, artifacts, and service state.
- Monitoring: operators must attach the metrics/log receipt for the release
  candidate and the rollback alert route before rollout.
- Rollback: `docs/release.md` is the rollback control surface; each release
  receipt must name the previous signed artifact and checksum.
- Abuse controls: agent, runner, and token-scope gates must pass before any
  hosted or remote deployment path is enabled.
