#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
exec bash "${ROOT}/scripts/check-agent-maps.sh"
