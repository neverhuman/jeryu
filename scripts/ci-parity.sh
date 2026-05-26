#!/usr/bin/env bash
# ci-parity.sh — canonical full CI parity gate for local pre-PR validation.
#
# Goal: if this script exits 0 locally, you have the same lane coverage the
# GitHub workflows exercise for the PR surface.
#
# Usage:
#   bash scripts/ci-parity.sh           # run everything
#   bash scripts/ci-parity.sh --fast    # skip slow integration-style checks
#
# Exits non-zero on first failure. Output is grouped per-check.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

FAST=0
for arg in "$@"; do
    case "$arg" in
        --fast) FAST=1 ;;
        *) echo "unknown arg: $arg" >&2; exit 2 ;;
    esac
done

GREEN=$'\033[32m'
RED=$'\033[31m'
DIM=$'\033[2m'
RESET=$'\033[0m'

step() {
    printf '\n%s━━ %s ━━%s\n' "$DIM" "$1" "$RESET"
}

run() {
    local label="$1"
    shift
    step "$label"
    if "$@"; then
        printf '%s✓ %s%s\n' "$GREEN" "$label" "$RESET"
    else
        local rc=$?
        printf '%s✗ %s (exit %d)%s\n' "$RED" "$label" "$rc" "$RESET"
        exit $rc
    fi
}

# ─── 1. Backend-specific native binary proof ────────────────────────────────
if [[ "${JERYU_DB_BACKEND:-sqlite}" == "redlinedb" || "${JERYU_DATABASE_URL:-}" == redline:* || "${JERYU_DATABASE_URL:-}" == redlinedb:* ]]; then
    run "Install RedlineDB binary" bash scripts/install-redlinedb.sh
fi
run "Install Jankurai binary" bash scripts/install-jankurai.sh
run "Jankurai version pin" bash -c 'jankurai --version | grep -Fx "jankurai 1.5.1"'
run "Workflow lint" actionlint .github/workflows/*.yml

# ─── 2. Rust workflow parity ────────────────────────────────────────────────
run "Format" bash ops/ci/rust-lane.sh fmt
run "Clippy" bash ops/ci/rust-lane.sh clippy
run "Build" bash ops/ci/rust-lane.sh build
run "Install Smoke" bash ops/ci/rust-lane.sh install-smoke
run "Test Selection" bash ops/ci/rust-lane.sh test-select
run "Library Tests (nextest)" bash ops/ci/rust-lane.sh test-lib
run "Offline Mock Tests" bash ops/ci/rust-lane.sh test-mock
if [[ "$FAST" == "0" ]]; then
    run "Integration Tests" bash ops/ci/rust-lane.sh test-integration
fi
run "TUI Smoke (1-frame render)" bash ops/ci/rust-lane.sh tui-smoke
run "Supply Chain Audit" bash ops/ci/rust-lane.sh supply-chain
run "Witness Graph" bash ops/ci/rust-lane.sh witness
run "VRC Map" bash ops/ci/rust-lane.sh vrc-map
run "VRC Plan" bash ops/ci/rust-lane.sh vrc-plan
run "AER Scan" bash ops/ci/rust-lane.sh aer
run "SSH Install E2E" bash ops/ci/rust-lane.sh ssh-install-e2e
run "TUI Screenshots" bash ops/ci/rust-lane.sh tui-screenshots
run "TUI Recording" bash ops/ci/rust-lane.sh tui-recording
run "Fixture Project Validation" bash ops/ci/rust-lane.sh fixture-project-test
run "Fixture Project Clippy" bash ops/ci/rust-lane.sh fixture-project-clippy
run "Rust Deny" bash ops/ci/rust-lane.sh deny

# ─── 3. Jankurai workflow parity ─────────────────────────────────────────────
mkdir -p target/ci-parity
run "Security strict preflight" bash ops/ci/jankurai-lane.sh security
run "Jankurai Audit" bash ops/ci/jankurai-lane.sh audit
run "Proof lanes" bash ops/ci/jankurai-lane.sh proof
run "Audit tools" bash ops/ci/jankurai-lane.sh tools
run "Bad-behavior checks" bash ops/ci/jankurai-lane.sh bad-behavior
run "CycloneDX SBOM" bash ops/ci/jankurai-lane.sh sbom

# ─── 4. Additional parity and hardening checks ──────────────────────────────
run "Cargo Deny" cargo deny check

# ─── 5. Scheduled hardening (matches weekly CI hardening job) ──────────────
if [[ "$FAST" == "0" ]]; then
    run "Scheduled Hardening" bash ops/ci/rust-lane.sh hardening
else
    echo "scheduled hardening skipped in fast mode"
fi

# ─── 6. Jansu messaging smoke (jansu-broker feature default-on) ─────────────
# Validates that the embedded broker + consumer-loop wire correctly. Skipped
# automatically when --no-default-features builds drop jansu-embedded.
if [[ "$FAST" == "0" ]]; then
    run "Jansu Messaging Smoke" \
        cargo test --features jansu-broker \
            --test jansu_webhook_jobs_roundtrip \
            --test jansu_consumer_resumes_after_restart \
            --test jansu_three_topics_no_crosstalk \
            -- --test-threads=1
fi

# ─── 7. No-default-features compile (diagnostic build-surface canary) ──────
if [[ "$FAST" == "0" ]]; then
    run "No-default-features Check" cargo check --no-default-features
fi

printf '\n%s━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%s\n' "$GREEN" "$RESET"
printf '%s✓ CI parity: ALL checks passed%s\n' "$GREEN" "$RESET"
printf '%sYou can push the current branch with full confidence.%s\n' "$GREEN" "$RESET"
printf '%s━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%s\n\n' "$GREEN" "$RESET"
