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

Local implementation checks have established the following, before final
commit qualification:

- The source build and installer passed default/explicit destination checks,
  stale-source and tampered-artifact rejection, and process tests against the
  installed binary. Those tests exercise authentication, repository and issue
  creation, Git push/clone, durable SQLite state, and restart persistence.
- Warning-denied Clippy passed before the latest repository-creation adapter.
  The ordinary workspace test run exposed a bounded startup timeout in the new
  process test. Its readiness window is now 60 seconds and its focused rerun
  passes; a complete rerun is required. The sandbox crate remains a separate
  required lane and has not been qualified on a disposable capable host.
- Web checks passed 180 unit tests, seven live backend browser tests, 70 UI
  browser tests, the 110-action coverage matrix, accessibility/rendered
  evidence, Storybook, and three Lighthouse runs. The collector passed all
  nine checks. Existing resource-size warnings remain. That invocation ended
  with a shell read error after its running wrapper was edited, so it is not
  a successful complete lane; the frozen wrapper must be rerun.
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
  Cargo advisory, license and ban checks passed. The source policy must be
  rerun after the public Redline repin. All five pinned binary CI tools passed
  download checksum verification; pinned Cargo tool installation remains a
  separate check.

Every original checkout and support directory remains retained. Ten existing
public Jeryu repository graphs were independently bundled and restored with
matching refs. No hosted branch, tag, protection, mirror writer, release
authority, service, or installed boundary has been changed by this candidate.
