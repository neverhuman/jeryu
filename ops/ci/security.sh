#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
mkdir -p target/security
if find . -path './.git' -prune -o -name '.env' -type f -print | grep -q .; then
  printf 'security check failed: committed .env file found\n' >&2
  exit 1
fi
if [[ -f Cargo.toml ]]; then
  cargo metadata --format-version 1 --no-deps >/dev/null
fi
if [[ "${JERYU_SECURITY_NETWORK:-0}" == "1" ]] && command -v cargo-deny >/dev/null 2>&1 && [[ -f deny.toml ]]; then
  cargo deny check
fi
if [[ "${JERYU_SECURITY_NETWORK:-0}" == "1" && -f package-lock.json ]] && command -v npm >/dev/null 2>&1; then
  npm audit --audit-level=critical --omit=dev --json > target/security/npm-audit.json || {
    cat target/security/npm-audit.json >&2
    exit 1
  }
fi
cat > target/security/evidence.json <<'JSON'
{"schema_version":"jeryu.split.security/v1","checks":["env-file","cargo-metadata","optional-cargo-deny","optional-npm-audit"]}
JSON
printf 'security ok\n'
