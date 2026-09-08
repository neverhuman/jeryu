#!/usr/bin/env bash
# Canonical source, dependency, and workflow security lane.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"
# shellcheck source=ops/ci/lib.sh
source ops/ci/lib.sh

GITLEAKS_VERSION="${GITLEAKS_VERSION:-8.21.2}"
ACTIONLINT_VERSION="${ACTIONLINT_VERSION:-1.7.8}"
checks=()

require_tool gitleaks
require_tool actionlint
require_tool jq
if [[ "$(gitleaks version)" != "${GITLEAKS_VERSION}" ]]; then
  printf 'gitleaks version mismatch: expected %s, got %s\n' \
    "${GITLEAKS_VERSION}" "$(gitleaks version 2>&1)" >&2
  exit 1
fi
if ! actionlint --version 2>&1 | grep -Eq "^${ACTIONLINT_VERSION//./\\.}([[:space:]]|$)"; then
  printf 'actionlint version mismatch: expected %s\n' "${ACTIONLINT_VERSION}" >&2
  exit 1
fi

mkdir -p target/security
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  {
    git ls-files -z
    git ls-files --others --exclude-standard -z
  } | sort -zu | while IFS= read -r -d '' path; do
    [[ -f "${path}" ]] || continue
    case "${path}" in
      target/*|node_modules/*|dist/*|apps/web/node_modules/*|apps/web/dist/*|apps/web/playwright-report/*|apps/web/storybook-static/*)
        continue
        ;;
    esac
    if LC_ALL=C grep -Iq . "${path}"; then
      printf '\n===== %s =====\n' "${path}"
      cat "${path}"
    fi
  done | gitleaks detect --pipe --redact --verbose
else
  gitleaks detect --no-git --redact --verbose
fi
checks+=("gitleaks-${GITLEAKS_VERSION}")

if [[ -d .github/workflows ]]; then
  actionlint .github/workflows/*.yml
  checks+=("actionlint-${ACTIONLINT_VERSION}")
fi

if find . -path './.git' -prune -o -name '.env' -type f -print | grep -q .; then
  printf 'security check failed: committed .env file found\n' >&2
  exit 1
fi
checks+=("env-file-absence")

if [[ -f Cargo.toml ]]; then
  cargo metadata --locked --format-version 1 --no-deps >/dev/null
  checks+=("cargo-metadata-locked")
fi

if [[ "${JERYU_SECURITY_NETWORK:-0}" == "1" ]]; then
  require_tool cargo-audit
  require_tool cargo-deny
  cargo audit --deny warnings
  cargo deny check advisories bans licenses sources
  bash ops/ci/dependency-sources.sh
  checks+=("cargo-audit" "cargo-deny" "hosted-dependency-sources")
  if [[ -f package-lock.json ]]; then
    require_tool npm
    npm audit --audit-level=critical --omit=dev --json > target/security/npm-audit.json || {
      cat target/security/npm-audit.json >&2
      exit 1
    }
    checks+=("npm-audit-critical")
  fi
fi

checks_json="$(printf '%s\n' "${checks[@]}" | jq -R . | jq -s .)"
jq -n \
  --arg schema "jeryu.split.security/v2" \
  --arg head "$(git rev-parse HEAD 2>/dev/null || printf unknown)" \
  --argjson network "$(if [[ "${JERYU_SECURITY_NETWORK:-0}" == "1" ]]; then printf true; else printf false; fi)" \
  --argjson checks "${checks_json}" \
  '{schema_version:$schema,git_head:$head,network_dependency_checks:$network,checks:$checks,conclusion:"success"}' \
  > target/security/evidence.json
printf 'security ok: %s\n' "${checks[*]}"
