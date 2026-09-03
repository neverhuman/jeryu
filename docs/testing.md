# Testing

Use the local CI entrypoints before pushing changes:

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`

`scripts/ci-local.sh` executes the canonical `ops/ci/pr-ci.sh` gate used for
the hosted `jeryu/required` check. `scripts/ci-doctor.sh` checks the governed
Jankurai identity. `just check` also runs hostile tests that prove family
cloning rejects malformed hosted slugs, wrong origins, dirty or symlinked
checkouts, and that required security scanners cannot fail or disappear while
the lane reports green.

Security repair evidence is written as `jeryu.split.security/v2` JSON to both
`target/jankurai/security/evidence.json` and `target/security/evidence.json`.
Each check records `name`, `status`, `policy`, and a bounded `detail`; the
top-level `conclusion` is `failure` whenever any check fails. After repairing a
failure, rerun `bash tests/security-lane-hostiles.sh`, then `just security`, and
compare the exact evidence file before running the canonical PR gate.

Agent-readable exception guidance:

- purpose: every typed error documents the caller-facing failure purpose
- reason: failures preserve enough context for local diagnosis
- common fixes: map repeated failures to a small set of operator repairs
- docs_url: point users to this file or a narrower runbook
- repair_hint: state the next command or config change to try

Cost and bounded-operation policy: budget, quota, spend cap, kill switch, and
stop condition evidence must be added before introducing paid or unbounded
network operations.
