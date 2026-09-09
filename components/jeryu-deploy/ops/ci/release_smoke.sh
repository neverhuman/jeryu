#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

BIN="${1:-target/release/jeryu}"

export JERYU_BOOTSTRAP_ADMIN_PASSWORD="${JERYU_BOOTSTRAP_ADMIN_PASSWORD:-release-smoke-admin-password-123}"
cargo run --locked -q -p jeryu-cli --bin jeryu-release-smoke -- "${BIN}"
