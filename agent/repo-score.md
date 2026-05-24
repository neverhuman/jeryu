# jankurai Repo Score

- Standard: `jankurai`
- Auditor: `1.5.1`
- Schema: `1.9.0`
- Paper edition: `2026.05-ed8`
- Target stack ID: `rust-ts-vite-react-redline-bounded-python`
- Target stack: `Rust core + TypeScript/React/Vite + PostgreSQL + generated contracts + exception-only Python AI/data service`
- Repo: `.`
- Run ID: `1779648051`
- Started at: `1779648051`
- Elapsed: `593` ms
- Scope: `changed-fast`
- Changed: `agent/repo-score.json, agent/repo-score.md, src/gitlab_auth.rs, src/llm/doctor.rs`
- Advisory: `changed-fast scans only changed files plus required control files; run the full audit before merge or release.`
- Raw score: `69`
- Final score: `69`
- Decision: `fail`
- Minimum score: `85`
- Caps applied: `no-one-command-setup-or-validation, authz-or-data-isolation-gap, release-readiness-gap, missing-rust-property-or-integration-tests, no-agent-friendly-exception-pattern, missing-agent-readable-docs, ci-local-parity`

## Hard Rule Caps

| Rule | Max Score | Applied |
| --- | ---: | --- |
| `no-root-agent-instructions` | 75 | no |
| `no-one-command-setup-or-validation` | 70 | yes |
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
| `authz-or-data-isolation-gap` | 78 | yes |
| `input-boundary-gap` | 78 | no |
| `agent-tool-supply-chain-gap` | 78 | no |
| `release-readiness-gap` | 80 | yes |
| `missing-rust-property-or-integration-tests` | 82 | yes |
| `no-agent-friendly-exception-pattern` | 76 | yes |
| `missing-agent-readable-docs` | 80 | yes |
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
| `ci-local-parity` | 70 | yes |

## Copy-Code Redundancy

- Status: `skipped` hard=`0` warning=`0` files=`0`
- Policy: min-lines=`10` min-tokens=`100` max-findings=`50` include-tests=`false` strict=`false`
- Duplicate volume: lines=`0` tokens=`0` bytes=`0`

## Dimensions

| Dimension | Weight | Score | Weighted | Evidence |
| --- | ---: | ---: | ---: | --- |
| Ownership and navigation surface | 13 | 78 | 10.14 | root `AGENTS.md` present; owner map present |
| Contract and boundary integrity | 13 | 88 | 11.44 | contract surface found; generated contract artifacts found |
| Proof lanes and test routing | 12 | 67 | 8.04 | deterministic fast lane found; GitHub workflow files present |
| Security and supply-chain posture | 12 | 74 | 8.88 | lockfile present; secret or dependency scan tooling found |
| Code shape and semantic surface | 12 | 90 | 10.80 | largest authored code file: src/llm/doctor.rs (303 LOC); authored code stays below hard LOC limits with no shape markers |
| Data truth and workflow safety | 8 | 60 | 4.80 | structured db boundary manifest present; db boundary routes roots, migrations, and constraints |
| Observability and repair evidence | 8 | 38 | 3.04 | observability libraries or patterns found |
| Context economy and agent instructions | 7 | 55 | 3.85 | root `AGENTS.md` present; root `AGENTS.md` stays short |
| Jankurai tool adoption and CI replacement | 7 | 30 | 2.10 | control-plane files present; applicable=15 |
| Python containment and polyglot hygiene | 4 | 100 | 4.00 | no Python files in scope |
| Build speed signals | 4 | 50 | 2.00 | build acceleration markers found; locked dependency graph present |

## Reference Profile Structure

- Applicable cells: `1` canonical=`0` noncanonical=`1` guidance missing=`1`

