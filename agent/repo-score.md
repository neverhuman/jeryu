# jankurai Repo Score

- Standard: `jankurai`
- Auditor: `1.5.1`
- Schema: `1.9.0`
- Paper edition: `2026.05-ed8`
- Target stack ID: `rust-ts-vite-react-redline-bounded-python`
- Target stack: `Rust core + TypeScript/React/Vite + PostgreSQL + generated contracts + exception-only Python AI/data service`
- Repo: `.`
- Run ID: `1779483855`
- Started at: `1779483855`
- Elapsed: `6881` ms
- Scope: `full`
- Raw score: `89`
- Final score: `89`
- Decision: `advisory`
- Minimum score: `85`
- Caps applied: `none`

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
| `vibe-placeholders-in-product-code` | 68 | no |
| `fallback-soup-in-product-code` | 70 | no |
| `future-hostile-dead-language-in-product-code` | 64 | no |
| `severe-duplication-in-product-code` | 70 | no |
| `generated-zone-mutation-risk` | 76 | no |
| `direct-db-access-from-wrong-layer` | 66 | no |
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
| `rust-bad-behavior` | 72 | no |
| `sql-bad-behavior` | 72 | no |
| `typescript-bad-behavior` | 72 | no |
| `docker-bad-behavior` | 72 | no |
| `python-bad-behavior` | 72 | no |
| `ci-bad-behavior` | 70 | no |
| `git-bad-behavior` | 70 | no |
| `gittools-bad-behavior` | 70 | no |
| `release-bad-behavior` | 70 | no |
| `web-security-bad-behavior` | 68 | no |
| `repo-rot-bad-behavior` | 88 | no |
| `comment-hygiene-dangerous-residue` | 72 | no |
| `ci-local-parity` | 70 | no |

## Copy-Code Redundancy

- Status: `review` hard=`0` warning=`22` files=`412`
- Policy: min-lines=`10` min-tokens=`100` max-findings=`50` include-tests=`false` strict=`false`
- Duplicate volume: lines=`34` tokens=`86` bytes=`866`

- Notes:
  - hard classes are limited to exact active-source file matches and substantial exact same-name units
  - warning classes include same-body different-name units and token/block duplication
  - tests, fixtures, stories, config, Docker, and migrations are omitted unless --include-tests is set

