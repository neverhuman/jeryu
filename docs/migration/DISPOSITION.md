# Original sources and retained support material

**No retirement is authorized by this catalog.** It records the eleven original
Jeryu checkouts, six duplicate Redline checkouts and known support groups.
Cleanup and optional Redline qualification do not block an independently
qualified SQLite release. Source qualification remains governed by
[current status](STATUS.md).

The checkout observation on **2026-09-10 at 19:30 UTC** read only directory
metadata, Git branch/commit/tree, tracked-status counts, ref counts and stash
counts. All seventeen original checkout paths remained physical directories.
Untracked and ignored material, live consumers, open handles and restoration
were not inspected. A zero tracked-status count does not mean a checkout is
clean or safe to remove. Exact private paths and identities remain in the
owner-only observation receipt, outside public source.

Paths below are relative to the original `jeryu-split` family container, which
is not a Git repository. The original `jeryu-release-ops/repos.manifest.toml`
remains release authority; the candidate monorepo manifest does not replace it.
An authority owner is a repository identity, not an authenticated current writer
or deletion approver. Individual holders and retirement custodians remain
unresolved unless a fresh stopped-head handoff identifies them.

A subsequent coordination entry at **20:05 UTC on 2026-09-10** records verified
preservation/restoration and an authorized source-custody transfer for the
original Core and Deploy checkouts to the hosted runtime implementation owner.
Both are active source inputs again. That handoff supersedes the unresolved
writer observation below; their later changes require a new comparison before
monorepo integration. It does not authorize either checkout's retirement.

## Original Jeryu checkouts

The [initial imports](imports.json), [later imports](import-updates.json),
[Core source mapping](core-successor-92827e5.tsv),
[Git source mapping](gitd-f10df129.tsv) and
[Runner source mapping](runner-48406aff.tsv) preserve prior source accounting.
They do not account automatically for subsequent refs, stashes or dirty state.

| Original checkout | Authority owner | Observed HEAD | Current disposition and missing evidence |
| --- | --- | --- | --- |
| `jeryu` | `jeryu/jeryu` | `30617123` | Active canonical monorepo candidate; retain at its current location. Three tracked-status rows observed under the current writer. Complete source qualification and protected handover remain pending. |
| `jeryu-cache` | `jeryu/jeryu-cache` | `f12af2dc` | Retained original library with recorded import; no current stopped-head or complete retirement admission. |
| `jeryu-ci-runner` | `jeryu/jeryu-ci-runner` | `48406aff` | Retained original; manifest runtime authority is `shadow-only`. Reconcile all later source/ref state and prove runner/deploy parity before retiring standalone artifacts. |
| `jeryu-core` | `jeryu/jeryu-core` | `d0952ff9` | Retained original library; one tracked-status row observed. Identify its current writer, compare later commits and working state, then integrate or preserve unique changes through review. |
| `jeryu-deploy` | `jeryu/jeryu-deploy` | `b388edc5` | Retained original; runtime authority is `shadow-only`. Seventeen tracked-status rows and two stashes observed. Their source accounting, holder handoff and runtime parity remain open. |
| `jeryu-intelligence` | `jeryu/jeryu-intelligence` | `6fb845c5` | Retained original library with recorded import; no current stopped-head or complete retirement admission. |
| `jeryu-jira` (Work) | `jeryu/jeryu-jira` | `1e89e09c` | Retained original library with recorded import; preserve the technical identity and qualify its standalone mirror before any original retirement. |
| `jeryu-release-ops` | `jeryu/jeryu-release-ops` | `7cb07826` | Active original control plane and release authority. Retain until a protected authority handover and generated compatibility projections are admitted. |
| `jeryu-tool` | `jeryu/jeryu-tool` | `6be9c4a5` | Retained original library and governed-tool source; compare all refs/state and resolve installed-tool consumers before retirement. |
| `jeryu-tool-finder` | `jeryu/jeryu-tool-finder` | `66980b33` | Retained original library. The original manifest still records identity binding as `pending`; resolve that binding and complete holder/source accounting. |
| `jeryu-web` | `jeryu/jeryu-web` | `37f0ca57` | Retained original: manifest inventory is `active`, with runtime authority `retirement-pending`. Browser parity, protected changes and the maintenance gate precede archival/removal from active inventory. |

The eight component checkouts other than Core and Deploy had no tracked-status
rows in this observation. None received a stopped-head handoff or new retirement
approval. Before retiring any original, compare every ref/tag/stash and tracked,
untracked and ignored state against the monorepo and private preservation;
recover unique source through review; restore and verify complete preservation;
then recheck exact paths, links, mounts, handles and live holders. Record admission
in the governing coordination ledgers and peer log before removing only the
admitted checkout.

## Duplicate Redline checkouts

Canonical source and evidence remain under Jain's `jain-redline` family. These
six `jeryu-redline/` repositories are retained historical inputs, never build
authority. All six observed checkouts were shallow and had zero tracked-status
rows; untracked state was not inspected.

