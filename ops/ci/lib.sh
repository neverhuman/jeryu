#!/usr/bin/env bash
set -euo pipefail

require_tool() {
  local name="$1"
  command -v "$name" >/dev/null 2>&1 || {
    printf 'missing required tool: %s\n' "$name" >&2
    exit 1
  }
}

require_jankurai() {
  local expected="jankurai 1.6.10"
  local actual
  actual="$(jankurai --version 2>/dev/null || true)"
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected %s, got %s\n' "$expected" "${actual:-missing jankurai}" >&2
    exit 1
  fi
}
