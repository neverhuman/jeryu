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
- `just artifact-support`

`scripts/ci-doctor.sh` checks the required local tools.

`just check` always runs all Codegraph test targets, including snapshot storage,
query behavior and drift of the generated symbol-row contract. Regenerate from
this component root with
`cargo run --locked -p jeryu-codegraph --example symbol-schema > contracts/codegraph.schema.json`.
The Rust serialization uses `crate_name`, accepts empty strings and unknown input
fields, and bounds `line` to an unsigned 32-bit integer. The schema preserves
those existing semantics. Storage helper/type extraction changes source routing
without changing the public reexports or database schema.

Agent-readable exception guidance:

- purpose: every typed error documents the caller-facing failure purpose
- reason: failures preserve enough context for local diagnosis
- common fixes: map repeated failures to a small set of operator repairs
- docs_url: point users to this file or a narrower runbook
- repair_hint: state the next command or config change to try

Cost and bounded-operation policy: budget, quota, spend cap, kill switch, and
stop condition evidence must be added before introducing paid or unbounded
network operations.