| Retained checkout | Governing source owner / role | Observed HEAD | Missing retirement evidence |
| --- | --- | --- | --- |
| `jeryu-redline/redline` | Canonical `jeryu/redlineDB`, public hub | `cdb1c5a4` | Fresh full canonical comparison, complete preservation readback and two-consumer admission. |
| `jeryu-redline/redline-core` | Canonical `jeryu/redline-core`, engine | `3567bdce` | Same; bind the canonical immutable engine tag rather than this duplicate tip. |
| `jeryu-redline/redline-testing` | Canonical `jeryu/redline-testing`, parity harness | `4a449d8d` | Same; historical comparison alone does not admit retirement. |
| `jeryu-redline/redline-web` | Canonical `jeryu/redline-web`, observability console | `09fd93be` | Same; preserve all later canonical history and original refs independently. |
| `jeryu-redline/redline-split-ops` | Canonical `jeryu/redline-split-ops`, control plane | `d698fa18` | Same; only the canonical control plane produces consumer evidence and locks. |
| `jeryu-redline/redline-central` | Canonical Redline family; specific maintained role and retirement custodian unresolved | `bd6bb421` | Also reconcile this historical identity: it is absent from the current canonical manifest's active repository/control-plane entries. |

The [preserved history](history/STATUS-2026-09-10-679d9cff.md) reports successful
restoration using both shallow bundles and preserved shallow-boundary files,
and a prior canonical comparison of tips/reflog commits. Neither was repeated
here. A bundle alone is insufficient for those shallow histories. Retirement
still requires current canonical comparison, verified restoration and a fresh
unwaived proof with `cutover_eligible=true`, binding Jain and Jeryu consumers.

## Support and custody groups

Counts below identify historical custody groups, not a new recursive directory
census. Historical restoration/removal is reported from the preserved status;
it does not authorize additional cleanup. Unknown ownership remains explicit.

| Material | Owner / custodian | Disposition and missing evidence |
| --- | --- | --- |
| Family `AGENTS.md`, `ops/`, legacy `repos.manifest.toml` and coordination records | Release Ops authority; current file custodians unresolved | Retained transition inputs. Preserve original bytes; admit generated projections or a thin pointer from the protected authority rather than maintain competing manifests. |
| Public migration manifests, root guidance, workflows and import mappings | `jeryu/jeryu` | Active historical evidence in `docs/migration/`; retain source mappings and distinguish archived configuration from current workspace inputs. |
| Cache source qualification unit `g98vqpbk` | Prior public-Jeryu qualification coordinator; current custodian unresolved | Original root still present with its recorded owner-only identity. Reviewed metadata records 26 internal hard-link pairs. The preservation proposal is reviewed; whole-root archive/restore execution and separate retirement admission remain missing. |
| Cache temporary qualification unit `IaFGwVJJ` | Prior public-Jeryu qualification coordinator; current custodian unresolved | Original root still present with its recorded owner-only identity. Preserve the entire temporary/resolver/partial-output unit; verify bytes and link topology with the source unit before separate retirement. |
| 343 retained session checkouts | Runtime workcell/WarmPool authority; current lease owner and custodian unresolved | Historical private archives cover 45,598 files and 36,811,805,404 bytes, with restoration reported complete. Originals remain retained; complete live-reference and private WarmPool lease exclusion is unresolved. No current membership recount or lease query occurred here. |
| `.work`, runtime exports, installed SQLite/SPA/service inputs and credentials | Installed-service operator; current named custodian unresolved | Retain required runtime state privately. Do not inspect or publish contents as part of this catalog. Consumer/parity evidence and a governed maintenance window precede any retirement or relocation. |
| Private bundles, archives, patches and recovery reports | Existing preservation custodian; current named owner unresolved | Retained recovery evidence outside public source. Maintain owner-only artifacts, verified restore instructions and receipts; a source import does not make archives disposable. |
| Active build targets/caches, current qualification scratch and unknown-owner fragments | Respective producer or runtime consumer; unresolved until identified | Retain. Establish exact membership, stopped processes, ownership and live consumers before proposing bounded preservation or cleanup. |
| Seven partial-source copies, 72 abandoned test directories, two empty support directories and the six-file `dirty/` artifact group | Historical cleanup coordinator; recovery custodian unresolved | Recorded retired after preservation/restoration. Recovery retains 214 partial source files and all 107 declared patch-target blobs; original canonical checkouts were excluded. |
| 18 Native/phase4 test roots and seven Cache scenario roots | Owning Runner/Cache test producers; recovery custodian unresolved | Recorded retired after restoration and exact-path checks. Private recovery includes the 40 Cache fixture files. |
| 476 Work database fixtures | Work test producer; recovery custodian unresolved | Recorded retired after individual admission; 19,496,960 bytes remain in private preservation. |
| 5,034 Web database fixtures | Deploy WebState test producer; recovery custodian unresolved | Recorded retired after individual admission; 278,851,584 bytes remain in private preservation. |
| Four failed qualification roots from `DTRUeeXO` | Prior qualification coordinator; recovery custodian unresolved | Recorded archived, restored and retired separately; retain all four private archives and original failed qualification evidence. |
| Peer-deleted qualification checkouts | Deleting peer / custody owner unresolved | Evidence gap remains: complete deletion-time dirty-state inventories/restoration receipts are absent. The SQLite feature patch is recoverable from immutable history; that does not establish complete preservation or unique source loss. |

The two Cache units were only statted at their roots for this observation. The
reviewed metadata and preservation proposal remain preparatory evidence; no
archive, restoration, source checkout removal or runtime change was executed.
