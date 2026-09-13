# Dependencies and audit status

**PENDING:** no current-head or release-pinned score in this dashboard is
qualified. Historical scores do not qualify newer source. The live SVG
publisher and protected audit admission remain unfinished.

| Repository | Role | Current-head score | Version used by Jeryu | Minimum | CI |
| --- | --- | --- | --- | --- | --- |
| Jeryu | Root application and workspaces | Pending | Candidate source | 85 | Pending complete matrix |
| Core | Git and domain libraries | Pending | Root workspace | 85 | Pending standalone proof |
| Cache | Build cache | Pending | Root workspace | 91 | Pending security/API proof |
| Runner | Optional execution | Pending | Root workspace | 91 | Pending complete proof |
| Intelligence | Reviews and Codegraph | Pending | Root workspace | 85 | Pending standalone proof |
| Work (`jeryu-jira`) | Work items | Pending | Root workspace | 91 | Pending standalone proof |
| Web | Browser and UX tooling | Pending | Root npm workspace | 85 | Pending final-source proof |
| Tool | Auditor acquisition | Pending | Root workspace | 85 | Pending protected proof |
| Tool Finder | Tool discovery | Pending | Root workspace | 85 | Pending protected proof |
| Deploy | Server, CLI and control tooling | Pending | Root workspace | 85 | Pending complete proof |
| Release Ops | Release libraries | Pending | Root workspace | 85 | Pending authority handover |
| [Jankurai](https://github.com/neverhuman/jankurai/tree/b88562fdb124aa86dedd70ab972e7d0d87e58be1) | Required CI auditor | Pending | `b88562fdb124aa86dedd70ab972e7d0d87e58be1` | 85 | Producer qualification open |

The application locks have no external first-party Git dependencies. Cargo
resolves Jeryu packages once through the root workspace. Ordinary installation
uses bundled SQLite and does not install the auditor or runner image.
Crates and npm dependencies retain their locked checksums/integrities and the
required vulnerability, license and source checks.

The [revision-bound repository census](migration/dependency-census-679d9cff.json)
records the source trees, public mirror heads, source inputs and readbacks for
`679d9cff`. Public repository availability is separate from a build or audit pass.
The [audit enrollment](../agent/audit-repositories.json) records the supporting
sources selected for execution. It does not grant release authority. The newer
Jankurai hub and its fourteen supporting repositories are prospective producer
inputs; they remain separate from the pinned producer until qualified adoption.
Versioned enrollment also records seven additional revisions consumed by those
prospective components' standalone locks. Different revisions of the same
repository receive separate census rows. Exact Cargo URL spellings remain
part of dependency identity; ownership aliases do not merge package sources.

The generated observation command is
`cargo run --locked -p jeryu-split-tool --bin jeryu-split -- dependency-inputs`.
It reads declared Cargo/npm workspaces and locks, tool manifests, workflow
references, image recipes and selected entrypoints, and emits input hashes,
known dependency identities and concrete unresolved records. It continues
after individual malformed inputs. Script execution and external producer
closures remain unresolved until their own adapters and evidence exist.

Inspect its `complete_for` results separately for application installation,
SQLite release audits, standalone mirrors, optional images and Redline. Its
command exit also includes malformed optional inputs and is not a SQLite
release gate. These observations do not establish public availability,
license sufficiency, build success or passing audits.
The first executed observation read 188 inputs and emitted 2,284 observations
with identical output on replay. Installation, audit, mirror and image inputs
still have explicit unresolved records. Redline's parsed-input closure does
not establish its independent compatibility or retirement proof.

Redline compatibility and original Redline retirement remain optional for SQLite
release eligibility. Missing public optional tags remain visible. The optional
agent image still depends on an unqualified Jekko distribution; the observed
public `@jeryu/jekko-cli` package endpoint returned 404.

Run `bash scripts/ci.sh audit` for the maintained census. It reports every enrolled
scope even when bootstrap, source acquisition or an earlier audit fails. Each
attempt uses private output outside the source tree; failures remain available
for review. Component, published mirror and external dependency results are
distinct. Missing source is `source_unavailable`, not a numeric score.

External audit sources now have an anonymous acquisition adapter. Supporting
dependencies require an exact commit; an unpinned standalone mirror may resolve
its declared maintained branch and records that observation separately. Each
scope fetches only the selected commit's full ancestor graph, verifies Git
objects, and creates a disposable `git clone --no-local` from that verified
input. Receipts retain the public URL, full commit/tree and object-set digest.
No personal Git configuration or credentials enter acquisition. Failed sources
and cleanup refusals remain private for inspection. Actual public-origin
execution of this adapter remains a qualification obligation.

The current census runs the pinned producer in a read-only Linux filesystem and
records report, source, policy, lockfile, executable and receipt hashes. Linux
Bubblewrap, GNU timeout, Git and `prlimit` are executor prerequisites. The independent protected
predecessor adapter is still pending, so the required audit lane fails closed.
Supplying a Git SHA for policy comparison cannot authenticate that predecessor.

`jeryu-split audit-plan --source-repo PATH --request FILE` prepares complete Git
graph work, including intermediate commits, branch lifecycle and retries. It
does not start audits, persist a hosted queue, publish checks or qualify an
event as audited. Automatic event enrollment, reconciliation and the trusted
publisher remain open in [current status](migration/STATUS.md).

The [local audit ledger](migration/AUDIT-LEDGER.md) preserves plans, attempts,
failures, retries and explicit closure acknowledgements. Trusted hosted intake,
execution admission and publication remain unfinished integrations.

The [private publication preparer](migration/AUDIT-PUBLICATION.md) binds audit
observations and their inputs in immutable owner-only bundles. It preserves
PENDING, FAIL and ERROR states while keeping every publication admission gate
closed. It does not publish score cards or authenticate supplied evidence.

`jeryu-split audit-readme --readme README.md --image-url URL --report-url URL`
checks Jankurai's managed marker block; `--write` inserts or updates that block.
It preserves surrounding text and rejects ambiguous markers and unsafe links.
Component and standalone scopes require different URLs. Only links under the
Jeryu Pages site or generated evidence branch are accepted. This helper does
not verify public delivery or manufacture audit results; enrollment awaits the
qualified publisher and renderer.
