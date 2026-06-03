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
- `cargo test -p jeryu-signrail --test release_witness` and
  `cargo clippy -p jeryu-signrail --all-targets -- -D warnings`: SignRail
  release signing, provenance, witness, and stage-receipt proof lane.

## Workcells

- `cargo test -p jeryu-runnerd workcell --jobs 40`: workcell lifecycle, epoch fencing, tar safety, and frozen CI repair helper proof lane.
- `cargo test -p jeryu-readmodel --jobs 40 && cd web && npm run typecheck`: read-model dashboard and generated contract proof lane for the workcells snapshot.
- `cargo test -p jeryu-api --features web --jobs 40`: required when the bootstrap payload or web feature flags change, including the `workcells` flag.

## Agent Egress

- `cargo test -p jeryu-agentbridge -p jeryu-egress --jobs 40`: in-cell agent substrate lane covering deterministic edit-bot staging, the adversarial parallel staging test, and the live-agent egress contract.
- `bash ops/ci/gates/agent-substrate.sh`: direct phase gate for the same lane; `bash scripts/ci-phases.sh` discovers it automatically.

Deterministic edit-bot tests use `NetworkPolicy::Deny` and `SecretPolicy::None`.
The live agent path is opt-in only: callers must use `jeryu-egress` to request
`egress-only`, provide host allowlist rules, name secret environment variables
without values or explicitly choose no secrets, and attach a budget receipt that
stops before the configured threshold.

PENDING is only allowed for a capability that is not built yet and must be
printed as PENDING, not PASS. The current phase gates report PASS=10,
PENDING=0, FAIL=0; if a future live capability is missing, mark only that gate
PENDING with evidence.

CI parity checks:
- `ops/ci/verify-jeryu-env.sh --build-local` builds the repo-local `jeryu`
  binary, rejects noncanonical remotes, and ensures CI does not select the
  retired `~/.jeryu/bin/jeryu` binary.
- `ops/ci/verify-jeryu-env.sh --build-local --release-guard` is wired into
  full release validation and fails while retired-provider runners, `~/.jeryu`,
  old `/home/ubuntu/jeryu`, local `:2224`, or other monitored listeners are
  still active.
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
- SignRail artifact-support failures also link
  `docs/signrail-release-signing.md` and preserve generated
  `target/artifact-support/signrail` receipt paths.
- Public read-only API additions, including `/api/v1/ecosystem` and
  `/api/v1/ci/runs/{id}/evidence`, require route tests that prove live data
  sourcing, camelCase response contracts, digest-verifiable payloads, and typed
  404 repair guidance. Rerun
  `cargo test -p jeryu-api --features web --jobs 40` plus the matching clippy
  lane before release evidence is recorded.
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
