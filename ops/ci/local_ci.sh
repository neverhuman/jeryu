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
require node

cleanup_generated_sboms() {
    find "$ROOT" -path '*/sbom.json' -delete
}

trap cleanup_generated_sboms EXIT

prepare_ux_qa_cli() {
    local tool_dir
    tool_dir="$(mktemp -d)"
    # Jankurai is always installed from URL with an explicit version tag.
    # Keep this tag in sync with the Rust install in .github/workflows/jankurai.yml.
    git clone --branch v1.4.1 --depth 1 https://github.com/neverhuman/jankurai "$tool_dir/jankurai" >/dev/null 2>&1
    (
        cd "$tool_dir/jankurai"
        npm ci >/dev/null 2>&1
        npx playwright install chromium --with-deps >/dev/null 2>&1
        npm run ux-qa:build >/dev/null 2>&1
    )
    printf '%s\n' "$tool_dir/jankurai/packages/ux-qa/dist/cli.js"
}

UX_QA_CLI="$(prepare_ux_qa_cli)"

prepare_ux_qa_smoke_config() {
    local smoke_dir
    smoke_dir="$ROOT/target/ci-local/ux-qa-smoke"
    mkdir -p "$smoke_dir"
    cat > "$smoke_dir/index.html" <<'HTML'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>UX QA smoke</title>
  <style>
    :root { color-scheme: light; font-family: system-ui, sans-serif; }
    body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f4f7fb; color: #1b2330; }
    main { width: min(560px, calc(100vw - 48px)); padding: 32px; border-radius: 24px; background: white; box-shadow: 0 18px 60px rgba(15, 23, 42, 0.12); }
    h1 { margin: 0 0 12px; font-size: 2.2rem; line-height: 1.05; }
    p { margin: 0 0 18px; font-size: 1rem; line-height: 1.5; color: #475569; }
    button { border: 0; border-radius: 999px; padding: 12px 18px; font: inherit; font-weight: 700; background: #0f172a; color: white; }
  </style>
</head>
<body>
  <main>
    <h1 id="state-label">Loading</h1>
    <p id="state-copy">This fixture exercises the UX QA tool with explicit route states.</p>
    <button type="button">Primary action</button>
  </main>
  <script>
    const state = new URLSearchParams(location.search).get("state") ?? "loading";
    const label = document.getElementById("state-label");
    const copy = document.getElementById("state-copy");
    const labels = {
      loading: "Loading",
      empty: "Empty",
      error: "Error",
      success: "Success",
      "permission-denied": "Permission denied"
    };
    const descriptions = {
      loading: "The page is waiting on data.",
      empty: "The page has no items to show.",
      error: "The page encountered a recoverable failure.",
      success: "The page loaded successfully.",
      "permission-denied": "The user lacks access to the requested view."
    };
    label.textContent = labels[state] ?? state;
    copy.textContent = descriptions[state] ?? "Unknown state.";
  </script>
</body>
</html>
HTML
    cat > "$smoke_dir/ux-qa.toml" <<EOF
artifactRoot = "target/ci-local/ux-qa"
readyState = "domcontentloaded"
timeoutMs = 15000
screenshotRequired = true
ariaSnapshotRequired = true
accessibilityScanRequired = true
requiredStates = ["loading", "empty", "error", "success", "permission-denied"]
stateQueryParam = "state"

[[routes]]
id = "ux-qa-smoke"
url = "file://$ROOT/target/ci-local/ux-qa-smoke/index.html"
states = ["loading", "empty", "error", "success", "permission-denied"]
EOF
    printf '%s\n' "$smoke_dir/ux-qa.toml"
}

UX_QA_SMOKE_CONFIG="$(prepare_ux_qa_smoke_config)"

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
run "UX QA smoke" bash -lc "cd '$ROOT' && node '$UX_QA_CLI' audit --config '$UX_QA_SMOKE_CONFIG' --out target/ci-local/ux-qa.json"
run "Generate CycloneDX SBOM" bash -lc 'mkdir -p target/ci-local/sbom && cargo cyclonedx --format json --override-filename sbom && mv sbom.json target/ci-local/sbom/ && test -f target/ci-local/sbom/sbom.json'
run "jankurai audit" jankurai audit . --mode advisory --json target/ci-local/repo-score.json --md target/ci-local/repo-score.md