| Cell | Status | Canonical | Detected | Aliases | Guidance | Owner | Proof lane | Agent fix |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `web` | `not_applicable` | `apps/web/` | `-` | `frontend/, ui/, packages/web/, packages/ui/` | `not_required` | `apps/web` | `rendered UX / Playwright` | `no action` |
| `api` | `not_applicable` | `apps/api/` | `-` | `api/, server/, backend/` | `not_required` | `apps/api` | `edge handler / contract tests` | `no action` |
| `domain` | `not_applicable` | `crates/domain/` | `-` | `domain/, core/` | `not_required` | `crates/domain` | `unit / property tests` | `no action` |
| `application` | `not_applicable` | `crates/application/` | `-` | `application/, usecases/, use-cases/` | `not_required` | `crates/application` | `use-case / authz tests` | `no action` |
| `adapters` | `not_applicable` | `crates/adapters/` | `-` | `adapters/, infra/, integrations/` | `not_required` | `crates/adapters` | `adapter integration tests` | `no action` |
| `workers` | `not_applicable` | `crates/workers/` | `-` | `workers/, jobs/, scheduler/, queue/` | `not_required` | `crates/workers` | `workflow / replay tests` | `no action` |
| `contracts` | `not_applicable` | `contracts/` | `-` | `openapi/, protobuf/, json-schema/, generated/` | `not_required` | `contracts` | `generation / drift checks` | `no action` |
| `db` | `not_applicable` | `db/` | `-` | `migrations/, constraints/, sql/` | `not_required` | `db` | `migration / constraint tests` | `no action` |
| `python-ai` | `not_applicable` | `python/ai-service/` | `-` | `python/, ai-service/, evals/, embeddings/, model/` | `not_required` | `python/ai-service` | `eval / contract tests` | `no action` |
| `ops` | `noncanonical` | `ops/` | `.github, .github/workflows` | `.github/, .github/workflows/, ci/, release/, observability/, security/` | `missing` | `ops` | `security lane / workflow lint` | `migrate the detected ops surface to `ops/` or document an alternate profile with owner, proof lane, expiry, and migration plan` |

## Rendered UX QA

- Web surface: `false`
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
- Applicable tools: `15`
- Configured: `15`
- CI evidence: `0`
- Artifact verified: `0`
- Replaced count: `0`
- Missing CI evidence: `audit-ci, proof-routing, proofbind, proofmark-rust, copy-code, security, ci-bad-behavior, git-bad-behavior, release-bad-behavior, contract-drift, rust-witness, authz-matrix, agent-tool-supply, release-readiness, cost-budget`

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
| `ux-qa` | `ux` | `auto` | `not_applicable` | `playwright, axe-core, visual baselines` | `target/jankurai/ux-qa.json` |
| `db-migration-analyze` | `db` | `auto` | `not_applicable` | `manual migration review` | `target/jankurai/migration-report.json` |
| `contract-drift` | `contract` | `auto` | `configured` | `handwritten contract drift checks, openapi diff` | `agent/repo-score.json, agent/repo-score.md` |
| `rust-witness` | `rust` | `auto` | `configured` | `manual witness graphing` | `target/jankurai/rust/witness-graph.json` |
| `vibe-coverage` | `audit` | `auto` | `not_applicable` | `manual vibe-coding coverage spreadsheet` | `target/jankurai/vibe-coverage.json, target/jankurai/vibe-coverage.md` |
| `coverage-evidence` | `proof` | `auto` | `not_applicable` | `manual coverage report review, ad hoc mutation survivor review` | `target/jankurai/coverage/coverage-audit.json, target/jankurai/coverage/coverage-audit.md` |
| `authz-matrix` | `security` | `auto` | `configured` | `manual authz matrix review` | `agent/repo-score.json, agent/repo-score.md` |
| `input-boundary` | `security` | `auto` | `not_applicable` | `manual unsafe sink review` | `agent/repo-score.json, agent/repo-score.md` |
| `agent-tool-supply` | `security` | `auto` | `configured` | `manual MCP/tool trust review` | `agent/repo-score.json, agent/repo-score.md` |
| `release-readiness` | `release` | `auto` | `configured` | `manual launch checklist` | `agent/repo-score.json, agent/repo-score.md` |
| `cost-budget` | `release` | `auto` | `configured` | `manual spend review` | `agent/repo-score.json, agent/repo-score.md` |

## Security evidence (ingested)

- Source: `target/jankurai/security/evidence.json`
- Envelope exit code: `0` · elapsed: `3746` ms · strict: `true`
- Commands — ran: `1`, skipped: `0`, failed: `0`
- Generated at: `1779628252`
- Git HEAD (envelope): `5225999f17824b139a4164be0bdd1878b7501262`

## Boundary manifest (ingested)

