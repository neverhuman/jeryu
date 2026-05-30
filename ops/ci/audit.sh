#!/usr/bin/env bash
set -euo pipefail
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check
else
  echo "cargo-deny not installed; skipping dependency audit smoke gate"
fi
