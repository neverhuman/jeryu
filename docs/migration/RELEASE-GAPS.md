# Public release gap register

Release readiness is **PENDING**. A source correction, diagnostic score, PR
success, or published historical tag does not qualify protected `main`.
Stable version assignment and release-tag publication follow qualification.

This register reconciles the public candidate at
`da8868da9d7f4547321738d2a97bc4e1d98661d2` with the local pending work based on
`752f8ce680f549794d70a57d52d7f3c5791b5d35`. Public observations below were read
from GitHub on 2026-09-11. Subsequent executions must record their actual source,
workflow run and attempt; results for either predecessor are historical.

## Current public evidence

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
| R01 | Reviewed integrated candidate / monorepo | `scripts/ci.sh source` | Public ancestry retained; pending slice integrated. Inventory, generated consumers, manifest paths, monorepo check and 128 CI dispatch scenarios pass; complete hosted source lane pending. | 65 followup |
| R02 | Exact complete CI inventory / Deploy | `jeryu-split ci-required-check`; `scripts/check-required-ci.sh` | Five validator tests pass, covering exact repository/source/workflow/run/attempt and missing, duplicate, substituted or unsuccessful jobs. Hosted final-head execution pending. | 65 followup |
| R03 | Required audit work survives later pushes / CI | `.github/workflows/ci.yml`, `jankurai-score.yml` | Unique run/attempt concurrency groups and cancellation disabled. Service reconciliation remains separate. | 65 followup |
| R04 | Full Rust product matrix / components | `scripts/ci.sh rust` | Public refactors failed formatting. Formatting corrected; complete test and Clippy union remains required. | 65 |
| R05 | Browser journey, accessibility, performance / Web | `scripts/ci.sh web` | Public consolidation lost the effective keyboard-scroll lint setting. Restored original behavior; lint and full rendered matrix required. | 65; Web 1 |
| R06 | Bootstrap, credentials, Git, issues, Work, PRs, checks, merge, restart / Core+Deploy+Work | `scripts/ci.sh runtime`, `scripts/ci.sh product` | Bootstrap now prepares and syncs the one-time receipt before creating an account and resumes from that receipt after interruption. New restart/custody regressions and final-source complete journey remain pending. | 65 |
| R07 | Interrupted repository/Core-Work repair / Core+Deploy+Work | API repository/Work modules and owning tests | Atomic linked Work creation exists. Complete durable recovery, visible pending repairs and idempotent retries remain unqualified. | 65 |
| R08 | Backup/restore, upgrade/rollback, permissions, TLS / Deploy | `scripts/test-source-install.sh`; `docs/recovery.md` | Public runtime failed fixture cleanup on inaccessible same-user process state. Retain failed custody; qualify isolated unprivileged execution and cross-version/TLS drills. | 65 |
| R09 | Anonymous source installation / Deploy | `scripts/build.sh`; `scripts/install.sh --from-source` | Build no longer reconstructs component source or adds a Python prerequisite for that operation. Final anonymous empty-cache source journey pending. | 65 followup |
| R10 | Full dependency closure / Deploy | `jeryu-split dependency-inputs` | Version-aware inventory implemented. Exact revision/hash/license/capability closure and public acquisition evidence remain required. | 65 |
| R11 | Public Cache scanners, advisories and API baseline / Cache | Cache `ops/ci/security.sh`, `ops/ci/api-compat.sh` | Source guards integrated. Full public scanner producer, pinned advisory DB and immutable API baseline remain unqualified. | 65; Cache 1 |
| R12 | Runner image ownership and immutable inputs / Runner | Runner `images/agent-sandbox`, `scripts/test-oci.sh` | Product image and duplicate Deploy route need complete reconciliation. Native and OCI-probe successes do not qualify the product image. | 65 |
| R13 | Enforced network denial / Runner | `cargo test -p jeryu-runner-oci --lib` | All 20 unit tests pass. Requested and effective Deny required; session bridge override removed; exact dispatch argv covered. Product image remains separately unqualified. | 65 |
| R14 | Auditor public acquisition / Tool | `scripts/ci.sh auditor` | Complete source/build/receipt path retained. Hosted builder image identity mismatch remains a real failed result; repair must retain immutable provenance. | 65; Tool 1 |
| R15 | Independent producer qualification / Jankurai owners | Producer owning protected quality gates | Classification, readonly behavior, child status and SVG corrections must be independently qualified before consumer adoption. No substitute binary or repin is admitted. | Owning producer PRs |
| R16 | Complete root/component/export/dependency census / Deploy | `scripts/ci.sh audit` | Every scope is recorded; producer, governing policy and execution admission still prevent a qualified aggregate. | 65 |
| R17 | Authentic predecessors and auxiliary proofs / Tool+Release Ops | `scripts/ci.sh auxiliary`; Tool `ops/ci/tool-adoption.sh` | Candidate-derived baseline is insufficient. Protected predecessor, complete changed paths/hunks, proofbind/proofmark/configuration/conformance remain open. | 65; Tool 1 |
| R18 | Coverage, mutation and stricter floors / components | Existing owning quantitative gates in `CI-COVERAGE.md` | Preserve all floors, zero hard findings/caps, soft limits, coverage/mutation/size/performance requirements. No waiver or lowered policy is admitted. | 65 |
| R19 | Authenticated durable webhook intake / Deploy | `jeryu-split audit-intake` | Raw-byte HMAC and durable local intake integrated. HTTP deployment and service authority are not implemented by this CLI. | 65 followup |
| R20 | Persistent queue, ancestry, retries and reconciliation / service | `audit-plan`, `audit-ledger`, intake store | Local accounting exists. Maintainer receiver/dispatcher, backfill, restart/outage/missed-delivery drills and enrollment remain open. | New service work |
| R21 | Authenticated executor results and publication / service | `jeryu-split audit-package` | Private immutable preparation exists and cannot grant verified PASS. Workflow/run/attempt/source/policy/exit/hash authentication and publication credentials remain separate open boundaries. | 65 followup |
| R22 | Immutable JSON/Markdown/SVG/provenance and Pages / service | `docs/migration/AUDIT-PUBLICATION.md` | Trusted sanitized publisher, first-party renderer, Pages artifact workflow and public reachability unqualified. | New service work |
| R23 | Race-safe live cards and README enrollment / service | `jeryu-split audit-readme` | Marker validator exists; pending publication, monotonic head pointers and separate maintained/pinned cards need service integration. | 65 |
| R24 | Deterministic exports and mirror divergence / Deploy | `scripts/ci.sh splits`; `prepare-mirror-update` | Repeated export preparation exists; actual exported source must independently build/test/audit before protected forward publication. | Component followups |
| R25 | All accepted PRs and consistent protections / maintainers | Eight-PR inventory below | None superseded here. Preserve accepted work and record merged replacement before closing duplicates. Independent reviewer and merger required. | All below |
| R26 | Resulting-main qualification / maintainers | Complete `scripts/ci.sh all` at every maintained final head | Open. PR execution alone never qualifies merge output. Optional Redline remains separate from SQLite readiness. | All below |
| R27 | Signed candidate archives and binary installation / Release Ops+Deploy | Central artifact/installer qualification | Signatures, checksums, SPDX/CycloneDX, provenance, install evidence and tampering/platform rejection remain open. Binary install stays closed. | Release followup |
| R28 | Exact original-directory dispositions / family owners | `DISPOSITION.md`; preservation/restoration gates | Original checkouts, duplicate Redline and failed-build custody retained. No directory retirement or installed-service activation performed. | Separate custody work |

