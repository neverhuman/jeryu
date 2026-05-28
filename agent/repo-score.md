# jankurai Repo Score

- Standard: `jankurai`
- Auditor: `1.5.1`
- Schema: `1.9.0`
- Paper edition: `2026.05-ed8`
- Target stack ID: `rust-ts-vite-react-redline-bounded-python`
- Target stack: `Rust core + TypeScript/React/Vite + PostgreSQL + generated contracts + exception-only Python AI/data service`
- Repo: `.`
- Run ID: `1779996442`
- Started at: `1779996442`
- Elapsed: `13438` ms
- Scope: `full`
- Raw score: `77`
- Final score: `64`
- Decision: `advisory`
- Minimum score: `85`
- Caps applied: `vibe-placeholders-in-product-code, fallback-soup-in-product-code, future-hostile-dead-language-in-product-code, generated-zone-mutation-risk, direct-db-access-from-wrong-layer, rust-bad-behavior, ci-bad-behavior, ci-local-parity`

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
| `non-optimal-product-language-found` | 74 | no |
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
| `false-green-test-risk` | 76 | no |
| `destructive-migration-risk` | 70 | no |
| `authz-or-data-isolation-gap` | 78 | no |
| `input-boundary-gap` | 78 | no |
| `agent-tool-supply-chain-gap` | 78 | no |
| `release-readiness-gap` | 80 | no |
| `missing-rust-property-or-integration-tests` | 82 | no |
| `no-agent-friendly-exception-pattern` | 76 | no |
| `missing-agent-readable-docs` | 80 | no |
| `streaming-runtime-drift` | 78 | no |
| `rust-bad-behavior` | 72 | yes |
| `sql-bad-behavior` | 72 | no |
| `typescript-bad-behavior` | 72 | no |
| `docker-bad-behavior` | 72 | no |
| `python-bad-behavior` | 72 | no |
| `ci-bad-behavior` | 70 | yes |
| `git-bad-behavior` | 70 | no |
| `gittools-bad-behavior` | 70 | no |
| `release-bad-behavior` | 70 | no |
| `web-security-bad-behavior` | 68 | no |
| `repo-rot-bad-behavior` | 88 | no |
| `comment-hygiene-dangerous-residue` | 72 | no |
| `ci-local-parity` | 70 | yes |

## Copy-Code Redundancy

- Status: `review` hard=`0` warning=`36` files=`549`
- Policy: min-lines=`10` min-tokens=`100` max-findings=`50` include-tests=`false` strict=`false`
- Duplicate volume: lines=`64` tokens=`177` bytes=`1830`

- Notes:
  - hard classes are limited to exact active-source file matches and substantial exact same-name units
  - warning classes include same-body different-name units and token/block duplication
  - tests, fixtures, stories, config, Docker, and migrations are omitted unless --include-tests is set

