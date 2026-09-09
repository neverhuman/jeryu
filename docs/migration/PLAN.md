# Public Jeryu migration and cutover

`neverhuman/jeryu` becomes the development and release entrypoint for a
standalone forge. A normal public clone contains the application source;
Linux x86_64 users build and install it without Jain, a private forge,
personal Git configuration, credentials, or neighboring checkouts. Preserve
Apache-2.0 licensing, v5 release lineage, existing GitHub ancestry and every
immutable tag. The current implementation is a candidate; measured results
and remaining failures belong in [STATUS.md](STATUS.md).

Development belongs in one Cargo workspace and one npm workspace, each with
one lockfile. Keep the ten component directories as ownership and export
boundaries. The ordinary product path is `scripts/build.sh`,
`scripts/install.sh --from-source`, then `jeryu serve`; running every CI lane
is a release qualification task. SQLite remains the durable runtime for this
release. Runner installation is optional and must state its host requirements.

The largest design risks are competing authorities, private tool/dependency
routes, CI wrappers that assume a particular machine, and splits that only
build when their neighbors happen to exist. Copying directories to GitHub
would preserve those problems. Address them before authority cutover.

| Material in the original family | Final disposition and gate |
| --- | --- |
| `jeryu` portal | Monorepo root with source installation, contribution, release and CI entrypoints; retain existing public ancestry. |
| `jeryu-core`, `jeryu-cache`, `jeryu-ci-runner`, `jeryu-intelligence`, `jeryu-jira`, `jeryu-web`, `jeryu-tool`, `jeryu-tool-finder` | Corresponding `components/<name>` directories, with audited source history and deterministic downstream exports. Work retains its existing `jeryu-jira` identity. |
| `jeryu-deploy` | Component containing server, CLI, packaging and integration source; recheck concurrent source changes before publication. |
| `jeryu-release-ops` | Component containing release tooling; its original manifest remains actual release authority until protected handover to the root. |
| Family `AGENTS.md`, `ops`, `repos.manifest.toml` | Preserve original bytes; reconcile maintained instructions/tooling and generate runtime/consumer projections from one release authority. |
| Six repositories under `jeryu-redline` | Retain privately until canonical comparison, verified bundle restoration, explicit disposition and fresh producer/two-consumer proof report `cutover_eligible=true`. Public Redline comes from its canonical producer. |
| `dirty`, `.work`, runtime exports, stashes, untracked files, bundles and build output | Owner-only custody with inventories and restoration evidence. Recover unique source into reviewed commits; sensitive/runtime/build material stays outside public source. |
| Original checkouts after integration | Retain until exact source/ref/untracked accounting, writer handoff and verified preservation permit retirement. A directory name is not retirement authority. |

1. **Freeze and account for source.** Record exact branches, commits, trees,
   tags, stashes and untracked state under coordinated writer claims. Bundle
   all refs and preserve non-Git material privately, then restore and compare.
   Scan every selected public history/ref for credentials, private data,
   licenses and large blobs. Triage findings without printing their values.
   Preserve unsafe original graphs privately and map sanitized public history
   explicitly. Keep linear integration commits and source provenance in
   `imports.json`; record later component deltas separately. Re-probe each
   original checkout immediately before publication so concurrent work is not
   silently dropped.

2. **Finish the standalone candidate.** Internal Jeryu dependencies resolve
   to checked-out workspace packages with one identity per package. Preserve
   package names and existing version distinctions. Build the web application
   before the locked release binary and reject a missing embedded bundle.
   Verify source and binary digests during installation. Test data-directory
   precedence, any-directory startup, one-time local-admin bootstrap,
   durable SQLite state, and real API-backed CLI failures and operations.
   Regenerate browser contracts from their Rust owners. Publish verified
   immutable external dependencies and governed auditor artifacts before
   claiming anonymous builds. Cargo resolves workspace dependencies even when
   a particular product does not compile them: an optional flag alone does
   not establish a build without an external fetch.

3. **Port and prove the complete CI contract.** Use the same root commands
   locally and on GitHub. The retained proof inventory binds the original
   workflows, declarations and thresholds; it is not proof of equivalent
   coverage. Replace machine-specific wrappers with mapped portable commands
   for Rust formatting/Clippy/features/tests/docs, web checks/contracts/
   production/Storybook/accessibility/browser flows, storage migrations,
   cache poisoning, release evidence, dependency/license/history auditing,
   governed tool verification, and packaging. Preserve coverage and mutation
   thresholds. The privileged sandbox lane must actually execute excluded
   isolation/escape tests on a disposable capable Linux host, with admission
   checks and zero required skips. Fast contributor feedback may be separate
   from full release qualification; a green subset must not be advertised as
   complete CI. Keep an always-running aggregate that rejects failures,
   cancellations and missing required proofs.

4. **Qualify one exact source commit centrally.** First pass the full local
   matrix on a clean candidate. Repeat clone/build/install and runtime checks
   in a fresh unprivileged environment with empty caches, no credentials,
   no personal Git configuration and no adjacent repositories. Exercise
   authentication, Git push/clone, issues, protected PR review/check/merge,
   restart persistence, CLI operations and optional runner execution. Then
   push the audited candidate branch and run the same commands on GitHub.
   Read back every required job at that exact SHA. Independent review and a
   separate merger precede a forward-only merge; repeat qualification on
   merged main and the immutable release tag. Require review, linear history,
   enforced administrators and real required checks without weakening existing
   protections or manufacturing approvals/statuses.

5. **Publish maintained split mirrors.** Generate each export twice from
   the approved source commit and compare trees. Resolve standalone locks
   without changing external versions or duplicating Jeryu identities. Bind
   external Jeryu packages to the originating monorepo commit; build/test each
   export in an isolated Git clone. Deploy obtains embedded web assets from
   that same immutable source. Publish only qualified exports through
   protected forward-only updates, preserving existing public history/tags
   and source provenance. Create the missing Work repository after central
   qualification. Generated contribution instructions route changes to the
   monorepo; disable competing upstream writers during cutover.

6. **Hand over authority and relocate once.** Prepare the installed service's
   replacement manifest projection and validate it before moving paths.
   Protect and review the Release Ops-to-root authority handover, hosted
   downstream mirroring and Jain immutable consumer references, using the
   installed-boundary lifecycle wherever applicable. Refresh Redline's
   producer and both consumer proofs. At the maintenance gate, rename the
   single canonical checkout to `/home/ubuntu/jeryu-split/jeryu`, update its
   remaining consumers and verify the installed projection. No worktrees,
   copied source families or symlink shims. The chat file at the destination
   is coordination only until that rename. Source publication does not change
   the running service or activate GA metadata.

7. **Release and close custody accounting.** Publish central platform archives,
   checksums, signatures, SBOMs and provenance. A binary installer must reject
   failed verification before replacing an installed binary. Record the
   final disposition of every row above, including unique source recovered
   from private material and every original ref. Retire original checkouts
   only with preservation proof and explicit holder handoff; retain custody
   artifacts outside the public source and runtime directories.

The two-agent work split is one canonical monorepo writer plus independent
dependency publication, design review and exact-commit verification. Use
`/home/ubuntu/jeryu-split/AGENT_CHAT.md` frequently for challenges and results;
claims and releases also go through the guarded family coordination ledgers.
Passing implementation tests, public dependency availability, authority
handover and service activation are separate claims with separate evidence.
