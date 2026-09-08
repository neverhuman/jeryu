# Testing

Use the local CI entrypoints before pushing changes:

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`
- `just redline-consumer-test`

A Redline dependency revision additionally requires `just release-readiness`
and `ops/ci/pr-ci.sh`. After merge, the evidence producer rechecks the clean
forge `main`, family-receipt checksum and manifest identity, locked engine
commit, transactional durability, checkpoint, and reopen before it can emit a
consumer receipt.

`scripts/ci-local.sh` delegates to the same `ops/ci/*.sh` lanes used by the
GitHub workflow. `scripts/ci-doctor.sh` checks the required local tools.

Agent-readable exception guidance:

- purpose: every typed error documents the caller-facing failure purpose
- reason: failures preserve enough context for local diagnosis
- common fixes: map repeated failures to a small set of operator repairs
- docs_url: point users to this file or a narrower runbook
- repair_hint: state the next command or config change to try

Cost and bounded-operation policy: budget, quota, spend cap, kill switch, and
stop condition evidence must be added before introducing paid or unbounded
network operations.
