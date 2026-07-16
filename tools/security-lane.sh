#!/usr/bin/env bash
# Canonical supply-chain entrypoint used by local and protected CI.
# The delegated lane executes gitleaks detect and actionlint, runs cached
# `cargo audit --no-fetch` when a Cargo lock exists, and renders a local
# SPDX-JSON SBOM with `syft dir:.`; every outcome is recorded in its receipt.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
exec bash ops/ci/security.sh "$@"
