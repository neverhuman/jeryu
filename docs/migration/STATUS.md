# Public monorepo qualification

This is an unqualified source candidate. GitHub source authority, release
authority, split publication, relocation, and the running service have not
been cut over. `handover.status` remains `pending-protected-review`.

The ten component imports retain byte-identical source trees and linear
ancestry from the existing GitHub portal main. `imports.json` binds each
original commit and tree to its integration commit. Complete Jeryu histories,
untracked files, runtime/support material and build output are preserved in
private custody. Historical active manifests and lockfiles are retained under
`original-manifests/`; product Cargo and npm use the new root files.

The latest user direction makes RedlineDB independent of the SQLite release.
The server already opens durable SQLite databases. Its only Redline build
edge was an Obs development contract, now preserved byte-for-byte in the
excluded `components/jeryu-release-ops/tests/redline` workspace with its own
Cargo-generated lock. `scripts/ci.sh redline` explicitly runs that optional
proof; required local/hosted lanes and Release Ops readiness do not invoke it.
The root candidate manifest and its validator declare bundled SQLite as the
default and Redline compatibility as nonblocking. Historical Redline authority,
immutable tags, two-consumer proof and original-retirement gates are unchanged.

Local locked all-feature metadata contains all 65 product packages, 459 total
packages, zero Redline packages and zero Git dependencies. Cargo removed 217
lockfile lines without upgrading packages. Split-tool tests (22 unit and four
CLI contracts), the API dependency identity test, all five Obs integration
tests, affected warning-denied Clippy, candidate manifest routing and CI
dispatch checks pass. The separate Redline harness passes formatting, Clippy,
the original transaction/reopen test and immutable four-package identity test.
An empty-cache build and installed runtime at the committed successor remain
the next proof; these local checks do not qualify publication or all CI.

At exact `7b9781b1`, the complete ordinary Rust lane passed formatting,
warning-denied Clippy and 2,202 tests with zero failures. Two ignored cases
belong to the optional paid-model check and the separate required OCI lane;
the required native sandbox lane also runs separately. The security baseline
passed dependency, license/source, history and workflow checks. npm retains
two low and three moderate advisories below its high-severity gate. A fresh
SQLite installation attempt stopped during namespace setup before Cargo ran;
its source and diagnostics remain preserved. These results do not establish
the complete CI matrix or a public release.

At exact `d74049f3`, the complete web lane passed: 97 generated contracts,
180 unit tests, production/Storybook, seven real-backend browser tests,
70 UI/accessibility browser tests, 110 action entries, Lighthouse and nine UX
collector checks. Its source-bound candidate auditor installation and all nine
independent auxiliary producer steps also passed. Complete auxiliary outputs
were archived, restored and compared before guarded scratch removal. Full
auxiliary admission remains pending. The actual root audit at that commit
still failed: 64 against 85, eight caps and 35 hard findings. Independent
review identified a real onboarding-state error swallow alongside upstream
classification defects; no threshold or finding was waived.

The onboarding helper now initializes only missing files. Malformed JSON,
non-object JSON and read failures return contextual errors before writes;
the existing optional auth-seeding caller reports the failure. Four API tests
cover ten scenarios, including preserving invalid bytes, modes and timestamps
and a read error that still executes under root. Pinned Cargo tests and API
Clippy pass. This repairs the identified consumer defect; the full auditor
has not yet been rerun on this successor.

Bundled JetBrains Mono fonts now carry their unchanged upstream OFL and a
notice binding the exact delivery hashes. The production web build and all
three standalone process tests pass, including HTTP hashes for the served
license/fonts and the notice content. The subsequent dependency-notice record
accounts for all 405 historical/current third-party source-map entries and
their exact locked archives. It distributes 24 upstream notice texts, with
explicit provenance for packages whose archives omit licenses and separate
build-producer attribution. See [the record and its limits](../notices/web-bundles.md).
The added served notice has an HTTP MIME/hash assertion; its production build
and runtime verification remain pending. Publication review remains separate.

