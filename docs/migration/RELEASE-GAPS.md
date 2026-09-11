# Public release gap register

Release readiness is **PENDING**. A source correction, diagnostic score, PR
success, or published historical tag does not qualify protected `main`.
Stable version assignment and release-tag publication follow qualification.

This register reconciles the public candidate at
`da8868da9d7f4547321738d2a97bc4e1d98661d2` with the local pending work based on
`752f8ce680f549794d70a57d52d7f3c5791b5d35`. Public observations below were read
from GitHub on 2026-09-11. Subsequent executions must record their actual source,
workflow run and attempt; results for either predecessor are historical.

The corrective source is in [draft PR 66](https://github.com/neverhuman/jeryu/pull/66).
PR 65's later `a350a5e8` Runner family reference is also retained; its public
commit and tree were read back against the named immutable tag. This historical
reference does not qualify the Runner mirror or change tracked build inputs.
Public history through `5c127131` is separately preserved. Its Bubblewrap
prerequisite is integrated and its predecessor assertion was already corrected.
Its new environment-selected custody/executor skips and permission-denied
deletion exceptions are not accepted; these requirements remain fail-closed.

The root catalog helper's same-HEAD copy to `accepted-baseline.json` is also
superseded: it never authenticated a predecessor and wrote report outputs before
admission. Standard score now validates before copying any successful report.
The canonical PR gate and compatibility catalog entrypoint invoke the complete
auxiliary-proof gate, which remains required in the hosted matrix and fails
while authentic predecessor/proof admission is unavailable.

At `7ca21a00`, the dedicated-account runtime lane passes. Rust stops before
tests because the newly created auditor installation is group-writable. The
fresh dedicated home now clears inherited/default hosted ACLs and reasserts
0700, so descendant privacy follows the installer's existing umask and checks.
Full Rust qualification remains required at the corrected committed head.

## Current public evidence

At `c929d6ed`, [PR run 34567779982](https://github.com/neverhuman/jeryu/actions/runs/34567779982)
passed source, public, product, security, web, native sandbox and OCI. Audit,
auditor, auxiliary, Rust, runtime and splits failed; legacy was still running at
readback. The pinned auditor again reproduced correctly, then the root audit
failed at 64 with six hard findings. Runtime lacked ancestor traversal for its
dedicated UID; Rust still used the runner UID. Both now select the dedicated
identity, with traversal ACLs, and maintained commands serialize custody tests.
Runner and Deploy split failures remain unresolved; future runs retain their
child logs separately from private runtime custody.

The next working-source corrections passed all seven planner integration
tests, all 43 Codegraph targets and all 11 acquisition tests under isolated
serial execution, plus relevant warning-denied Clippy and workflow linters.
An initial acquisition filter ran zero tests; a two-thread retry passed 10 and
failed one custody cleanup. Both attempts remain retained and are superseded
only for the corrected serial command by the complete 11-test result.
Tool's complete `just security` passed with two exact historical synthetic
fingerprints excluded; a fresh secret-pattern canary still failed as required.
These are local targeted results, not final-head CI qualification.

[PR 66 run 34563986065, attempt 1](https://github.com/neverhuman/jeryu/actions/runs/34563986065)
executes the complete matrix at `e97aa6330832db8a270e5150a86ab65644e59ad4`.
Product, security, sandbox, web, source, OCI and public passed. Auxiliary,
auditor, audit, runtime, Rust and legacy failed; splits was still running at
the readback. The immutable builder check confused the OCI index digest with
the engine's image handle. It now admits the pinned repository digest and
Linux amd64 platform, then binds container inspection to the admitted handle.
All 12 Tool builder regressions, package warning-denied Clippy and workflow
linters pass locally. A real reproducible builder run remains required.

Runtime retained fixtures when hosted same-user process state was unreadable.
The workflow now prepares a dedicated unprivileged runtime UID and runs the
same maintained command with an empty credential environment. Cleanup and
live-handle checks remain strict. Its hosted result is pending.

At corrected `7b4b2ffb`, [PR run 34565427389](https://github.com/neverhuman/jeryu/actions/runs/34565427389)
reproduced and verified the pinned auditor executable through the complete
public build/receipt path. The following root audit failed at score 64 with
six hard findings. Runtime setup encountered toolchains copied from the host
home skeleton; it now requests an explicitly empty dedicated home. Neither
result is a qualification of current source or `main`.

The separate `audit-service` receiver now has 20 passing intake tests, including
five real HTTP/restart tests, and package warning-denied Clippy. Raw bytes commit
before HTTP acknowledgement; failed writes, duplicate signatures and replaced
databases cannot report successful reception. Product dependency inspection
confirms that `jeryu-cli` does not depend on the receiver package. Deployment,
complete queue dispatch, reconciliation and publication remain open.

[PR 65](https://github.com/neverhuman/jeryu/pull/65) remains open without an
approval. At its exact `5c881aaa` head,
[run 34554016443, attempt 1](https://github.com/neverhuman/jeryu/actions/runs/34554016443)
has nine failed verification jobs: source, Rust, web, runtime, splits, legacy,
auxiliary, audit and auditor. Public, security, product, sandbox and OCI passed
that attempt. The aggregate failed. These successes do not qualify the current
working source or resulting `main`.

The main branch protection readback retained one independent approval,
latest-push approval, stale-review dismissal, enforced administrators, strict
checks and linear history. The audit and auditor jobs are now explicitly
required alongside the aggregate and the other 12 lanes, all bound to the
observed GitHub Actions application (ID 15368). Readback verified all 15 checks
and retained review/administrator/history settings. No protection reduction
is permitted.

PR 65 subsequently advanced through seven commits to `73b60a03`. That history
is retained. Its API helper/fixture corrections and tool-lock naming changes
are integrated. Its environment-selected score-only audit, reduced auxiliary
lane, receipt substitution and permission-error cleanup exceptions are
replaced with the preceding complete admission requirements. Hosted environment
variables do not authenticate provenance or prove absent live consumers.

Four further public commits through `da8868da` update historical family tags
and the Runner's hosted shortcut. Their ancestry and tag references are
retained; Runner's hostile-PATH regression still exercises the complete
receipt guard. Builds use tracked source rather than replacing it from tags.

Local checks before that integration passed all 20 Runner OCI unit tests,
all four bootstrap interruption/custody tests, web lint (existing warnings
remain), and all-feature/all-target warning-denied Clippy for API, split-tool
and Runner. After integration, the serial isolated split-tool run passed 144
of 147 tests; three sandbox executor tests failed namespace setup. All 10 Tool
builder tests and all six public-candidate verifier tests passed, including
the hosted matching-binary/provenance regression. Selected Clippy also passed
for Tool. All 128 CI matrix/dispatch scenarios then passed in an exact-commit
disposable clone at `30da6fda`, under its isolated unprivileged owner. Clean
source readback and guarded clone removal passed. Reconciliation onto
`da8868da` changed only historical family references relative to that tested
tree; renderer, inventory, manifest, monorepo and public-preflight checks
passed again. These results are targeted evidence; complete final-head CI,
anonymous source installation and release qualification remain open.

## Source reconciliation

[Component PR dispositions](PR-RECONCILIATION.md) record the exact reviewed
Intelligence and Tool Finder heads, accepted storage/contracts work and
superseded policy/CI shortcuts. Original PRs remain open until protected merged
replacements exist.

The public commits `72035eaf` and `5c881aaa` remain in ancestry. The pending
intake, immutable package preparation, Cache source checks, builder diagnostics
and Runner denial changes were preserved and reapplied without conflicts.
The first compilation exposed prohibited unsafe operations in that pending
slice and an ambiguous map type. Safe Rust filesystem operations retain atomic
no-replace publication and FIFO refusal; the workspace unsafe-code prohibition
remains intact. Compilation and the complete owning suites must pass before
these source changes count as verified.

Disposition of public additions:

- Preserve structural extraction, public source acquisition, root workspaces,
  installation guidance and immutable tag history, subject to owning tests.
- Replace the `GITHUB_ACTIONS` auditor shortcut with the existing complete
  source/build/receipt bootstrap and generated verifier. A matching archive,
  executable digest and version alone cannot grant provenance or authority.
- Treat `family.lock.toml` as historical export evidence. Builds consume tracked
  monorepo source. The compatibility fetch command verifies it and cannot
  replace component directories from historical tags.
- Keep the original audit term policy and effective web lint behavior. The
  single `.mjs` ESLint configuration retains keyboard-scroll guidance and the
  added browser-storage restriction with explicit rule names.
- Historical badges are not current evidence. README links and the audit job
  must not present their numeric results as verified current-head scores.

## Required work and commands

“Implemented; verification pending” means source exists, with no qualification
claim. Missing maintained commands remain explicit gaps.

| ID | Requirement / owner | Maintained command or owning source | Current result / correction | PR |
| --- | --- | --- | --- | --- |
| R01 | Reviewed integrated candidate / monorepo | `scripts/ci.sh source` | Public ancestry retained; pending slice integrated. Inventory, generated consumers, manifest paths, monorepo check and 128 CI dispatch scenarios pass; complete hosted source lane pending. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R02 | Exact complete CI inventory / Deploy | `jeryu-split ci-required-check`; `scripts/check-required-ci.sh` | Five validator tests pass, covering exact repository/source/workflow/run/attempt and missing, duplicate, substituted or unsuccessful jobs. Hosted final-head execution pending. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R03 | Required audit work survives later pushes / CI | `.github/workflows/ci.yml`, `jankurai-score.yml` | Unique run/attempt concurrency groups and cancellation disabled. Service reconciliation remains separate. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R04 | Full Rust product matrix / components | `scripts/ci.sh rust` | Public refactors failed formatting. Formatting corrected; complete test and Clippy union remains required. | 65 |
| R05 | Browser journey, accessibility, performance / Web | `scripts/ci.sh web` | Public consolidation lost the effective keyboard-scroll lint setting. Restored original behavior; lint and full rendered matrix required. | 65; Web 1 |
| R06 | Bootstrap, credentials, Git, issues, Work, PRs, checks, merge, restart / Core+Deploy+Work | `scripts/ci.sh runtime`, `scripts/ci.sh product` | Bootstrap now prepares and syncs the one-time receipt before creating an account and resumes from that receipt after interruption. New restart/custody regressions and final-source complete journey remain pending. | 65 |
| R07 | Interrupted repository/Core-Work repair / Core+Deploy+Work | API repository/Work modules and owning tests | Core now atomically journals the original request/UUID, retains pending materialization and deletion identities, and supports exact retries. All 265 Core tests pass, including six new failure/reopen/migration regressions; API/Core Clippy passes. Browser/Git phase recovery, Core-Work repair and owning migration analysis remain required. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R08 | Backup/restore, upgrade/rollback, permissions, TLS / Deploy | `scripts/test-source-install.sh`; `docs/recovery.md` | Dedicated-account hosted runtime passes at 7ca21a00 and 9be7395e with strict cleanup retained. Cross-version upgrade/rollback and remote TLS drills remain required. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R09 | Anonymous source installation / Deploy | `scripts/build.sh`; `scripts/install.sh --from-source` | Build no longer reconstructs component source or adds a Python prerequisite for that operation. Final anonymous empty-cache source journey pending. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R10 | Full dependency closure / Deploy | `jeryu-split dependency-inputs` | Version-aware inventory implemented. Exact revision/hash/license/capability closure and public acquisition evidence remain required. | 65 |
| R11 | Public Cache scanners, advisories and API baseline / Cache | Cache `ops/ci/security.sh`, `ops/ci/api-compat.sh` | Source guards integrated. Full public scanner producer, pinned advisory DB and immutable API baseline remain unqualified. | 65; Cache 1 |
| R12 | Runner image ownership and immutable inputs / Runner | Runner `images/agent-sandbox`, `scripts/test-oci.sh` | Product image and duplicate Deploy route need complete reconciliation. Native and OCI-probe successes do not qualify the product image. | 65 |
| R13 | Enforced network denial / Runner | `cargo test -p jeryu-runner-oci --lib` | All 20 unit tests pass. Requested and effective Deny required; session bridge override removed; exact dispatch argv covered. Product image remains separately unqualified. | 65 |
| R14 | Auditor public acquisition / Tool | `scripts/ci.sh auditor` | All 12 builder regressions pass. Real hosted reproduction and candidate receipt verification pass at 7b4b2ffb; subsequent root audit fails score 64/hard 6. Independent producer qualification remains separate. | [66](https://github.com/neverhuman/jeryu/pull/66); Tool 1 |
| R15 | Independent producer qualification / Jankurai owners | Producer owning protected quality gates | Classification, readonly behavior, child status and SVG corrections must be independently qualified before consumer adoption. No substitute binary or repin is admitted. | Owning producer PRs |
| R16 | Complete root/component/export/dependency census / Deploy | `scripts/ci.sh audit` | Every scope is recorded; producer, governing policy and execution admission still prevent a qualified aggregate. | 65 |
| R17 | Authentic predecessors and auxiliary proofs / Tool+Release Ops | `scripts/ci.sh auxiliary`; Tool `ops/ci/tool-adoption.sh` | Candidate-derived baseline is insufficient. Protected predecessor, complete changed paths/hunks, proofbind/proofmark/configuration/conformance remain open. | 65; Tool 1 |
| R18 | Coverage, mutation and stricter floors / components | Existing owning quantitative gates in `CI-COVERAGE.md` | Preserve all floors, zero hard findings/caps, soft limits, coverage/mutation/size/performance requirements. No waiver or lowered policy is admitted. | 65 |
| R19 | Authenticated durable webhook intake / Deploy | `jeryu-split audit-service`; `audit-intake` | Separate bounded HTTP receiver implemented with raw-byte HMAC, durable acknowledgement, private held configuration and startup replay. All 20 intake tests pass, including five real HTTP/restart tests. Deployed enrollment remains unqualified. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R20 | Persistent queue, ancestry, retries and reconciliation / service | `audit-plan`, `audit-ledger`, intake store | Seven planner tests pass, now including every PR/release ancestor and both merge parents. Local accounting and the receiver exist. Dispatcher, backfill, outage/missed-delivery drills and enrollment remain open. | [66](https://github.com/neverhuman/jeryu/pull/66); service followup |
| R21 | Authenticated executor results and publication / service | `jeryu-split audit-package` | Private immutable preparation exists and cannot grant verified PASS. Workflow/run/attempt/source/policy/exit/hash authentication and publication credentials remain separate open boundaries. | [66](https://github.com/neverhuman/jeryu/pull/66) |
| R22 | Immutable JSON/Markdown/SVG/provenance and Pages / service | `docs/migration/AUDIT-PUBLICATION.md` | Trusted sanitized publisher, first-party renderer, Pages artifact workflow and public reachability unqualified. | New service work |
| R23 | Race-safe live cards and README enrollment / service | `jeryu-split audit-readme` | Marker validator exists; pending publication, monotonic head pointers and separate maintained/pinned cards need service integration. | 65 |
| R24 | Deterministic exports and mirror divergence / Deploy | `scripts/ci.sh splits`; `prepare-mirror-update` | Repeated export preparation exists; actual exported source must independently build/test/audit before protected forward publication. | Component followups |
| R25 | All accepted PRs and consistent protections / maintainers | Eight-PR inventory below | None superseded here. Preserve accepted work and record merged replacement before closing duplicates. Independent reviewer and merger required. | All below |
| R26 | Resulting-main qualification / maintainers | Complete `scripts/ci.sh all` at every maintained final head | Open. PR execution alone never qualifies merge output. Optional Redline remains separate from SQLite readiness. | All below |
| R27 | Signed candidate archives and binary installation / Release Ops+Deploy | Central artifact/installer qualification | Signatures, checksums, SPDX/CycloneDX, provenance, install evidence and tampering/platform rejection remain open. Binary install stays closed. | Release followup |
| R28 | Exact original-directory dispositions / family owners | `DISPOSITION.md`; preservation/restoration gates | Original checkouts, duplicate Redline and failed-build custody retained. No directory retirement or installed-service activation performed. | Separate custody work |

## Open public PRs

The original eight have advanced, and Deploy 1 plus corrective PR 66 bring the
current inventory to ten open PRs across all eleven repositories. Full
accepted-change dispositions still require review and merged replacements.

| Repository / PR | Exact observed head | Disposition |
| --- | --- | --- |
| [jeryu 65](https://github.com/neverhuman/jeryu/pull/65) | `4367868b3dc208ef34256ce585cd52e6fcb1a280` | History preserved; accepted Git HEAD compatibility corrected in production and fixture; hosted-only skips superseded |
| [jeryu 66](https://github.com/neverhuman/jeryu/pull/66) | `c929d6ed0be562c3bba49e332fe19ac31c492e29` | Draft corrective source; complete matrix remains failed |
| [Cache 1](https://github.com/neverhuman/jeryu-cache/pull/1) | `a1571865b27ebce9b74154aedd265610df2eaae4` | Open; reconcile complete census with thin score CI |
| [Deploy 1](https://github.com/neverhuman/jeryu-deploy/pull/1) | `c8f43edead0b8d276e9e13ed517823a223f6807a` | Newly open; accepted work and standalone qualification require reconciliation |
| [Work 1](https://github.com/neverhuman/jeryu-jira/pull/1) | `9ad978a9fbd73c830e3ad0884eb74981d5e89460` | Open; reconcile complete census with thin score CI |
| [Intelligence 1](https://github.com/neverhuman/jeryu-intelligence/pull/1) | `ca6ae2934600c95e2471ea038877733472928681` | Storage and corrected generated contract integrated; dispositions recorded; protected replacement pending |
| [Tool 1](https://github.com/neverhuman/jeryu-tool/pull/1) | `d904e09cb1f49b3dc0feb47da2cde6b97b5f4391` | Open; require full provenance and authentic predecessor |
| [Tool Finder 1](https://github.com/neverhuman/jeryu-tool-finder/pull/1) | `ee0e0ca335ad1f7c4adaa4ffd7d15e387160dffa` | Product source already identical; boundary guidance integrated; stronger controls retained |
| [Web 1](https://github.com/neverhuman/jeryu-web/pull/1) | `b600754ec4d583d5a6ba29a8a321b6d6c5986bdd` | Open; reconcile complete census with thin score CI |
| [Release Ops 1](https://github.com/neverhuman/jeryu-release-ops/pull/1) | `008c707e1341a0bce85a12c518d2f899d9dbcfc6` | Open; preserve release authority and complete checks |

Core and Runner had no open public PRs at this readback. Their source
and final-head qualification obligations still apply.
