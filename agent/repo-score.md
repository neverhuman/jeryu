# jankurai Repo Score

- Standard: `jankurai`
- Auditor: `1.5.1`
- Schema: `1.9.0`
- Paper edition: `2026.05-ed8`
- Target stack ID: `rust-ts-vite-react-redline-bounded-python`
- Target stack: `Rust core + TypeScript/React/Vite + PostgreSQL + generated contracts + exception-only Python AI/data service`
- Repo: `.`
- Run ID: `1779912505`
- Started at: `1779912505`
- Elapsed: `13311` ms
- Scope: `full`
- Raw score: `75`
- Final score: `64`
- Decision: `advisory`
- Minimum score: `85`
- Caps applied: `non-optimal-product-language-found, vibe-placeholders-in-product-code, fallback-soup-in-product-code, future-hostile-dead-language-in-product-code, generated-zone-mutation-risk, direct-db-access-from-wrong-layer, false-green-test-risk, streaming-runtime-drift, typescript-bad-behavior, ci-bad-behavior, web-security-bad-behavior, ci-local-parity`

## Hard Rule Caps

| Rule | Max Score | Applied |
| --- | ---: | --- |
| `no-root-agent-instructions` | 75 | no |
| `no-one-command-setup-or-validation` | 70 | no |
| `no-deterministic-fast-lane` | 65 | no |
| `no-security-lane-on-high-risk-repo` | 60 | no |
| `generated-contracts-or-public-api-drift-untested` | 80 | no |
| `python-direct-product-truth-or-db-ownership` | 72 | no |
| `no-secret-or-dependency-scanning-in-ci` | 78 | no |
| `no-jankurai-audit-lane-in-ci` | 82 | no |
| `jankurai-required-tool-ci-evidence-gap` | 88 | no |
| `non-optimal-product-language-found` | 74 | yes |
| `too-much-python-in-product-surface` | 72 | no |
| `boundary-reclassification-evidence-gap` | 72 | no |
| `vibe-placeholders-in-product-code` | 68 | yes |
| `fallback-soup-in-product-code` | 70 | yes |
| `future-hostile-dead-language-in-product-code` | 64 | yes |
| `severe-duplication-in-product-code` | 70 | no |
| `generated-zone-mutation-risk` | 76 | yes |
| `direct-db-access-from-wrong-layer` | 66 | yes |
| `missing-web-e2e-lane` | 82 | no |
| `missing-rendered-ux-qa-lane` | 84 | no |
| `prompt-injection-risk` | 78 | no |
| `overbroad-agent-agency` | 65 | no |
| `secret-like-content-detected` | 60 | no |
| `false-green-test-risk` | 76 | yes |
| `destructive-migration-risk` | 70 | no |
| `authz-or-data-isolation-gap` | 78 | no |
| `input-boundary-gap` | 78 | no |
| `agent-tool-supply-chain-gap` | 78 | no |
| `release-readiness-gap` | 80 | no |
| `missing-rust-property-or-integration-tests` | 82 | no |
| `no-agent-friendly-exception-pattern` | 76 | no |
| `missing-agent-readable-docs` | 80 | no |
| `streaming-runtime-drift` | 78 | yes |
| `rust-bad-behavior` | 72 | no |
| `sql-bad-behavior` | 72 | no |
| `typescript-bad-behavior` | 72 | yes |
| `docker-bad-behavior` | 72 | no |
| `python-bad-behavior` | 72 | no |
| `ci-bad-behavior` | 70 | yes |
| `git-bad-behavior` | 70 | no |
| `gittools-bad-behavior` | 70 | no |
| `release-bad-behavior` | 70 | no |
| `web-security-bad-behavior` | 68 | yes |
| `repo-rot-bad-behavior` | 88 | no |
| `comment-hygiene-dangerous-residue` | 72 | no |
| `ci-local-parity` | 70 | yes |

## Copy-Code Redundancy

- Status: `review` hard=`0` warning=`32` files=`656`
- Policy: min-lines=`10` min-tokens=`100` max-findings=`50` include-tests=`false` strict=`false`
- Duplicate volume: lines=`67` tokens=`174` bytes=`1794`

- Notes:
  - hard classes are limited to exact active-source file matches and substantial exact same-name units
  - warning classes include same-body different-name units and token/block duplication
  - tests, fixtures, stories, config, Docker, and migrations are omitted unless --include-tests is set

| Kind | Severity | Language | Lines | Tokens | Instances | Reason |
| --- | --- | --- | ---: | ---: | --- | --- |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `crates/cargo-witness/src/diagnose_workspace.rs:166-167, src/api/agent_session.rs:123-124, src/api/agent_session.rs:198-199, src/api/entity.rs:122-123, src/api/entity.rs:131-132, src/api/entity.rs:154-155, src/api/entity.rs:164-165, src/api/snapshot.rs:87-88, src/api/snapshot.rs:140-141, src/bugtracker/types_enums.rs:71-72, src/bugtracker/types_enums.rs:158-159, src/runtime_support/mod.rs:57-58, src/runtime_support/mod.rs:74-75, src/runtime_support/mod.rs:103-104, src/runtime_support/mod.rs:131-132, src/runtime_support/mod.rs:138-139, src/runtime_support/mod.rs:172-173` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 4 | `db/state.rs:1852-1853, db/state.rs:1915-1916, db/state.rs:1950-1951, db/state.rs:2044-2045, db/state.rs:2057-2058, db/state.rs:2116-2117, db/state.rs:2215-2216, db/state.rs:2773-2774, db/state.rs:3124-3125, db/state.rs:3151-3152, db/state.rs:3178-3179, db/state.rs:3197-3198` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `typescript` | 7 | 18 | `apps/web/src/pages/RepositoryCodePage.tsx:35-42, apps/web/src/pages/RepositoryOverviewPage.tsx:38-45` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 6 | 22 | `crates/cargo-aer/src/helpers.rs:93-99, crates/cargo-vrc/src/planner_support_paths.rs:168-174` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 6 | 18 | `src/web/rest/merge_requests.rs:605-611, src/web/rest/reviews.rs:208-214` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 3 | `src/web/rest/issues.rs:36-38, src/web/rest/issues.rs:50-52, src/web/rest/issues.rs:67-69, src/web/rest/issues.rs:84-86` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 5 | 11 | `src/repos/service.rs:250-255, src/repos/settings.rs:225-230` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 4 | 19 | `src/web/csrf.rs:31-35, src/web/rest/auth.rs:242-246` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 3 | `src/capability_execute.rs:86-88, src/capability_inspect.rs:20-22, src/capability_inspect_read.rs:132-134` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `typescript` | 2 | 3 | `apps/web/src/pages/MergeRequestPage.tsx:51-53, apps/web/src/pages/RepositoryFilePage.tsx:34-36, apps/web/src/pages/RepositorySettingsPage.tsx:56-58` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/bugtracker/types_enums.rs:71-72, src/bugtracker/types_enums.rs:158-159, src/runtime_support/mod.rs:57-58, src/runtime_support/mod.rs:103-104` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/api/agent_session.rs:123-124, src/api/entity.rs:122-123, src/api/entity.rs:154-155, src/api/snapshot.rs:140-141` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/api/agent_session.rs:198-199, src/api/entity.rs:131-132, src/api/entity.rs:164-165, src/api/snapshot.rs:87-88` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 10 | `src/merge/review.rs:385-387, src/merge/service.rs:400-402` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `src/git_host/gitlab.rs:130-131, src/git_host/gitlab.rs:142-143, src/git_host/gitlab.rs:163-164` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 4 | `src/runtime_support/mod.rs:37-38, src/runtime_support/mod.rs:83-84, src/runtime_support/mod.rs:147-148` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 3 | `src/runtime_support/mod.rs:45-46, src/runtime_support/mod.rs:91-92, src/runtime_support/mod.rs:155-156` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 4 | `src/remote_support.rs:83-85, src/runner_backend_remote_support.rs:115-117` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 4 | `src/messaging/backend.rs:191-193, src/messaging/backend.rs:203-205` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 4 | `src/messaging/backend.rs:167-169, src/messaging/backend.rs:179-181` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 4 | `src/mcp/tools_schema.rs:29-31, src/mcp/tools_schema.rs:33-35` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/bugtracker/types_enums.rs:97-98, src/bugtracker/types_enums.rs:132-133, src/bugtracker/types_enums.rs:169-170` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `db/state.rs:2387-2388, db/state.rs:3224-3225, db/state.rs:3262-3263` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 3 | `crates/arc-bench/src/exceptions.rs:130-132, crates/arc-bench/src/witness_loop.rs:154-156` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 3 | `src/repo.rs:88-90, src/repo.rs:92-94` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 1 | `crates/cargo-aer/src/helpers.rs:101-103, crates/cargo-vrc/src/planner_support_paths.rs:176-178` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 1 | `src/autonomy/policy_yaml_types.rs:218-220, src/node_types.rs:80-82` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `db/state.rs:2289-2290, db/state.rs:2967-2968` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 4 | `src/runtime_support/mod.rs:64-65, src/runtime_support/mod.rs:110-111` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `typescript` | 1 | 3 | `apps/web/src/test/mocks.ts:11-12, apps/web/src/test/server.ts:27-28` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `db/state.rs:672-673, db/state.rs:679-680` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 1 | `src/api/entity.rs:195-196, src/gateway/singleflight.rs:64-65` | `same body appears under different names across files` |

