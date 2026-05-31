#!/usr/bin/env bash
# Thin local/hosted parity wrapper for the affected fast lane.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

bash ci-fast-push.sh --no-push
