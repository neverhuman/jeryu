#!/usr/bin/env bash
# Canonical source, dependency, and workflow security lane.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"
# shellcheck source=ops/ci/lib.sh
source ops/ci/lib.sh

# Public candidate security consumes the one admitted monorepo lock.
cargo_lock_path="$PWD/Cargo.lock"
security_manifests=("$PWD/Cargo.toml")
if [[ ${JERYU_MONOREPO_CANDIDATE:-0} != 0 ]]; then
  require_jankurai
  source ops/ci/workspace-lock.sh
  jeryu_deploy_record_workspace_lock "$ROOT"
  # shellcheck source=ops/ci/cargo-scope.sh
  source ops/ci/cargo-scope.sh
  [[ $component_root == "$git_root/components/jeryu-deploy" ]] || exit 1
  cargo_lock_path="$git_root/Cargo.lock"
  security_manifests=()
  for security_package in "${owned_packages[@]}"; do
    security_manifest=$(jq -er --arg name "$security_package" '[.packages[] | select(.name == $name) | .manifest_path] | if length == 1 then .[0] else error("owning manifest identity changed") end' <<< "$cargo_metadata")
    [[ $security_manifest == "$component_root/"* && -f $security_manifest &&
       ! -L $security_manifest && $(realpath -e -- "$security_manifest") == "$security_manifest" &&
       $(stat -c %h -- "$security_manifest") == 1 ]] || exit 1
    security_manifests+=("$security_manifest")
  done
fi
[[ -f $cargo_lock_path && ! -L $cargo_lock_path &&
   $(realpath -e -- "$cargo_lock_path") == "$cargo_lock_path" &&
   $(stat -c %h -- "$cargo_lock_path") == 1 ]] || {
  printf 'security requires a physical single-link workspace lock\n' >&2; exit 1;
}
exec {cargo_lock_fd}< "$cargo_lock_path"
cargo_lock_identity=$(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "/proc/$BASHPID/fd/$cargo_lock_fd")
[[ $(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$cargo_lock_path") == "$cargo_lock_identity" ]] || exit 1
cargo_lock_before=$(sha256sum -- "$cargo_lock_path")

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
  cargo audit --deny warnings --file "$cargo_lock_path"
  # Each owning root retains its complete dependency closure; never exclude a dependency.
  for security_manifest in "${security_manifests[@]}"; do
    cargo deny --locked --manifest-path "$security_manifest" check --config "$ROOT/deny.toml" advisories bans licenses sources
  done
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


[[ -f $cargo_lock_path && ! -L $cargo_lock_path &&
   $(realpath -e -- "$cargo_lock_path") == "$cargo_lock_path" &&
   $(stat -c %h -- "$cargo_lock_path") == 1 &&
   $(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$cargo_lock_path") == "$cargo_lock_identity" &&
   $(stat -Lc '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "/proc/$BASHPID/fd/$cargo_lock_fd") == "$cargo_lock_identity" &&
   $(sha256sum -- "$cargo_lock_path") == "$cargo_lock_before" ]] || {
  printf 'workspace lock changed during security checks\n' >&2; exit 1;
}
if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
  jeryu_deploy_assert_workspace_lock_unchanged
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
