#!/usr/bin/env bash
# Verify CI is using this repo's Jeryu surfaces, not retired local state.
set -euo pipefail

build_local=0
release_guard=0
for arg in "$@"; do
  case "${arg}" in
    --build-local) build_local=1 ;;
    --release-guard) release_guard=1 ;;
    *) echo "unknown argument: ${arg}" >&2; exit 2 ;;
  esac
done

ROOT="$(git rev-parse --show-toplevel)"
cd "${ROOT}"

if [ "${GITHUB_ACTIONS:-}" != "true" ]; then
  expected="${JERYU_CANONICAL_ROOT:-/home/ubuntu/jeryu}"
  if [ -d "${expected}" ]; then
    actual_real="$(realpath "${ROOT}")"
    expected_real="$(realpath "${expected}")"
    if [ "${actual_real}" != "${expected_real}" ]; then
      echo "wrong Jeryu root: got ${actual_real}, want ${expected_real}" >&2
      exit 1
    fi
  fi
  case "$(realpath "${ROOT}")" in
    */jeryu_rust) echo "wrong Jeryu root: /home/ubuntu/jeryu_rust is not canonical" >&2; exit 1 ;;
  esac
fi

remote="$(git remote get-url origin 2>/dev/null || true)"
case "${remote}" in
  ""|git@github.com:neverhuman/jeryu.git|https://github.com/neverhuman/jeryu|https://github.com/neverhuman/jeryu.git)
    ;;
  *)
    echo "noncanonical origin remote: ${remote}" >&2
    exit 1
    ;;
esac

retired_path=""
if path_jeryu="$(command -v jeryu 2>/dev/null)"; then
  case "${path_jeryu}" in
    "${HOME}/.jeryu/"*) retired_path="${path_jeryu}" ;;
  esac
fi

if [ "${build_local}" = "1" ]; then
  cargo build -q -p jeryu-cli --bin jeryu --jobs "${JERYU_CI_JOBS:-40}"
fi

repo_bin=""
for candidate in "${ROOT}/target/debug/jeryu" "${ROOT}/target/release/jeryu"; do
  if [ -x "${candidate}" ]; then
    repo_bin="${candidate}"
    break
  fi
done

if [ -z "${repo_bin}" ]; then
  echo "repo-built jeryu binary not found; run cargo build -p jeryu-cli --bin jeryu" >&2
  exit 1
fi

version="$("${repo_bin}" --version)"
case "${version}" in
  jeryu\ *) ;;
  *) echo "unexpected repo jeryu version output: ${version}" >&2; exit 1 ;;
esac

echo "jeryu repo binary ok: ${repo_bin} (${version})"
if [ -n "${retired_path}" ]; then
  echo "retired PATH jeryu ignored: ${retired_path}"
fi
if [ -n "${remote}" ]; then
  echo "origin remote ok: ${remote}"
fi

if [ "${release_guard}" = "1" ]; then
  summary="${JERYU_LOCAL_STATE_VERIFY_SUMMARY:-target/ci-fast/local-state-verify.json}"
  bash ops/ci/local-state.sh --verify --summary "${summary}" || exit 1
  echo "release local-state guard ok"
fi
