#!/usr/bin/env bash
# ci-parity.sh — run the same checks remote CI runs, locally.
#
# Goal: if this script exits 0 locally, you can have FULL confidence that
# remote CI on PR #2 will pass. Mirrors `.github/workflows/rust.yml` +
# `.github/workflows/jankurai.yml`.
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

# ─── 1. RedlineDB binary proof (embedded DB tooling, not a service) ──────────
run "Install RedlineDB binary" bash scripts/install-redlinedb.sh
run "Install Jankurai binary" bash scripts/install-jankurai.sh
run "Jankurai version pin" bash -c 'jankurai --version | grep -Fx "jankurai 1.5.1"'

# ─── 2. Format (matches CI: cargo fmt --all -- --check) ──────────────────────
run "Format" cargo fmt --all -- --check

# ─── 3. Clippy (matches CI: cargo clippy --all-targets --all-features -- -D warnings) ───────
run "Clippy" cargo clippy --all-targets --all-features -- -D warnings

# ─── 4. Build (matches CI: cargo build --verbose) ────────────────────────────
run "Build" cargo build --verbose

# ─── 5. Library tests (matches CI: cargo nextest run -p jeryu --lib) ─────────
run "Library Tests (nextest)" cargo nextest run -p jeryu --lib

# ─── 6. Integration tests (matches CI: cargo test --tests --verbose -- --test-threads=1) ──
if [[ "$FAST" == "0" ]]; then
    run "Integration Tests" cargo test --tests --verbose -- --test-threads=1
fi

# ─── 7. TUI Smoke (matches CI: cargo run -- tui --once) ──────────────────────
PARITY_PREFIX="/tmp/jeryu-ci-parity-$$"
mkdir -p "$PARITY_PREFIX"
trap 'rm -rf "$PARITY_PREFIX"' EXIT
PARITY_DB="$PARITY_PREFIX/tui-smoke.redlineDB"
run "TUI Smoke (1-frame render)" env JERYU_DATABASE_URL="redline:$PARITY_DB?mode=rwc" cargo run --quiet -- tui --once

# ─── 8. Install Smoke (matches CI: cargo run -- install --dry-run) ──────────
run "Install Smoke (dry-run)" \
    cargo run --quiet -- install --dry-run --json --color never --prefix "$PARITY_PREFIX"

# ─── 9. TUI tuiwright tests (matches CI: TERM=xterm-256color cargo test --test tui_tuiwright) ──
run "TUI Tuiwright Tests" \
    env TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1

# ─── 10. Fixture Project Validation (matches CI: cd fixture && cargo test) ───
run "Fixture Project Validation" \
    bash -c 'cd tests/fixtures/fixture_project && cargo test --quiet'

# ─── 11. actionlint (matches CI's "Workflow lint" step in jankurai.yml) ──────
# shellcheck disable=SC2046  # we want word-splitting from the glob
run "Workflow Lint (actionlint)" actionlint $(ls .github/workflows/*.yml)

# ─── 12. Jankurai audit (matches CI: bash ops/ci/jankurai-lane.sh audit) ─────
mkdir -p target/ci-parity
run "Jankurai Audit" jankurai audit . \
    --full \
    --mode advisory \
    --json target/ci-parity/repo-score.json \
    --md target/ci-parity/repo-score.md

# ─── 13. Cargo deny (matches CI: cargo deny check) ───────────────────────────
run "Cargo Deny" cargo deny check

# ─── 14. Jansu messaging smoke (jansu-broker feature default-on) ─────────────
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

# ─── 15. No-default-features compile (diagnostic build-surface canary) ──────
if [[ "$FAST" == "0" ]]; then
    run "No-default-features Check" cargo check --no-default-features
fi

printf '\n%s━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%s\n' "$GREEN" "$RESET"
printf '%s✓ CI parity: ALL checks passed%s\n' "$GREEN" "$RESET"
printf '%sYou can push the current branch with full confidence.%s\n' "$GREEN" "$RESET"
printf '%s━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%s\n\n' "$GREEN" "$RESET"
