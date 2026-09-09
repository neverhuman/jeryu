# Maintained proof coverage

Local and GitHub CI invoke `bash scripts/ci.sh LANE`. A successful inventory
check proves source accounting, not proof execution. `proof-inventory.json`
hashes retained entrypoints, their implementation/input trees, hostile tests,
policies, release validators and the root commands. Regenerate it with
`cargo run --locked -p jeryu-split-tool --bin jeryu-split -- proof-inventory
> docs/migration/proof-inventory.json`.
Every command below must pass at the final reviewed source commit; earlier
commit results do not qualify later changes.

| Required proof | Root command | Preserved acceptance and remaining work |
| --- | --- | --- |
| Rust product/protocol/property/migrations/docs | `bash scripts/ci.sh rust` | Formatting; warning-denied Clippy on all targets/features; workspace tests. Includes Work properties/SQLite migrations, Runner schema contracts, Finder CLI contracts, Git differential oracle, CI IR/proof/workcell/agentbridge and API tests. The paid external-model smoke is optional. Native sandbox and real Docker execution need separate lanes; a silent early return in the historical Docker smoke is not execution evidence. |
| Browser/contracts/UX | `bash scripts/ci.sh web` | Generated Rust contracts, unit/real-backend/UI browser tests, action matrix, Storybook, paired screenshots and rendered receipts; zero serious/critical axe violations; aggregate gzip JavaScript below 358,400 bytes; all collected Lighthouse performance scores at least 0.7. Lighthouse timing/resource warnings are not hard gates. |
| Source installation/runtime | `bash scripts/ci.sh runtime` | Embedded production build, build/source digest binding, installer tamper/stale-source refusal, installed binary authentication/Git/CLI/restart durability. Protected PR flow uses distinct authenticated author/reviewer/merger, rejects direct main pushes/self-approval/old-head approval and writer-published checks, excludes foreign-head checks, persists reviews across restart and merges the exact reviewed Git commit. Optional runner execution needs separate qualification. |
| Cache CLI poisoning; Codegraph CLI persistence | `bash scripts/ci.sh product` | All seven adversarial scenarios pass, including the five retained named safety assertions. Codegraph scans real source and a second process reads an equal nonempty cluster set from SQLite. |
| Native sandbox | `bash scripts/ci.sh sandbox` | Disposable Linux admission; every required test executes; four escapes blocked; zero false skips; resource limits, terminal/secret and watchdog proofs. |
| Docker confinement | `bash scripts/ci.sh oci` | Required disposable Linux rootful Docker/cgroup v2 lane, also emitted for Runner exports. Nine checks: writable workspace/read-only root, no host socket, no credential environment, denied egress, PID limit 32, memory limit 32 MiB, denied unshare syscall, NoNewPrivs, and cgroup limits. Regular matrix profile uses 64 PIDs/64 MiB. Successful positive controls and operation-specific denial evidence are required; engine/entrypoint exit 125–127 cannot count as confinement. Source/probe/seccomp identities bind the receipt, written only after guarded resource cleanup. This qualifies the network-denied `from_agent_job` profile; ordinary/final-session profiles and product image remain separate. Exact-source VM execution is pending. |
| Agent container image | **Pending portable command** | Retain custom seccomp, refusal wrappers, Git read/add/commit versus checkout/push guard, Cargo/Node/TypeScript presence, and governed auditor identity. A probe image alone does not qualify the product image. Remove private/floating image build inputs and tolerated producer failures. |
| Supply-chain baseline | `bash scripts/ci.sh security` | Verified tool downloads, npm high/critical rejection, `cargo audit --deny warnings`, license/source bans, history secrets, workflow checks and SPDX. Source/tool-bound CycloneDX receipts and Cache's closed eight-check/eight-tool custody remain pending. |
| Split exports | `bash scripts/ci.sh splits` | Generate twice with identical results, standalone locks/dependency identity, independently build/test from the provenance commit. Public source availability and privileged Runner proofs remain separate prerequisites. |
| Governed score | **Pending portable command** | Zero hard findings and caps. Floors: root/Core/Cache/Deploy/Work/Release Ops/Web 85; Intelligence 82; Finder 75; Tool 65; Runner effective floor 91 (its shared implementation strengthens the policy's 80). Public governed auditor and verified identity/receipt are prerequisites. |
| Protected-base execution/ratchets | **Pending portable command** | Cache/Work candidate at least 91 and protected baseline at least 85; Runner at least 91; Deploy at least 85. Nonnegative score delta where asserted; no new caps/hard findings; unchanged policy; exact changed-path set and clean source. Positive command/receipt count; every command exit zero; zero failed receipts; verification passes with zero issues. Copy-code hard classes/instances both zero. Baselines must come from authenticated protected source. |
| Coverage/mutation | **Pending portable command** | Deploy API baseline 0.8044, epsilon 0.005, effective lower bound 0.7994 with upward-only ratchet; changed-line coverage 0.90; total 0.75 advisory. Retain mutation policy `hard_survivors_on_changed_paths=1`; final audit hard findings zero and both evidence sources present. Tool requires real LCOV and passing coverage audit with zero hard findings. Missing coverage tools fail. |
| Cache Rust API compatibility | **Pending portable command** | Four libraries compared with immutable `jeryu-cache-v5.0.0-split.1` at `6bc56b87…`; no removed/changed APIs; unchanged governed build/policy/configuration; exact receipt hashes and final revalidation; verified cargo-public-api 0.52.0 identity/custody. |
| Shell/governance contracts | **Pending portable command** | Installer/root seal/renderer/auditor substitution/source authority/receipt custody; dispatch injection/order; repair receipts; ShellCheck at warning severity; closed schemas and complete owner/test routing. Component Git-root and package-count assumptions require explicit monorepo scope. |
| Signed central releases | **Pending portable command** | Signed source and matching PR publication metadata; binary route smoke; SPDX/CycloneDX/provenance/hash identities; previous signed rollback identity; SignRail local/dev-canary/prod receipt contracts with 100% signature coverage and matching source/rollback/artifact digests. Validate contracts without activating production. |

The `legacy` lane stays required while these pending gates are ported. Its
historical wrappers alone are insufficient: Core, Runner, Deploy,
Intelligence and Tool compatibility `ci-local.sh` scripts ignore `required`
and execute only their smaller `just` subsets. Separate proof/release
workflows must each receive a root command.

Do not port false success behavior. Core/Intelligence/Release Ops/Web carry
identical auxiliary proof templates that swallow producer failures and
generate substitute artifacts. Tool creates a ratchet baseline from its own
candidate, and one Deploy coverage wrapper translates missing-tool exit 3
into success. The stricter Cache/Runner/Work/Deploy assertions above are the
equivalence requirements. Required aggregate success remains unavailable
until every required proof has a functioning command and exact-source
execution evidence.