| Kind | Severity | Language | Lines | Tokens | Instances | Reason |
| --- | --- | --- | ---: | ---: | --- | --- |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `crates/cargo-witness/src/diagnose_workspace.rs:166-167, src/api/agent_session.rs:123-124, src/api/agent_session.rs:198-199, src/api/entity.rs:122-123, src/api/entity.rs:131-132, src/api/entity.rs:154-155, src/api/entity.rs:164-165, src/api/snapshot.rs:87-88, src/api/snapshot.rs:140-141, src/bugtracker/types_enums.rs:71-72, src/bugtracker/types_enums.rs:158-159, src/runtime_support/mod.rs:57-58, src/runtime_support/mod.rs:74-75, src/runtime_support/mod.rs:103-104, src/runtime_support/mod.rs:131-132, src/runtime_support/mod.rs:138-139, src/runtime_support/mod.rs:172-173` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 4 | `db/state.rs:1852-1853, db/state.rs:1915-1916, db/state.rs:1950-1951, db/state.rs:2044-2045, db/state.rs:2057-2058, db/state.rs:2116-2117, db/state.rs:2215-2216, db/state.rs:2773-2774, db/state.rs:3124-3125, db/state.rs:3151-3152, db/state.rs:3178-3179, db/state.rs:3197-3198` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 6 | 22 | `crates/cargo-aer/src/helpers.rs:93-99, crates/cargo-vrc/src/planner_support_paths.rs:168-174` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 6 | 18 | `src/web/rest/merge_requests.rs:605-611, src/web/rest/reviews.rs:208-214` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 3 | `src/web/rest/issues.rs:36-38, src/web/rest/issues.rs:50-52, src/web/rest/issues.rs:67-69, src/web/rest/issues.rs:84-86` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `src/access.rs:369-370, src/access.rs:381-382, src/access.rs:401-402, src/access.rs:428-429, src/access.rs:462-463, src/access.rs:474-475` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 5 | 11 | `src/repos/service.rs:253-258, src/repos/settings.rs:227-232` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 4 | 19 | `src/web/csrf.rs:31-35, src/web/rest/auth.rs:242-246` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 3 | `src/capability_execute.rs:86-88, src/capability_inspect.rs:20-22, src/capability_inspect_read.rs:132-134` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `src/access.rs:333-334, src/access.rs:350-351, src/access.rs:413-414, src/access.rs:440-441` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/bugtracker/types_enums.rs:71-72, src/bugtracker/types_enums.rs:158-159, src/runtime_support/mod.rs:57-58, src/runtime_support/mod.rs:103-104` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/api/agent_session.rs:123-124, src/api/entity.rs:122-123, src/api/entity.rs:154-155, src/api/snapshot.rs:140-141` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/api/agent_session.rs:198-199, src/api/entity.rs:131-132, src/api/entity.rs:164-165, src/api/snapshot.rs:87-88` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 10 | `src/merge/review.rs:385-387, src/merge/service.rs:400-402` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `src/git_host/gitlab.rs:130-131, src/git_host/gitlab.rs:142-143, src/git_host/gitlab.rs:163-164` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `src/access.rs:1079-1080, src/access.rs:1116-1117, src/access.rs:1143-1144` | `same body appears under different names across files` |
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
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 1 | `src/access.rs:855-856, src/access.rs:890-891, src/config_paths.rs:101-102` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 1 | `crates/cargo-aer/src/helpers.rs:101-103, crates/cargo-vrc/src/planner_support_paths.rs:176-178` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 1 | `src/autonomy/policy_yaml_types.rs:218-220, src/node_types.rs:80-82` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `src/access.rs:490-491, src/access.rs:498-499` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `db/state.rs:2289-2290, db/state.rs:2967-2968` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 4 | `src/runtime_support/mod.rs:64-65, src/runtime_support/mod.rs:110-111` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 3 | `src/access.rs:1061-1062, src/access.rs:1093-1094` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `typescript` | 1 | 3 | `apps/web/src/test/mocks.ts:11-12, apps/web/src/test/server.ts:27-28` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `db/state.rs:672-673, db/state.rs:679-680` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 1 | `src/api/entity.rs:195-196, src/gateway/singleflight.rs:64-65` | `same body appears under different names across files` |

## Dimensions

| Dimension | Weight | Score | Weighted | Evidence |
| --- | ---: | ---: | ---: | --- |
| Ownership and navigation surface | 13 | 100 | 13.00 | root `AGENTS.md` present; `CODEOWNERS` present |
| Contract and boundary integrity | 13 | 83 | 10.79 | contract surface found; generated contract artifacts found |
| Proof lanes and test routing | 12 | 100 | 12.00 | one-command setup/validation lane found; deterministic fast lane found |
| Security and supply-chain posture | 12 | 86 | 10.32 | lockfile present; secret or dependency scan tooling found |
| Code shape and semantic surface | 12 | 0 | 0.00 | largest authored code file: src/access.rs (1183 LOC); code file exceeds 500 LOC |
| Data truth and workflow safety | 8 | 75 | 6.00 | database surface present; structured db boundary manifest present |
| Observability and repair evidence | 8 | 98 | 7.84 | observability libraries or patterns found; diagnostic shaping hints found |
| Context economy and agent instructions | 7 | 100 | 7.00 | root `AGENTS.md` present; root `AGENTS.md` stays short |
| Jankurai tool adoption and CI replacement | 7 | 30 | 2.10 | control-plane files present; applicable=18 |
| Python containment and polyglot hygiene | 4 | 100 | 4.00 | no Python files in scope |
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
- Tuiwright TUI flows: `1` flow(s) across `1` file(s); assertions=`1` actions=`0` artifacts=`none`

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
- Envelope exit code: `0` · elapsed: `45332` ms · strict: `true`
- Commands — ran: `4`, skipped: `0`, failed: `0`
- Generated at: `1779969580`
- Git HEAD (envelope): `32563da488f82426350f449b9d02026c16086383`

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
   Fingerprint: `sha256:7807f7298839f90215ceda4c9fcc7615e9ccdaa0d72fba0ef824eff77106286c`
   Evidence: largest authored code file: src/access.rs (1183 LOC), code file exceeds 500 LOC, code file exceeds 1000 LOC, most code files stay under 300 LOC
2. `high` `security` `.github/workflows/post-merge-deploy.yml:91`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:b3eec8736aaf282ea4cbfce7232c114dbd353262f3cfa5be38c24bdb38e4d75b`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/post-merge-deploy.yml, line=91, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
3. `high` `security` `.github/workflows/rust.yml:67`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:40e23562fe3963b06fa51735e0472efcbb82857596b2fc62385c20dd6c0b12a4`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=67, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
4. `high` `security` `.github/workflows/rust.yml:110`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:29d6abd7e2a51b6349feb9e6fd7526dff9813d9f7bfbcd4dc20948974e43665f`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=110, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
5. `high` `security` `.github/workflows/rust.yml:147`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:63fd58658a9423421c29b4b9b3108f987494d6ec723d8ee97f8f8146d19c69e7`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=147, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
6. `high` `security` `.github/workflows/rust.yml:181`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:d919f1edd0f50570817eb7e2a591f96abb2988afbc06ac95c57a6bffcbd6534e`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=181, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
7. `high` `security` `.github/workflows/rust.yml:207`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:8484d416d216016c52228a54d86d866d709cdec715a7cfd54a8847e60056269c`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=207, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
8. `high` `security` `.github/workflows/rust.yml:227`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:70b655f322230c4c519124f184c8f191b97265ca0624578bca07b175404021c6`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=227, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
9. `high` `security` `.github/workflows/rust.yml:285`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:95e4d7c8438f6896446d60be589d90743262db5218c9194552ba45799ab1ff6c`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=285, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
10. `high` `security` `.github/workflows/rust.yml:311`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:151f99f1b699ecf29ddb9473ab8b1c45d212a4ae4efa92a9b22fcc7ac6b168e6`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=311, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
11. `high` `security` `.github/workflows/rust.yml:341`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:cef8d7b03d4329da5ff798da23b416438a548161be7c3bcfd351fd4be72e18fd`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=341, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
12. `high` `security` `.github/workflows/rust.yml:367`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:5e690a4d37f893ca91a1aef3a62a81be04e41daf7ffa3b65adf7a378593f4289`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=367, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
13. `high` `security` `.github/workflows/rust.yml:395`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:0882c908e32e1dcb2d597695e22cf7bdc0c094469f2047ff9ccccbf212381b34`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=395, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
14. `high` `security` `.github/workflows/rust.yml:424`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:fdc31dd3e9319eb79d9fc127af79fcf41b152113119c144239fbfaa3d322c4ad`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=424, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
15. `high` `security` `.github/workflows/rust.yml:456`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.secret.echo-or-debug`
   Reason: secret-bearing workflow step writes sensitive values to logs
   Fix: never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Rerun: `just security`
   Fingerprint: `sha256:27005ace6bb0330c18d826f57209e5b6e698751e4d3fd95dbe54035c4086bce4`
   Evidence: detector=ci.secret.echo-or-debug, path=.github/workflows/rust.yml, line=456, proof_window=None, snippet=else echo "no passwordless sudo; skipping mold"; fi
16. `high` `ci` `.github/workflows/web.yml:37`
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
17. `high` `security` `.github/workflows/web.yml:273`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.security-scan.nonblocking`
   Reason: security or proof job is explicitly non-blocking
   Fix: remove the non-blocking override so scan failures stop the pipeline
   Rerun: `just security`
   Fingerprint: `sha256:149d5a7aa9b26779d886c8446194ab1c80cc64935c29a3375592e58395615f05`
   Evidence: detector=ci.security-scan.nonblocking, path=.github/workflows/web.yml, line=273, proof_window=None, snippet=kill "$bff_pid" || true
