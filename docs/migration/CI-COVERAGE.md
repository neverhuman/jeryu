# Maintained proof coverage

Local and GitHub CI invoke `bash scripts/ci.sh LANE`. A successful inventory
check proves source accounting, not proof execution. `proof-inventory.json`
hashes retained entrypoints, their implementation/input trees, hostile tests,
policies, release validators and the root commands. Regenerate it with
`cargo run --locked -p jeryu-split-tool --bin jeryu-split -- proof-inventory
> docs/migration/proof-inventory.json`.
Every command below must pass at the final reviewed source commit; earlier
commit results do not qualify later changes. Redline compatibility is separately
opted into with `bash scripts/ci.sh redline`; it is not a SQLite release gate.
The unchanged SQL contract lives in `components/jeryu-release-ops/tests/redline`
with its own lockfile. Ordinary Rust checks cover all 65 product packages and
assert that even their all-feature graph uses bundled SQLite without Redline.
The protected Redline consumer-evidence producer and original-retirement
requirements remain separate; this separation does not waive those proofs.

| Required proof | Root command | Preserved acceptance and remaining work |
| --- | --- | --- |
| Rust product/protocol/property/migrations/docs | `bash scripts/ci.sh rust` | Formatting; warning-denied Clippy on all targets/features; workspace tests. Includes Work properties/SQLite migrations, Runner schema contracts, Finder CLI contracts, Git differential oracle, CI IR/proof/workcell/agentbridge and API tests. The paid external-model smoke is optional. Native sandbox and real Docker execution need separate lanes; a silent early return in the historical Docker smoke is not execution evidence. |
| Browser/contracts/UX | `bash scripts/ci.sh web` | Generated Rust contracts, unit/real-backend/UI browser tests, action matrix, Storybook, paired screenshots and rendered receipts; zero serious/critical axe violations; aggregate gzip JavaScript below 358,400 bytes; all collected Lighthouse performance scores at least 0.7. Lighthouse timing/resource warnings are not hard gates. |
| Source installation/runtime | `bash scripts/ci.sh runtime` | Embedded production build, build/source digest binding, installer tamper/stale-source refusal, installed binary authentication/Git/CLI/restart durability. Protected PR flow uses distinct authenticated author/reviewer/merger, rejects direct main pushes/self-approval/old-head approval and writer-published checks, excludes foreign-head checks, persists reviews across restart and merges the exact reviewed Git commit. Optional runner execution needs separate qualification. |
| Cache CLI poisoning; Codegraph CLI persistence | `bash scripts/ci.sh product` | All seven adversarial scenarios pass, including the five retained named safety assertions. Codegraph scans real source and a second process reads an equal nonempty cluster set from SQLite. |
| Native sandbox | `bash scripts/ci.sh sandbox` | Disposable Linux admission; every required test executes; four escapes blocked; zero false skips; resource limits, terminal/secret and watchdog proofs. |
| Docker confinement | `bash scripts/ci.sh oci` | Required disposable Linux rootful Docker/cgroup v2 lane, also emitted for Runner exports. Nine checks: writable workspace/read-only root, no host socket, no credential environment, denied egress, PID limit 32, memory limit 32 MiB, denied unshare syscall, NoNewPrivs, and cgroup limits. Regular matrix profile uses 64 PIDs/64 MiB. Successful positive controls and operation-specific denial evidence are required; engine/entrypoint exit 125–127 cannot count as confinement. Source/probe/seccomp identities bind the receipt, written only after guarded resource cleanup. This qualifies the network-denied `from_agent_job` profile; ordinary/final-session profiles and product image remain separate. Exact `4d1e501c` passed all nine checks across 15 probe/control runs in a fresh Ubuntu 24.04 VM with Docker 29.1.3, zero failures/skips, and guarded guest removal. Later source requires renewed qualification. |
| Agent container image | **Pending portable command** | Retain custom seccomp, refusal wrappers, Git read/add/commit versus checkout/push guard, Cargo/Node/TypeScript presence, and governed auditor identity. A probe image alone does not qualify the product image. Remove private/floating image build inputs and tolerated producer failures. |
| Supply-chain baseline | `bash scripts/ci.sh security` | Verified tool downloads, npm high/critical rejection, `cargo audit --deny warnings`, license/source bans, history secrets, workflow checks and SPDX. Source/tool-bound CycloneDX receipts and Cache's closed eight-check/eight-tool custody remain pending. |
| Split exports | `bash scripts/ci.sh splits` | Generate twice with identical results, standalone locks/dependency identity, independently build/test from the provenance commit. Reject missing/unreadable inventories and changed source; retain complete scratch on failure and inspect links/mounts/identity before successful cleanup. The 24 synthetic cases check orchestration, not actual exports. Public source availability and privileged Runner proofs remain separate prerequisites. |
| Public candidate auditor | `bash scripts/ci.sh auditor` (also bootstrapped by `legacy`) | All eleven consumer scopes and 83 generated targets; exact committed pin/builder inputs; anonymous immutable source and locked dependencies; unchanged vendor/image/compiler/context/binary pins; actual offline nonroot build; atomic installation, rollback, closed candidate receipt and held-descriptor execution. Eight transaction failure tests and retained installer/root-seal/verifier suites pass. Exact `5fab2aea` passed actual installation, same-head reuse, bootstrap locking and source/path refusals. Candidate evidence keeps protected authority pending; final source requires renewed qualification. |
| Independent auxiliary producers | `bash scripts/ci.sh auxiliary independent` | Preliminary callable subset, outside `all` and the hosted matrix: strict owned copy-code, actual migration analysis, one complete 65-package default-feature Rust map/witness with owning manifests and cross-component edges. The four owning wrappers delegate to one implementation. Full/default admission remains nonzero for the concrete missing protected baseline, changed-set/hunk, proofbind/proofmark, configuration and conformance gates; standalone export admission is pending. See [command scope](AUXILIARY-PROOFS.md). |
| Governed score | **Pending portable command** | Zero hard findings and caps. Floors: root/Core/Cache/Deploy/Work/Release Ops/Web 85; Intelligence 82; Finder 75; Tool 65; Runner effective floor 91 (its shared implementation strengthens the policy's 80). Public governed auditor and verified identity/receipt are prerequisites. |
| Protected-base execution/ratchets | **Pending portable command** | Cache/Work candidate at least 91 and protected baseline at least 85; Runner at least 91; Deploy at least 85. Nonnegative score delta where asserted; no new caps/hard findings; unchanged policy; exact changed-path set and clean source. Positive command/receipt count; every command exit zero; zero failed receipts; verification passes with zero issues. Copy-code hard classes/instances both zero. Baselines must come from authenticated protected source. |
| Coverage/mutation | **Pending portable command** | Deploy API baseline 0.8044, epsilon 0.005, effective lower bound 0.7994 with upward-only ratchet; changed-line coverage 0.90; total 0.75 advisory. Retain mutation policy `hard_survivors_on_changed_paths=1`; final audit hard findings zero and both evidence sources present. Tool requires real LCOV and passing coverage audit with zero hard findings. Missing coverage tools fail. |
| Cache Rust API compatibility | **Pending portable command** | Four libraries compared with immutable `jeryu-cache-v5.0.0-split.1` at `6bc56b87…`; no removed/changed APIs; unchanged governed build/policy/configuration; exact receipt hashes and final revalidation; verified cargo-public-api 0.52.0 identity/custody. |
| Shell/governance contracts | **Pending portable command** | Installer/root seal/renderer/auditor substitution/source authority/receipt custody; dispatch injection/order; repair receipts; ShellCheck at warning severity; closed schemas and complete owner/test routing. Component Git-root and package-count assumptions require explicit monorepo scope. |
| Signed central releases | **Pending portable command** | Signed source and matching PR publication metadata; binary route smoke; SPDX/CycloneDX/provenance/hash identities; previous signed rollback identity; SignRail local/dev-canary/prod receipt contracts with 100% signature coverage and matching source/rollback/artifact digests. Validate contracts without activating production. |

The `legacy` lane stays required while these pending gates are ported.
Core, Runner, Deploy, Intelligence and Tool now accept exactly `required`
in `scripts/ci-local.sh` and delegate to their existing full
`ops/ci/pr-ci.sh`; no arguments retain `just fast` then `just check`.
Invalid or extra arguments fail before dispatch. Each full wrapper includes
the original quick assertions: Core, Runner, Intelligence and Tool already
invoke fast/check directly; Deploy now invokes check, which also covers its
fast recipe and includes agent maps, script syntax and phase/coverage tests.
Runner contract/sandbox commands and Intelligence oracle/tool-build commands
remain in their full wrappers; their actual execution still needs proof.

`bash tests/component-ci-dispatch.sh`, also called by the root check,
uses only synthetic command responses to verify selection, ordering, component
root, argument refusal and nonzero status propagation. It neither runs nor
qualifies component CI. The existing full wrappers still require portable
proof admission and the separate proof/release workflows still need root
commands; the dispatcher repair alone cannot make the required union pass.

Deploy's lock guard now binds Cargo's actual workspace manifest and root lock
for both standalone exports and the monorepo. Its 39 synthetic cases and
read-only workspace discovery pass. Core, Runner, Deploy and Intelligence
still guard metadata checks on a component-local `Cargo.toml` that is absent
in the monorepo. Those checks need explicit component package selection;
Runner's version assertion must preserve other components' 5.1.0 packages.
This remaining omission is not covered by the lock or dispatcher repairs.

Do not port false success behavior. Core/Intelligence/Release Ops/Web now
delegate independent auxiliary producers to one root implementation; their
candidate-to-baseline copy and substitute outputs were removed. The default
full command executes useful producers and remains nonzero for unavailable
proof admission. Tool still creates a ratchet baseline from its own candidate. Deploy's phase wrapper now preserves missing-tool exit 3 as failure;
its aggregate rejects pending results, nonzero exits and mismatched or nonfinal
PASS lines. Nineteen isolated shell cases cover these reporting failures;
actual coverage and mutation execution remains required. The stricter
Cache/Runner/Work/Deploy assertions above are the
equivalence requirements. Required aggregate success remains unavailable
until every required proof has a functioning command and exact-source
execution evidence.
