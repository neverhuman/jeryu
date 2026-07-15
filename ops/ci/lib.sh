#!/usr/bin/env bash
set -euo pipefail

# BEGIN GENERATED JANKURAI PIN — DO NOT EDIT
export JERYU_GOVERNED_JANKURAI_BIN="${JERYU_JANKURAI_BIN:-/home/ubuntu/.jeryu/bin/jankurai}"
export JERYU_JANKURAI_VERSION="jankurai 1.6.11"
export JERYU_JANKURAI_SHA256="fdb42e5fa7d9851c0729e59bf1e582c895aa9cfc03a7175b420c6025d2fd014e"
export JERYU_JANKURAI_SOURCE_REV="dface7397fe24d46b0b1885ddd5782c34edbff49"
export JERYU_JANKURAI_SOURCE_TAG="v1.6.11-deadlang-precision-split.1"
export JERYU_JANKURAI_SOURCE_TREE="34a8a1fb59bc4ebfadf12c45d95f169d06acc781"
export JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256="2fbca5d04083e3c8d32f383d5b6b4520b8911690b26968c6fbcb210e1202b938"
export JERYU_JANKURAI_CARGO_LOCK_SHA256="b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a"
export JERYU_JANKURAI_RUST_TOOLCHAIN="1.95.0"
export JERYU_JANKURAI_TARGET_TRIPLE="x86_64-unknown-linux-gnu"
# END GENERATED JANKURAI PIN

require_tool() {
  local name="$1"
  command -v "$name" >/dev/null 2>&1 || {
    printf 'missing required tool: %s\n' "$name" >&2
    exit 1
  }
}

require_jankurai() {
  local bin="${JERYU_GOVERNED_JANKURAI_BIN}"
  local bin_dir normalized resolved actual actual_sha
  if [[ "${bin}" != /* || ! -f "${bin}" || -L "${bin}" || ! -x "${bin}" ]]; then
    printf 'governed jankurai must be an absolute executable regular file: %s\n' "${bin}" >&2
    exit 1
  fi
  normalized="$(realpath -m "${bin}")"
  if [[ "${normalized}" != "${bin}" ]]; then
    printf 'governed jankurai path traverses a symlink: %s -> %s\n' "${bin}" "${normalized}" >&2
    exit 1
  fi
  bin_dir="$(dirname "${bin}")"
  export PATH="${bin_dir}:${PATH}"
  resolved="$(command -v jankurai 2>/dev/null || true)"
  if [[ "${resolved}" != "${bin}" ]]; then
    printf 'governed jankurai shadowed: expected %s, resolved %s\n' "${bin}" "${resolved:-missing}" >&2
    exit 1
  fi
  actual="$("${bin}" --version 2>/dev/null || true)"
  actual_sha="$(sha256sum "${bin}" 2>/dev/null | awk '{print $1}')"
  if [[ "${actual}" != "${JERYU_JANKURAI_VERSION}" ]] ||
     [[ "${actual_sha}" != "${JERYU_JANKURAI_SHA256}" ]]; then
    printf 'governed jankurai identity mismatch at %s: version=%s sha256=%s\n' \
      "${bin}" "${actual:-missing}" "${actual_sha:-missing}" >&2
    exit 1
  fi
  export JANKURAI_NO_UPDATE_CHECK=1 GIT_TERMINAL_PROMPT=0
}
