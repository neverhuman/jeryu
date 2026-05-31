#!/usr/bin/env bash
# Workflow linting lane: statically lint the GitHub Actions workflows with
# actionlint (syntax, shellcheck, expression checks) and zizmor (CI security
# auditor). Both run offline against .github/workflows/*.yml.
#
# Tools used: actionlint, zizmor.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

log() { printf '[workflow-lint] %s\n' "$*"; }

jeryu_gate() {
  local crate="$1"; shift
  if [ "$crate" = "jeryu-repogate" ]; then
    cargo run -q --release -p "${crate}" -- "$@"
    return
  fi
  local bin="target/release/${crate}"
  if [ -x "${bin}" ]; then
    "${bin}" "$@"
  else
    cargo run -q --release -p "${crate}" -- "$@"
  fi
}

shopt -s nullglob
WORKFLOWS=(.github/workflows/*.yml .github/workflows/*.yaml)
shopt -u nullglob
if [ "${#WORKFLOWS[@]}" -eq 0 ]; then
  echo "[workflow-lint] no workflows found under .github/workflows" >&2
  exit 1
fi

# --- actionlint -------------------------------------------------------------
if command -v actionlint >/dev/null 2>&1; then
  log "running actionlint on ${#WORKFLOWS[@]} workflow file(s)"
  actionlint "${WORKFLOWS[@]}"
  log "actionlint: clean"
else
  echo "[workflow-lint] actionlint not installed; install from https://github.com/rhysd/actionlint" >&2
  exit 1
fi

# --- zizmor -----------------------------------------------------------------
if command -v zizmor >/dev/null 2>&1; then
  log "running zizmor (offline) on ${#WORKFLOWS[@]} workflow file(s)"
  # --offline avoids network calls to the GitHub API in air-gapped CI/local runs.
  zizmor --offline --min-severity medium "${WORKFLOWS[@]}"
  log "zizmor: no medium+ findings"
else
  echo "[workflow-lint] zizmor not installed; install with: cargo install zizmor" >&2
  exit 1
fi

log "checking workflow/manifest parity"
jeryu_gate jeryu-repogate ci-lanes-check

log "workflow linting complete"
