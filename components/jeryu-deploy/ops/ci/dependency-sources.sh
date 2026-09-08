#!/usr/bin/env bash
# Validate Cargo source identity separately from effective Git transport.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
source "${ROOT}/ops/ci/hosted-git-env.sh"

overlay="${ROOT}/.cargo/hosted-gitconfig"
pin_policy="${ROOT}/.cargo/hosted-pin-refs.tsv"
active_global="${GIT_CONFIG_GLOBAL}"
credential_helper="/home/ubuntu/.config/jeryu/bin/git-credential-neverhuman-org"
for path in "${overlay}" "${pin_policy}" "${ROOT}/Cargo.lock" "${ROOT}/deny.toml" \
  "${active_global}"; do
  if [[ ! -f "${path}" || -L "${path}" || "$(stat -c '%h' -- "${path}")" != 1 ]]; then
    printf 'dependency source input must be a one-link regular file: %s\n' "${path}" >&2
    exit 1
  fi
done
active_global_sha256="$(sha256sum "${active_global}" | awk '{print $1}')"

config_file() {
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    git config --file "$1" "${@:2}"
}

expected_pairs=(
  'http://127.0.0.1:8787/git/jeryu/jeryu-core.git|https://git.neverhuman.org/git/jeryu/jeryu-core.git'
  'https://github.com/neverhuman/jeryu-core.git|https://git.neverhuman.org/git/jeryu/jeryu-core.git'
  'http://127.0.0.1:8787/git/jeryu/jeryu-intelligence.git|https://git.neverhuman.org/git/jeryu/jeryu-intelligence.git'
  'https://github.com/neverhuman/jeryu-intelligence.git|https://git.neverhuman.org/git/jeryu/jeryu-intelligence.git'
  'https://github.com/neverhuman/jeryu-ci-runner.git|https://git.neverhuman.org/git/jeryu/jeryu-ci-runner.git'
  'https://github.com/neverhuman/jeryu-jira.git|https://git.neverhuman.org/git/jeryu/jeryu-jira.git'
  'https://github.com/neverhuman/jeryu-release-ops.git|https://git.neverhuman.org/git/jeryu/jeryu-release-ops.git'
)

