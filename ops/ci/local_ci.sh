#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

run() {
    local label="$1"
    shift
    printf '\n==> %s\n' "$label"
    "$@"
}

require() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "missing required tool: $1" >&2
        exit 1
    }
}

require cargo
require just
require jankurai
require docker
require git
require npm
require npx

cleanup_generated_sboms() {
    find "$ROOT" -path '*/sbom.json' -delete
}

trap cleanup_generated_sboms EXIT

prepare_ux_qa_cli() {
    local tool_dir
    tool_dir="$(mktemp -d)"
    git clone --depth 1 https://github.com/neverhuman/jankurai "$tool_dir/jankurai" >/dev/null 2>&1
    (
        cd "$tool_dir/jankurai"
        npm ci >/dev/null 2>&1
        npx playwright install chromium --with-deps >/dev/null 2>&1
        npm run ux-qa:build >/dev/null 2>&1
    )
    printf '%s\n' "$tool_dir/jankurai/packages/ux-qa/dist/cli.js"
}

UX_QA_CLI="$(prepare_ux_qa_cli)"

run "cargo fmt --all -- --check" cargo fmt --all -- --check
run "cargo clippy --workspace --exclude jeryu --all-targets --all-features -- -D warnings" \
    cargo clippy --workspace --exclude jeryu --all-targets --all-features -- -D warnings
run "just fast" just fast
run "cargo build --verbose" cargo build --verbose
run "cargo test --lib --verbose" cargo test --lib --verbose
run "cargo test --tests --verbose" cargo test --tests --verbose
run "TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1" \
    env TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1
run "cargo test --test ssh_install_test -- --ignored --test-threads=1" \
    cargo test --test ssh_install_test -- --ignored --test-threads=1
run "cargo test --test tui_recording -- --ignored --exact tui_demo_recording" \
    cargo test --test tui_recording -- --ignored --exact tui_demo_recording
run "fixture project validation" bash -lc 'cd tests/fixtures/fixture_project && cargo test --verbose'
run "bash tools/security-lane.sh ." bash tools/security-lane.sh .
run "UX QA smoke" bash -lc "cd '$ROOT' && node '$UX_QA_CLI' audit --config agent/ux-qa.toml --out target/ci-local/ux-qa.json"
run "Generate CycloneDX SBOM" bash -lc 'mkdir -p target/ci-local/sbom && cargo cyclonedx --format json --override-filename sbom && mv sbom.json target/ci-local/sbom/ && test -f target/ci-local/sbom/sbom.json'
run "jankurai audit" jankurai audit . --mode advisory --json target/ci-local/repo-score.json --md target/ci-local/repo-score.md