- Path: `agent/boundaries.toml`
- Stack: `rust-ts-vite-react-redline-jansu-bounded-python` · version: `0.4.0`
- Queue path counts — adapter: `2`, event_contract: `1`, generated_type: `1`, client_marker: `6`, streaming_exception: `2`
- Content fingerprint: `sha256:4b3b4ff624a9f9779a21572e8ef75f33d157f141606516169418613995ad2199`

## Boundary Reclassifications

No audited runtime boundary reclassifications declared.

## Findings

1. `high` `proof` `.`
   Check: `HLT-000-SCORE-DIMENSION:proof` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `unmapped`
   Reason: no one-command setup or validation lane was detected
   Fix: add a canonical `setup`, `check`, `test`, or `verify` lane in one root command file
   Rerun: `just fast`
   Fingerprint: `sha256:7010147691f443ae19d3d8603c11ec84958d455885b09e09eca0b9fa91933bde`
   Evidence: no root setup/check/test/verify target surfaced
2. `medium` `context` `.github`
   Rule: `HLT-038-REFERENCE-PROFILE-STRUCTURE-GAP`
   Check: `HLT-038-REFERENCE-PROFILE-STRUCTURE-GAP:context` `soft` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `docs/audit-rubric.md#required-shape`
   Reason: reference-profile cell `ops` is detected at a noncanonical path
   Fix: migrate the detected ops surface to `ops/` or document an alternate profile with owner, proof lane, expiry, and migration plan
   Rerun: `just fast`
   Fingerprint: `sha256:12a7cb3de44727e5607afe0a2df603f1f07be2916aa0ede17340426bdc33d1f7`
   Evidence: canonical_path=ops/, detected_paths=.github, .github/workflows, aliases=.github/, .github/workflows/, ci/, release/, observability/, security/, guidance_status=missing, owner=ops, proof_lane=security lane / workflow lint
3. `medium` `security` `.github/workflows/jankurai.yml`
   Rule: `HLT-016-SUPPLY-CHAIN-DRIFT`
   Check: `HLT-016-SUPPLY-CHAIN-DRIFT:security` `soft` confidence `0.76`
   Route: TLR `Security, secrets, agency`, lane `security`, owner `ops`
   Docs: `docs/audit-rubric.md#top-level-risk-mapping`
   Reason: `Security and supply-chain posture` scored 74 below the standard floor of 85
   Fix: wire secret, dependency, provenance, and workflow scans into an operational CI lane
   Rerun: `just security`
   Fingerprint: `sha256:01fc7a432f2156e053bba436bec7bbf27acbd481603cb754abb49c6b1443010e`
   Evidence: lockfile present, secret or dependency scan tooling found, provenance/SBOM tooling found, workflow linting tooling found
4. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.lib-missing`
   Reason: ops/ci/lib.sh is the shared helper module (artifact assertions, tool pins) every lane sources
   Fix: add ops/ci/lib.sh defining shared helpers and tool version pins
   Rerun: `just fast`
   Fingerprint: `sha256:37915fba1911bbff8067832d71760cb1c395b643f8bf5d61e8dd1f4ab2bcc5ca`
   Evidence: detector=ci.local-parity.lib-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
5. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.pre-push-hook-missing`
   Reason: without a mandatory pre-push gate, broken code can be pushed and CI is the first place a failure shows up
   Fix: add ops/git-hooks/pre-push that runs `bash ops/ci/quality-gates.sh` and wire it via `git config core.hooksPath ops/git-hooks`
   Rerun: `just fast`
   Fingerprint: `sha256:1eddecd5e7ed9fc3919ef4f85fe6c719399ff0704e889c28f7134662a1512fd7`
   Evidence: detector=ci.local-parity.pre-push-hook-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
6. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.toolchain-not-pinned`
   Reason: without a pinned toolchain, local and CI Rust versions can drift silently
   Fix: add rust-toolchain.toml pinning the channel and required components
   Rerun: `just fast`
   Fingerprint: `sha256:aeda4ba55313d39812e5517df9776cbc1051614d2130136364997739c8fe1f42`
   Evidence: detector=ci.local-parity.toolchain-not-pinned, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
7. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.doctor-missing`
   Reason: without a doctor script, developers cannot confirm their local environment matches CI
   Fix: add scripts/ci-doctor.sh listing every tool the ops/ci scripts depend on
   Rerun: `just fast`
   Fingerprint: `sha256:4f3bac5529ba53c005699b913af04e7bdeaa4a5538bcd3cd0d736f210438a6e5`
   Evidence: detector=ci.local-parity.doctor-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
8. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.runner-missing`
   Reason: scripts/ci-local.sh is the local entry point that delegates to the same ops/ci scripts the workflows call
   Fix: add scripts/ci-local.sh exposing each CI lane locally
   Rerun: `just fast`
   Fingerprint: `sha256:8c0df3fda16a40e9a6b8ccf848f557b954f5560e130076937fc056ae021e3fce`
   Evidence: detector=ci.local-parity.runner-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
9. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.script-missing`
   Reason: missing scripts mean the local runner cannot reproduce the CI step
   Fix: create the referenced ops/ci script with the same commands the workflow used to run
   Rerun: `just fast`
   Fingerprint: `sha256:c91ab322872c5468a71c79edd53f858aea807aed717972692e3747e145e22b0d`
   Evidence: detector=ci.local-parity.script-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
10. `high` `ci` `.github/workflows/release-ready.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.script-missing`
   Reason: missing scripts mean the local runner cannot reproduce the CI step
   Fix: create the referenced ops/ci script with the same commands the workflow used to run
   Rerun: `just fast`
   Fingerprint: `sha256:e515578030d79937245ec35152132fac2cd1ae33dba195376a79ebd500bbbf5f`
   Evidence: detector=ci.local-parity.script-missing, path=.github/workflows/release-ready.yml, line=1, proof_window=None, snippet=name: release-ready
11. `high` `ci` `.github/workflows/release.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.script-missing`
   Reason: missing scripts mean the local runner cannot reproduce the CI step
   Fix: create the referenced ops/ci script with the same commands the workflow used to run
   Rerun: `just fast`
   Fingerprint: `sha256:b6fffda026d3065ffa3bedb80fd974c4c1af87b5ef55e58230a4b91167c77dcd`
   Evidence: detector=ci.local-parity.script-missing, path=.github/workflows/release.yml, line=1, proof_window=None, snippet=name: Release
12. `high` `ci` `.github/workflows/rust.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.script-missing`
   Reason: missing scripts mean the local runner cannot reproduce the CI step
   Fix: create the referenced ops/ci script with the same commands the workflow used to run
   Rerun: `just fast`
   Fingerprint: `sha256:0b7267e6a486eb538849aea0062851ab22c26fb93c39cc554f136345679da064`
   Evidence: detector=ci.local-parity.script-missing, path=.github/workflows/rust.yml, line=1, proof_window=None, snippet=name: Rust
13. `medium` `context` `AGENTS.md`
   Rule: `HLT-015-CONTEXT-SETUP-GAP`
   Check: `HLT-015-CONTEXT-SETUP-GAP:context` `soft` confidence `0.76`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `docs/agent-native-standard.md`
   Reason: `Context economy and agent instructions` scored 55 below the standard floor of 85
   Fix: keep root guidance short and route durable detail through agent-readable manifests and docs
   Rerun: `just fast`
   Fingerprint: `sha256:2bff722fd945ed436b8ecb595b6281e2ea086ca47bca37f5c0e7e56142706f39`
   Evidence: root `AGENTS.md` present, root `AGENTS.md` stays short, machine-readable routing artifacts present, thin IDE/agent adapters are present
14. `medium` `proof` `Justfile`
   Rule: `HLT-018-PERF-CONCURRENCY-DRIFT`
   Check: `HLT-018-PERF-CONCURRENCY-DRIFT:proof` `soft` confidence `0.76`
   Route: TLR `Verification`, lane `fast`, owner `workspace`
   Docs: `docs/testing.md`
   Reason: `Build speed signals` scored 50 below the standard floor of 85
   Fix: add fast deterministic build/test targets, caches, and narrow proof lanes for agent iteration
   Rerun: `just fast`
   Fingerprint: `sha256:865e1d16c42efbc42c756e4fd48ff0c751d36cda71e4934c8c05a19a65d9aa55`
   Evidence: build acceleration markers found, locked dependency graph present, CI cache hint found, missing one-command setup/validation