## Dimensions

| Dimension | Weight | Score | Weighted | Evidence |
| --- | ---: | ---: | ---: | --- |
| Ownership and navigation surface | 13 | 100 | 13.00 | root `AGENTS.md` present; `CODEOWNERS` present |
| Contract and boundary integrity | 13 | 71 | 9.23 | contract surface found; generated contract artifacts found |
| Proof lanes and test routing | 12 | 100 | 12.00 | one-command setup/validation lane found; deterministic fast lane found |
| Security and supply-chain posture | 12 | 86 | 10.32 | lockfile present; secret or dependency scan tooling found |
| Code shape and semantic surface | 12 | 0 | 0.00 | largest authored code file: src/git_host/gitlab_browse.rs (805 LOC); code file exceeds 500 LOC |
| Data truth and workflow safety | 8 | 75 | 6.00 | database surface present; structured db boundary manifest present |
| Observability and repair evidence | 8 | 98 | 7.84 | observability libraries or patterns found; diagnostic shaping hints found |
| Context economy and agent instructions | 7 | 100 | 7.00 | root `AGENTS.md` present; root `AGENTS.md` stays short |
| Jankurai tool adoption and CI replacement | 7 | 30 | 2.10 | control-plane files present; applicable=18 |
| Python containment and polyglot hygiene | 4 | 90 | 3.60 | no Python files in scope; non-optimal product language marker |
| Build speed signals | 4 | 95 | 3.80 | build acceleration markers found; targeted test/build commands found |

## Reference Profile Structure

- Applicable cells: `10` canonical=`10` noncanonical=`0` guidance missing=`0`

| Cell | Status | Canonical | Detected | Aliases | Guidance | Owner | Proof lane | Agent fix |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `web` | `canonical` | `apps/web/` | `apps/web` | `frontend/, ui/, packages/web/, packages/ui/` | `present` | `apps/web` | `rendered UX / Playwright` | `keep `apps/web/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `api` | `canonical` | `apps/api/` | `apps/api` | `api/, server/, backend/` | `present` | `apps/api` | `edge handler / contract tests` | `keep `apps/api/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `domain` | `canonical` | `crates/domain/` | `crates/domain` | `domain/, core/` | `present` | `crates/domain` | `unit / property tests` | `keep `crates/domain/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `application` | `canonical` | `crates/application/` | `crates/application` | `application/, usecases/, use-cases/` | `present` | `crates/application` | `use-case / authz tests` | `keep `crates/application/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `adapters` | `canonical` | `crates/adapters/` | `crates/adapters` | `adapters/, infra/, integrations/` | `present` | `crates/adapters` | `adapter integration tests` | `keep `crates/adapters/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `workers` | `canonical` | `crates/workers/` | `crates/workers` | `workers/, jobs/, scheduler/, queue/` | `present` | `crates/workers` | `workflow / replay tests` | `keep `crates/workers/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `contracts` | `canonical` | `contracts/` | `contracts` | `openapi/, protobuf/, json-schema/, generated/` | `present` | `contracts` | `generation / drift checks` | `keep `contracts/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `db` | `canonical` | `db/` | `db` | `migrations/, constraints/, sql/` | `present` | `db` | `migration / constraint tests` | `keep `db/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `python-ai` | `canonical` | `python/ai-service/` | `python, python/ai-service` | `python/, ai-service/, evals/, embeddings/, model/` | `present` | `python/ai-service` | `eval / contract tests` | `keep `python/ai-service/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `ops` | `canonical` | `ops/` | `.github, .github/workflows, ops` | `.github/, .github/workflows/, ci/, release/, observability/, security/` | `present` | `ops` | `security lane / workflow lint` | `keep `ops/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |

## Rendered UX QA

- Web surface: `true`
- Layered UX lane: `true`
- Missing: `none`

### Ingested UX QA report (`target/jankurai/ux-qa.json`)
- Report count: `10`
- Worst decision: `pass`
- Total violations: `0`
- Summary errors / warnings: `0` / `0`
- Artifact counts: `accessibility=10, aria-snapshot=10, screenshot=10`
- Artifact fingerprints: `30`
- Visual baseline counts: missing=`10` changed=`0` review=`0` block=`0`
- Missing required states: `0` report(s) `none`
- Missing required artifacts: `0` report(s) `none`
- Accessibility violations / incomplete / passes: `0` / `0` / `170`

## Tool Adoption

- Control plane present: `true`
- Applicable tools: `18`
- Configured: `18`
- CI evidence: `0`
- Artifact verified: `0`
- Replaced count: `0`
- Missing CI evidence: `audit-ci, proof-routing, proofbind, proofmark-rust, copy-code, security, ci-bad-behavior, git-bad-behavior, release-bad-behavior, ux-qa, db-migration-analyze, contract-drift, rust-witness, authz-matrix, input-boundary, agent-tool-supply, release-readiness, cost-budget`

| Tool | Category | Mode | Status | Replaced | Artifacts |
| --- | --- | --- | --- | --- | --- |
| `audit-ci` | `audit` | `auto` | `configured` | `manual repo scoring, ad hoc score gates` | `agent/repo-score.json, agent/repo-score.md` |
| `proof-routing` | `proof` | `auto` | `configured` | `ad hoc proof lane selection, manual proof receipts` | `agent/repo-score.json, agent/repo-score.md, target/jankurai/repair-queue.jsonl` |
| `proofbind` | `proof` | `advisory` | `configured` | `manual changed-surface routing, ad hoc proof obligation lists` | `target/jankurai/proofbind/surface-witness.json, target/jankurai/proofbind/obligations.json` |
| `proofmark-rust` | `proof` | `advisory` | `configured` | `line-only coverage review, manual in-diff mutation review` | `target/jankurai/proofmark/proofmark-receipt.json, target/jankurai/proofmark/proof-receipt.json` |
| `copy-code` | `audit` | `advisory` | `configured` | `ad hoc copy-code review, manual duplication triage` | `target/jankurai/copy-code.json, target/jankurai/copy-code.md` |
| `security` | `security` | `auto` | `configured` | `gitleaks, dependency review, SBOM/provenance` | `target/jankurai/security/evidence.json` |
| `ci-bad-behavior` | `security` | `advisory` | `configured` | `mutable workflow refs, secret echo/debug workflow checks, non-blocking security scans` | `target/jankurai/language-bad-behavior.log` |
| `git-bad-behavior` | `audit` | `advisory` | `configured` | `destructive git automation, force-push release scripts, hidden stash-based state` | `target/jankurai/language-bad-behavior.log` |
| `release-bad-behavior` | `release` | `advisory` | `configured` | `manual release checklist, ad hoc tag and artifact review, manual provenance review` | `target/jankurai/language-bad-behavior.log` |
| `ux-qa` | `ux` | `auto` | `configured` | `playwright, axe-core, visual baselines` | `target/jankurai/ux-qa.json` |
| `db-migration-analyze` | `db` | `auto` | `configured` | `manual migration review` | `target/jankurai/migration-report.json` |
| `contract-drift` | `contract` | `auto` | `configured` | `handwritten contract drift checks, openapi diff` | `agent/repo-score.json, agent/repo-score.md` |
| `rust-witness` | `rust` | `auto` | `configured` | `manual witness graphing` | `target/jankurai/rust/witness-graph.json` |
| `vibe-coverage` | `audit` | `auto` | `not_applicable` | `manual vibe-coding coverage spreadsheet` | `target/jankurai/vibe-coverage.json, target/jankurai/vibe-coverage.md` |
| `coverage-evidence` | `proof` | `auto` | `not_applicable` | `manual coverage report review, ad hoc mutation survivor review` | `target/jankurai/coverage/coverage-audit.json, target/jankurai/coverage/coverage-audit.md` |
| `authz-matrix` | `security` | `auto` | `configured` | `manual authz matrix review` | `agent/repo-score.json, agent/repo-score.md` |
| `input-boundary` | `security` | `auto` | `configured` | `manual unsafe sink review` | `agent/repo-score.json, agent/repo-score.md` |
| `agent-tool-supply` | `security` | `auto` | `configured` | `manual MCP/tool trust review` | `agent/repo-score.json, agent/repo-score.md` |
| `release-readiness` | `release` | `auto` | `configured` | `manual launch checklist` | `agent/repo-score.json, agent/repo-score.md` |
| `cost-budget` | `release` | `auto` | `configured` | `manual spend review` | `agent/repo-score.json, agent/repo-score.md` |

