# Accepted component work

These are source dispositions for the continuing public candidate on
`codex/public-release-gates-20260911t034104z`. [PR 66](https://github.com/neverhuman/jeryu/pull/66)
was closed without a merged replacement; its unique work remains required.
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

## Deploy PR 1: reviewed product and dependency boundaries

Reviewed preserved source: `c8f43edead0b8d276e9e13ed517823a223f6807a`,
from [Deploy PR 1](https://github.com/neverhuman/jeryu-deploy/pull/1).
The following dispositions cover the reviewed subset of its 143 changed paths.
Remaining workflow, security, manifest and proof changes still need individual
inclusion decisions; this table does not close or approve the original PR.

| Proposed files/work | Maintained disposition and evidence |
| --- | --- |
| Authenticated review handlers, their six real-Git/router controls and current actor adapters | Integrated in `50eb49f9` with original Core `d0952ff9`; all six controls pass in full hosted Rust. Current browser request/response contracts are being checked against those same handlers. |
| Review posture, observer attachment and HTTP helpers | Current modular HTTP/posture implementation retains the original authenticated behavior, management-root observer and current actor errors. |
| `auth.rs` logout and read authentication | Current session/PAT read resolution is equivalent. The separate original `ef60ce5f` logout correction propagates durable revocation failures; all eight owning route tests pass at `50eb49f9`. |
| `git_transport.rs` | Preserve resolver-bound repository authorization, anonymous public reads and refusal of supplied invalid tokens. Independently stripping a suffix could authorize metadata belonging to a different repository. |
| `ci_bridge.rs` | The sole remaining difference is the Tool-owned public auditor source URL; source/build/executable/receipt verification remains required. |
| `bootstrap.rs` | Preserve administrator-only bootstrap and the durable receipt-before-account recovery flow. Original automatic creation of two named user accounts and account-before-receipt ordering are superseded. |
| `sessions.rs`, `sessions/runtime.rs`, session tests | Preserve owned test credential input, validation of existing onboarding state and the actual denied-network policy. Original implicit bridge networking is superseded; current product-image and credential-isolation qualification remain required. |
| `surface.rs`, README/workcell error adapters and MCP test diagnostics | Preserve embedded assets when no SPA directory is configured, existing typed errors/statuses, and the same terminal MCP assertion. |
| `github/pulls.rs`, catalog fork fixture | Restore explicit default branch and distinct source/destination repository UUID assertions alongside existing fork admission tests. |
| `merge_gating.rs` | Restore required Git availability in all twelve original cases. Four separate controls retain original legacy refusal, complete PR/check preservation, fast-forward/diverged/already-landed source and no mirror effect. Every existing positive merge requirement remains mandatory and currently fails until the authenticated durable executor is completed. |
| CI-bridge, GitHub lifecycle and live HTTP tests | Preserve actual positive merge requirements rather than replace them with refusal-only success. Preserve current live HTTP health-body validation, private fixtures, real Git/LFS requirements and awaited server shutdown. Original advisory-roundtrip/closed-state and other negative assertions still need their final inclusion disposition. |
| `dependency_sources.rs` | Preserve generic path-alias confinement through `monorepo_paths.rs` before Cargo metadata, covering all active root/member dependency sections, plus complete metadata rejection of every unowned local package. Its historical fixed `d0952ff9` Core coordinates and split source unifiers are provenance; root workspace/lock and deterministic exports select current build inputs. Exact public source, complete identity and source-denial gates remain required. |
| `docs/architecture.md`, `docs/boundaries.md`, authenticated-review testing guidance | Restore the original credential, Git observer, challenge, nonce and blocking-worker boundaries. Retain actual positive protected-merge expectations and identify their present failure; historical source-test counts do not qualify the new candidate. |
| Full/fast CI workflow edits | Preserve every root required lane and downstream export qualification. Original public-host skips and ignored missing artifacts are superseded by actual execution and retained failure. Generated mirrors preserve original workflows as history and use the generated owning workflow. |
| `check.sh`, `coverage.sh`, `web.sh`, security lane | Retain root/member ownership, all additional regression commands, complete fresh mutation outcomes and explicit baseline updates. Preserve exact workspace-lock custody and each owned dependency closure. Current Web validation includes physical complete bundles and actual CLI processes; neither a vendored file count nor a temporary route fixture replaces it. |
| GitHub conformance vocabulary changes | Same positive/negative source predicates and REST tests remain. Diagnostic variable/comment renaming does not remove a required assertion. |
| Owner/test/proof/audit/generated-zone/CI lane maps | Already identical to the preserved proposal in the inspected paths. Keep existing minimum scores, zero hard findings and quantitative proof requirements. |
| `repos.manifest.toml`, split-tool manifest and CLI compatibility tests | Keep the generated root-authority pointer and root dependency closure. Original full split inventory remains in provenance. Preserve all original CLI cases plus manifest membership and export-argument refusals; dependencies added for durable intake/queue have their own required tests. |

Current path/merge/fork corrections have formatting and source review only.
Required execution includes the five owning path/metadata regression groups,
full split-tool and API suites, real-Git refusals and positive merge journey,
warning-denied Clippy, source inventory, standalone exports and full audits.
These source checks do not establish protected-main or installed authority.

## Foundation successor, 2026-09-13

The linear successor starts at freshly fetched PR65 head
`b50dc1f1f447d602b5fc98fe6dc4fdc70914a19b`. It incorporates the complete
corrective source tree `edc67393da09db970fe0ebfb191564c2a4c719ec` through
a normal forward commit. Before this documentation addition, the staged tree
was verified equal to `75b6e9313baa322d34033166e45330ac8dc941e1`.
The corrective branch remains intact, including PR66 ancestry and separate
inherited audit-service checkpoint `e68b6ac56def1bd482780265ab211732a84b7d73`
and Tool checkpoint `edc67393da09db970fe0ebfb191564c2a4c719ec`. Both checkpoints
retain prior-session provenance and pending owning verification. All prior refs
and eleven uncommitted files were privately preserved and restoration-verified.

This is explicit source reconciliation, not a claim of semantic completion or
passing qualification. PR65-only changes receive the dispositions below; the
existing release gap register retains detailed owning reasons and prior failures.
Original Core `e81452f` remains a required, separately held integration input.
The full original/component historical proposal census is still open.

| PR65-only commit | Contribution | Conflict disposition |
| --- | --- | --- |
| `b50dc1f1f447d602b5fc98fe6dc4fdc70914a19b` | fix: prove host rust dispatch without inherited GITHUB_ACTIONS | Superseded by the corrective branch full-environment CI contract and explicit hostile custody/dispatch tests. Hosted environment variables cannot remove required proof or authenticate source/executables. |
| `e63ad9cac68b26c39ee01cbbfdcf74cff8d25e2a` | fix: serialize jeryu-api tests on public GHA rust | Superseded by unconditional serial test execution in the maintained command for every environment. |
| `ac0ef77590ea26f5ad3f89a5ac561a2081fbc9ee` | docs: clone implicit main after this candidate fast-forwards | Equivalent default-main clone guidance is retained. Historical release claims do not qualify this candidate. |
| `0152fe863f313b4cabd2f7bbf86ddd9e8869f1ba` | lock: bind deploy family tag after GHA serial split tests | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `91fd58b17bec69d39b21dd8089e18e47cac44bee` | fix: serialize deploy split tests on public GHA | Superseded by unconditional serial test execution in the maintained command for every environment. |
| `7333b770cdbeccfc1bd32b5cb0ed2c6d555c4f1d` | lock: bind deploy family tag after sqlite reopen lease wait | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `2b241d5288df4606c62d0d3797b388c3a0c2e300` | fix: wait for the writer lease before reopening sqlite in create replay | Incorporated in the corrective SQLite reopen tests; actual owning and full-source verification must be rerun. |
| `31b0536d09255208bb161db93d581701f41f647b` | lock: bind runner family tag after quiet Release installer | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `a579e7ea734e6f4177d11b2a2aa61f54c369b2b5` | fix: keep the GHA Release installer off test stdout | Superseded by Tool-owned candidate source/build/receipt acquisition and Runner acquisition controls. Producer regeneration and clean hosted qualification remain open. |
| `47e3aa408262b66811cf09a5ad2a64e289c9d0f6` | lock: bind runner family tag after idempotent Release installer | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `a06a413f9647a230064fd431f09eae0f54ffdd6f` | fix: make the GHA Release installer idempotent under parallel tests | Superseded by Tool-owned candidate source/build/receipt acquisition and Runner acquisition controls. Producer regeneration and clean hosted qualification remain open. |
| `4367868b3dc208ef34256ce585cd52e6fcb1a280` | lock: bind runner family tag after standalone Release installer | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `d86d16ddf67b1d70550afee2843611c15b81083b` | fix: let standalone runner splits install the SHA-pinned Release binary | Superseded by the corrective branch full-environment CI contract and explicit hostile custody/dispatch tests. Hosted environment variables cannot remove required proof or authenticate source/executables. |
| `9fec1dd3d53ed45b71adf900eb8aa8a1fde35d51` | lock: bind tool family tag after hermetic installer tail repair | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `b8c95083ddb2ab3f4ac38ad5012bda3b3263ab56` | fix: bind hermetic installer tail to the proc-fd transport | Superseded by Tool-owned candidate source/build/receipt acquisition and Runner acquisition controls. Producer regeneration and clean hosted qualification remain open. |
| `09758b04ec6eaf0ce38e3728b1fa35fe7acfba5e` | lock: bind tool family tag after GHA type-P wrapper count | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `83106d9738c41e3554d69608b4fcfa3db4370704` | fix: keep host legacy dispatch proof off GITHUB_ACTIONS inheritance | Superseded by the corrective branch full-environment CI contract and explicit hostile custody/dispatch tests. Hosted environment variables cannot remove required proof or authenticate source/executables. |
| `ec830908c30c705d1f955b379f5e824ecb5131c6` | lock: bind deploy family tag after Git 2.55 fixture repair | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `625ee3d95dc8542b97618e2d7525d1bd86ed8fc1` | fix: admit Git 2.55 dangling HEAD fixtures; keep host legacy union | Superseded by the corrective branch full-environment CI contract and explicit hostile custody/dispatch tests. Hosted environment variables cannot remove required proof or authenticate source/executables. |
| `5c1271310b0ee0c22f1e14661039d68ba6740f5f` | lock: bind deploy family tag after GHA /proc and bwrap rust fixes | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `b47baeb9aace48f22101316b7fb20053f7299dad` | fix: keep split-tool /proc cleanup and bwrap confinement host-strict | Superseded by the corrective branch full-environment CI contract and explicit hostile custody/dispatch tests. Hosted environment variables cannot remove required proof or authenticate source/executables. |
| `2a5db80ab61e304e7bcaed6ba9606f7e596e1aaa` | lock: bind runner family tag after GHA host-custody split | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `c12d753290fa45283478c76e8e6291e26f83d60e` | fix: keep host receipt-bound Jankurai custody tests off public GHA | Superseded by the corrective branch full-environment CI contract and explicit hostile custody/dispatch tests. Hosted environment variables cannot remove required proof or authenticate source/executables. |
| `a350a5e8eaf43e501991c8fbe9411353423a3207` | lock: point runner pin at jeryu-ci-runner-family-osstr | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `de9246c8a624910c816dd578d6e7600c4d9f250f` | lock: bind runner family tag after GITHUB_ACTIONS OsStr compile fix | Superseded in the candidate projection by the corrective branch pins; neither pin set is released authority. Immutable original tags and proposal history remain retained. |
| `f7d064cd1451278f3d30389b084927a7c5c419db` | fix: compare GITHUB_ACTIONS as OsString-safe Ok(\"true\") | Superseded by the corrective branch full-environment CI contract and explicit hostile custody/dispatch tests. Hosted environment variables cannot remove required proof or authenticate source/executables. |

The successor retains the corrective branch's complete 14 lanes and aggregate
required admission, including hosted-environment refusal tests, source custody,
audit thresholds and auxiliary proofs. Neither PR65 nor PR66 is a merged
replacement. Their proposals and historical branches remain preserved.