18. `high` `security` `.gitlab-ci.yml:1`
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
19. `high` `security` `.gitlab-ci.yml:1`
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
20. `high` `security` `.gitlab-ci.yml:1`
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
21. `high` `security` `.gitlab-ci.yml:218`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.security-scan.nonblocking`
   Reason: security or proof job is explicitly non-blocking
   Fix: remove the non-blocking override so scan failures stop the pipeline
   Rerun: `just security`
   Fingerprint: `sha256:dcb36edcf1522659b286efd82a13f2f0c125e04eb961f1a762fec9425ba9553b`
   Evidence: detector=ci.security-scan.nonblocking, path=.gitlab-ci.yml, line=218, proof_window=None, snippet=allow_failure: true
22. `high` `security` `.gitlab-ci.yml:235`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.security-scan.nonblocking`
   Reason: security or proof job is explicitly non-blocking
   Fix: remove the non-blocking override so scan failures stop the pipeline
   Rerun: `just security`
   Fingerprint: `sha256:a7105e5582400a151d74893c4f2beec4792537bf8040a5f47789a7b3ff41291d`
   Evidence: detector=ci.security-scan.nonblocking, path=.gitlab-ci.yml, line=235, proof_window=None, snippet=allow_failure: true
23. `high` `security` `.gitlab-ci.yml:261`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.security-scan.nonblocking`
   Reason: security or proof job is explicitly non-blocking
   Fix: remove the non-blocking override so scan failures stop the pipeline
   Rerun: `just security`
   Fingerprint: `sha256:d1b9464fc09bdfe6e501d7eaf4af6d19537dbdd7bdc47b5b9f81990f339c789f`
   Evidence: detector=ci.security-scan.nonblocking, path=.gitlab-ci.yml, line=261, proof_window=None, snippet=allow_failure: true
24. `high` `security` `.gitlab-ci.yml:296`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.artifact.cache.secret-path`
   Reason: workflow stores a secret-bearing path in cache or artifact upload
   Fix: limit the path to build outputs and keep credential files out of caches and artifacts
   Rerun: `just security`
   Fingerprint: `sha256:8dc2ebd4248ff9ab5a55b01446d634232d48ca999485a0411b0eebde02ee72df`
   Evidence: detector=ci.artifact.cache.secret-path, path=.gitlab-ci.yml, line=296, proof_window=None, snippet=GITHUB_OUTPUT: "$CI_PROJECT_DIR/.release-resolve.env"
