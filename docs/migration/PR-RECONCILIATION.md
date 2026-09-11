# Accepted component work

These are source dispositions for draft [PR 66](https://github.com/neverhuman/jeryu/pull/66).
The original PRs stay open until accepted work has a protected merged replacement.
Preserved public commits and verified bundles retain every proposed change,
including changes whose behavior is superseded. A source disposition is not an
independent approval, passing audit, standalone qualification or merge.

## Intelligence PR 1

Reviewed source: `ca6ae2934600c95e2471ea038877733472928681`, from
[Intelligence PR 1](https://github.com/neverhuman/jeryu-intelligence/pull/1).

| Proposed files/work | Candidate disposition and reason |
| --- | --- |
| `storage.rs`, `storage/helpers.rs`, `storage/persist.rs`, `storage/types.rs` | Integrate the extraction, retain public reexports and format it with the root toolchain. Persistence remains transactional. |
| `db/migrations/0001_codegraph.sql` | Already identical; preserve the owning SQL bytes and embedded include. |
| `contracts/AGENTS.md`, `contracts/codegraph.schema.json` | Integrate the contract with corrections through the owning Rust generator. The proposal used `crate` instead of serialized `crate_name`, disallowed accepted empty strings/unknown fields and named itself as its own generator. The corrected schema describes existing Rust semantics. |
| Generated zones, owner map, test map | Integrate contract ownership and a real Rust producer with an executed drift check; preserve all existing routes. |
| `Justfile`, `fast.sh`, `check.sh` | Keep the complete existing gate and add all Codegraph targets to `check`. This covers the proposed library test scope plus integration and schema tests without requiring an additional test runner. Keep `release-readiness`. |
| README, `docs/release.md` | Preserve navigation, component purpose and auditor-only cutover guidance. Current results remain pending; a historical numeric badge is not current evidence. |
| Audit policy | Keep the current minimum 85 and zero hard findings; the proposed 82 floor is below the accepted release minimum. |
| `common.sh`, `ensure-jankurai.sh`, `lib.sh`, `pr-ci.sh`, `ci-doctor.sh` | Retain Tool-generated identity and complete public candidate source/build/receipt admission. Preserve explicit preparation followed by offline checks; an environment variable cannot strip `--offline` or authenticate a binary. |
| `test-governed-jankurai-path.sh` | Already identical; preserve hostile path and receipt tests. |
| CI workflows, binary installer, score/badge scripts | Keep the full root matrix, noncanceling census and separate publication boundary. Supersede hosted skips, download-only provenance, canceled earlier work and recurring source badge commits. |

Owning validation: `cargo test --locked -p jeryu-codegraph --all-targets`,
warning-denied all-target Clippy, `just check`, the root source/inventory lane,
and the complete audit and standalone export gates. Source reconciliation alone
does not satisfy the last three requirements.

## Tool Finder PR 1

Reviewed source: `ee0e0ca335ad1f7c4adaa4ffd7d15e387160dffa`, from
[Tool Finder PR 1](https://github.com/neverhuman/jeryu-tool-finder/pull/1).

| Proposed files/work | Candidate disposition and reason |
| --- | --- |
| `src/main.rs`, `src/scan.rs`, `src/summary.rs`, CLI help and contract test | Already identical, including Rust summary delegation and its refusal controls. |
| Root/ops instructions, changelog, architecture, release and tool-finder docs, deny policy, standard, proof lanes, generated zones | Already identical; preserve them with root monorepo routing taking precedence over historical split routing. |
| Contract and `db/` guidance, maps and data boundaries | Integrate missing guidance. Finder owns dossiers and delegates graph storage; these files introduce no second database or migrations. |
| Cargo manifest/lock | Retain root workspace dependencies and lock. The original standalone coordinates remain historical evidence; exported manifests are generated for actual independent qualification. |
| `Justfile` | Preserve the full default product gate. The proposed `--lib` fast commands cannot test this binary-only crate; current required Cargo tests cover the binary and integration contracts. |
| `agent/tool-adoption.toml` | Do not import the broad disabled-tool catalog. Required security, proof binding/marking, routing and release controls remain obligations. |
| `agent/cost-budget.toml` | Do not present an unused benchmark kill switch or wall-clock claim as enforced evidence. Bounded execution and cost controls need actual owning commands and tests. |
| README | Preserve navigation and commands; retained pending audit status avoids a historical score appearing to qualify new source. |
| Check, source-authority, security, score, artifact and test scripts | Preserve accepted functionality plus the current stronger candidate-source, exact-report, dependency and guarded-cleanup controls. Do not discard the additional negative tests or permit missing decision fields to default to success. |
| Pre-push hook and `scripts/ci-local.sh` | Already identical; the full owning gate remains mandatory. |
| CI/library/doctor and new hosted score/install/badge scripts | Keep the complete public acquisition and receipt path. Supersede hosted-only skips, ambient binary selection and source badge commits with the required separate service/publication work. A path named accepted-baseline does not authenticate its predecessor. |

Owning validation remains `bash scripts/ci-local.sh required`, including check,
score, security, contract and artifact lanes. Source equality and root Rust
tests do not replace the complete owning proof or exported-source qualification.
