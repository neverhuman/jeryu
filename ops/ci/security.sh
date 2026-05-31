#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

echo "security lane: cache adversary, poisoning matrix, zero-evidence, secret scanning, dependency scanning, workflow linting, and release provenance/SBOM"

# Adversarial cache + zero-evidence proofs.
cargo test -p jeryu-cache-adversary --jobs "${JERYU_CI_JOBS}"
./tests/cache_poisoning_matrix.sh
jeryu_gate jeryu-evidence .

# Secret scanning.
./scripts/secret-scan.sh

# Dependency review: cargo audit + cargo deny policy (advisories, bans, licenses, sources).
cargo audit --deny warnings
cargo deny check advisories bans licenses sources

# Workflow linting (actionlint + zizmor) over .github/workflows/*.
bash ops/ci/workflow-lint.sh

# SBOM + supply-chain provenance (syft SBOM, grype scan, SLSA provenance, cosign signing).
bash ops/ci/sbom-provenance.sh

# Release signing/provenance proofs.
cargo test -p jeryu-signrail --jobs "${JERYU_CI_JOBS}"
