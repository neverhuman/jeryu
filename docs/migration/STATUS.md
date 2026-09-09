# Public monorepo qualification

This is an unqualified source candidate. GitHub source authority, release
authority, split publication, relocation, and the running service have not
been cut over. `handover.status` remains `pending-protected-review`.

The ten component imports retain byte-identical source trees and linear
ancestry from the existing GitHub portal main. `imports.json` binds each
original commit and tree to its integration commit. Complete Jeryu histories,
untracked files, runtime/support material and build output are preserved in
private custody. Historical active manifests and lockfiles are retained under
`original-manifests/`; Cargo and npm use only the new root files.

`import-updates.json` records later source deltas without replacing the
original import mappings. Deploy's two-commit update through `b388edc5` was
preserved, restored, scanned and integrated at `bdd6c04a`. It preserves the
reviewed main commit during push callbacks and verifies a restarted service's
actual executable digest. All 13 bridge tests and 11 activation scenarios
passed locally, as did warning-denied Clippy. Importing candidate source does
not claim the original protected PR has merged or that a service was restarted.

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

- Publish the audited canonical Redline tag
  `redline-core-v4.1.0-jain.6` at
  `d0de59930141baffcfa2b514480e75b14627f24d` and the governed Jankurai source
  and tool artifacts. Neither required tag was anonymously available on
  GitHub at the initial readback. Cargo regenerated the four-crate Redline
  closure with the public URL and the same immutable commit. This local
  preparation fetched the canonical producer through an explicit temporary
  Git transport override; it is not anonymous build evidence. The public
  preflight independently reads tags without credentials or personal Git
  configuration and still fails until publication.
  GitHub rejected the initial exact Redline tag mirror because that OAuth
  connection lacked `workflow` scope. Anonymous readback proved no public ref
  changed. The owner subsequently supplied a separate workflow-capable
  credential explicitly; the independent dependency publisher has the scoped
  mirror task. Publication still requires exact anonymous readback. Independent
  reviewer and merger identities remain unresolved.
- Complete portable CI and split-export implementation, proof-lane mapping,
  full source/history/license audit, dependency advisory repair, and every
  ordinary and privileged local proof. Missing capabilities must fail closed.
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
independent verification of the resulting commit remains required. No
component mirror has been published or marked qualified.

All six duplicate Redline tips and their reflog commits are reachable from
their full canonical repositories, with no missing commit objects or
uncommitted paths. This read-only comparison and the prior restoration proof
do not waive fresh producer/two-consumer evidence or authorize retirement.

Every original checkout and support directory remains retained. Ten existing
public Jeryu repository graphs were independently bundled and restored with
matching refs. No hosted branch, tag, protection, mirror writer, release
authority, service, or installed boundary has been changed by this candidate.