mapfile -t configured_pairs < <(
  awk '
    /^\[url "/ {
      target = $0
      sub(/^\[url "/, "", target)
      sub(/"\]$/, "", target)
      next
    }
    /^[[:space:]]*insteadOf[[:space:]]*=/ {
      source = $0
      sub(/^[^=]*=[[:space:]]*/, "", source)
      print source "|" target
    }
  ' "${overlay}" | LC_ALL=C sort
)
mapfile -t expected_sorted < <(printf '%s\n' "${expected_pairs[@]}" | LC_ALL=C sort)
if [[ "$(printf '%s\n' "${configured_pairs[@]}")" != "$(printf '%s\n' "${expected_sorted[@]}")" ]]; then
  printf 'hosted Git overlay mapping inventory differs from the exact allowlist\n' >&2
  exit 1
fi
if ! overlay_config_text="$(config_file "${overlay}" --list)"; then
  printf 'sealed hosted Git policy is not valid Git configuration\n' >&2
  exit 1
fi
if ! active_config_text="$(config_file "${active_global}" --list)"; then
  printf 'active Git transport input is not valid Git configuration\n' >&2
  exit 1
fi
mapfile -t overlay_config < <(printf '%s\n' "${overlay_config_text}" | LC_ALL=C sort)
mapfile -t active_config < <(printf '%s\n' "${active_config_text}" | LC_ALL=C sort)
if [[ "$(printf '%s\n' "${active_config[@]}")" != \
      "$(printf '%s\n' "${overlay_config[@]}")" ]]; then
  printf 'active Git transport configuration differs from the sealed hosted policy\n' >&2
  exit 1
fi
if config_file "${overlay}" --get-regexp '^(include|includeif)\.' >/dev/null 2>&1; then
  printf 'hosted Git overlay may not include ambient configuration\n' >&2
  exit 1
fi
if [[ "$(config_file "${overlay}" --get-all credential.helper)" != '' ]] ||
   [[ "$(config_file "${overlay}" --get-all credential.https://git.neverhuman.org.helper)" != "${credential_helper}" ]]; then
  printf 'hosted Git overlay credential-helper policy differs from the exact default\n' >&2
  exit 1
fi
if [[ "$(config_file "${overlay}" --get http.https://git.neverhuman.org.postbuffer)" != 1 ]]; then
  printf 'hosted Git overlay is missing the exact smart-HTTP compatibility setting\n' >&2
  exit 1
fi
if [[ ! -f "${credential_helper}" || -L "${credential_helper}" ||
      ! -x "${credential_helper}" || "$(stat -c '%h' -- "${credential_helper}")" != 1 ]]; then
  printf 'hosted credential helper must be an executable one-link regular file: %s\n' \
    "${credential_helper}" >&2
  exit 1
fi

# The quoted program is intentionally expanded by the child shell.
# shellcheck disable=SC2016
default_global="$(env -u GIT_CONFIG_GLOBAL bash -c \
  'source "$1"; printf "%s" "$GIT_CONFIG_GLOBAL"' _ \
  "${ROOT}/ops/ci/hosted-git-env.sh")"
if [[ "${default_global}" != "${overlay}" ]]; then
  printf 'CI Git environment did not select the repository overlay\n' >&2
  exit 1
fi
caller_global="$(GIT_CONFIG_GLOBAL="${ROOT}/deny.toml" bash -c \
  'source "$1"; printf "%s" "$GIT_CONFIG_GLOBAL"' _ \
  "${ROOT}/ops/ci/hosted-git-env.sh")"
if [[ "${caller_global}" != "${ROOT}/deny.toml" ]]; then
  printf 'CI Git environment overrode a caller-supplied sealed config\n' >&2
  exit 1
fi

effective_url() {
  local source="$1"
  (
    cd /
    unset GIT_CONFIG GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS GIT_CONFIG_SYSTEM
    while IFS= read -r injected; do
      [[ -z "${injected}" ]] || unset "${injected}"
    done < <(compgen -A variable GIT_CONFIG_KEY_ || true)
    while IFS= read -r injected; do
      [[ -z "${injected}" ]] || unset "${injected}"
    done < <(compgen -A variable GIT_CONFIG_VALUE_ || true)
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="${active_global}" \
      git ls-remote --get-url "${source}"
  )
}

for pair in "${expected_pairs[@]}"; do
  source_url="${pair%%|*}"
  expected_url="${pair#*|}"
  actual_url="$(effective_url "${source_url}")"
  if [[ "${actual_url}" != "${expected_url}" ]]; then
    printf 'dependency transport mismatch: source=%s expected=%s actual=%s\n' \
      "${source_url}" "${expected_url}" "${actual_url}" >&2
    exit 1
  fi
done

expected_lock_sources=(
  'http://127.0.0.1:8787/git/jeryu/jeryu-core.git'
  'http://127.0.0.1:8787/git/jeryu/jeryu-intelligence.git'
  'https://github.com/neverhuman/jeryu-ci-runner.git'
  'https://github.com/neverhuman/jeryu-intelligence.git'
  'https://github.com/neverhuman/jeryu-jira.git'
  'https://github.com/neverhuman/jeryu-release-ops.git'
)
mapfile -t lock_sources < <(
  awk -F '"' '/^source = "git\+/ {
    source = substr($2, 5)
    sub(/\?.*$/, "", source)
    print source
  }' Cargo.lock | LC_ALL=C sort -u
)
mapfile -t expected_lock_sorted < <(printf '%s\n' "${expected_lock_sources[@]}" | LC_ALL=C sort)
if [[ "$(printf '%s\n' "${lock_sources[@]}")" != "$(printf '%s\n' "${expected_lock_sorted[@]}")" ]]; then
  printf 'Cargo.lock Git source inventory differs from deny/transport policy\n' >&2
  exit 1
fi
while IFS= read -r locked_source; do
  if [[ ! "${locked_source}" =~ ^git\+[^?]+\?tag=[A-Za-z0-9._-]+#[0-9a-f]{40}$ ]]; then
    printf 'Cargo.lock Git source is not immutable tag plus full commit: %s\n' \
      "${locked_source}" >&2
    exit 1
  fi
done < <(awk -F '"' '/^source = "git\+/ { print $2 }' Cargo.lock)

mapfile -t hosted_pins < <(awk '!/^[[:space:]]*(#|$)/ { print }' "${pin_policy}")
if [[ "${#hosted_pins[@]}" -ne 5 ]]; then
  printf 'hosted Cargo pin policy must contain exactly five repositories\n' >&2
  exit 1
fi
mapfile -t lock_pin_identities < <(
  awk -F '"' '/^source = "git\+/ {
    identity = $2
    sub(/^.*\?tag=/, "", identity)
    sub(/#/, "|", identity)
    print identity
  }' Cargo.lock | LC_ALL=C sort -u
)
mapfile -t expected_pin_identities < <(
  printf '%s\n' "${hosted_pins[@]}" | cut -d '|' -f 2,3 | LC_ALL=C sort -u
)
if [[ "$(printf '%s\n' "${lock_pin_identities[@]}")" != \
      "$(printf '%s\n' "${expected_pin_identities[@]}")" ]]; then
  printf 'Cargo.lock tag/commit inventory differs from hosted pin policy\n' >&2
  exit 1
fi

for pin in "${hosted_pins[@]}"; do
  IFS='|' read -r repo tag commit support_ref extra <<< "${pin}"
  if [[ -n "${extra:-}" || ! "${repo}" =~ ^jeryu-[a-z0-9-]+$ ||
        ! "${tag}" =~ ^[a-z0-9][a-z0-9._-]*$ ||
        ! "${commit}" =~ ^[0-9a-f]{40}$ ||
        "${support_ref}" != "refs/heads/preserve/hosted-cargo/${tag}" ]]; then
    printf 'malformed hosted Cargo pin policy row: %s\n' "${pin}" >&2
    exit 1
  fi
  remote="https://git.neverhuman.org/git/jeryu/${repo}.git"
  remote_refs="$(git ls-remote "${remote}" \
    "refs/tags/${tag}" "refs/tags/${tag}^{}" "${support_ref}")"
  tag_object="$(awk -v ref="refs/tags/${tag}" '$2 == ref { print $1 }' <<< "${remote_refs}")"
  tag_peeled="$(awk -v ref="refs/tags/${tag}^{}" '$2 == ref { print $1 }' <<< "${remote_refs}")"
  tag_target="${tag_peeled:-${tag_object}}"
  advertised_commit="$(awk -v ref="${support_ref}" '$2 == ref { print $1 }' <<< "${remote_refs}")"
  if [[ "${tag_target}" != "${commit}" || "${advertised_commit}" != "${commit}" ]]; then
    printf 'hosted Cargo pin mismatch: repo=%s tag_target=%s support_ref=%s expected=%s\n' \
      "${repo}" "${tag_target:-absent}" "${advertised_commit:-absent}" "${commit}" >&2
    exit 1
  fi
done

cargo deny check sources
cargo test --locked -p jeryu-api --features web --test hosted_dependency_transport -- \
  --test-threads=1
if [[ "$(sha256sum "${active_global}" | awk '{print $1}')" != "${active_global_sha256}" ]]; then
  printf 'active Git transport configuration changed during validation\n' >&2
  exit 1
fi
mkdir -p target/security
jq -n \
  --arg head "$(git rev-parse HEAD)" \
  --arg overlay_sha256 "$(sha256sum "${overlay}" | awk '{print $1}')" \
  --arg lock_sha256 "$(sha256sum Cargo.lock | awk '{print $1}')" \
  --arg deny_sha256 "$(sha256sum deny.toml | awk '{print $1}')" \
  --arg pin_policy_sha256 "$(sha256sum "${pin_policy}" | awk '{print $1}')" \
  --arg active_git_config_sha256 "${active_global_sha256}" \
  --arg credential_helper_sha256 "$(sha256sum "${credential_helper}" | awk '{print $1}')" \
  --argjson lock_sources "${#lock_sources[@]}" \
  --argjson hosted_mappings "${#expected_pairs[@]}" \
  --argjson hosted_pin_refs "${#hosted_pins[@]}" \
  '{schema_version:"jeryu.dependency-sources/v1",git_head:$head,
    overlay_sha256:$overlay_sha256,lock_sha256:$lock_sha256,
    deny_policy_sha256:$deny_sha256,pin_policy_sha256:$pin_policy_sha256,
    active_git_config_sha256:$active_git_config_sha256,
    credential_helper_sha256:$credential_helper_sha256,
    lock_sources:$lock_sources,hosted_mappings:$hosted_mappings,
    hosted_pin_refs:$hosted_pin_refs,unknown_git:"deny",conclusion:"success"}' \
  > target/security/dependency-sources.json
printf 'dependency sources ok: lock_sources=%d hosted_mappings=%d hosted_pin_refs=%d unknown_git=deny\n' \
  "${#lock_sources[@]}" "${#expected_pairs[@]}" "${#hosted_pins[@]}"
