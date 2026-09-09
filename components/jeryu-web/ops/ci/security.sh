#!/usr/bin/env bash
# jeryu-web security lane — secret scanning, dependency advisories, workflow
# security linting, and SBOM generation. Blocking on committed secrets/.env;
# dependency advisories are recorded (non-blocking) until triaged.
#
# Tool posture (family-standard, cf. redline-web / jmcp-web):
#   gitleaks detect · npm audit --audit-level=high · cargo audit · zizmor · syft SBOM
set -euo pipefail
source ops/ci/lib.sh
sec_log() { printf 'security: %s\n' "$*" >&2; }

if ! command -v npm >/dev/null 2>&1; then
  sec_log "npm is required to audit both dependency lockfiles"
  exit 1
fi
for lockfile in package-lock.json apps/web/package-lock.json; do
  if [[ ! -f "$lockfile" || -L "$lockfile" ]]; then
    sec_log "required npm lockfile is missing or not a regular file: $lockfile"
    exit 1
  fi
done
mkdir -p target/security

# --- secret scanning: gitleaks detect over tracked + untracked text -----------
if command -v gitleaks >/dev/null 2>&1; then
  sec_log "gitleaks detect (secret scan)"
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    {
      git ls-files -z
      git ls-files --others --exclude-standard -z
    } | sort -zu | while IFS= read -r -d '' path; do
      [[ -f "$path" ]] || continue
      case "$path" in
        target/*|node_modules/*|dist/*|apps/web/node_modules/*|apps/web/dist/*|apps/web/playwright-report/*|apps/web/storybook-static/*)
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
else
  sec_log "gitleaks not installed — secret scan skipped"
fi

# --- committed .env guard (hard fail) -----------------------------------------
if find . -path './.git' -prune -o -name '.env' -type f -print | grep -q .; then
  printf 'security check failed: committed .env file found\n' >&2
  exit 1
fi

# --- npm dependency advisories: audit both independently tracked lockfiles -----
root_npm_audit_status="clean"
app_npm_audit_status="clean"
sec_log "npm audit --audit-level=high (root workspace)"
if ! npm audit --audit-level=high --json > target/security/npm-audit-root.json 2>/dev/null; then
  root_npm_audit_status="advisories-recorded"
  sec_log "npm audit recorded advisories (non-blocking) — target/security/npm-audit-root.json"
fi
sec_log "npm audit --audit-level=high (apps/web)"
if ! npm audit --prefix apps/web --audit-level=high --json > target/security/npm-audit.json 2>/dev/null; then
  app_npm_audit_status="advisories-recorded"
  sec_log "npm audit recorded advisories (non-blocking) — target/security/npm-audit.json"
fi
npm_audit_status="clean"
if [[ "$root_npm_audit_status" != "clean" || "$app_npm_audit_status" != "clean" ]]; then
  npm_audit_status="advisories-recorded"
fi

# --- Rust dependency advisories: cargo audit (only when a Cargo.lock exists) ---
if [[ -f Cargo.lock ]] && command -v cargo-audit >/dev/null 2>&1; then
  sec_log "cargo audit (RustSec advisories)"
  cargo audit || sec_log "cargo audit recorded advisories (non-blocking)"
fi
if [[ -f Cargo.toml ]]; then
  cargo metadata --format-version 1 --no-deps >/dev/null
fi
if [[ "${JERYU_SECURITY_NETWORK:-0}" == "1" ]] && command -v cargo-deny >/dev/null 2>&1 && [[ -f deny.toml && -f Cargo.toml ]]; then
  cargo deny check
fi

# --- workflow security lint: zizmor -------------------------------------------
zizmor_status="skipped"
if command -v zizmor >/dev/null 2>&1 && [[ -d .github/workflows ]]; then
  sec_log "zizmor (GitHub Actions security lint)"
  if zizmor .github/workflows >/dev/null 2>&1; then
    zizmor_status="clean"
  else
    zizmor_status="findings-recorded"
    sec_log "zizmor recorded findings (non-blocking)"
  fi
elif command -v actionlint >/dev/null 2>&1 && [[ -d .github/workflows ]]; then
  actionlint .github/workflows/*.yml || sec_log "actionlint recorded findings (non-blocking)"
fi

# --- SBOM: syft (SPDX-JSON) ---------------------------------------------------
sbom_status="skipped"
if command -v syft >/dev/null 2>&1; then
  sec_log "syft SBOM (spdx-json)"
  if syft dir:. -o spdx-json=target/security/jeryu-web.spdx.json >/dev/null 2>&1; then
    sbom_status="generated"
  else
    sec_log "syft SBOM skipped"
  fi
fi

# --- evidence receipt ---------------------------------------------------------
cat > target/security/evidence.json <<JSON
{"schema_version":"jeryu.split.security/v1","checks":["gitleaks-detect","env-file","npm-audit","npm-audit-root","npm-audit-app","cargo-audit","zizmor","syft-sbom"],"npm_audit":"${npm_audit_status}","root_npm_audit":"${root_npm_audit_status}","app_npm_audit":"${app_npm_audit_status}","zizmor":"${zizmor_status}","sbom":"${sbom_status}"}
JSON
printf 'security ok\n'