15. `medium` `context` `agent/owner-map.json`
   Rule: `HLT-003-OWNERLESS-PATH`
   Check: `HLT-003-OWNERLESS-PATH:context` `soft` confidence `0.76`
   Route: TLR `Context/setup`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#ownership-boundaries`
   Reason: `Ownership and navigation surface` scored 78 below the standard floor of 85
   Fix: tighten owner/test maps and root routing until agents can localize ownership without inference
   Rerun: `just fast`
   Fingerprint: `sha256:50b75235e0417de46f72c67838a5e57b786789ff288dd26a70944c6bea9e9ebd`
   Evidence: root `AGENTS.md` present, owner map present, test/proof routing map present, owner map covers audited paths
16. `medium` `proof` `agent/test-map.json`
   Rule: `HLT-004-UNMAPPED-PROOF`
   Check: `HLT-004-UNMAPPED-PROOF:proof` `soft` confidence `0.76`
   Route: TLR `Verification`, lane `fast`, owner `agent`
   Docs: `agent/JANKURAI_STANDARD.md#proof-lanes`
   Reason: `Proof lanes and test routing` scored 67 below the standard floor of 85
   Fix: route each owned path to a deterministic proof command and make the lane executable in CI
   Rerun: `just fast`
   Fingerprint: `sha256:fc8e7534b8c6e968060e4ed0a77b0967425381577bf19215a7738404685ea237`
   Evidence: deterministic fast lane found, GitHub workflow files present, test/proof routing map present, jankurai audit lane found in CI
17. `high` `test` `crates/`
   Rule: `HLT-008-FALSE-GREEN-RISK`
   Check: `HLT-008-FALSE-GREEN-RISK:test` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `fast`, owner `tools`
   Docs: `docs/testing.md`
   Reason: Rust surface lacks required property and/or integration tests
   Fix: add `proptest` or equivalent invariant tests plus `tests/` integration coverage routed through `cargo nextest` or `cargo test`
   Rerun: `just fast`
   Fingerprint: `sha256:8ece7234070a20910736663e65a530625acd16dac7fa57476cfc7c9a74bd745c`
   Evidence: Rust surface detected
18. `high` `exceptions` `crates/domain`
   Rule: `HLT-017-OPAQUE-OBSERVABILITY`
   Check: `HLT-017-OPAQUE-OBSERVABILITY:exceptions` `hard` confidence `0.88`
   Route: TLR `Repair`, lane `observability`, owner `tools`
   Docs: `agent/JANKURAI_STANDARD.md#repair-receipts`
   Reason: no agent-friendly exception/error pattern was detected
   Fix: define a typed exception surface with purpose, reason, common fixes, docs_url, and repair_hint so the next rerun is local
   Rerun: `just score`
   Fingerprint: `sha256:538667a01e35d8e91eae100627364816dd225911862fa2fa1578642af63d4af8`
   Evidence: route repair work to the next agent, opaque failures slow local debugging and reruns, add a typed repair hint; name the common fixes; point at the local docs URL, docs/testing.md
19. `medium` `data` `db/`
   Rule: `HLT-006-DIRECT-DB-WRONG-LAYER`
   Check: `HLT-006-DIRECT-DB-WRONG-LAYER:data` `soft` confidence `0.76`
   Route: TLR `Contracts/data`, lane `db`, owner `data`
   Docs: `docs/audit-rubric.md#required-shape`
   Reason: `Data truth and workflow safety` scored 60 below the standard floor of 85
   Fix: move durable truth into migrations, constraints, adapters, and application-owned transactions
   Rerun: `just fast`
   Fingerprint: `sha256:bc3c154999ceeadf008cf312a5b1205941d2a5bc9868961a1358cefa07b821ae`
   Evidence: structured db boundary manifest present, db boundary routes roots, migrations, and constraints
20. `medium` `docs` `docs/`
   Check: `HLT-000-SCORE-DIMENSION:docs` `soft` confidence `0.76`
   Route: TLR `Context/setup`, lane `audit`, owner `standard`
   Reason: agent-readable documentation is incomplete
   Fix: add concise docs for architecture, boundaries, tests, generated zones, and audit rules; route them from root `AGENTS.md`
   Rerun: `just score`
   Fingerprint: `sha256:7a7bbff17bd45fa833f208a469d73fc717e5fd8687e3d8d20098aa2ce66f2e92`
   Evidence: README.md, docs/architecture.md or docs/boundaries.md, docs/testing.md
