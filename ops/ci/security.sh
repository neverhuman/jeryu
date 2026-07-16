#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
mkdir -p target/security
if command -v gitleaks >/dev/null 2>&1; then
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    {
      git ls-files -z
      git ls-files --others --exclude-standard -z
    } | sort -zu | while IFS= read -r -d '' path; do
      [[ -f "$path" ]] || continue
      case "$path" in
        target/*|.jankurai/*)
          continue
          ;;
      esac
      if LC_ALL=C grep -Iq . "$path"; then
        printf '\n===== %s =====\n' "$path"
        cat "$path"
      fi
    done | gitleaks detect --pipe --redact --verbose
  else
    gitleaks detect --no-git --redact --verbose
  fi
fi
if command -v actionlint >/dev/null 2>&1 && [[ -d .github/workflows ]]; then
  actionlint .github/workflows/*.yml
fi
if find . -path './.git' -prune -o -name '.env' -type f -print | grep -q .; then
  printf 'security check failed: committed .env file found\n' >&2
  exit 1
fi
cargo_audit_status="skipped-no-lock"
if [[ -f Cargo.lock ]] && command -v cargo-audit >/dev/null 2>&1; then
  if cargo audit --no-fetch --format json > target/security/cargo-audit.json 2>/dev/null; then
    cargo_audit_status="clean"
  else
    cargo_audit_status="findings-or-offline-db-unavailable"
  fi
fi
sbom_status="skipped-tool-unavailable"
if command -v syft >/dev/null 2>&1; then
  if syft dir:. --exclude './target/**' --exclude './.git/**' \
    -o spdx-json=target/security/jeryu.spdx.json >/dev/null 2>&1; then
    sbom_status="generated"
  else
    sbom_status="generation-failed"
  fi
fi
cat > target/security/evidence.json <<JSON
{"schema_version":"jeryu.split.security/v1","checks":["gitleaks-detect","actionlint","env-file","cargo-audit-no-fetch","syft-sbom"],"cargo_audit":"${cargo_audit_status}","sbom":"${sbom_status}"}
JSON
printf 'security ok\n'