| Kind | Severity | Language | Lines | Tokens | Instances | Reason |
| --- | --- | --- | ---: | ---: | --- | --- |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `crates/cargo-witness/src/diagnose_workspace.rs:166-167, src/api/agent_session.rs:123-124, src/api/agent_session.rs:198-199, src/api/entity.rs:118-119, src/api/entity.rs:127-128, src/api/entity.rs:150-151, src/api/entity.rs:160-161, src/api/snapshot.rs:87-88, src/api/snapshot.rs:140-141, src/bugtracker/types_enums.rs:71-72, src/bugtracker/types_enums.rs:158-159, src/runtime_support/mod.rs:57-58, src/runtime_support/mod.rs:74-75, src/runtime_support/mod.rs:103-104, src/runtime_support/mod.rs:131-132, src/runtime_support/mod.rs:138-139, src/runtime_support/mod.rs:172-173` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 4 | `db/state.rs:1690-1691, db/state.rs:1753-1754, db/state.rs:1844-1845, db/state.rs:1857-1858, db/state.rs:1896-1897, db/state.rs:2818-2819, db/state.rs:2845-2846, db/state.rs:2872-2873, db/state.rs:2891-2892, db/state.rs:3300-3301` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 6 | 22 | `crates/cargo-aer/src/helpers.rs:93-99, crates/cargo-vrc/src/planner_support_paths.rs:168-174` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 3 | `src/capability_execute.rs:86-88, src/capability_inspect.rs:20-22, src/capability_inspect_read.rs:132-134` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `db/state.rs:2129-2130, db/state.rs:2918-2919, db/state.rs:2956-2957, db/state.rs:3058-3059` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/bugtracker/types_enums.rs:71-72, src/bugtracker/types_enums.rs:158-159, src/runtime_support/mod.rs:57-58, src/runtime_support/mod.rs:103-104` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/api/agent_session.rs:123-124, src/api/entity.rs:118-119, src/api/entity.rs:150-151, src/api/snapshot.rs:140-141` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/api/agent_session.rs:198-199, src/api/entity.rs:127-128, src/api/entity.rs:160-161, src/api/snapshot.rs:87-88` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 7 | `src/repo_direct.rs:48-49, src/repo_direct.rs:124-125, src/repo_direct.rs:140-141` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 4 | `src/runtime_support/mod.rs:37-38, src/runtime_support/mod.rs:83-84, src/runtime_support/mod.rs:147-148` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 3 | `src/runtime_support/mod.rs:45-46, src/runtime_support/mod.rs:91-92, src/runtime_support/mod.rs:155-156` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 4 | `src/messaging/backend.rs:191-193, src/messaging/backend.rs:203-205` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 4 | `src/messaging/backend.rs:167-169, src/messaging/backend.rs:179-181` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 4 | `src/mcp/tools_schema.rs:29-31, src/mcp/tools_schema.rs:33-35` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 2 | `src/bugtracker/types_enums.rs:97-98, src/bugtracker/types_enums.rs:132-133, src/bugtracker/types_enums.rs:169-170` | `same-name semantic unit copied across multiple files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 3 | `crates/arc-bench/src/exceptions.rs:130-132, crates/arc-bench/src/witness_loop.rs:154-156` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 2 | 3 | `src/repo.rs:88-90, src/repo.rs:92-94` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 2 | 1 | `crates/cargo-aer/src/helpers.rs:101-103, crates/cargo-vrc/src/planner_support_paths.rs:176-178` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `db/state.rs:2031-2032, db/state.rs:2661-2662` | `same body appears under different names across files` |
| `ExactUnitSameName` | `Warning` | `rust` | 1 | 4 | `src/runtime_support/mod.rs:64-65, src/runtime_support/mod.rs:110-111` | `same-name semantic unit copied across multiple files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `db/state.rs:641-642, db/state.rs:648-649` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 1 | `src/api/entity.rs:191-192, src/gateway/singleflight.rs:64-65` | `same body appears under different names across files` |

## Dimensions

| Dimension | Weight | Score | Weighted | Evidence |
| --- | ---: | ---: | ---: | --- |
| Ownership and navigation surface | 13 | 100 | 13.00 | root `AGENTS.md` present; `CODEOWNERS` present |
| Contract and boundary integrity | 13 | 98 | 12.74 | contract surface found; generated contract artifacts found |
| Proof lanes and test routing | 12 | 100 | 12.00 | one-command setup/validation lane found; deterministic fast lane found |
| Security and supply-chain posture | 12 | 86 | 10.32 | lockfile present; secret or dependency scan tooling found |
| Code shape and semantic surface | 12 | 70 | 8.40 | largest authored code file: src/commands/secrets.rs (233 LOC); most code files stay under 300 LOC |
| Data truth and workflow safety | 8 | 95 | 7.60 | database surface present; structured db boundary manifest present |
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
- Envelope exit code: `0` · elapsed: `3855` ms · strict: `true`
- Commands — ran: `1`, skipped: `0`, failed: `0`
- Generated at: `1779483715`
- Git HEAD (envelope): `71105623fdc1b720abd3a73c9f7c1949010db287`

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
   Reason: `Code shape and semantic surface` scored 70 below the standard floor of 85
   Fix: split large or ambiguous authored code into smaller semantic modules with focused tests
   Rerun: `just fast`
   Fingerprint: `sha256:0b73096cd3ffb00a3803057882ff31709719fef71265cb28921488213306ee44`
   Evidence: largest authored code file: src/commands/secrets.rs (233 LOC), most code files stay under 300 LOC, copy-code advisory classes found: 22 (advisory only, no score impact), IO markers found in domain/core files

## Policy

- Policy file: `./agent/audit-policy.toml`
- Minimum score: `85`
- Fail on: `critical, high`

## Agent Fix Queue

1. `medium` `HLT-001-DEAD-MARKER` `.` - split large or ambiguous authored code into smaller semantic modules with focused tests
   Route: `Entropy`/`fast`
