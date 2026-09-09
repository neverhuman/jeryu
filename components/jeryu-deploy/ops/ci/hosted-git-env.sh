#!/usr/bin/env bash
# Source-only Cargo Git transport environment. Cargo's `[env]` configuration is
# not applied to Cargo's own fetch child, so CI entrypoints source this before
# invoking Cargo. A caller-supplied global config takes priority only when the
# source gate proves it is semantically identical to the sealed hosted policy.

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  printf 'hosted-git-env.sh must be sourced\n' >&2
  exit 2
fi

hosted_git_env_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
hosted_git_env_overlay="${hosted_git_env_root}/.cargo/hosted-gitconfig"

if [[ -v GIT_CONFIG_GLOBAL ]]; then
  if [[ "${GIT_CONFIG_GLOBAL}" != /* || ! -f "${GIT_CONFIG_GLOBAL}" ||
        -L "${GIT_CONFIG_GLOBAL}" ||
        "$(stat -c '%h' -- "${GIT_CONFIG_GLOBAL}")" != 1 ]]; then
    printf 'caller GIT_CONFIG_GLOBAL must be an absolute one-link regular file\n' >&2
    return 1
  fi
else
  if [[ ! -f "${hosted_git_env_overlay}" || -L "${hosted_git_env_overlay}" ||
        "$(stat -c '%h' -- "${hosted_git_env_overlay}")" != 1 ]]; then
    printf 'hosted Git overlay must be a one-link regular file: %s\n' \
      "${hosted_git_env_overlay}" >&2
    return 1
  fi
  export GIT_CONFIG_GLOBAL="${hosted_git_env_overlay}"
fi

# Cargo's Git child must not inherit command-scoped config injections that the
# checked-in or caller-supplied global config cannot attest.
unset GIT_CONFIG GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS GIT_CONFIG_SYSTEM
while IFS= read -r hosted_git_env_injected; do
  [[ -z "${hosted_git_env_injected}" ]] || unset "${hosted_git_env_injected}"
done < <(compgen -A variable GIT_CONFIG_KEY_ || true)
while IFS= read -r hosted_git_env_injected; do
  [[ -z "${hosted_git_env_injected}" ]] || unset "${hosted_git_env_injected}"
done < <(compgen -A variable GIT_CONFIG_VALUE_ || true)
export GIT_CONFIG_NOSYSTEM=1
export CARGO_NET_GIT_FETCH_WITH_CLI="${CARGO_NET_GIT_FETCH_WITH_CLI:-true}"
export GIT_TERMINAL_PROMPT="${GIT_TERMINAL_PROMPT:-0}"
unset hosted_git_env_root hosted_git_env_overlay hosted_git_env_injected