Split qualification now rejects unreadable/empty component inventories,
requires exact clean source and exports before/after execution, and uses fresh
evidence directories. A failure retains the outer qualification checkout tree;
the inherited lock resolver and Deploy browser helper still remove their own
inner temporary clones. Successful outer cleanup checks ownership, identity,
links and mounts. All 24
synthetic orchestration cases and shell checks pass. These cases do not replace
the required ten real independent split builds at the final source commit.

Five component CI entrypoints (Core, Runner, Deploy, Intelligence and Tool)
now route exactly `required` to their existing full PR wrappers. No arguments
preserve the quick checks; invalid or extra arguments fail before dispatch.
Deploy's full wrapper also retains the metadata, map, shell and phase/coverage
checks previously available through its quick recipes. All 65 synthetic cases
pass against the checked-in scripts, including child failure propagation, and
the changed shell files pass syntax and warning-level ShellCheck. This repairs
dispatch coverage only; the full component wrappers and remaining specialized
proof lanes are still unqualified.

Deploy's full PR wrapper now discovers the API member's Cargo workspace and
checks its actual root lock, preserving standalone exports and the monorepo
layout. It rejects missing, linked, replaced or modified inputs and retains
both existing recheck points. All 39 synthetic cases and actual read-only
workspace discovery pass; this does not qualify the full wrapper. The separate
Core, Runner, Deploy and Intelligence metadata guards still assume a local
component manifest and require repair before complete legacy qualification.

Legacy cleanup accounts for 214 partial merge files and all 107 declared dirty
patch outputs in restored private Git histories and a path-by-path disposition
index. All 52 partial `.merged` files contain conflicts; static review found
their product behavior retained or deliberately superseded in current source.
The seven partial source copies, 72 abandoned test directories, empty support
directories and the remaining six-file `dirty/` artifact directory were retired
after verified preservation and link/mount/identity checks. Raw patches and
reports remain in owner-only archives. This source accounting does not replace
execution or release qualification; original canonical repositories remain.

All 343 retained session checkouts now have private archives covering
45,598 files and 36,811,805,404 bytes, including their complete Git state.
Every archive was restored and independently rehashed; original/restored
inventories, refs, indexes, working state and strict Git integrity checks
matched. Restoration scratch was removed after inspection. Every original
remains because complete exclusion of live workcell-pool references is still
unresolved. Preservation does not authorize retirement.

A later census found 18 empty Native/phase4 test roots and seven Cache scenario
roots left by completed tests. All 25 were archived, restored and compared,
then removed after repeated identity, link, mount and privileged read-only
handle checks. The archive retains all 40 Cache fixture files. Cache tests now
own a private outer fixture directory and clean it on every exit; ten tests
and Clippy pass. Portal hostile tests also guard their scratch, and simulated
security scanners write only inside an isolated synthetic test repository.
Runner Native/phase4 tests now retain private fixture owners through dispatch
and assertions, with identity/link/mount checks before cleanup. All eight
Native and nine phase4 cases passed, along with formatting and warning-denied
Clippy; the isolated test parent was empty afterward.

The next cleanup preserved and restored 476 closed Work test databases, then
unlinked only those individually inventoried files after repeated checks.
Their 19,496,960 bytes remain in private custody. Work unit and property
fixtures now own their database directory and sidecars; seven focused unit
tests and four property-target tests passed, retaining 32 cases per property.
Formatting, Clippy and an empty isolated scratch directory also passed.

Finder now distinguishes its component directory from the shared monorepo
Git root, binds every tracked monorepo input and requires the actual candidate
auditor verifier. Its candidate dependency check requires one workspace
identity for Finder, Codegraph and Rustjet and their exact resolved edges.
Its candidate artifact producer binds the shared lock, toolchain and manifests,
records the complete Cargo configuration chain and refuses credential files
and unsupported build overrides. It uses a fresh private build target and
empty Git configuration, with pending candidate provenance. Sixty-five pure
score cases and candidate artifact/configuration/provenance/cleanup contracts
pass. Score publication and artifact consumption both require full standard
audits and independently reject hard findings, contradictory copy-code results,
caps and malformed data. Five earlier boundary/cleanup/dependency suites and
shell checks also pass. Actual candidate score/security/artifact production and
evidence-mutating hostile tests remain pending in a qualified disposable clone. Cleanup preserves
the hostile tests' external-path topology; standalone exports retain their
own scratch helper.