25. `high` `security` `.gitlab-ci.yml:310`
   Rule: `HLT-034-CI-BAD-BEHAVIOR`
   Check: `HLT-034-CI-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/testing.md`
   Matched term: `ci.artifact.cache.secret-path`
   Reason: workflow stores a secret-bearing path in cache or artifact upload
   Fix: limit the path to build outputs and keep credential files out of caches and artifacts
   Rerun: `just security`
   Fingerprint: `sha256:2addd84cfedde703d1be57b6b94063e8c244ea549f0db5d17c23e800c41af73c`
   Evidence: detector=ci.artifact.cache.secret-path, path=.gitlab-ci.yml, line=310, proof_window=None, snippet=dotenv: .release-resolve.env
26. `medium` `boundary` `agent/boundaries.toml`
   Rule: `HLT-007-HANDWRITTEN-CONTRACT`
   Check: `HLT-007-HANDWRITTEN-CONTRACT:boundary` `soft` confidence `0.76`
   Route: TLR `Contracts/data`, lane `contract`, owner `agent`
   Docs: `docs/audit-rubric.md#known-vibe-coding-insults`
   Reason: `Contract and boundary integrity` scored 83 below the standard floor of 85
   Fix: add generated contracts and boundary checks for public APIs, data access, and cross-runtime seams
   Rerun: `just fast`
   Fingerprint: `sha256:ee08873e431007d2b4d353e99836fccc5865b0fe11b85dfef7f0ef45df654f81`
   Evidence: contract surface found, generated contract artifacts found, polyglot boundary layout present, boundary manifest present