## Open public PRs

These are the eight open PRs observed across all eleven repositories. Full
accepted-change dispositions still require review and merged replacements.

| Repository / PR | Exact observed head | Disposition |
| --- | --- | --- |
| [jeryu 65](https://github.com/neverhuman/jeryu/pull/65) | `73b60a03f932659a847910da9b86182770cd2711` | Integrated locally; stronger admission restored, followup and independent approval pending |
| [Cache 1](https://github.com/neverhuman/jeryu-cache/pull/1) | `d6d9cf0b3b1fd8160679044a93ea7243335197c4` | Open; reconcile complete census with thin score CI |
| [Work 1](https://github.com/neverhuman/jeryu-jira/pull/1) | `86024ece2b3954859580ff746d801b39264b453f` | Open; reconcile complete census with thin score CI |
| [Intelligence 1](https://github.com/neverhuman/jeryu-intelligence/pull/1) | `c5c0ab442d96dbf9b5bec8df2e3cd553e1c6f4c3` | Open; inspect accepted source and audit policies |
| [Tool 1](https://github.com/neverhuman/jeryu-tool/pull/1) | `2c971826ffbdba9f1b13d56ffc3c49923e4dc611` | Open; require full provenance and authentic predecessor |
| [Tool Finder 1](https://github.com/neverhuman/jeryu-tool-finder/pull/1) | `6bfd62cc9dcf0692dd52373a129c495f9a5f817f` | Open; inspect accepted source and audit policies |
| [Web 1](https://github.com/neverhuman/jeryu-web/pull/1) | `cf1c20aeec4505f60ce749df38d39fda138683b2` | Open; reconcile complete census with thin score CI |
| [Release Ops 1](https://github.com/neverhuman/jeryu-release-ops/pull/1) | `f660497972a2db2774afc7855a027a4ffd3c4018` | Open; preserve release authority and complete checks |

Core, Runner and Deploy had no open public PRs at this readback. Their source
and final-head qualification obligations still apply.