A wider census identified 5,034 Web test databases outside the source family.
Their 278,851,584 bytes were privately archived, restored and compared before
individual removal, with repeated metadata, link, mount, open-handle and held-
descriptor checks. Test WebState clones now share a fixture owner that removes
both auxiliary databases and sidecars when the last state drops. Three lifetime
and cleanup-refusal tests, two Codegraph route tests, one Work route test,
formatting and warning-denied API Clippy passed; isolated scratch was empty.
The production database paths and session workspace behavior are unchanged.

All 343 dirty session Git checkouts remain retained. The live agent-run API
returned no overlapping references, but its registry does not expose private
WarmPool leases. Complete preservation and live-reference disposition remain
required before removal. One checkout per canonical source repository does
not imply that these runtime and test directories have been retired.

Deploy phase aggregation now requires each command to exit zero and print its
own final PASS result. Pending, missing or mismatched results and a PASS line
followed by a failed exit cannot qualify. Missing coverage tools preserve a
nonzero failure. Nineteen isolated dispatcher cases and shell checks passed;
they do not produce actual coverage or mutation evidence.

Coverage validation now requires a valid baseline and complete LCOV measurements
for every requested crate. It compares the unrounded ratio with the existing
floor. Mutation evidence must come from a fresh completed run, with a passing
baseline and one consistent terminal result for every selected mutant. Aborted,
partial, stale or empty results fail. All 63 synthetic evidence cases and shell
checks pass; the existing coverage floors, upward-only baseline update policy
and final mutation audit requirements remain. Real producer qualification is
still pending.

Source-install tests now preserve verified original artifacts before deliberate
tampering, detach Cargo hardlinks, and restore through held directory and file
descriptors. Failed tests or uncertain cleanup retain recovery copies and
return failure. Eighteen synthetic transaction scenarios and shell checks
passed, followed by the real fresh-identity installation proof below.

Exact `f7d92039f0e84b7ed6f87a9f1657820b985ccaba` passed a source build and
installation under a fresh unprivileged Linux identity with empty Cargo, npm,
HOME and XDG caches. The isolated filesystem hid the host home, neighboring
repositories and service sockets; private host-forge connections were refused.
Public npm and immutable Redline access passed. The web/release build, source
installer, stale/tampered artifact refusals and all three installed-binary
standalone tests passed, including protected PR review/check/merge and durable
restart. Source, artifact and compiler hashes matched afterward. The disposable
clone was removed after mount and symlink inspection. This proves the preserved
local commit with anonymous dependencies; it does not establish public GitHub
origin availability or qualify the complete CI matrix or subsequent changes.

The ordinary Rust lane at the same commit passed formatting, warning-denied
all-target/all-feature Clippy and 2,196 tests across 257 test/doc-test suites.
Two cases were ignored there: optional paid model API smoke and the required
separate Docker lane. Native sandbox is also a separate required lane.
The isolated test scratch was removed and source remained clean.

Work's contract generator now has the unique binary name
`jeryu-jira-export-contracts`; Core retains `export_contracts`. This removes
their shared output collision while preserving package identities and all
97 generated contracts. The combined contract drift check, affected Clippy,
19 Work tests including properties/doc-tests, and scratch guards passed.
Root contract and product commands use the shared scratch cleanup guard.
Product proof now requires readable clean source state and a fresh evidence
directory, and reports success only after cleanup. At exact `089b4629`, all
seven Cache scenarios and Codegraph persistence across separate processes
passed, with clean source and verified scratch cleanup. Dirty-source and
failed Git-read refusals also passed.

The same checkpoint exposed JSON ordering drift between package-only and
workspace builds. Inventory, split npm manifests/locks, split provenance and
candidate auditor provenance now recursively sort object keys before rendering,
preserving array order and existing newline conventions. Split-tool tests and
the actual candidate renderer regression pass with both default and
`serde_json/preserve_order` features; warning-denied Clippy passes. Generated
inventory agreement remains source accounting, not execution evidence.
At clean `c928f3aa`, all ten actual split export trees matched across those
feature builds, and the full 83-target candidate renderer emitted identical
bytes with no drift. These unresolved-lock exports prove reproducibility;
independent builds and publication qualification remain separate requirements.

