#!/usr/bin/env bash
# Canonical jeryu-web security lane wrapper.
#
# Runs the operational security posture and is the single entry point referenced
# by `just security`, the root `npm run security` script, and CI. The lane runs:
#   gitleaks detect   — secret scanning
#   npm audit --audit-level=high   — root-workspace and apps/web advisories
#   cargo audit       — Rust dependency advisories (when a Cargo.lock exists)
#   zizmor            — GitHub Actions workflow security lint
#   syft              — SPDX-JSON SBOM generation
set -euo pipefail
here="$(cd "$(/usr/bin/dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
if [[ "$#" -ne 0 ]]; then
  printf 'security-lane accepts no arguments\n' >&2
  exit 2
fi
cd "$here"
security_script="$here/ops/ci/security.sh"
if [[ ! -f "$security_script" || -L "$security_script" ]]; then
  printf 'security lane target is missing or not a regular file\n' >&2
  exit 1
fi
exec /usr/bin/bash "$security_script"
