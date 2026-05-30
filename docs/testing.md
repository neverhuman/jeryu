# Testing

Local CI is the source of truth. Hosted CI may mirror these commands later, but it must not replace them or make a local gate silently green.

Default worker count is 40. CI scripts source `ops/ci/common.sh`, which sets `JERYU_CI_JOBS=40` and `CARGO_BUILD_JOBS=40` unless the caller explicitly overrides them.

Primary lanes:
- `just fast`: deterministic fast lane for agent iteration.
- `just ci`: per-phase gate aggregator with explicit PASS, FAIL, and PENDING states.
- `just full`: workspace foundation gate with fmt, check, tests, clippy, zero-evidence, docs, release, score, and doctor checks.
- `just security`: cache adversary, poisoning matrix, zero-evidence, and secret scan.
- `just audit`: Jankurai audit plus dependency-audit integration when the tool is installed.

PENDING is only allowed for a capability that is not built yet and must be printed as PENDING, not PASS. The current example is the live runner sandbox escape matrix until native seccomp, Landlock, and cgroup enforcement is wired.

Repair evidence:
- Every failed lane must print the exact rerun command and the local artifact path when one exists.
- Common fixes are routed through `agent/test-map.json`; use the narrowest lane for the changed path before running `just full`.
- Repair hint: if a Jankurai finding names a path, first run `jankurai diff-audit --base-ref origin/main .`, then the mapped proof command for that path.

Budget and stop conditions:
- Default local CI uses 40 workers and should finish quickly on this workspace; if a lane exceeds 20 minutes, stop and split it into a narrower proof lane.
- Do not keep retrying a flaky or missing live-capability gate. Mark it PENDING with evidence until the runtime exists.
- Paid or networked tools must be opt-in and must have an explicit environment variable gate plus a documented stop condition.

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
