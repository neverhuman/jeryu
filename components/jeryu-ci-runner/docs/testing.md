# Testing

Run `bash scripts/ci-local.sh` without arguments for the existing
`just fast` then `just check` loop. Exactly
`bash scripts/ci-local.sh required` runs `ops/ci/pr-ci.sh` from this
component root and preserves its exit status. Unknown or extra arguments fail
before any lane runs. Required dispatch does not itself qualify every retained
proof; the full wrapper and separate capability lanes must actually pass.

Use the local CI entrypoints before pushing changes:

- `just fast`
- `just check`
- `just score`
- `just security`
- `just contract-drift`
- `just artifact-support`

`ops/ci/pr-ci.sh` is the canonical source gate for the protected hosted
`jeryu-ci-runner/required` context. `scripts/ci-doctor.sh` checks the
required local tools.
The compatibility workflow must never be treated as a substitute for a genuine
hosted runner result.

The security lane runs networked dependency checks by default through
`just security`. It preserves Cargo's immutable source identity while
`ops/ci/dependency-sources.sh` proves the active Git transport policy, hosted
tag/support-ref binding, Cargo Deny allowlist, and `git.neverhuman.org`
destination. Its evidence includes the exact lock hash and CycloneDX SBOM hash.

The contract lane compares `schemas/jeryu.runner.v1.schema.json` directly with
the authoritative Rust wire sources and includes hostile drift cases. The
schema is a structural interoperability aid; runtime acceptance still requires
the Rust validator.

Agent-readable exception guidance:

- purpose: every typed error documents the caller-facing failure purpose
- reason: failures preserve enough context for local diagnosis
- common fixes: map repeated failures to a small set of operator repairs
- docs_url: point users to this file or a narrower runbook
- repair_hint: state the next command or config change to try

Cost and bounded-operation policy: budget, quota, spend cap, kill switch, and
stop condition evidence must be added before introducing paid or unbounded
network operations.
