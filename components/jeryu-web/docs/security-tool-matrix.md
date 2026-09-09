# Security Tool Matrix

The `jeryu-web` security lane (`tools/security-lane.sh` → `ops/ci/security.sh`)
runs directly through `just security` and through the exact named
`scripts/ci-local.sh security` path. The protected `required` dispatcher enters
`ops/ci/pr-ci.sh`, which executes the same security body before the Web product
tests.

| Concern | Tool | Command | Blocking |
| --- | --- | --- | --- |
| Secret scanning | gitleaks | `gitleaks detect` (tracked + untracked) | yes (secrets) |
| Committed secrets | built-in | `.env` file guard | yes |
| Root JS dependency advisories | npm | `npm audit --audit-level=high` (root workspace lock) | recorded |
| Web JS dependency advisories | npm | `npm audit --prefix apps/web --audit-level=high` | recorded |
| Rust dependency advisories | cargo-audit | `cargo audit` (when `Cargo.lock`) | recorded |
| Workflow security lint | zizmor | `zizmor .github/workflows` | recorded |
| SBOM / provenance | syft | `syft dir:. -o spdx-json` | recorded |

Dependency advisories and workflow findings are **recorded** (written to
`target/security/`) rather than merge-blocking until triaged; committed secrets
and `.env` files are a hard fail. Enable network-dependent scans with
`JERYU_SECURITY_NETWORK=1` (set in CI). Evidence receipt:
`target/security/evidence.json`. The root and `apps/web` lockfiles are audited
separately and write `npm-audit-root.json` and `npm-audit.json`; neither lock can
silently stand in for the other. Missing npm or either expected physical lockfile
is a hard failure. The legacy `npm_audit` receipt field conservatively aggregates
both audit results.