27. `high` `generated` `agent/generated-zones.toml:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone file `contracts/generated/*.ts` is missing
   Fix: regenerate `contracts/generated/*.ts` using the declared command, or remove the zone entry if the file was deleted intentionally
   Rerun: `just fast`
   Fingerprint: `sha256:8e76087395e4444ba8f23ebc426af795e80f7f4583a4f24e6810a36dde53809f`
   Evidence: generated zone integrity violation
28. `high` `generated` `contracts/generated/SettingsDiffPreview.ts:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `contracts`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone is not protected strongly enough against hand edits
   Fix: add `agent/generated-zones.toml`, require generated/do-not-edit markers, and route repairs to the source contract
   Rerun: `just fast`
   Fingerprint: `sha256:ae83c2c666a4a8b7e3af664f02e66f9d546ad7ced4563b4b8636120bac46c7eb`
   Evidence: generated file contains TODO/stub markers
29. `medium` `data` `db/`
   Rule: `HLT-006-DIRECT-DB-WRONG-LAYER`
   Check: `HLT-006-DIRECT-DB-WRONG-LAYER:data` `soft` confidence `0.76`
   Route: TLR `Contracts/data`, lane `db`, owner `data`
   Docs: `docs/audit-rubric.md#required-shape`
   Reason: `Data truth and workflow safety` scored 75 below the standard floor of 85
   Fix: move durable truth into migrations, constraints, adapters, and application-owned transactions
   Rerun: `just fast`
   Fingerprint: `sha256:9363f8264162b95439ce1a8ccfb0913a811deffe70879f2092632cf0f0007bc5`
   Evidence: database surface present, structured db boundary manifest present, db boundary routes roots, migrations, and constraints, migration directory present
30. `high` `generated` `schemas/web-api.openapi.json:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `contracts`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone file `schemas/web-api.openapi.json` missing generated header
   Fix: add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Rerun: `just fast`
   Fingerprint: `sha256:9df0d66fbaa197a39863c0969d092e27fc5feb758655502906dab41041cbbf54`
   Evidence: generated zone integrity violation
31. `high` `generated` `schemas/websocket-events.schema.json:1`
   Rule: `HLT-002-GENERATED-MUTATION`
   Check: `HLT-002-GENERATED-MUTATION:generated` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `contract`, owner `contracts`
   Docs: `agent/JANKURAI_STANDARD.md#generated-zones`
   Reason: generated zone file `schemas/websocket-events.schema.json` missing generated header
   Fix: add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Rerun: `just fast`
   Fingerprint: `sha256:4afb01cc719ff6306ba5a1780c89bb34680f360faed096b547400882b2d9c79d`
   Evidence: generated zone integrity violation
32. `high` `vibe` `src/access.rs:428`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:4cd06cabc3afa2e633e2b05d271486e11972173062904caa85192d3825fec14b`
   Evidence: src/access.rs:428, future-hostile/dead-language term `old` appears
33. `high` `vibe` `src/access.rs:431`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:f5afd127b84ac119b8b65bb9fbcaf2a7af7cac22e288178582325fd35b87fdba`
   Evidence: src/access.rs:431, future-hostile/dead-language term `old` appears
34. `high` `vibe` `src/access.rs:433`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:8d8f1262def75c699ec96937ce5da4d088124d97098219b68e1a87bde6ea86ad`
   Evidence: src/access.rs:433, future-hostile/dead-language term `old` appears
35. `high` `vibe` `src/access.rs:435`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `old` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:c01f4b8d22b9662b0e2015d6ca000e3781776750d137ebf7364f6b3f6a8a2193`
   Evidence: src/access.rs:435, future-hostile/dead-language term `old` appears
36. `high` `security` `src/access.rs:988`
   Rule: `HLT-029-RUST-BAD-BEHAVIOR`
   Check: `HLT-029-RUST-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#top-level-risk-mapping`
   Matched term: `rust.unsafe.undocumented-block`
   Reason: no nearby SAFETY comment was found
   Fix: add a precise `SAFETY:` comment or remove the unsafe block
   Rerun: `just fast`
   Fingerprint: `sha256:4e715b93ae494c829dd8b6d14f486bbe06019024dead5b1940b1349d9a610dad`
   Evidence: detector=unsafe {, proof-window=NearbySafetyComment, snippet=unsafe {
37. `high` `security` `src/access.rs:1006`
   Rule: `HLT-029-RUST-BAD-BEHAVIOR`
   Check: `HLT-029-RUST-BAD-BEHAVIOR:security` `hard` confidence `0.95`
   Route: TLR `Security, secrets, agency`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#top-level-risk-mapping`
   Matched term: `rust.unsafe.undocumented-block`
   Reason: no nearby SAFETY comment was found
   Fix: add a precise `SAFETY:` comment or remove the unsafe block
   Rerun: `just fast`
   Fingerprint: `sha256:4e715b93ae494c829dd8b6d14f486bbe06019024dead5b1940b1349d9a610dad`
   Evidence: detector=unsafe {, proof-window=NearbySafetyComment, snippet=unsafe {
38. `medium` `proof` `src/api/review.rs:144`
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
39. `high` `vibe` `src/bin/jeryu_export_schemas.rs:34`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: fallback soup detected in product code
   Fix: collapse fallback chains into explicit typed states with bounded retry policy, telemetry, and documented repair guidance
   Rerun: `just fast`
   Fingerprint: `sha256:392c3ae390d2a10557b6fb60b8ec70d4aa89c64428adc8c2a6e469f7d2fdde38`
   Evidence: src/bin/jeryu_export_schemas.rs:34 let out_dir = std::env::var("JERYU_SCHEMA_OUT_DIR").unwrap_or_else(|_| "schemas".into());
40. `high` `data` `src/git_host/gitlab.rs:1`
   Rule: `HLT-006-DIRECT-DB-WRONG-LAYER`
   Check: `HLT-006-DIRECT-DB-WRONG-LAYER:data` `hard` confidence `0.95`
   Route: TLR `Contracts/data`, lane `db`, owner `evidence-gate`
   Docs: `docs/audit-rubric.md#required-shape`
   Reason: direct database access appears in a wrong layer
   Fix: move SQL and DB clients to `crates/adapters` or `db/`; expose typed application/domain APIs upward
   Rerun: `just fast`
   Fingerprint: `sha256:1621abfcc8630fc03ca49b10c5d92ccd21d7e9cc888b1039f8da16902d4276de`
   Evidence: DB marker in non-adapter layer
41. `high` `vibe` `src/merge/service.rs:320`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: product code contains TODO/stub/unimplemented/unreachable placeholder markers
   Fix: replace placeholders with implemented behavior, typed unsupported-state errors, or a tracked exception record with docs
   Rerun: `just fast`
   Fingerprint: `sha256:e38bfe094384da98db51b289fc5e2ff1418a41bb6e0f54132dc013f783f2008a`
   Evidence: src/merge/service.rs:320 "mr.close not implemented in Phase 3 (host adapter pending)".into(),
42. `high` `vibe` `src/web/command.rs:31`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stub` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:12c4299245c41fdbe3ccc54b0b8ae35116f49d693a474b293dd0b3e5609c0875`
   Evidence: src/web/command.rs:31, future-hostile/dead-language term `stub` appears
43. `high` `vibe` `src/web/command.rs:36`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `stub` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:1da1a79742f3f8ee915159428e6f6fee1836b79a3fec056e357419896e2ad117`
   Evidence: src/web/command.rs:36, future-hostile/dead-language term `stub` appears
44. `high` `vibe` `src/web/command.rs:126`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `legacy` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:affb5bcbb64a2336a9b515b1c25ab0d011c312b07f27822a9682e935fb6c1440`
   Evidence: src/web/command.rs:126, future-hostile/dead-language term `legacy` appears
45. `high` `vibe` `src/web/rest/merge_requests.rs:209`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:834a5f159ac1a584d7526359c9656edb734f63af595c1d7f7e460f410c02b329`
   Evidence: src/web/rest/merge_requests.rs:209, future-hostile/dead-language term `placeholder` appears
46. `high` `vibe` `src/web/rest/merge_requests.rs:243`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `placeholder` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:520dd92a1e8a139e3ba11b6a49bcd2c2a15902687f3ff96b11a823d44485c64f`
   Evidence: src/web/rest/merge_requests.rs:243, future-hostile/dead-language term `placeholder` appears
47. `high` `vibe` `src/web/router.rs:40`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `legacy` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:4aef3e67f3def962e282285ed53b1b9b2f4714bcaca54c198585b5d8ad7761e2`
   Evidence: src/web/router.rs:40, future-hostile/dead-language term `legacy` appears
48. `high` `vibe` `src/web/router.rs:226`
   Rule: `HLT-001-DEAD-MARKER`
   Check: `HLT-001-DEAD-MARKER:vibe` `hard` confidence `0.88`
   Route: TLR `Entropy`, lane `fast`, owner `workspace`
   Docs: `docs/audit-rubric.md#future-hostile-language-rule`
   Reason: future-hostile/dead-language term `legacy` appears in product/runtime code
   Fix: remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Rerun: `just fast`
   Fingerprint: `sha256:a76687b4480463020564568cae1b117dc3ff8affa7d2282edee86c2c2125c082`
   Evidence: src/web/router.rs:226, future-hostile/dead-language term `legacy` appears
49. `high` `vibe` `src/web/static_assets.rs:53`
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
2. `high` `HLT-002-GENERATED-MUTATION` `contracts/generated/SettingsDiffPreview.ts` - add `agent/generated-zones.toml`, require generated/do-not-edit markers, and route repairs to the source contract
   Route: `Contracts/data`/`contract`
3. `high` `HLT-002-GENERATED-MUTATION` `schemas/web-api.openapi.json` - add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Route: `Contracts/data`/`contract`
4. `high` `HLT-002-GENERATED-MUTATION` `schemas/websocket-events.schema.json` - add a `Generated by: <tool>` / `DO NOT EDIT BY HAND` header block with source and regeneration command
   Route: `Contracts/data`/`contract`
5. `high` `HLT-006-DIRECT-DB-WRONG-LAYER` `src/git_host/gitlab.rs` - move SQL and DB clients to `crates/adapters` or `db/`; expose typed application/domain APIs upward
   Route: `Contracts/data`/`db`
6. `medium` `HLT-007-HANDWRITTEN-CONTRACT` `agent/boundaries.toml` - add generated contracts and boundary checks for public APIs, data access, and cross-runtime seams
   Route: `Contracts/data`/`contract`
7. `medium` `HLT-006-DIRECT-DB-WRONG-LAYER` `db/` - move durable truth into migrations, constraints, adapters, and application-owned transactions
   Route: `Contracts/data`/`db`
8. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/web.yml` - extract the workflow steps into ops/ci/<lane>.sh and call them with `bash ops/ci/<lane>.sh`
   Route: `Verification`/`fast`
9. `medium` `HLT-027-HUMAN-REVIEW-EVIDENCE-GAP` `src/api/review.rs` - attach raw CI logs, review receipts, and replayable commands instead of accepting claims or summaries
   Route: `Repair`/`audit`
10. `high` `HLT-034-CI-BAD-BEHAVIOR` `.github/workflows/post-merge-deploy.yml` - never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Route: `Security, secrets, agency`/`security`
11. `high` `HLT-034-CI-BAD-BEHAVIOR` `.github/workflows/rust.yml` - never echo secrets; pass them directly to trusted binaries and keep shell tracing off
   Route: `Security, secrets, agency`/`security`
12. `high` `HLT-034-CI-BAD-BEHAVIOR` `.github/workflows/web.yml` - remove the non-blocking override so scan failures stop the pipeline
   Route: `Security, secrets, agency`/`security`
13. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - add workflow-level concurrency with cancel-in-progress
   Route: `Security, secrets, agency`/`security`
14. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - add top-level `permissions: contents: read` and job-specific write scopes only where needed
   Route: `Security, secrets, agency`/`security`
15. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - set an explicit timeout-minutes on each job
   Route: `Security, secrets, agency`/`security`
16. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - remove the non-blocking override so scan failures stop the pipeline
   Route: `Security, secrets, agency`/`security`
17. `high` `HLT-034-CI-BAD-BEHAVIOR` `.gitlab-ci.yml` - limit the path to build outputs and keep credential files out of caches and artifacts
   Route: `Security, secrets, agency`/`security`
18. `high` `HLT-001-DEAD-MARKER` `src/access.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
19. `high` `HLT-029-RUST-BAD-BEHAVIOR` `src/access.rs` - add a precise `SAFETY:` comment or remove the unsafe block
   Route: `Security, secrets, agency`/`fast`
20. `high` `HLT-001-DEAD-MARKER` `src/bin/jeryu_export_schemas.rs` - collapse fallback chains into explicit typed states with bounded retry policy, telemetry, and documented repair guidance
   Route: `Entropy`/`fast`
21. `high` `HLT-001-DEAD-MARKER` `src/merge/service.rs` - replace placeholders with implemented behavior, typed unsupported-state errors, or a tracked exception record with docs
   Route: `Entropy`/`fast`
22. `high` `HLT-001-DEAD-MARKER` `src/web/command.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
23. `high` `HLT-001-DEAD-MARKER` `src/web/rest/merge_requests.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
24. `high` `HLT-001-DEAD-MARKER` `src/web/router.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
25. `high` `HLT-001-DEAD-MARKER` `src/web/static_assets.rs` - remove or rename the marker, implement the intended behavior, model a typed unsupported state, or move docs/generated/vendor/product-copy text into an allowlisted context
   Route: `Entropy`/`fast`
26. `medium` `HLT-001-DEAD-MARKER` `.` - split large or ambiguous authored code into smaller semantic modules with focused tests
   Route: `Entropy`/`fast`
