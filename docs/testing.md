# Testing

Local CI is the source of truth. Hosted CI mirrors these commands, but it must
not replace them or make a local gate silently green.

Default worker count is 40. CI scripts source `ops/ci/common.sh` or
`ops/ci/ci-env.sh`, which set `JERYU_CI_JOBS=40` and `CARGO_BUILD_JOBS=40`
unless the caller explicitly overrides them. Local Jeryu runners default to
`native-rust-hot`; GitHub-hosted fallback runs `native-rust-clean` on ordinary
Ubuntu runners. Docker/OCI is opt-in for jobs that require container isolation.

Primary lanes:
- `bash ci-fast-push.sh --no-push`: canonical local/hosted fast gate for pushes
  and PR checks.
- `just fast`: deterministic fast lane for agent iteration.
- `just ci`: per-phase gate aggregator with explicit PASS, FAIL, and PENDING states.
- `just full`: workspace foundation gate with fmt, check, tests, clippy, zero-evidence, docs, release, score, and doctor checks.
- `just security`: cache adversary, poisoning matrix, zero-evidence, and secret scan.
- `just audit`: Jankurai audit plus dependency-audit integration when the tool is installed.

PENDING is only allowed for a capability that is not built yet and must be
printed as PENDING, not PASS. The current phase gates report PASS=7,
PENDING=0, FAIL=0; if a future live capability is missing, mark only that gate
PENDING with evidence.

CI parity checks:
- `ops/ci/verify-jeryu-env.sh --build-local` builds the repo-local `jeryu`
  binary, rejects noncanonical remotes, and ensures CI does not select the
  legacy `~/.jeryu/bin/jeryu` binary.
- `ops/ci/ensure-jankurai.sh` is the single local/hosted bootstrap for pinned
  Jankurai 1.6.10.
- Hosted `ci-fast` fetches `origin/main` and runs `ci-fast-push.sh --no-push`
  so affected planning, Jankurai diff audit, and local push behavior match.

Repair evidence:
- Every failed lane must print the exact rerun command and the local artifact path when one exists.
- Common fixes are routed through `agent/test-map.json`; use the narrowest lane for the changed path before running `just full`.
- Typed repair surfaces must name `purpose`, `reason`, common fixes, `docs_url`,
  and `repair_hint` so the next rerun is local and agent-readable.
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