21. `high` `release` `docs/release.md`
   Rule: `HLT-025-RELEASE-READINESS-GAP`
   Check: `HLT-025-RELEASE-READINESS-GAP:release` `hard` confidence `0.88`
   Route: TLR `Verification`, lane `release`, owner `standard`
   Docs: `docs/testing.md`
   Matched term: `release structure`
   Reason: launch gates need artifact-backed release evidence
   Fix: add a release control surface with version source, changelog, release process docs, CI or script evidence, integrity/provenance evidence, and rollback guidance
   Rerun: `just check`
   Fingerprint: `sha256:c7eefce130f9057e693ec4f1e52a32ae746bb45e440d5f623037f50ad020472e`
   Evidence: release structure missing: changelog, release process doc
22. `medium` `observability` `docs/testing.md`
   Rule: `HLT-017-OPAQUE-OBSERVABILITY`
   Check: `HLT-017-OPAQUE-OBSERVABILITY:observability` `soft` confidence `0.76`
   Route: TLR `Repair`, lane `observability`, owner `standard`
   Docs: `agent/JANKURAI_STANDARD.md#repair-receipts`
   Reason: `Observability and repair evidence` scored 38 below the standard floor of 85
   Fix: add structured errors, telemetry, and repair receipts that tell the next agent where to rerun proof
   Rerun: `just score`
   Fingerprint: `sha256:4c1e67e787263871c688fc1dc278b4e0cb4b428b9bd63da4ee5fd5818c530ded`
   Evidence: observability libraries or patterns found, no agent-friendly exception pattern found
23. `medium` `release` `docs/testing.md`
   Rule: `HLT-026-COST-BUDGET-GAP`
   Check: `HLT-026-COST-BUDGET-GAP:release` `soft` confidence `0.88`
   Route: TLR `Verification`, lane `release`, owner `standard`
   Docs: `docs/testing.md`
   Matched term: `budget`
   Reason: unbounded paid work needs budgets and stop conditions
   Fix: add explicit budgets, quotas, stop conditions, and kill-switch evidence for paid or unbounded operations
   Rerun: `just check`
   Fingerprint: `sha256:edd248b7afc24b644107205fa5b84a88103ac4b622009ff9f19b779de8798f59`
   Evidence: cost surface found without budget/stop-condition policy
24. `medium` `context` `ops/`
   Rule: `HLT-038-REFERENCE-PROFILE-STRUCTURE-GAP`
   Check: `HLT-038-REFERENCE-PROFILE-STRUCTURE-GAP:context` `soft` confidence `0.88`
   Route: TLR `Context/setup`, lane `fast`, owner `ops`
   Docs: `docs/audit-rubric.md#required-shape`
   Reason: reference-profile cell `ops` lacks local AGENTS.md guidance
   Fix: add `ops/AGENTS.md` with owns / forbidden / proof lane guidance
   Rerun: `just fast`
   Fingerprint: `sha256:afd6d62dcc0304f7e4872a9edce56c957a74d6c2101cdf6218dce16b4297ba55`
   Evidence: canonical_path=ops/, detected_paths=.github, .github/workflows, guidance_status=missing, owner=ops, proof_lane=security lane / workflow lint
25. `high` `security` `src/gitlab_auth.rs:5`
   Rule: `HLT-022-AUTHZ-ISOLATION-GAP`
   Check: `HLT-022-AUTHZ-ISOLATION-GAP:security` `hard` confidence `0.88`
   Route: TLR `Business truth`, lane `db`, owner `workspace`
   Docs: `docs/audit-rubric.md#top-level-risk-mapping`
   Matched term: `rls`
   Reason: authz/data isolation requires negative proof evidence
   Fix: add owner/non-owner authorization tests or RLS evidence for the touched data boundary
   Rerun: `just fast`
   Fingerprint: `sha256:e695e393f566f7335e555cd1b9657b2dc5e6a1b7b011865550d1818c3ed7acfc`
   Evidence: //! non-local GitLab URLs, but local GitLab credentials are normalized back into

## Policy

- Policy file: `./agent/audit-policy.toml`
- Minimum score: `85`
- Fail on: `critical, high`

## Agent Fix Queue

1. `high` `HLT-022-AUTHZ-ISOLATION-GAP` `src/gitlab_auth.rs` - add owner/non-owner authorization tests or RLS evidence for the touched data boundary
   Route: `Business truth`/`db`
