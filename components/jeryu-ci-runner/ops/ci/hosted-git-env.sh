#!/usr/bin/env bash
# Source-only Cargo Git transport environment. Cargo does not apply its own
# `[env]` table early enough to govern the Git fetch child, so host CI sources
# this helper before invoking Cargo.

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  printf 'hosted-git-env.sh must be sourced\n' >&2
  exit 2
fi

hosted_git_env_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
hosted_git_env_overlay="${hosted_git_env_root}/.cargo/hosted-gitconfig"
hosted_git_env_overlay_sha256='8a9e02724c1af6c2e3d2edd74e94bbdedd39ffe6a65afecb346893a73d582e45'
hosted_git_env_caller_global="${GIT_CONFIG_GLOBAL-}"

# Command-scoped configuration, tracing, askpass, and TLS overrides can bypass
# the reviewed URL/credential policy or disclose credentials. Scrub them before
# inspecting or exporting any Git configuration used by Cargo.
unset GIT_CONFIG GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS GIT_CONFIG_SYSTEM
unset GIT_ASKPASS SSH_ASKPASS GIT_SSH GIT_SSH_COMMAND GIT_PROXY_COMMAND
unset GIT_SSL_CAINFO GIT_SSL_CAPATH GIT_SSL_NO_VERIFY GIT_CURL_VERBOSE
unset GIT_EXEC_PATH GIT_TEMPLATE_DIR GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR
unset GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY NO_PROXY
unset http_proxy https_proxy all_proxy no_proxy
unset CURL_CA_BUNDLE SSL_CERT_FILE SSL_CERT_DIR
while IFS= read -r hosted_git_env_injected; do
  [[ -z "${hosted_git_env_injected}" ]] || unset "${hosted_git_env_injected}"
done < <(compgen -A variable GIT_CONFIG_KEY_ || true)
while IFS= read -r hosted_git_env_injected; do
  [[ -z "${hosted_git_env_injected}" ]] || unset "${hosted_git_env_injected}"
done < <(compgen -A variable GIT_CONFIG_VALUE_ || true)
while IFS= read -r hosted_git_env_injected; do
  [[ -z "${hosted_git_env_injected}" ]] || unset "${hosted_git_env_injected}"
done < <(compgen -A variable GIT_TRACE || true)

if [[ ! -f "${hosted_git_env_overlay}" || -L "${hosted_git_env_overlay}" ||
      "$(/usr/bin/stat -c '%h' -- "${hosted_git_env_overlay}")" != 1 ||
      "$(/usr/bin/sha256sum "${hosted_git_env_overlay}" | /usr/bin/awk '{print $1}')" != \
        "${hosted_git_env_overlay_sha256}" ]]; then
  printf 'hosted Git overlay is not the exact reviewed one-link policy: %s\n' \
    "${hosted_git_env_overlay}" >&2
  return 1
fi

if [[ -n "${hosted_git_env_caller_global}" ]]; then
  if [[ "${hosted_git_env_caller_global}" != /* ||
        ! -f "${hosted_git_env_caller_global}" ||
        -L "${hosted_git_env_caller_global}" ||
        "$(/usr/bin/stat -c '%h' -- "${hosted_git_env_caller_global}")" != 1 ]]; then
    printf 'caller GIT_CONFIG_GLOBAL must be an absolute one-link regular file\n' >&2
    return 1
  fi
  if ! /usr/bin/cmp --silent -- "${hosted_git_env_caller_global}" \
    "${hosted_git_env_overlay}"; then
    printf 'caller GIT_CONFIG_GLOBAL differs from the exact reviewed hosted policy\n' >&2
    return 1
  fi
fi
export GIT_CONFIG_GLOBAL="${hosted_git_env_overlay}"

export GIT_CONFIG_NOSYSTEM=1
export CARGO_NET_GIT_FETCH_WITH_CLI=true
export GIT_TERMINAL_PROMPT=0
unset hosted_git_env_root hosted_git_env_overlay hosted_git_env_overlay_sha256
unset hosted_git_env_caller_global hosted_git_env_injected
