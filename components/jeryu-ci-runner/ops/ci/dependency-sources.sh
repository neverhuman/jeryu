#!/usr/bin/env bash
# Validate Cargo source identity separately from effective Git transport.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

# Installed split transport remains below; public monorepo uses its real graph.
if [[ ${JERYU_MONOREPO_CANDIDATE:-0} != 0 ]]; then
  candidate_root=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git -C "$ROOT" rev-parse --show-toplevel)
  [[ $ROOT == "$candidate_root/components/jeryu-ci-runner" ]] || {
    printf 'public dependency proof requires the exact monorepo component\n' >&2; exit 1;
  }
  exec bash "$candidate_root/ops/ci/public-dependency-sources.sh" jeryu-ci-runner
fi
# shellcheck source=ops/ci/hosted-git-env.sh
source "${ROOT}/ops/ci/hosted-git-env.sh"

overlay="${ROOT}/.cargo/hosted-gitconfig"
pin_policy="${ROOT}/.cargo/hosted-pin-refs.tsv"
active_global="${GIT_CONFIG_GLOBAL}"
credential_helper="/home/ubuntu/.config/jeryu/bin/git-credential-neverhuman-org"
helper_binary_sha256_expected='c87ebbe3af617f34de169ec35e21e02afb3f9ee37f637bbcab075224c700ddb1'
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
  'https://github.com/neverhuman/jeryu-core.git|https://git.neverhuman.org/git/jeryu/jeryu-core.git'
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
if [[ "$(printf '%s\n' "${configured_pairs[@]}")" != \
      "$(printf '%s\n' "${expected_sorted[@]}")" ]]; then
  printf 'hosted Git overlay mapping inventory differs from the exact allowlist\n' >&2
  exit 1
fi

overlay_config_text="$(config_file "${overlay}" --list)" || {
  printf 'sealed hosted Git policy is not valid Git configuration\n' >&2
  exit 1
}
active_config_text="$(config_file "${active_global}" --list)" || {
  printf 'active Git transport input is not valid Git configuration\n' >&2
  exit 1
}
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
   [[ "$(config_file "${overlay}" --get-all credential.https://git.neverhuman.org.helper)" != \
      "${credential_helper}" ]]; then
  printf 'hosted Git overlay credential-helper policy differs from the exact default\n' >&2
  exit 1
fi
if [[ "$(config_file "${overlay}" --get http.https://git.neverhuman.org.postbuffer)" != 1 ]]; then
  printf 'hosted Git overlay is missing the exact smart-HTTP compatibility setting\n' >&2
  exit 1
fi
if [[ ! -f "${credential_helper}" || -L "${credential_helper}" ||
      ! -x "${credential_helper}" ||
      "$(stat -c '%u:%g:%a:%h' -- "${credential_helper}")" != \
        "$(id -u):$(id -g):700:1" ||
      "$(sha256sum "${credential_helper}" | awk '{print $1}')" != \
        "${helper_binary_sha256_expected}" ]]; then
  printf 'hosted credential helper must match the reviewed owner/mode/hash: %s\n' \
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
caller_global="$(GIT_CONFIG_GLOBAL="${overlay}" bash -c \
  'source "$1"; printf "%s" "$GIT_CONFIG_GLOBAL"' _ \
  "${ROOT}/ops/ci/hosted-git-env.sh")"
if [[ "${caller_global}" != "${overlay}" ]]; then
  printf 'CI Git environment did not canonicalize an exact caller policy\n' >&2
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

expected_lock_source='https://github.com/neverhuman/jeryu-core.git'
mapfile -t locked_git_rows < <(awk -F '"' '/^source = "git\+/ { print $2 }' Cargo.lock)
if [[ "${#locked_git_rows[@]}" -ne 2 ]]; then
  printf 'Cargo.lock must contain exactly two governed Git package rows\n' >&2
  exit 1
