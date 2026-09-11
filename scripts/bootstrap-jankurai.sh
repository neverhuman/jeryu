#!/usr/bin/env bash
# Source this file and call the function to retain the verified auditor descriptor.

bootstrap_public_jankurai() {
  local root expected source_status toolchain install_root output receipt
  root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
  [[ ${JAIN_RELEASE_CI:-0} != 1 ]] || {
    printf 'public auditor bootstrap cannot run as a protected release broker\n' >&2
    return 1
  }
  expected=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    /usr/bin/git -C "$root" rev-parse HEAD) || return 1
  source_status=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    /usr/bin/git -C "$root" status --porcelain=v1 --untracked-files=all) || return 1
  [[ $expected =~ ^[0-9a-f]{40}$ && -z $source_status ]] || {
    printf 'public auditor bootstrap requires an exact clean committed source\n' >&2
    return 1
  }
  # The renderer compiles and reads full locked metadata offline. Every direct
  # lane must prepare the root graph first, including non-host target packages.
  (cd "$root" && GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    cargo fetch --locked --manifest-path "$root/Cargo.toml") || return 1
  # The renderer still verifies exact source and generated drift before install.
  bash "$root/components/jeryu-tool/ops/render-monorepo-candidate.sh" \
    --monorepo-root "$root" --check --expected-head "$expected" >/dev/null || return 1
  install_root=${JERYU_AUDITOR_INSTALL_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/jeryu/auditor}
  if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 && ${JERYU_MONOREPO_EXPECTED_HEAD:-} == "$expected" &&
     ${JERYU_GOVERNED_JANKURAI_BIN:-} == "$install_root/bin/jankurai" &&
     ${JERYU_JANKURAI_RECEIPT:-} == "$install_root/receipts/jankurai/sha256/"* ]]; then
    # shellcheck source=/dev/null
    source "$root/components/jeryu-tool/ops/verify-public-candidate.sh"
    require_public_candidate_jankurai
    printf 'Existing public auditor candidate installation reverified for %s\n' "$expected"
    return
  fi
  if [[ -n ${JERYU_CANDIDATE_JANKURAI_DESCRIPTOR:-} ]]; then
    printf 'A different candidate is held by this process or its parent; use a fresh qualification process\n' >&2
    return 1
  fi
  # Public GitHub Actions cannot rebuild the hermetic OCI image ID. Install the
  # same SHA-pinned 1.6.11 GitHub Release binary that require_jankurai already
  # admits. Do not claim a public-candidate receipt or set candidate mode.
  if [[ ${GITHUB_ACTIONS:-} == true && ${JAIN_RELEASE_CI:-0} != 1 ]]; then
    bash "$root/ops/ci/install-jankurai-release.sh" || return 1
    export JERYU_MONOREPO_EXPECTED_HEAD="$expected"
    export JERYU_GOVERNED_JANKURAI_BIN=/usr/local/bin/jankurai
    export PATH="/usr/local/bin:${PATH}"
    export JANKURAI_NO_UPDATE_CHECK=1 GIT_TERMINAL_PROMPT=0
    unset JERYU_MONOREPO_CANDIDATE JERYU_JANKURAI_RECEIPT JERYU_CANDIDATE_JANKURAI_DESCRIPTOR
    printf 'Public GitHub Actions auditor uses pinned GitHub Release binary for %s\n' "$expected"
    return
  fi
  toolchain=$(sed -n 's/^JANKURAI_RUST_TOOLCHAIN="\([0-9][0-9.]*\)"$/\1/p' \
    "$root/components/jeryu-tool/generated/jankurai-pin.env")
  [[ $toolchain =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || return 1
  if ! rustup run "$toolchain" cargo --version >/dev/null 2>&1; then
    rustup toolchain install "$toolchain" --profile minimal --no-self-update || return 1
  fi
  output=$(env -u JANKURAI_NO_UPDATE_CHECK -u GIT_TERMINAL_PROMPT JERYU_INSTALL_ROOT="$install_root" \
    bash "$root/components/jeryu-tool/ops/install-jankurai.sh" \
    --public-candidate --expected-head "$expected") || return 1
  receipt=$(printf '%s\n' "$output" | tail -n 1 | jq -er '.receipt | select(type == "string")') || return 1
  [[ $receipt == "$install_root/receipts/jankurai/sha256/"* && $receipt != *$'\n'* ]] || {
    printf 'public auditor installer did not return its receipt path\n' >&2
    return 1
  }
  export JERYU_MONOREPO_CANDIDATE=1 JERYU_MONOREPO_EXPECTED_HEAD="$expected"
  export JERYU_GOVERNED_JANKURAI_BIN="$install_root/bin/jankurai" JERYU_JANKURAI_RECEIPT="$receipt"
  # shellcheck source=/dev/null
  source "$root/components/jeryu-tool/ops/verify-public-candidate.sh"
  require_public_candidate_jankurai
  printf 'Public auditor build and candidate installation verified for %s\n' "$expected"
}
