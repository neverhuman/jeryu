# jeryu-core

Forge/domain truth, git storage, read models, TUI, durable DB migrations.

The `jeryu-core` crate exports object-safe `ForgeReadService` domain contracts
for repository, pull-request, check, protection, and audit reads. Transport
adapters consume those contracts without depending on the concrete `ForgeCore`
store or introducing HTTP types into the domain crate.

Authenticated reviews use `authenticate_actor` with an actual PAT or session
credential, followed by a persisted `create_review_challenge` and exact-head
`submit_bound_review`/`dismiss_bound_review`. Session mutations additionally
require the session CSRF proof; `SessionReadOnly` is accepted only for reads.
Actors are opaque and revalidated under Core's shared guards. Session issuance
now requires the account password in the same guarded operation; PAT issuance
derives its account from a live opaque actor. A login string or `AccountSummary`
cannot be converted to a qualified actor. These are consumer API changes; adapt
the actual transport callers and cut a fresh immutable dependency successor.

The owning service must use `open_managed(database, storage_root)` and attach
one `jeryu_gitd::ManagedReviewGitObserver` to that shared runtime. Configure an
absolute approved Git executable, private managed storage and a private service
umask. The observer rejects alternate/symbolic routing and returns actual direct
refs, commits, trees and physical custody. It clears ambient Git configuration,
limits subprocess output, and uses bounded process-group cleanup; unresolved
child custody disables further observation pending administrative recovery.
Installing exclusive storage and transport custody remains separate work.

`bound_review_history` enforces live read authentication. `review_qualification`
is the common read-model projection of effective bound verdicts and advisory
history. Historical login-authored reviews and unbound checks cannot qualify.
`evaluate_branch_protection_with` is an advisory pure calculation over supplied
rows, not an authorization capability. Required-attempt publication and the
Core-owned durable executor are still unavailable: legacy split readiness,
caller-result finalization and synthetic merge methods all refuse before Git
dispatch. A source-review result never asserts merge or installed qualification.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d` by
`ops/split/materialize.py`. It is part of the independent Jeryu split family and keeps source paths
stable where practical so ownership remains auditable; family membership is derived from the
Jeryu authority manifest rather than a count embedded here.

The Phase 12 JeryuCache contract remains documented in `docs/PHASE12_SPEC.md`. Runtime cache/CAS
behavior is owned by the separately released `jeryu-cache` repository; this repository consumes
that boundary through pinned interfaces.

## Quick start

Use the pinned Rust toolchain and run the same local entry points used by the
protected gate:

```bash
just fast
just focused
just check
just score
just security
```

The release-supporting full gate is `bash ops/ci/pr-ci.sh`; it additionally
requires the governed Jankurai binary and the pinned security tools documented
by `ops/ci/security.sh`.

## Owned Cargo Packages

- `crates/jeryu-core`
- `crates/domain`
- `crates/jeryu-gitd`
- `crates/jeryu-mirror`
- `crates/jeryu-mirror-cli`
- `crates/jeryu-readmodel`
- `crates/jeryu-tui`
- `crates/jeryu-bugtracker`
- `crates/jeryu-enterprise`
- `crates/jeryu-proof`

## Source Coverage

- `crates/jeryu-core/**`
- `crates/domain/**`
- `crates/jeryu-gitd/**`
- `crates/jeryu-mirror/**`
- `crates/jeryu-mirror-cli/**`
- `crates/jeryu-readmodel/**`
- `crates/jeryu-tui/**`
- `crates/jeryu-bugtracker/**`
- `crates/jeryu-enterprise/**`
- `crates/jeryu-proof/**`
- `db/**`
- `contracts/generated/**`
- `contracts/AGENTS.md`

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`