## Security evidence (ingested)

- Source: `target/jankurai/security/evidence.json`
- Envelope exit code: `0` · elapsed: `3785` ms · strict: `true`
- Commands — ran: `1`, skipped: `0`, failed: `0`
- Generated at: `1779671912`
- Git HEAD (envelope): `0fe29e08935061817d002221052ef45494379613`

## Boundary manifest (ingested)

- Path: `agent/boundaries.toml`
- Stack: `rust-ts-vite-react-redline-jansu-bounded-python` · version: `0.4.0`
- Queue path counts — adapter: `2`, event_contract: `1`, generated_type: `1`, client_marker: `6`, streaming_exception: `2`
- Content fingerprint: `sha256:4b3b4ff624a9f9779a21572e8ef75f33d157f141606516169418613995ad2199`

## Boundary Reclassifications

No audited runtime boundary reclassifications declared.

## Findings

1. `medium` `shape` `.`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:shape` `soft` confidence `0.76`
   Route: TLR `Entropy`, lane `fast`, owner `tools`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: `Code shape and semantic surface` scored 0 below the standard floor of 85
   Fix: split large or ambiguous authored code into smaller semantic modules with focused tests
   Rerun: `just fast`
   Fingerprint: `sha256:649c734222c135b46a14d8d1c0ab99ec7a9510f74cf5f856448f5bea03c5c872`
   Evidence: largest authored code file: src/git_host/gitlab_browse.rs (805 LOC), code file exceeds 500 LOC, most code files stay under 300 LOC, copy-code advisory classes found: 32 (advisory only, no score impact)
2. `high` `ci` `.github/workflows/web.yml:37`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.workflow-not-thin`
   Reason: without a single source of truth, local runs drift from CI and breakage is only visible after push
   Fix: extract the workflow steps into ops/ci/<lane>.sh and call them with `bash ops/ci/<lane>.sh`
   Rerun: `just fast`
   Fingerprint: `sha256:8044b47c92fad46dca0bf4c20c16d01862913cfe3c2213ebddeee04d84a04d45`
   Evidence: detector=ci.local-parity.workflow-not-thin, path=.github/workflows/web.yml, line=37, proof_window=None, snippet=jobs:
3. `high` `security` `.github/workflows/web.yml:178`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.security-scan.nonblocking`
   Reason: security or proof job is explicitly non-blocking
   Fix: remove the non-blocking override so scan failures stop the pipeline
   Rerun: `just security`
   Fingerprint: `sha256:d6c0a2941587c72d91b0a7cc49f484413e276881d613ad1421e1e39e8942eb0c`
   Evidence: detector=ci.security-scan.nonblocking, path=.github/workflows/web.yml, line=178, proof_window=None, snippet=kill "$bff_pid" || true
4. `high` `security` `.gitlab-ci.yml:1`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.concurrency.missing`
   Reason: workflow can run duplicate stale audits for the same ref
   Fix: add workflow-level concurrency with cancel-in-progress
   Rerun: `just security`
   Fingerprint: `sha256:744b2196e311cfcfc419410267391cc263acf1dc549629f0e26f42efd779ea6e`
   Evidence: detector=ci.concurrency.missing, path=.gitlab-ci.yml, line=1, proof_window=None, snippet=stages:
5. `high` `security` `.gitlab-ci.yml:1`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.permissions.missing`
   Reason: workflow permissions default is not pinned in source
   Fix: add top-level `permissions: contents: read` and job-specific write scopes only where needed
   Rerun: `just security`
   Fingerprint: `sha256:baada997e52f80a56bb65833dcc2b930203b5179aca7069b534ee05aa89ea1d9`
   Evidence: detector=ci.permissions.missing, path=.gitlab-ci.yml, line=1, proof_window=None, snippet=stages:
6. `high` `security` `.gitlab-ci.yml:1`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.timeout.missing`
   Reason: workflow can run without a checked time bound
   Fix: set an explicit timeout-minutes on each job
   Rerun: `just security`
   Fingerprint: `sha256:e535dded9509b14b95dbbdb28d7ed4938771f67d27784d8f41e97def80f2bde8`
   Evidence: detector=ci.timeout.missing, path=.gitlab-ci.yml, line=1, proof_window=None, snippet=stages:
7. `high` `security` `.gitlab-ci.yml:297`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.artifact.cache.secret-path`
   Reason: workflow stores a secret-bearing path in cache or artifact upload
   Fix: limit the path to build outputs and keep credential files out of caches and artifacts
   Rerun: `just security`
   Fingerprint: `sha256:d869a268fe8fc73424b623cdc02c1b84e5a08c7dbb85ee540d7ccdee251de228`
   Evidence: detector=ci.artifact.cache.secret-path, path=.gitlab-ci.yml, line=297, proof_window=None, snippet=GITHUB_OUTPUT: "$CI_PROJECT_DIR/.release-resolve.env"
8. `high` `security` `.gitlab-ci.yml:311`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.artifact.cache.secret-path`
   Reason: workflow stores a secret-bearing path in cache or artifact upload
   Fix: limit the path to build outputs and keep credential files out of caches and artifacts
   Rerun: `just security`
   Fingerprint: `sha256:82e8299c42e754ef2aee229edfd467c5e8c4dc72466b3186a16576b1385936a4`
   Evidence: detector=ci.artifact.cache.secret-path, path=.gitlab-ci.yml, line=311, proof_window=None, snippet=dotenv: .release-resolve.env
9. `medium` `boundary` `agent/boundaries.toml`
   Rule: `HLT-007-HANDWRITTEN-CONTRACT`
   Check: `HLT-007-HANDWRITTEN-CONTRACT:boundary` `soft` confidence `0.76`
   Route: TLR `Contracts/data`, lane `contract`, owner `agent`
   Docs: `docs/audit-rubric.md#known-vibe-coding-insults`
   Reason: `Contract and boundary integrity` scored 71 below the standard floor of 85
   Fix: add generated contracts and boundary checks for public APIs, data access, and cross-runtime seams
   Rerun: `just fast`
   Fingerprint: `sha256:986c6ac5e35836ddc4efd8ed18e7989e8c71861bde6547558702f879f95c125c`
   Evidence: contract surface found, generated contract artifacts found, polyglot boundary layout present, boundary manifest present
