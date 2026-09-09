# Testing

Use the local CI entrypoints before pushing changes:

- `rtk just fast`
- `rtk just check`
- `rtk just score`
- `rtk just security`
- `rtk just artifact-support`
- `rtk just contract-drift`

`scripts/ci-local.sh` requires exactly one lane argument. `required`,
`security`, `score`, `contract-drift`, and `artifact-support` delegate to the
same canonical scripts used to produce hosted proof. The complete local gate is
`rtk bash scripts/ci-local.sh required`.

`agent/proof-lanes.toml` uses the Jankurai 1.6.11 ordered `[[lane]]` schema.
Every command referenced by `agent/test-map.json` is signed by an exact lane;
unsigned commands, routing risks, and manual-approval requirements fail proof.

`contract-drift` binds the immutable split.1 baseline, exact head and tree,
build/policy/configuration inputs, and the exact unique four-library public API
set. It directly invokes the physical governed `cargo-public-api 0.52.0`
binary through a held descriptor, with an absolute canonical path and SHA-256.
Local proof uses the root-owned mode-`0555` native-tool bundle; release proof
requires its root-owned mode-`0555` read-only content-addressed mount. Empty or
unstructured tool reports, removed or changed API, moving/dirty source, and any
unreviewed build, policy, or configuration change fail closed. An honest
no-diff report becomes a nonempty normalized JSON record rather than an empty
success artifact.

Sealed release proof remains fail-closed until the release broker mounts the
already-governed native-tool bundle for contract-proof consumers. That
follow-up must use a separate proof-tool consumer predicate; adding
`jeryu-cache` to `jain_native_learners_for_repo` would incorrectly activate
native-library verification and build semantics.

`artifact-support` emits a closed v3 receipt from a clean exact head/tree for
exact locked Cargo metadata, a deterministic release-input inventory (including
`.cargo/**` and `rust-toolchain.toml`), and an SPDX SBOM. The canonical evidence
root and every receipt subsidiary must remain physical, stable, regular, and
single-link. The receipt binds their SHA-256 digests plus the fixed physical
Syft path, SHA-256, version, custody domain, ownership, mode, and link count.
Syft is likewise invoked only through a held descriptor from root-owned
non-writable local or read-only release custody; the old exit-zero
`status = "bootstrap"` receipt is not accepted.

Run `rtk bash tests/ci_local_dispatch_test.sh`,
`rtk bash tests/pr_ci_order_test.sh`,
`rtk bash tests/security_lane_test.sh`,
`rtk bash tests/contract_drift_test.sh`, and
`rtk bash tests/artifact_support_test.sh` to verify exact lane routing, hostile
drift/tool and physical-file failures, closed receipt validation, final
evidence ordering, and fail-closed argument handling. `scripts/ci-doctor.sh`
checks the required local tools.

Agent-readable exception guidance:

- purpose: every typed error documents the caller-facing failure purpose
- reason: failures preserve enough context for local diagnosis
- common fixes: map repeated failures to a small set of operator repairs
- docs_url: point users to this file or a narrower runbook
- repair_hint: state the next command or config change to try

The security lane requires exact Gitleaks, Actionlint, Cargo Audit, Cargo Deny,
and Syft identities. It runs from cached inputs without network access and
fails closed if a tool, advisory database, policy result, or SBOM identity is
missing or invalid.
