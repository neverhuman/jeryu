# Current qualification status

**PENDING.** The public monorepo is a source candidate. Root manifest handover
remains `pending-protected-review`; protected release authority and the
installed service have not been cut over. Keep the canonical checkout at
`/home/ubuntu/jain-split/jeryu-split/jeryu`.

The historical source readback below is bound to
`679d9cff275973ce0357d5e1db12118166c1d6a7`, tree
`cfb8642e3e401513d2537ba5ba05b0c389d63bda`, and public readbacks on
2026-09-10. Implementation added after that revision requires its own evidence.
The complete chronological record is preserved in
[historical status](history/STATUS-2026-09-10-679d9cff.md).

## Established source shape

- One root Cargo workspace with 65 local packages and one npm workspace.
- Embedded production browser assets, source/binary verification, local
  administrator bootstrap and durable SQLite are implemented.
- Redline is an optional excluded compatibility harness and does not enter
  the default product dependency graph.
- Deterministic component export and forward-only offline publication
  preparation exist; protected mirror publication remains unqualified.

Historical build, runtime, sandbox, OCI and selected component results are
recorded with their original source identities in the preserved history.
They are not results for a newer source revision. The last saved complete
root audit in that record was a failure for `b4206091`; there is no fresh
complete passing census for `679d9cff` in this checkpoint.

## Open release gates

| Area | Status and missing evidence |
| --- | --- |
| Audit | PENDING: fresh complete repository census, independent producer qualification, exact report validation, zero hard findings/caps and effective floors |
| Auxiliary proofs | PENDING: authentic protected predecessor, complete changed paths/hunks, proof binding/marking, negative tests, coverage and conformance |
| Public verification | PENDING: portable Work/Cache security admission, public tool/artifact acquisition and exact dependency receipts |
| Complete CI | PENDING: complete local matrix, hosted matrix and resulting-main qualification at one final revision |
| Installation | PENDING: anonymous public-origin build/install/runtime sequence at the final public candidate |
| Optional runner image | PENDING: immutable public inputs and actual product-image tests, separate from native/OCI probes |
| Mirrors | PENDING: actual standalone export builds/tests/audits and protected publication |
| Every-commit audits | PENDING: full commit accounting, retries, reconciliation and governing-policy enforcement |
| Live score cards | PENDING: trusted evidence publisher, immutable reports, managed links and race-safe current pointers |
| Release/recovery | PENDING: signed central artifacts, SBOMs/provenance and standalone upgrade/backup/restore qualification |
| Custody | PENDING: exact original-directory dispositions and remaining verified retirement obligations |

The source policy floor is at least 85. Cache, Runner and Work retain stronger
91-point gates; root soft-finding limits and all existing ratchets remain.
No displayed numeric score is evidence for current head without an exact
source, producer, policy and execution binding.

## Current implementation work

The required `audit` lane now records every enrolled scope, including missing
sources and failed execution. Shared Rust tooling validates complete reports,
prepares Git graph work without truncating intermediate commits, and maintains
README marker blocks. Actual automatic scheduling, qualified SVG rendering,
trusted publication and authenticated governing-policy admission remain open.
The census aggregate deliberately remains failed until those required inputs
are admitted; no source availability or synthetic fixture is an audit pass.

Work now writes security evidence through its owning Rust package. The scanner
commands and failure outcome remain required. The complete selected source,
standalone exports and public installation still need final qualification.

Selected working-tree verification based on `30617123` passed the complete API
suite (304 tests), owning Work suite (32), CLI snapshots (42), seven standalone
process scenarios and 209 browser unit tests. Selected rendered scenarios and
an actual backend browser scenario also passed. Warning-denied Clippy passed
for API, CLI and Work. These results precede the final candidate; later source
changes still require their owning checks. An initial stale API expectation
failed, was independently reviewed and corrected, and remains in retained
failure evidence. No complete matrix or release audit is implied.

The subsequent acquisition/recovery slice passed 104 split-tool tests and 44
Tool control tests, with warning-denied Clippy for the split tool, Tool and
CLI. Seven standalone process scenarios and three recovery-helper tests passed
across the retained runs, including stopped whole-data archive/restore of Git,
credentials, Work items and comments. The latest Work guidance also passed a
rendered browser test and screenshot inspection. The split-tool and standalone
tests used already-built binaries under a fresh unprivileged UID with zero capabilities;
they do not establish an empty-cache source installation. Earlier cleanup
permission failures and recovery-fixture corrections remain retained.

## Public repositories and dependencies

GitHub `neverhuman/jeryu` main was still
`4c93436abdc6160885216c9ea71414501db2f54c` at the readback.
All eleven planned Jeryu repositories were public. Work's `jeryu-jira`
now exists at main `4450477878ff230690217483fe1b64f6dbc80e56`;
the earlier 404 is historical and no longer establishes a missing repository.
Work, Tool and Tool Finder had no branch protection at the readback.

The Jeryu auditor remains pinned to Jankurai
`b88562fdb124aa86dedd70ab972e7d0d87e58be1`. The newer public Jankurai hub and
its supporting repositories are a separate prospective producer closure.
Their availability does not approve a consumer repin. The
[dependency dashboard](../dependencies.md) keeps current heads, pinned source
and optional capabilities separate.

## Preservation boundaries

Original checkouts remain until stopped-head handoff, unique-state comparison,
verified preservation/restoration and exact-path retirement admission.
The 343 archived session checkouts still have unresolved live workcell/WarmPool
references. The two old Cache units require whole-root restoration proof,
including their 26 internal hard-link pairs, before separate retirement.

Duplicate Redline repositories remain retirement-pending until canonical
comparison, restoration and the unwaived two-consumer gate pass. Optional
Redline and broader cleanup results remain visible without blocking an
independently qualified SQLite release. The peer-deleted-checkout evidence
gap remains documented in the historical record.

See [CI coverage](CI-COVERAGE.md), [auxiliary proofs](AUXILIARY-PROOFS.md),
[capability coverage](CAPABILITY-COVERAGE.md), [original dispositions](DISPOSITION.md),
[split publication](SPLIT-PUBLICATION.md) and the [migration plan](PLAN.md).
