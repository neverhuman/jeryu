#!/usr/bin/env bash
# Canonical security-lane wrapper.
#
# This is the canonical path (`tools/security-lane.sh`) that `jankurai security run`
# invokes by default and that the supply-chain posture audit treats as the security
# lane entrypoint. It delegates to the real, runnable security lane so that local
# runs, CI, and `jankurai security run` all execute the identical command surface.
#
# Command surface (all real and runnable):
#   * secret scanning            -> scripts/secret-scan.sh and gitleaks detect --no-git
#   * dependency review          -> cargo audit --deny warnings, cargo deny, npm audit --prefix web
#   * SBOM + provenance/SLSA      -> ops/ci/sbom-provenance.sh (syft SBOM, grype, cosign)
#   * workflow linting           -> ops/ci/workflow-lint.sh (actionlint, zizmor)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

exec bash ops/ci/security.sh "$@"