Four auxiliary proof wrappers now share actual copy-code and migration
producers, with one root command for the complete 65-package default-feature
Rust map/witness graph. They preserve failed producer exits and reject missing,
malformed or contradictory reports. All 75 synthetic cases, shell checks and
root check suites pass. This preliminary command remains outside `all` and
the hosted matrix until actual producer qualification and the missing full
proof adapters pass. Default/full execution must fail for unavailable protected
baseline, changed-hunk, proofbind/proofmark, configuration and conformance
admission. Standalone wrappers also refuse pending an authenticated export
adapter. See [auxiliary command scope](AUXILIARY-PROOFS.md).

The real installed auditor ran against exact `5fab2aea` and failed the root
gate at 64 against 85, with eight caps and 32 high/critical findings. Root/Core
gates now reject malformed policy/report data and count actual findings
independently of advisory decision summaries. Sixty in-memory refusal and
threshold cases pass. Hosted CI uses a thin `ops/ci/monorepo.sh` delegate to
the existing shared root command, and root attribute metadata has ownership
and test routes. Component context and complete text coverage still require
auditor repair; these checks do not qualify the full audit or CI matrix.
The Jankurai maintainer confirmed that released `v1.7.0` still has these
component-classification and full-text inventory defects. The existing
immutable consumer pin remains unchanged pending the required producer fixes.

`import-updates.json` records later source deltas without replacing the
original import mappings. Deploy's two-commit update through `b388edc5` was
preserved, restored, scanned and integrated at `bdd6c04a`. It preserves the
reviewed main commit during push callbacks and verifies a restarted service's
actual executable digest. All 13 bridge tests and 11 activation scenarios
passed locally, as did warning-denied Clippy. Importing candidate source does
not claim the original protected PR has merged or that a service was restarted.

Runner's handed-off scheduler correction through `d645d07f` was privately
bundled/restored, reviewed, scanned and imported at `1c732b7a`, preserving its
author. Lease transitions now require the full lease identity and current
scheduler time; expiration consumes attempts and cancellation is terminal.
The scheduler tests and full workspace Clippy passed. Durable queue/transport
integration and operator cancellation remain separate runtime work.

The duplicate Redline repositories remain in private custody at their original
locations. Their shallow bundles require the preserved shallow-boundary files
for restoration; a bundle alone is insufficient. All six restore with those
files. Canonical comparison and two-consumer proof remain required before
retirement. No Redline lock was edited.

History scanning found two non-secret examples: an API documentation
`Idempotency-Key` and a password-change test fixture. Their original findings
and contextual triage are retained privately. This is a scanner result, not a
claim that the complete publication audit has finished.

Remaining release gates include:

- Publish and verify the governed Jankurai tool artifacts. Its existing
  `v1.6.11-deadlang-precision-split.3` source tag is now public at
  `b88562fdb124aa86dedd70ab972e7d0d87e58be1` in `neverhuman/jankurai`.
  Independent audit covered 121 diffs in the 133-commit graph and all 1,895
  unique text blobs. The seven matches were documented API-hash examples and
  validation signing-key fixtures already present in public history. Root and
  six Rust packages are MIT; the bundled JetBrains Mono font is SIL OFL 1.1.
  Before publication the public history was bundled/restored; anonymous
  readback confirmed the exact new tag and every existing public ref unchanged.
  The candidate manifest now declares an explicit public distribution URL
  separately from its unchanged producer/build identity. Schema 1 remains
  supported; schema 2 rejects unknown or substituted distribution fields.
  Anonymous immutable-tag preflight passes. Availability alone is not build
  proof, and the protected authority handover remains pending.
  The actual portable builder at `ecb8c89d` subsequently reproduced the
  unchanged golden binary and build-context digests from anonymously acquired
  public source and an initially empty registry cache. All source, vendor,
  compiler, linker and pinned image identities matched. The nonroot Docker
  build used two CPUs with networking disabled; input hashes matched afterward
  and both builder/source scratch passed guarded cleanup. Three maintained
  Rust refusal/cleanup/dispatch tests and Clippy also pass. This closes the
  builder check, without creating an installation receipt or changing the
  installed auditor.
  An explicit candidate renderer, portable installer, closed receipt verifier
  and root `auditor` command are now implemented. Tool unit/integration tests,
  Clippy and the four retained installer/root-seal/renderer/governed-path shell
  suites pass locally. Their separate candidate receipt keeps protected
  approval pending. At `5fab2aea`, all 83 generated targets across the root and
  ten components passed the full drift check. The actual public candidate
  installer fetched source and locked dependencies anonymously, reproduced the
  same golden binary/context in the offline Docker builder, and published a
  source-bound candidate receipt. Same-head installation reuse, shared-lock
  bootstrap reuse, held-descriptor execution and wrong-source/path refusals
  passed. Its disposable exact-commit clone was removed after guarded cleanup.
  The shared family ancestors are group-writable, so this proof used a private
  cache directory; renderer compilation reused existing Cargo tool caches.
  Final empty-cache monorepo qualification and later candidate commits still
  require their own complete matrix.
  The canonical
  Redline tag `redline-core-v4.1.0-jain.6` is now anonymously available from
  `neverhuman/redline-core` at the exact
  `d0de59930141baffcfa2b514480e75b14627f24d` commit; independent readback also
  confirmed unchanged public main. Cargo's four-crate closure retains that
  immutable identity. This removes the Redline transport blocker; anonymous
  qualification of the complete source and governed auditor remains required.
  No production storage backend or Redline consumer lock was changed.
