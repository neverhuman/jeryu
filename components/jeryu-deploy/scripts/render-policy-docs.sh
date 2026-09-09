#!/usr/bin/env bash
set -euo pipefail
mkdir -p docs/generated
{
  echo '# Generated cache laws'
  echo
  grep -E '^(id|text) =' policies/cache-laws.toml || true
} > docs/generated/cache-laws.md