fi
for locked_source in "${locked_git_rows[@]}"; do
  if [[ ! "${locked_source}" =~ ^git\+${expected_lock_source//./\\.}\?tag=[A-Za-z0-9._-]+#[0-9a-f]{40}$ ]]; then
    printf 'Cargo.lock Git source is not the exact immutable source/tag/commit: %s\n' \
      "${locked_source}" >&2
    exit 1
  fi
done
mapfile -t lock_sources < <(
  printf '%s\n' "${locked_git_rows[@]}" | sed -e 's/^git+//' -e 's/?tag=.*$//' | LC_ALL=C sort -u
)
if [[ "${#lock_sources[@]}" -ne 1 || "${lock_sources[0]}" != "${expected_lock_source}" ]]; then
  printf 'Cargo.lock Git source inventory differs from deny/transport policy\n' >&2
  exit 1
fi

mapfile -t hosted_pins < <(awk '!/^[[:space:]]*(#|$)/ { print }' "${pin_policy}")
if [[ "${#hosted_pins[@]}" -ne 1 ]]; then
  printf 'hosted Cargo pin policy must contain exactly one repository\n' >&2
  exit 1
fi
pin="${hosted_pins[0]}"
IFS='|' read -r repo tag commit support_ref extra <<< "${pin}"
if [[ -n "${extra:-}" || "${repo}" != jeryu-core ||
      "${tag}" != jeryu-core-v5.0.0-split.0 ||
      "${commit}" != 0e29dc90673ffdf9959aaaa1f05482301f15fabe ||
      "${support_ref}" != "refs/heads/preserve/hosted-cargo/${tag}" ]]; then
  printf 'malformed or unexpected hosted Cargo pin policy row: %s\n' "${pin}" >&2
  exit 1
fi
for locked_source in "${locked_git_rows[@]}"; do
  if [[ "${locked_source}" != "git+${expected_lock_source}?tag=${tag}#${commit}" ]]; then
    printf 'Cargo.lock tag/commit differs from hosted pin policy\n' >&2
    exit 1
  fi
done

remote="https://git.neverhuman.org/git/jeryu/${repo}.git"
remote_refs="$(git ls-remote "${remote}" \
  "refs/tags/${tag}" "refs/tags/${tag}^{}" "${support_ref}")"
tag_object="$(awk -v ref="refs/tags/${tag}" '$2 == ref { print $1 }' <<< "${remote_refs}")"
tag_peeled="$(awk -v ref="refs/tags/${tag}^{}" '$2 == ref { print $1 }' <<< "${remote_refs}")"
tag_target="${tag_peeled:-${tag_object}}"
advertised_commit="$(awk -v ref="${support_ref}" '$2 == ref { print $1 }' <<< "${remote_refs}")"
if [[ "${tag_target}" != "${commit}" || "${advertised_commit}" != "${commit}" ]]; then
  printf 'hosted Cargo pin mismatch: tag_target=%s support_ref=%s expected=%s\n' \
    "${tag_target:-absent}" "${advertised_commit:-absent}" "${commit}" >&2
  exit 1
fi

cargo deny check sources
cargo test --locked -p jeryu-runnerd --test hosted_dependency_transport -- \
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
  '{schema_version:"jeryu.dependency-sources/v1",git_head:$head,
    overlay_sha256:$overlay_sha256,lock_sha256:$lock_sha256,
    deny_policy_sha256:$deny_sha256,pin_policy_sha256:$pin_policy_sha256,
    active_git_config_sha256:$active_git_config_sha256,
    credential_helper_sha256:$credential_helper_sha256,
    lock_sources:1,hosted_mappings:1,hosted_pin_refs:1,
    unknown_git:"deny",conclusion:"success"}' \
  > target/security/dependency-sources.json
printf 'dependency sources ok: lock_sources=1 hosted_mappings=1 hosted_pin_refs=1 unknown_git=deny\n'