- Complete portable CI and split-export implementation, proof-lane mapping,
  full source/history/license audit, dependency advisory repair, and every
  ordinary and privileged local proof. Missing capabilities must fail closed.
  [CI coverage](CI-COVERAGE.md) records each retained assertion, explicit root
  command and unported gate. Source inventory now includes proof implementations
  and inputs; agreement of those hashes cannot substitute for execution.
- Qualify clone/build/install/runtime with an unprivileged empty-cache Linux
  environment, then the exact GitHub candidate, merged main, and release tag.
- Obtain independent review and a separate merger for protected authority
  handover. Publish component mirrors only after central qualification.
- Prepare and verify the installed runtime projection before the coordinated
  relocation and maintenance gate. Preserve Jain's immutable consumer pins.

The CLI now routes implemented forge/agent/control-plane operations to the
configured HTTP API. Historical CI, runner mutation, proof, release, cache
self-test and agent-auth commands that had only in-memory simulations return
explicit errors until server transports are implemented. They cannot report
successful operations against discarded temporary state.

The frozen candidate `169659957435fb896aa748574d58c0f51c7a50d2` passed the
ordinary Rust, web and source-install/runtime commands, followed by a separate
disposable VM sandbox proof. Subsequent implementation changes require their
own qualification. Evidence for that candidate establishes:

- The source build and installer passed default/explicit destination checks,
  stale-source and tampered-artifact rejection, and process tests against the
  installed binary. Those tests exercise authentication, repository and issue
  creation, Git push/clone, durable SQLite state, and restart persistence.
- Formatting, warning-denied Clippy across all targets/features, and the
  complete ordinary workspace test command passed. One paid external model
  API smoke test remains explicitly optional and ignored; it requires user
  credentials and consumes quota. Sandbox tests ran separately in a disposable
  Ubuntu 24.04 KVM guest from a signature- and checksum-verified cloud image.
  All 35 sandbox tests passed with zero ignored tests. Admission verified all
  required kernel capabilities; all four escape attempts were blocked, with
  zero false skips. Memory/process confinement, launch/watchdog, terminal
  injection and secret-path defenses passed. The guest was removed afterward.
- Web checks passed 180 unit tests, seven live backend browser tests, 70 UI
  browser tests, the 110-action coverage matrix, accessibility/rendered
  evidence, Storybook, and three Lighthouse runs. The collector passed all
  nine checks. Existing resource-size warnings remain. The frozen command
  completed successfully; the earlier invocation interrupted by an edited
  running wrapper is retained as a failure, not qualification evidence.
- Regeneration found stale browser PR types and four missing contracts. The
  browser now receives all 80 Read Model and 17 Work contracts directly from
  their Rust generators. `scripts/contracts.sh --check` compares both owners
  and the complete browser set; `--write` regenerates them.
