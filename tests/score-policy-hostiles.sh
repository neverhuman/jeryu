#!/usr/bin/env bash
# Exercise the original 256 policy fixtures through every actual owning script.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
exec bash "$root/tests/score-report-hostiles.sh" --policy-matrix