10. `high` `generated` `agent/generated-zones.toml:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone file `contracts/generated/*.ts` is missing
   Fix: regenerate `contracts/generated/*.ts` using the declared command, or remove the zone entry if the file was deleted intentionally
   Rerun: `just fast`
   Fingerprint: `sha256:8e76087395e4444ba8f23ebc426af795e80f7f4583a4f24e6810a36dde53809f`
   Evidence: generated zone integrity violation
11. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `.gitlab-ci.yml` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:2ef7c6845ba4737d48f4730ddf1f45e3c77ef868a38c77a0d636bf3569facb9d`
   Evidence: .gitlab-ci.yml
12. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `ROADMAP.md` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:f64b721f056ed9e1f1136309119c50a7f370bcd523a9206a745d7ead80a76232`
   Evidence: ROADMAP.md
13. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `WEB_WORK_CLAUDE.md` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:b15b68b2cf7b91d906924ca0b18beff5282e7b51cee94fb307b691d004bdc5d5`
   Evidence: WEB_WORK_CLAUDE.md
14. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `WEB_WORK_CODEX.md` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:145a1547586187fe2151f7a221d19e7490e5349a48bb49105a36a0080ef3a661`
   Evidence: WEB_WORK_CODEX.md
15. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `package-lock.json` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:05c6b9a3bde0a36a87dd5d4dc3176b70d699bcec7b55b22e550d1711e790496a`
   Evidence: package-lock.json
16. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `schemas/web-api.openapi.json` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:17963256a249fca3711b7160dae1a3042c2bd244900cb72d025bcfaab2638349`
   Evidence: schemas/web-api.openapi.json
17. `high` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: path `schemas/websocket-events.schema.json` has no owner-map route
   Fix: add the narrowest stable prefix for this path to `agent/owner-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:82dc010e5a3bb3185bb68f47f4596af45176902ee56699b0f7ca76e1a397ddfd`
   Evidence: schemas/websocket-events.schema.json
18. `high` `proof` `agent/test-map.json`
   Rule: `HLT-004-UNMAPPED-PROOF`
   Check: `HLT-004-UNMAPPED-PROOF:proof` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#proof-lanes`
   Reason: path `ROADMAP.md` has no test-map proof route
   Fix: add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:662c25d4231ccda9b3d08fc27c6cd3eb2fc43fff15a7b9062fd091cb0baad3ad`
   Evidence: ROADMAP.md
19. `high` `proof` `agent/test-map.json`
   Rule: `HLT-004-UNMAPPED-PROOF`
   Check: `HLT-004-UNMAPPED-PROOF:proof` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#proof-lanes`
   Reason: path `WEB_WORK_CLAUDE.md` has no test-map proof route
   Fix: add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:1a34bbc419fb910a90c68136444fc23ecdd47b4126f57690dceeb9c88672a91a`
   Evidence: WEB_WORK_CLAUDE.md
20. `high` `proof` `agent/test-map.json`
   Rule: `HLT-004-UNMAPPED-PROOF`
   Check: `HLT-004-UNMAPPED-PROOF:proof` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#proof-lanes`
   Reason: path `WEB_WORK_CODEX.md` has no test-map proof route
   Fix: add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:2c03336e49b344f02e284e116c5ee4bb18f2a0c379c1cc299257ab6a71105845`
   Evidence: WEB_WORK_CODEX.md
21. `high` `proof` `agent/test-map.json`
   Rule: `HLT-004-UNMAPPED-PROOF`
   Check: `HLT-004-UNMAPPED-PROOF:proof` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#proof-lanes`
   Reason: path `schemas/web-api.openapi.json` has no test-map proof route
   Fix: add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:bc9537fd89d01259c682be4eb1f0258fa0a31a645289bc7483aad49d3e1bfe21`
   Evidence: schemas/web-api.openapi.json
22. `high` `proof` `agent/test-map.json`
   Rule: `HLT-004-UNMAPPED-PROOF`
   Check: `HLT-004-UNMAPPED-PROOF:proof` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#proof-lanes`
   Reason: path `schemas/websocket-events.schema.json` has no test-map proof route
   Fix: add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Rerun: `just fast`
   Fingerprint: `sha256:3ecf5ea8cb77a090879f9abfd7684e093f465a7102c33e2595d1ba593c596423`
   Evidence: schemas/websocket-events.schema.json
23. `high` `test` `apps/web/e2e/03-readme.spec.ts:120`
   Rule: `HLT-008-FALSE-GREEN-RISK`
   Check: `HLT-008-FALSE-GREEN-RISK:test` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Reason: test code contains disabled, focused, tautological, or snapshot-only proof
   Fix: replace false-green tests with behavior assertions, red/green evidence, and mutation or fault checks for changed behavior
   Rerun: `just fast`
   Fingerprint: `sha256:1cb9150ba18fa6482c215b310d67d0e91ceafab05e1767bacc3fbba5c64f4d4e`
   Evidence: test.skip(
24. `high` `vibe` `apps/web/e2e/fixtures/auth.ts:33`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:8a3ee2106085018d751d12ab6794fb249c06af4c3170713ac2ced926ebdbaba8`
   Evidence: apps/web/e2e/fixtures/auth.ts:33, future-hostile/dead-language term `placeholder` appears
25. `high` `vibe` `apps/web/e2e/fixtures/mocks.ts:287`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stale` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:3fc256623c83fcb918222808a5d43cb783888fb1f3b0ce98d2c6255c53de2270`
   Evidence: apps/web/e2e/fixtures/mocks.ts:287, future-hostile/dead-language term `stale` appears
26. `high` `vibe` `apps/web/e2e/pages/RepositoriesPage.ts:21`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:3c9f7c1c4f81a90ee101dbfed149d3bf7dcd3ca8bbbe49aef2eae911f38153ec`
   Evidence: apps/web/e2e/pages/RepositoriesPage.ts:21, future-hostile/dead-language term `placeholder` appears
27. `high` `boundary` `apps/web/e2e/pages/RepositoriesPage.ts:56`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.suppress.ts-nocheck`
   Reason: broad suppression is hard to audit
   Fix: remove the broad suppression or scope it to a single justified line
   Rerun: `just fast`
   Fingerprint: `sha256:971a8fda865186cb1efc6024b795fe5dc4f1dd362424f46e90d15946e0363d4c`
   Evidence: detector=typescript.suppress.ts-nocheck, path=apps/web/e2e/pages/RepositoriesPage.ts, line=56, snippet=// eslint-disable-next-line @typescript-eslint/no-unused-vars
28. `high` `stack` `apps/web/eslint.config.js`
   Check: `HLT-000-SCORE-DIMENSION:stack` `hard` confidence `0.88`
   Route: TLR `Context/setup`, lane `audit`, owner `apps`
   Reason: runtime code uses a language outside the chosen optimal stack
   Fix: move product runtime behavior to Rust core, TypeScript web, SQL migrations, or generated contracts; Python needs a dated advanced-ML/data exception
   Rerun: `just score`
   Fingerprint: `sha256:da002ad8185f6b43496adf318beaf145d8c04e01ea3489df63209dfb213975b8`
   Evidence: apps/web/eslint.config.js uses `.js`, Rust core + TypeScript/React/Vite + PostgreSQL + generated contracts + exception-only Python AI/data service
29. `high` `security` `apps/web/eslint.config.js:23`
   Rule: `HLT-039-WEB-SECURITY-BAD-BEHAVIOR`
   Check: `HLT-039-WEB-SECURITY-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `apps`
   Docs: `docs/language-bad-behavior.md#web-security-and-repo-rot-detectors`
   Matched term: `websec.storage.token`
   Reason: localStorage and sessionStorage are readable by injected JavaScript
   Fix: prefer HttpOnly Secure SameSite cookies or a bounded in-memory token flow with documented threat model
   Rerun: `just security`
   Fingerprint: `sha256:271d7e3c2c02c4be9b6e53e83685a17b64d0efb0a8ea61cd8db383e9168deb09`
   Evidence: detector=websec.storage.token, path=apps/web/eslint.config.js, line=23, proof_window=None, snippet=sessionStorage: 'readonly', localStorage: 'readonly',
30. `high` `vibe` `apps/web/eslint.config.js:68`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `unused` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:4373032ea892864a840f56f3d227bf1dd2f5767f9d4715c83ec438bc57189fc5`
   Evidence: apps/web/eslint.config.js:68, future-hostile/dead-language term `unused` appears
31. `high` `vibe` `apps/web/eslint.config.js:70`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `unused` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:3b83058a34ca5fc5d02b633936d1e2c46e76d29a941e4fa6b5f17fb0232095ec`
   Evidence: apps/web/eslint.config.js:70, future-hostile/dead-language term `unused` appears
32. `high` `vibe` `apps/web/eslint.config.js:104`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `unused` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:88e22fc64b4e13a8e2a32cf32a4f68314bc9930c2f1a90890ffec88b4e35fd3d`
   Evidence: apps/web/eslint.config.js:104, future-hostile/dead-language term `unused` appears
33. `high` `boundary` `apps/web/playwright.config.ts:9`
   Rule: `HLT-019-STREAMING-RUNTIME-DRIFT`
   Check: `HLT-019-STREAMING-RUNTIME-DRIFT:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `db`, owner `apps`
   Docs: `docs/streaming.md`
   Reason: queue or streaming runtime client appears outside the declared adapter boundary
   Fix: move Kafka/Tansu/Iggy/Fluvio/NATS/Redis-stream clients behind `crates/adapters/queues` or document a brownfield exception with owner, expiry, and migration path
   Rerun: `just fast`
   Fingerprint: `sha256:1ec28ed5e3520ea1cf385fb1086c16e31b76553717eec85f14ec3bcd67f63796`
   Evidence: streaming client marker `iggy` appears outside `crates/adapters/queues`
34. `high` `boundary` `apps/web/src/api/client.ts:92`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.types.any-boundary`
   Reason: value shape is not proven before the cast
   Fix: validate the value first, then narrow it with a proof-aware decoder
   Rerun: `just fast`
   Fingerprint: `sha256:2518b2686764330dbfb499a5916679e67a931a1a8dc64a9bbde9ee231c832f51`
   Evidence: detector=typescript.types.any-boundary, path=apps/web/src/api/client.ts, line=92, snippet=const body = (await response.json()) as { error?: ApiErrorEnvelope };
35. `high` `vibe` `apps/web/src/api/client.ts:108`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: fallback soup detected in product code
   Fix: collapse fallback chains into explicit typed states with bounded retry policy, telemetry, and documented repair guidance
   Rerun: `just fast`
   Fingerprint: `sha256:acbd4dd851cfefddeef0f9d7383776e0b405d11a6f85faedf41a87331f52aeb5`
   Evidence: apps/web/src/api/client.ts:108 return undefined as unknown as T;
36. `high` `boundary` `apps/web/src/api/client.ts:114`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.types.any-boundary`
   Reason: value shape is not proven before the cast
   Fix: validate the value first, then narrow it with a proof-aware decoder
   Rerun: `just fast`
   Fingerprint: `sha256:0ee248e9d10c7e6c6919107992e2262f5b87ab2238556743cd7ee649f0ab585a`
   Evidence: detector=typescript.types.any-boundary, path=apps/web/src/api/client.ts, line=114, snippet=return (await response.json()) as T;
37. `high` `boundary` `apps/web/src/api/websocket.ts:183`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.types.any-boundary`
   Reason: value shape is not proven before the cast
   Fix: validate the value first, then narrow it with a proof-aware decoder
   Rerun: `just fast`
   Fingerprint: `sha256:aae7b82b677f78bc861c815ab4f53fbac5902afca11ddfe80690a272a0e33cbb`
   Evidence: detector=typescript.types.any-boundary, path=apps/web/src/api/websocket.ts, line=183, snippet=frame = JSON.parse(event.data) as ServerWsMessage;
38. `high` `data` `apps/web/src/components/browser/BranchSelector.tsx:1`
   Rule: `HLT-006-DIRECT-DB-WRONG-LAYER`
   Check: `HLT-006-DIRECT-DB-WRONG-LAYER:data` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `db`, owner `apps`
   Docs: `docs/audit-rubric.md#required-shape`
   Reason: direct database access appears in a wrong layer
   Fix: move SQL and DB clients to `crates/adapters` or `db/`; expose typed application/domain APIs upward
   Rerun: `just fast`
   Fingerprint: `sha256:b08969322c27ae1bfe4a9cb6da8e895c19b9a676063ee527b9f8a2851edb5aa0`
   Evidence: DB marker in non-adapter layer
39. `high` `vibe` `apps/web/src/components/browser/BranchSelector.tsx:83`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:54a0a15dbd5899cae51dbd142eaa7a0018e56abf0b657120c083c44982ad8ba9`
   Evidence: apps/web/src/components/browser/BranchSelector.tsx:83, future-hostile/dead-language term `placeholder` appears
40. `high` `vibe` `apps/web/src/components/browser/CodeViewer.tsx:142`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `fallback` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:50d7d75390a832809f494ac89fd6ce3b2b496b536de50c4cf768fa5bcc81cede`
   Evidence: apps/web/src/components/browser/CodeViewer.tsx:142, future-hostile/dead-language term `fallback` appears
41. `high` `boundary` `apps/web/src/components/browser/MarkdownRenderer.tsx:99`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.runtime.dangerous-eval-dom`
   Reason: sink is not proven safe locally
   Fix: replace the dynamic sink with a bounded parser, sanitizer, or typed renderer
   Rerun: `just fast`
   Fingerprint: `sha256:bab699759489193306dde72282bdfbd8d1049c8c0a6e2e893f8d62f604625a10`
   Evidence: detector=typescript.runtime.dangerous-eval-dom, path=apps/web/src/components/browser/MarkdownRenderer.tsx, line=99, snippet=dangerouslySetInnerHTML={{ __html: safeHtml }}
42. `high` `vibe` `apps/web/src/components/merge/DiffViewer.tsx:215`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:53593f3a8f5af7a0ab7802fbbf392e2a2a306a225a1c8921bb90f961eec01216`
   Evidence: apps/web/src/components/merge/DiffViewer.tsx:215, future-hostile/dead-language term `old` appears
43. `high` `vibe` `apps/web/src/components/merge/DiffViewer.tsx:249`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:1cdb0f5e68844e3ad15da01dfaf511f694730aff55e0d026475eac0ca7dd6db1`
   Evidence: apps/web/src/components/merge/DiffViewer.tsx:249, future-hostile/dead-language term `placeholder` appears
44. `high` `vibe` `apps/web/src/components/merge/InlineComment.tsx:27`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:94e89c4d0f9d0fbd88cc98c28a0183d7c25404fdb4d0232c9c11b96654d66ee3`
   Evidence: apps/web/src/components/merge/InlineComment.tsx:27, future-hostile/dead-language term `placeholder` appears
45. `high` `vibe` `apps/web/src/components/merge/InlineComment.tsx:77`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:ae722def7120b41d89a7b7a73dc3eea538ae8d33350fe5b76be6480be87f5134`
   Evidence: apps/web/src/components/merge/InlineComment.tsx:77, future-hostile/dead-language term `placeholder` appears
46. `high` `vibe` `apps/web/src/components/merge/InlineComment.tsx:109`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:057088a0fdfe202b2e989e02cafa97f786cc2902eb6d779506fd56e3225e15a7`
   Evidence: apps/web/src/components/merge/InlineComment.tsx:109, future-hostile/dead-language term `placeholder` appears
47. `high` `vibe` `apps/web/src/components/merge/MergeGatePanel.stories.tsx:59`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stale` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:e03b48891545a1318263c134337eb131c706ff26b6b05b496f09fec5f370e2c6`
   Evidence: apps/web/src/components/merge/MergeGatePanel.stories.tsx:59, future-hostile/dead-language term `stale` appears
48. `high` `vibe` `apps/web/src/components/merge/ReviewSidebar.tsx:130`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:b046ad9ce10a6bef833d1e52ec71de4c423265c8ff8e27d747770d82c52fe940`
   Evidence: apps/web/src/components/merge/ReviewSidebar.tsx:130, future-hostile/dead-language term `placeholder` appears
49. `high` `vibe` `apps/web/src/components/repo/CreateRepoDialog.tsx:388`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:030f77abe27dcb41051223bfaa94c2112560985262cd4015f6d38b854d042b92`
   Evidence: apps/web/src/components/repo/CreateRepoDialog.tsx:388, future-hostile/dead-language term `placeholder` appears
50. `high` `vibe` `apps/web/src/components/settings/AgentPolicyEditor.tsx:97`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:b8c206ad0f022b827fbe25adf066b46fdd819e40efe61bc212a30aba260bbd78`
   Evidence: apps/web/src/components/settings/AgentPolicyEditor.tsx:97, future-hostile/dead-language term `placeholder` appears
51. `high` `vibe` `apps/web/src/components/settings/AgentPolicyEditor.tsx:113`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:10612d346d9639e4067341590fc791e24eebb0c205ea820c0b3bf0a337c09774`
   Evidence: apps/web/src/components/settings/AgentPolicyEditor.tsx:113, future-hostile/dead-language term `placeholder` appears
52. `high` `vibe` `apps/web/src/components/settings/AgentPolicyEditor.tsx:122`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:de3a7ad62a8cc5a8b422ebee05cb3cb68c8c2153f305ad38ab0c0cff5d91e840`
   Evidence: apps/web/src/components/settings/AgentPolicyEditor.tsx:122, future-hostile/dead-language term `placeholder` appears
53. `high` `vibe` `apps/web/src/components/settings/BranchProtectionEditor.tsx:85`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:d9e0bc61be4c9b348364212aae95cb25450ecacf6362827f1da950889057bb69`
   Evidence: apps/web/src/components/settings/BranchProtectionEditor.tsx:85, future-hostile/dead-language term `placeholder` appears
54. `high` `vibe` `apps/web/src/components/settings/BranchProtectionEditor.tsx:182`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:4847900dcb14ab9fe236882bada58451e8e9393047921122fde7151b44f55a28`
   Evidence: apps/web/src/components/settings/BranchProtectionEditor.tsx:182, future-hostile/dead-language term `placeholder` appears
55. `high` `vibe` `apps/web/src/components/settings/MergePolicyEditor.tsx:99`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stale` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:ebc8fa4702995231dd96abd95a2b1130ec7dc554940f2432864a3c3d79ad54a1`
   Evidence: apps/web/src/components/settings/MergePolicyEditor.tsx:99, future-hostile/dead-language term `stale` appears
56. `high` `vibe` `apps/web/src/components/settings/SettingsDiffPreview.stories.tsx:48`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:ae9a0764672ea8264d182b1c0384e11e591aae8b0ca51f6f70bbe12843520750`
   Evidence: apps/web/src/components/settings/SettingsDiffPreview.stories.tsx:48, future-hostile/dead-language term `old` appears
57. `high` `vibe` `apps/web/src/components/settings/__tests__/SettingsDiffPreview.test.tsx:45`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:fa86664bde49392790d82592daf0f0fb2e9a06ad3d903210d84109e1dff722ae`
   Evidence: apps/web/src/components/settings/__tests__/SettingsDiffPreview.test.tsx:45, future-hostile/dead-language term `old` appears
58. `high` `vibe` `apps/web/src/components/settings/__tests__/SettingsDiffPreview.test.tsx:51`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:d429d1e1e5b1df3ade1aaa2a74f3444c7f19772acc4e0d0bdcc3782ea4c6b260`
   Evidence: apps/web/src/components/settings/__tests__/SettingsDiffPreview.test.tsx:51, future-hostile/dead-language term `old` appears
59. `high` `boundary` `apps/web/src/hooks/useBootstrap.ts:9`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.types.any-boundary`
   Reason: value shape is not proven before the cast
   Fix: validate the value first, then narrow it with a proof-aware decoder
   Rerun: `just fast`
   Fingerprint: `sha256:f4c107004b5b5aceb0b5282a35274d6099bff1974c67ccba244cb8bb98c07915`
   Evidence: detector=typescript.types.any-boundary, path=apps/web/src/hooks/useBootstrap.ts, line=9, snippet=export const BOOTSTRAP_QUERY_KEY = ['bootstrap'] as const;
60. `high` `boundary` `apps/web/src/hooks/useSearch.ts:89`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.types.any-boundary`
   Reason: value shape is not proven before the cast
   Fix: validate the value first, then narrow it with a proof-aware decoder
   Rerun: `just fast`
   Fingerprint: `sha256:42edc4685d8e5739faf5b809d1a04383eaa4aeecbecde67e4e921bd716015abb`
   Evidence: detector=typescript.types.any-boundary, path=apps/web/src/hooks/useSearch.ts, line=89, snippet=queryKey: ['search', trimmed, kinds.join(','), options.limit ?? 20] as const,
61. `high` `vibe` `apps/web/src/layout/CommandPalette.tsx:60`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:737b3ed969fcd1cac8d7f67f14c3e9f6355b2d42cc20c55af03db2228c3c4459`
   Evidence: apps/web/src/layout/CommandPalette.tsx:60, future-hostile/dead-language term `placeholder` appears
62. `high` `vibe` `apps/web/src/pages/NotFoundPage.tsx:19`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stale` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:dbfc7a846c86fc4e1129bb08f2f92552e130b531e11a6aa4a72f6e95be846487`
   Evidence: apps/web/src/pages/NotFoundPage.tsx:19, future-hostile/dead-language term `stale` appears
63. `high` `vibe` `apps/web/src/pages/RepositoriesPage.tsx:169`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:beae17053ba5cad361b9da66b7e8e13990b1015dfec5b7efe4868e843fc469ee`
   Evidence: apps/web/src/pages/RepositoriesPage.tsx:169, future-hostile/dead-language term `placeholder` appears
64. `high` `vibe` `apps/web/src/pages/RepositoryCodePage.tsx:222`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:29053500c7d7ca796418f14403a3eac4fd90006b9ef44b31b307263a532a98dd`
   Evidence: apps/web/src/pages/RepositoryCodePage.tsx:222, future-hostile/dead-language term `placeholder` appears
65. `high` `boundary` `apps/web/src/pages/SearchResultsPage.tsx:125`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.suppress.ts-nocheck`
   Reason: broad suppression is hard to audit
   Fix: remove the broad suppression or scope it to a single justified line
   Rerun: `just fast`
   Fingerprint: `sha256:7e90c8f338294b77b9273fddd27d54d5c4969d767f3a4399ec8a2524d2da22ac`
   Evidence: detector=typescript.suppress.ts-nocheck, path=apps/web/src/pages/SearchResultsPage.tsx, line=125, snippet=// eslint-disable-next-line react-hooks/exhaustive-deps
66. `high` `vibe` `apps/web/src/pages/SearchResultsPage.tsx:323`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:e11a74a1312fd12b58415af270ace1456f711d24175bd26545357905f077b516`
   Evidence: apps/web/src/pages/SearchResultsPage.tsx:323, future-hostile/dead-language term `placeholder` appears
67. `high` `vibe` `apps/web/src/pages/StubPage.tsx:38`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stub` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:f1fa20a14b9052d3e8642bb6b2fe605cebe7d35a774dbf309bb114074643f605`
   Evidence: apps/web/src/pages/StubPage.tsx:38, future-hostile/dead-language term `stub` appears
68. `high` `vibe` `apps/web/src/pages/StubPage.tsx:45`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `apps`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: product code contains TODO/stub/unimplemented/unreachable placeholder markers
   Fix: replace placeholders with implemented behavior, typed unsupported-state errors, or a tracked exception record with docs
   Rerun: `just fast`
   Fingerprint: `sha256:0cd99e4fc5ed3531d0b9b5903def91cb8dd522cf6ffd699ed89ba88c2ad1dadf`
   Evidence: apps/web/src/pages/StubPage.tsx:45 title={`${workPackage} not implemented`}
69. `high` `boundary` `apps/web/src/stores/preferencesStore.ts:69`
   Rule: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR`
   Check: `HLT-031-TYPESCRIPT-BAD-BEHAVIOR:boundary` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `fast`, owner `apps`
   Docs: `docs/testing.md`
   Matched term: `typescript.types.any-boundary`
   Reason: value shape is not proven before the cast
   Fix: validate the value first, then narrow it with a proof-aware decoder
   Rerun: `just fast`
   Fingerprint: `sha256:d604bb1a9b898cba21235ba1a9fec0e3d2d006a499ec5e85cfd24813a49fe0f6`
   Evidence: detector=typescript.types.any-boundary, path=apps/web/src/stores/preferencesStore.ts, line=69, snippet=const parsed = JSON.parse(raw) as Partial<typeof DEFAULTS>;
70. `high` `security` `apps/web/src/stores/realtimeStore.ts:63`
   Rule: `HLT-039-WEB-SECURITY-BAD-BEHAVIOR`
   Check: `HLT-039-WEB-SECURITY-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `apps`
   Docs: `docs/language-bad-behavior.md#web-security-and-repo-rot-detectors`
   Matched term: `websec.storage.token`
   Reason: localStorage and sessionStorage are readable by injected JavaScript
   Fix: prefer HttpOnly Secure SameSite cookies or a bounded in-memory token flow with documented threat model
   Rerun: `just security`
   Fingerprint: `sha256:08b33b98a9304bd163c1c789bc4bb0647472b391c634a0d4c30dae09b3afc39c`
   Evidence: detector=websec.storage.token, path=apps/web/src/stores/realtimeStore.ts, line=63, proof_window=None, snippet=const raw = window.sessionStorage.getItem(SEQ_STORAGE_KEY);
71. `high` `security` `apps/web/src/stores/realtimeStore.ts:77`
   Rule: `HLT-039-WEB-SECURITY-BAD-BEHAVIOR`
   Check: `HLT-039-WEB-SECURITY-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `apps`
   Docs: `docs/language-bad-behavior.md#web-security-and-repo-rot-detectors`
   Matched term: `websec.storage.token`
   Reason: localStorage and sessionStorage are readable by injected JavaScript
   Fix: prefer HttpOnly Secure SameSite cookies or a bounded in-memory token flow with documented threat model
   Rerun: `just security`
   Fingerprint: `sha256:efa0ee3fb55a2eaf1093e5e77c7331b19325a6c95501e711a8723aefbe0cf1a3`
   Evidence: detector=websec.storage.token, path=apps/web/src/stores/realtimeStore.ts, line=77, proof_window=None, snippet=window.sessionStorage.setItem(SEQ_STORAGE_KEY, seq.toString());
72. `high` `generated` `contracts/generated/SettingsDiffPreview.ts:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `contracts`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone is not protected strongly enough against hand edits
   Fix: add `agent/generated-zones.toml`, require generated/do-not-edit markers, and route repairs to the source contract
   Rerun: `just fast`
   Fingerprint: `sha256:ae83c2c666a4a8b7e3af664f02e66f9d546ad7ced4563b4b8636120bac46c7eb`
   Evidence: generated file contains TODO/stub markers
73. `medium` `data` `db/`
   Rule: `HLT-006-DIRECT-DB-WRONG-LAYER`
   Check: `HLT-006-DIRECT-DB-WRONG-LAYER:data` `soft` confidence `0.76`
   Route: TLR `Contracts/data`, lane `db`, owner `data`
   Docs: `docs/audit-rubric.md#required-shape`
   Reason: `Data truth and workflow safety` scored 75 below the standard floor of 85
   Fix: move durable truth into migrations, constraints, adapters, and application-owned transactions
   Rerun: `just fast`
   Fingerprint: `sha256:9363f8264162b95439ce1a8ccfb0913a811deffe70879f2092632cf0f0007bc5`
   Evidence: database surface present, structured db boundary manifest present, db boundary routes roots, migrations, and constraints, migration directory present
74. `high` `generated` `schemas/web-api.openapi.json:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `tools`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone file `schemas/web-api.openapi.json` missing generated header
   Fix: add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Rerun: `just fast`
   Fingerprint: `sha256:9df0d66fbaa197a39863c0969d092e27fc5feb758655502906dab41041cbbf54`
   Evidence: generated zone integrity violation
75. `high` `generated` `schemas/websocket-events.schema.json:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `tools`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone file `schemas/websocket-events.schema.json` missing generated header
   Fix: add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Rerun: `just fast`
   Fingerprint: `sha256:4afb01cc719ff6306ba5a1780c89bb34680f360faed096b547400882b2d9c79d`
   Evidence: generated zone integrity violation
76. `medium` `proof` `src/api/review.rs:144`
   Rule: `HLT-027-HUMAN-REVIEW-EVIDENCE-GAP`
   Check: `HLT-027-HUMAN-REVIEW-EVIDENCE-GAP:proof` `soft` confidence `0.88`
   Route: TLR `Repair`, lane `audit`, owner `workspace`
   Docs: `docs/testing.md`
   Matched term: `review evidence`
   Reason: proof and review claims need receipts
   Fix: attach raw CI logs, review receipts, and replayable commands instead of accepting claims or summaries
   Rerun: `just score`
   Fingerprint: `sha256:792ee9a1f3f4c7c060ac73e7a27f566a85158c774815c97ba5aac7a16c98780b`
   Evidence: body_markdown: "LGTM".into(),
77. `high` `vibe` `src/web/command.rs:31`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stub` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:12c4299245c41fdbe3ccc54b0b8ae35116f49d693a474b293dd0b3e5609c0875`
   Evidence: src/web/command.rs:31, future-hostile/dead-language term `stub` appears
78. `high` `vibe` `src/web/command.rs:36`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stub` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:1da1a79742f3f8ee915159428e6f6fee1836b79a3fec056e357419896e2ad117`
   Evidence: src/web/command.rs:36, future-hostile/dead-language term `stub` appears
79. `high` `vibe` `src/web/command.rs:126`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `legacy` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:affb5bcbb64a2336a9b515b1c25ab0d011c312b07f27822a9682e935fb6c1440`
   Evidence: src/web/command.rs:126, future-hostile/dead-language term `legacy` appears
80. `high` `vibe` `src/web/error.rs:58`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stale` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:fe0bb1477a22423b2a619d405dce522d73ecf113205054720002de34836e7b53`
   Evidence: src/web/error.rs:58, future-hostile/dead-language term `stale` appears
81. `high` `vibe` `src/web/error.rs:60`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stale` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:a1344d309bd3dd2a6c51b1191dc87151eff2074af51cc8c583b8923b54394155`
   Evidence: src/web/error.rs:60, future-hostile/dead-language term `stale` appears
82. `high` `vibe` `src/web/rest/merge_requests.rs:209`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:834a5f159ac1a584d7526359c9656edb734f63af595c1d7f7e460f410c02b329`
   Evidence: src/web/rest/merge_requests.rs:209, future-hostile/dead-language term `placeholder` appears
83. `high` `vibe` `src/web/rest/merge_requests.rs:243`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:520dd92a1e8a139e3ba11b6a49bcd2c2a15902687f3ff96b11a823d44485c64f`
   Evidence: src/web/rest/merge_requests.rs:243, future-hostile/dead-language term `placeholder` appears
84. `high` `vibe` `src/web/router.rs:40`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `legacy` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:4aef3e67f3def962e282285ed53b1b9b2f4714bcaca54c198585b5d8ad7761e2`
   Evidence: src/web/router.rs:40, future-hostile/dead-language term `legacy` appears
85. `high` `vibe` `src/web/router.rs:226`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `legacy` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:a76687b4480463020564568cae1b117dc3ff8affa7d2282edee86c2c2125c082`
   Evidence: src/web/router.rs:226, future-hostile/dead-language term `legacy` appears
86. `high` `vibe` `src/web/static_assets.rs:53`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `fallback` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:aff55d186741ddb852572254a91426316d2f7b02bd3bc476e817386aed48e593`
   Evidence: src/web/static_assets.rs:53, future-hostile/dead-language term `fallback` appears

## Policy

- Policy file: `./agent/audit-policy.toml`
- Minimum score: `85`
- Fail on: `critical, high`

## Agent Fix Queue

1. `high` `HLT-002-GENERATED-MUTATION` `agent/generated-zones.toml` - regenerate `contracts/generated/*.ts` using the declared command, or remove the zone entry if the file was deleted intentionally
   Route: `Contracts/data`/`contract`
2. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/e2e/pages/RepositoriesPage.ts` - remove the broad suppression or scope it to a single justified line
   Route: `Contracts/data`/`fast`
3. `high` `HLT-019-STREAMING-RUNTIME-DRIFT` `apps/web/playwright.config.ts` - move Kafka/Tansu/Iggy/Fluvio/NATS/Redis-stream clients behind `crates/adapters/queues` or document a brownfield exception with owner, expiry, and migration path
   Route: `Contracts/data`/`db`
4. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/src/api/client.ts` - validate the value first, then narrow it with a proof-aware decoder
   Route: `Contracts/data`/`fast`
5. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/src/api/websocket.ts` - validate the value first, then narrow it with a proof-aware decoder
   Route: `Contracts/data`/`fast`
6. `high` `HLT-006-DIRECT-DB-WRONG-LAYER` `apps/web/src/components/browser/BranchSelector.tsx` - move SQL and DB clients to `crates/adapters` or `db/`; expose typed application/domain APIs upward
   Route: `Contracts/data`/`db`
7. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/src/components/browser/MarkdownRenderer.tsx` - replace the dynamic sink with a bounded parser, sanitizer, or typed renderer
   Route: `Contracts/data`/`fast`
8. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/src/hooks/useBootstrap.ts` - validate the value first, then narrow it with a proof-aware decoder
   Route: `Contracts/data`/`fast`
9. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/src/hooks/useSearch.ts` - validate the value first, then narrow it with a proof-aware decoder
   Route: `Contracts/data`/`fast`
10. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/src/pages/SearchResultsPage.tsx` - remove the broad suppression or scope it to a single justified line
   Route: `Contracts/data`/`fast`
11. `high` `HLT-031-TYPESCRIPT-BAD-BEHAVIOR` `apps/web/src/stores/preferencesStore.ts` - validate the value first, then narrow it with a proof-aware decoder
   Route: `Contracts/data`/`fast`
12. `high` `HLT-002-GENERATED-MUTATION` `contracts/generated/SettingsDiffPreview.ts` - add `agent/generated-zones.toml`, require generated/do-not-edit markers, and route repairs to the source contract
   Route: `Contracts/data`/`contract`
13. `high` `HLT-002-GENERATED-MUTATION` `schemas/web-api.openapi.json` - add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Route: `Contracts/data`/`contract`
14. `high` `HLT-002-GENERATED-MUTATION` `schemas/websocket-events.schema.json` - add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Route: `Contracts/data`/`contract`
15. `medium` `HLT-007-HANDWRITTEN-CONTRACT` `agent/boundaries.toml` - add generated contracts and boundary checks for public APIs, data access, and cross-runtime seams
   Route: `Contracts/data`/`contract`
16. `medium` `HLT-006-DIRECT-DB-WRONG-LAYER` `db/` - move durable truth into migrations, constraints, adapters, and application-owned transactions
   Route: `Contracts/data`/`db`
17. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/web.yml` - extract the workflow steps into ops/ci/<lane>.sh and call them with `bash ops/ci/<lane>.sh`
   Route: `Verification`/`fast`
18. `high` `HLT-004-UNMAPPED-PROOF` `agent/test-map.json` - add the narrowest stable prefix and runnable proof command to `agent/test-map.json`
   Route: `Verification`/`fast`
19. `high` `HLT-008-FALSE-GREEN-RISK` `apps/web/e2e/03-readme.spec.ts` - replace false-green tests with behavior assertions, red/green evidence, and mutation or fault checks for changed behavior
   Route: `Verification`/`fast`
20. `medium` `HLT-027-HUMAN-REVIEW-EVIDENCE-GAP` `src/api/review.rs` - attach raw CI logs, review receipts, and replayable commands instead of accepting claims or summaries
   Route: `Repair`/`audit`
21. `high` `HLT-003-OWNERLESS-PATH` `agent/owner-map.json` - add the narrowest stable prefix for this path to `agent/owner-map.json`
   Route: `Context/setup`/`fast`
22. `high` `apps/web/eslint.config.js` - move product runtime behavior to Rust core, TypeScript web, SQL migrations, or generated contracts; Python needs a dated advanced-ML/data exception
   Route: `Context/setup`/`audit`
23. `high` `HLT-034-CI-BAD-BEHAVIOR` `.github/workflows/web.yml` - remove the non-blocking override so scan failures stop the pipeline
   Route: `Security, secrets, agency`/`security`
24. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - add workflow-level concurrency with cancel-in-progress
   Route: `Security, secrets, agency`/`security`
25. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - add top-level `permissions: contents: read` and job-specific write scopes only where needed
   Route: `Security, secrets, agency`/`security`
26. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - set an explicit timeout-minutes on each job
   Route: `Security, secrets, agency`/`security`
27. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - limit the path to build outputs and keep credential files out of caches and artifacts
   Route: `Security, secrets, agency`/`security`
28. `high` `HLT-001-DEAD-MARKER` `apps/web/e2e/fixtures/auth.ts` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
29. `high` `HLT-001-DEAD-MARKER` `apps/web/e2e/fixtures/mocks.ts` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
30. `high` `HLT-001-DEAD-MARKER` `apps/web/e2e/pages/RepositoriesPage.ts` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
31. `high` `HLT-039-WEB-SECURITY-BAD-BEHAVIOR` `apps/web/eslint.config.js` - prefer HttpOnly Secure SameSite cookies or a bounded in-memory token flow with documented threat model
   Route: `Security, secrets, agency`/`security`
32. `high` `HLT-001-DEAD-MARKER` `apps/web/eslint.config.js` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
33. `high` `HLT-001-DEAD-MARKER` `apps/web/src/api/client.ts` - collapse fallback chains into explicit typed states with bounded retry policy, telemetry, and documented repair guidance
   Route: `Entropy`/`fast`
34. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/browser/BranchSelector.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
35. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/browser/CodeViewer.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
36. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/merge/DiffViewer.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
37. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/merge/InlineComment.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
38. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/merge/MergeGatePanel.stories.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
39. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/merge/ReviewSidebar.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
40. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/repo/CreateRepoDialog.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
41. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/settings/AgentPolicyEditor.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
42. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/settings/BranchProtectionEditor.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
43. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/settings/MergePolicyEditor.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
44. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/settings/SettingsDiffPreview.stories.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
45. `high` `HLT-001-DEAD-MARKER` `apps/web/src/components/settings/__tests__/SettingsDiffPreview.test.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
46. `high` `HLT-001-DEAD-MARKER` `apps/web/src/layout/CommandPalette.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
47. `high` `HLT-001-DEAD-MARKER` `apps/web/src/pages/NotFoundPage.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
48. `high` `HLT-001-DEAD-MARKER` `apps/web/src/pages/RepositoriesPage.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
49. `high` `HLT-001-DEAD-MARKER` `apps/web/src/pages/RepositoryCodePage.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
50. `high` `HLT-001-DEAD-MARKER` `apps/web/src/pages/SearchResultsPage.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
51. `high` `HLT-001-DEAD-MARKER` `apps/web/src/pages/StubPage.tsx` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
52. `high` `HLT-001-DEAD-MARKER` `apps/web/src/pages/StubPage.tsx` - replace placeholders with implemented behavior, typed unsupported-state errors, or a tracked exception record with docs
   Route: `Entropy`/`fast`
53. `high` `HLT-039-WEB-SECURITY-BAD-BEHAVIOR` `apps/web/src/stores/realtimeStore.ts` - prefer HttpOnly Secure SameSite cookies or a bounded in-memory token flow with documented threat model
   Route: `Security, secrets, agency`/`security`
54. `high` `HLT-001-DEAD-MARKER` `src/web/command.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
55. `high` `HLT-001-DEAD-MARKER` `src/web/error.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
56. `high` `HLT-001-DEAD-MARKER` `src/web/rest/merge_requests.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
57. `high` `HLT-001-DEAD-MARKER` `src/web/router.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
58. `high` `HLT-001-DEAD-MARKER` `src/web/static_assets.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
59. `medium` `HLT-001-DEAD-MARKER` `.` - split large or ambiguous authored code into smaller semantic modules with focused tests
   Route: `Entropy`/`fast`
