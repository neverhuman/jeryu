#!/usr/bin/env bash
# Install or verify the pinned Jankurai binary used by local and hosted gates.
set -euo pipefail

JANKURAI_REPO="${JANKURAI_REPO:-https://github.com/neverhuman/jankurai.git}"
JANKURAI_TAG="${JANKURAI_TAG:-v1.6.10-deadlang-precision}"
JANKURAI_REV="${JANKURAI_REV:-68bd6114373cf407a930011b76669af306cb0cb1}"
JANKURAI_VERSION="${JANKURAI_VERSION:-jankurai 1.6.10}"
JERYU_JANKURAI_CLEAN_GIT_CONFIG="${JERYU_JANKURAI_CLEAN_GIT_CONFIG:-1}"

jankurai_git() {
  if [ "${JERYU_JANKURAI_CLEAN_GIT_CONFIG}" = "1" ]; then
    GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git "$@"
  else
    git "$@"
  fi
}

jankurai_cargo_install() {
  if [ "${JERYU_JANKURAI_CLEAN_GIT_CONFIG}" = "1" ]; then
    GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 cargo install "$@"
  else
    cargo install "$@"
  fi
}

verify_pinned_rev_fetchable() {
  local tmp
  tmp="$(mktemp -d)"
  if ! (
    jankurai_git -C "${tmp}" init -q &&
      jankurai_git -C "${tmp}" remote add origin "${JANKURAI_REPO}" &&
      jankurai_git -C "${tmp}" fetch --depth 1 origin "${JANKURAI_REV}" >/dev/null 2>&1 &&
      [ "$(jankurai_git -C "${tmp}" rev-parse FETCH_HEAD)" = "${JANKURAI_REV}" ]
  ); then
    rm -rf "${tmp}"
    echo "jankurai pinned rev unavailable: ${JANKURAI_REV}" >&2
    exit 1
  fi
  rm -rf "${tmp}"
  echo "jankurai pinned rev ok: ${JANKURAI_REV}"
}

resolve_tag_rev() {
  local refs
  refs="$(jankurai_git ls-remote --tags "${JANKURAI_REPO}" \
    "refs/tags/${JANKURAI_TAG}" "refs/tags/${JANKURAI_TAG}^{}")"
  printf '%s\n' "${refs}" | awk -v direct="refs/tags/${JANKURAI_TAG}" -v peeled="refs/tags/${JANKURAI_TAG}^{}" '
    $2 == peeled {
      print $1
      found = 1
      exit
    }
    $2 == direct {
      direct_rev = $1
    }
    END {
      if (!found && direct_rev != "") {
        print direct_rev
      }
    }
  '
}

strict_tag="${JERYU_JANKURAI_STRICT_TAG:-}"
if [ -z "${strict_tag}" ] && [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  strict_tag=1
fi

if [ "${strict_tag:-0}" = "1" ]; then
  raw_tag_rev="$(resolve_tag_rev)"
  tag_rev="$(printf '%s\n' "${raw_tag_rev}" | tr -d '\r' | grep -Eo '^[0-9a-fA-F]{40}$' | head -n 1 || true)"
  if [ -z "${tag_rev}" ]; then
    echo "jankurai tag not published: ${JANKURAI_TAG}; verifying pinned rev ${JANKURAI_REV}"
    verify_pinned_rev_fetchable
  elif [ "${tag_rev}" != "${JANKURAI_REV}" ]; then
    echo "jankurai tag differs: got ${tag_rev} want ${JANKURAI_REV}; verifying pinned rev" >&2
    verify_pinned_rev_fetchable
  else
    echo "jankurai tag ok: ${JANKURAI_TAG} -> ${JANKURAI_REV}"
  fi
fi

PINNED_BIN="${CARGO_HOME:-$HOME/.cargo}/bin/jankurai"

# A stale jankurai earlier on PATH (e.g. a pooled cargo-home, ~/.local/bin) can
# shadow the pinned ~/.cargo/bin build and make the gate silently audit with the
# wrong version. Overwrite whatever `jankurai` resolves to with the pinned binary
# whenever the resolved path is writable, so the gate never runs a wrong version.
force_resolved_to_pin() {
  local resolved
  resolved="$(command -v jankurai 2>/dev/null || true)"
  if [ -x "${PINNED_BIN}" ] && [ -n "${resolved}" ] &&
    [ "${resolved}" != "${PINNED_BIN}" ] && [ -w "${resolved}" ]; then
    install -m755 "${PINNED_BIN}" "${resolved}" 2>/dev/null || true
  fi
}

if [ -x "${PINNED_BIN}" ] && "${PINNED_BIN}" --version | grep -qx "${JANKURAI_VERSION}"; then
  force_resolved_to_pin
fi
if command -v jankurai >/dev/null 2>&1 && jankurai --version | grep -qx "${JANKURAI_VERSION}"; then
  echo "jankurai ok: ${JANKURAI_VERSION}"
  exit 0
fi

install_profile_args=()
if [ "${JERYU_JANKURAI_INSTALL_DEBUG:-}" = "1" ] ||
  { [ -z "${JERYU_JANKURAI_INSTALL_DEBUG:-}" ] && [ "${GITHUB_ACTIONS:-}" = "true" ]; }; then
  install_profile_args+=(--debug)
  echo "jankurai install profile: debug"
fi

echo "installing pinned ${JANKURAI_VERSION} from ${JANKURAI_REPO}@${JANKURAI_REV}"
jankurai_cargo_install --git "${JANKURAI_REPO}" --rev "${JANKURAI_REV}" --locked "${install_profile_args[@]}" --bin jankurai jankurai
force_resolved_to_pin
jankurai --version | grep -qx "${JANKURAI_VERSION}"
echo "jankurai ok: ${JANKURAI_VERSION}"