- The real browser Create Repository flow initially returned HTTP 405. The
  adapter now validates ownership and input, previews without writes, creates
  managed Git storage, initializes the requested branch/README, and preserves
  durable idempotency receipts. Replays after SQLite reopen pass. Partial
  creation fails closed and requires recovery; existing orphaned Git storage
  is never adopted. Topics, templates and internal visibility remain explicit
  unsupported requests rather than silently discarded fields.
- npm audit reports zero high/critical, three moderate and two low findings.
  Cargo advisory, license, source and ban checks passed after the public
  Redline repin. Root workflow actionlint and zizmor checks passed. All five
  pinned binary CI tools passed download checksum verification. The subsequent
  complete portable security command also passed installation of pinned
  cargo-audit, cargo-deny and zizmor, dependency/history/workflow checks and
  SPDX SBOM generation. The governed auditor remains a separate prerequisite.

Clean `e2e8897f` subsequently passed source, product and release-build/source-
install/runtime lanes, including all seven Cache poisoning scenarios and
Codegraph cluster equality across independent CLI processes. A later process
test also passed the full authenticated protected PR journey and Clippy:
distinct author/reviewer/merger, blocked direct-main push, self-approval and
old-head approval refusal, administrative check publication, exact-head check
filtering, persisted reviewer/head identity across restart, and exact reviewed
Git commit/tree after merge and another restart. Installed-binary execution
of that new test remains part of the next frozen runtime run.

The split exporter now prepares all ten components, including the web npm
workspace, with standalone manifests, locks, contribution routing and CI
wrappers. Cargo/npm resolved each component lock in disposable Git clones;
the resolver rejected no source identity or external version drift. These
preparatory resolutions used an explicit temporary Git transport override to
the exact preserved local source commits. They are not anonymous proof.
All ten locked exports from `f8d215885f36178883a4ea900a076195b799dea5`
were byte-identical across repeated generation. Cache, Core, Intelligence,
Work, Tool Finder, Tool and Web passed independent checks in disposable Git
clones. Runner, Deploy and Release Ops failed tests that assumed every Jeryu
dependency was local or that the complete 65-package monorepo was present.
The updated tests retain those monorepo assertions and require external split
dependencies to bind the provenance commit. Deploy also builds its embedded
web dependency from that exact public source commit in a disposable clone.
At `075134c0`, eight exports passed; Runner and Deploy exposed two remaining
test assumptions. Runner's standalone closure does not consume Gitd, and
Deploy does not inherit the external Obs package's Redline dev-dependency.
The repaired tests retain the full monorepo assertions and verify those
standalone closures. Targeted tests and warning-denied Clippy passed;
the full `a9c5e5a2` run then passed nine components. Deploy revealed a further
standalone governance-root assumption and a real live-HTTP readiness defect:
the nonexistent `/healthz` route could be satisfied by the monorepo's SPA.
Its readiness test now requires the actual `/health` backend JSON identity.
All five HTTP tests and Clippy passed, followed by the independent Deploy
export at `2073b366`. Thus all ten components have passed across repaired
candidates. A complete final-commit rerun and anonymous qualification remain
required. No component mirror has been published or marked qualified.

All six duplicate Redline tips and their reflog commits are reachable from
their full canonical repositories, with no missing commit objects or
uncommitted paths. This read-only comparison and the prior restoration proof
do not waive fresh producer/two-consumer evidence or authorize retirement.

Every original canonical checkout remains retained. Seventy-two abandoned
test scratch directories, two empty support directories, and seven redundant
legacy partial-source copies were removed only after private preservation,
restoration and final symlink/mount/inode/open-handle checks. All 214 partial
source files and all 107 target blobs declared by the two old patches have
verified private Git recovery histories and restored bundles. Raw patches,
reports and archives remain retained; recovery is distinct from product
review/integration. No linked worktrees were found in the eleven Jeryu
checkouts. Active CI output and unowned scratch were excluded from cleanup.

Ten existing public Jeryu repository graphs were independently bundled and
restored with matching refs. No Jeryu hosted branch, tag, protection, mirror
writer, release authority, service, or installed boundary has been changed by
this candidate. The separately authorized Redline tag mirror is recorded above.