2. `medium` `HLT-006-DIRECT-DB-WRONG-LAYER` `db/` - move durable truth into migrations, constraints, adapters, and application-owned transactions
   Route: `Contracts/data`/`db`
3. `high` `.` - add a canonical `setup`, `check`, `test`, or `verify` lane in one root command file
   Route: `Verification`/`fast`
4. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add ops/ci/lib.sh defining shared helpers and tool version pins
   Route: `Verification`/`fast`
5. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add ops/git-hooks/pre-push that runs `bash ops/ci/quality-gates.sh` and wire it via `git config core.hooksPath ops/git-hooks`
   Route: `Verification`/`fast`
6. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add rust-toolchain.toml pinning the channel and required components
   Route: `Verification`/`fast`
7. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add scripts/ci-doctor.sh listing every tool the ops/ci scripts depend on
   Route: `Verification`/`fast`
8. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add scripts/ci-local.sh exposing each CI lane locally
   Route: `Verification`/`fast`
9. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - create the referenced ops/ci script with the same commands the workflow used to run
   Route: `Verification`/`fast`
10. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/release-ready.yml` - create the referenced ops/ci script with the same commands the workflow used to run
   Route: `Verification`/`fast`
11. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/release.yml` - create the referenced ops/ci script with the same commands the workflow used to run
   Route: `Verification`/`fast`
12. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/rust.yml` - create the referenced ops/ci script with the same commands the workflow used to run
   Route: `Verification`/`fast`
13. `high` `HLT-008-FALSE-GREEN-RISK` `crates/` - add `proptest` or equivalent invariant tests plus `tests/` integration coverage routed through `cargo nextest` or `cargo test`
   Route: `Verification`/`fast`
14. `high` `HLT-025-RELEASE-READINESS-GAP` `docs/release.md` - add a release control surface with version source, changelog, release process docs, CI or script evidence, integrity/provenance evidence, and rollback guidance
   Route: `Verification`/`release`
15. `medium` `HLT-018-PERF-CONCURRENCY-DRIFT` `Justfile` - add fast deterministic build/test targets, caches, and narrow proof lanes for agent iteration
   Route: `Verification`/`fast`
16. `medium` `HLT-004-UNMAPPED-PROOF` `agent/test-map.json` - route each owned path to a deterministic proof command and make the lane executable in CI
   Route: `Verification`/`fast`
17. `medium` `HLT-026-COST-BUDGET-GAP` `docs/testing.md` - add explicit budgets, quotas, stop conditions, and kill-switch evidence for paid or unbounded operations
   Route: `Verification`/`release`
18. `high` `HLT-017-OPAQUE-OBSERVABILITY` `crates/domain` - define a typed exception surface with purpose, reason, common fixes, docs_url, and repair_hint so the next rerun is local
   Route: `Repair`/`observability`
19. `medium` `HLT-017-OPAQUE-OBSERVABILITY` `docs/testing.md` - add structured errors, telemetry, and repair receipts that tell the next agent where to rerun proof
   Route: `Repair`/`observability`
20. `medium` `HLT-038-REFERENCE-PROFILE-STRUCTURE-GAP` `.github` - migrate the detected ops surface to `ops/` or document an alternate profile with owner, proof lane, expiry, and migration plan
   Route: `Context/setup`/`fast`
21. `medium` `HLT-015-CONTEXT-SETUP-GAP` `AGENTS.md` - keep root guidance short and route durable detail through agent-readable manifests and docs
   Route: `Context/setup`/`fast`
22. `medium` `HLT-003-OWNERLESS-PATH` `agent/owner-map.json` - tighten owner/test maps and root routing until agents can localize ownership without inference
   Route: `Context/setup`/`fast`
23. `medium` `docs/` - add concise docs for architecture, boundaries, tests, generated zones, and audit rules; route them from root `AGENTS.md`
   Route: `Context/setup`/`audit`
24. `medium` `HLT-038-REFERENCE-PROFILE-STRUCTURE-GAP` `ops/` - add `ops/AGENTS.md` with owns / forbidden / proof lane guidance
   Route: `Context/setup`/`fast`
25. `medium` `HLT-016-SUPPLY-CHAIN-DRIFT` `.github/workflows/jankurai.yml` - wire secret, dependency, provenance, and workflow scans into an operational CI lane
   Route: `Security, secrets, agency`/`security`
