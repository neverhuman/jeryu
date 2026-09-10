#!/usr/bin/env bash
# Sourced only by the legacy premerge candidate branch of pr-ci.sh.
# Retention supplies no release authority or proof that child processes stopped.

# Reuse the maintained owner/mode/physical-directory checks. Sourcing this file
# does not invoke the repair emitter or borrow its protected-head authority.
# shellcheck source=ops/ci/repair-receipt.sh
source "$(dirname "${BASH_SOURCE[0]}")/repair-receipt.sh"

premerge_finish() {
  local status=$?
  trap - EXIT HUP INT TERM
  printf '[pr-ci] premerge state reserved for separate verified retirement: candidate=%s evidence=%s exit=%s\n' \
    "${candidate_root:-not-allocated}" "${evidence_dir:-not-allocated}" "${status}" >&2 || true
  exit "${status}"
}

premerge_begin() {
  local root="$1" ancestor owner mode
  [[ "$root" == /* && ! "$root" =~ [[:cntrl:]] &&
     ! -L "$root" && -d "$root" && "$(realpath -e -- "$root")" == "$root" ]] || {
    printf 'premerge requires a physical repository root without path aliases\n' >&2
    return 1
  }
  # A private attempt is unsafe if another user can replace an ancestor. The
  # fixed /tmp parent is the sole sticky-directory exception, as for mktemp.
  ancestor="$root"
  while :; do
    [[ -d "$ancestor" && ! -L "$ancestor" &&
       "$(realpath -e -- "$ancestor")" == "$ancestor" ]] || return 1
    owner="$(stat -c %u -- "$ancestor")" || return 1
    mode="$(stat -c %a -- "$ancestor")" || return 1
    [[ "$owner" == 0 || "$owner" == "$EUID" ]] || return 1
    if [[ "$ancestor" != /tmp || "$owner" != 0 || "$mode" != 1777 ]]; then
      (( (8#$mode & 8#022) == 0 )) || {
        printf 'premerge ancestor is writable by another user: %s\n' "$ancestor" >&2
        return 1
      }
    fi
    [[ "$ancestor" != / ]] || break
    ancestor="$(dirname -- "$ancestor")"
  done
  repair_ensure_directory "$root" || return 1
  repair_ensure_directory "$root/target" || return 1
  repair_ensure_directory "$root/target/jankurai" || return 1
  [[ -d /tmp && ! -L /tmp && "$(realpath -e /tmp)" == /tmp &&
     "$(stat -c %u /tmp)" == 0 && "$(stat -c %a /tmp)" == 1777 ]] || return 1

  umask 077
  evidence_dir="$(mktemp -d "$root/target/jankurai/premerge-candidate.XXXXXXXX")" || return "$?"
  candidate_root=""
  # Even a subsequent allocation failure keeps the first fresh evidence root.
  # Signals change only the exit status; they never delete possibly live state.
  trap premerge_finish EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  candidate_root="$(mktemp -d /tmp/jeryu-tool-premerge-candidate.XXXXXXXX)" || return "$?"
  repair_ensure_directory "$evidence_dir" 077 || return 1
  repair_ensure_directory "$candidate_root" 077 || return 1
  printf 'candidate_root=%s\nevidence_dir=%s\n' "$candidate_root" "$evidence_dir" \
    >"$evidence_dir/attempt-paths.txt"
  printf '[pr-ci] private premerge attempt: candidate=%s evidence=%s\n' \
    "$candidate_root" "$evidence_dir" >&2
}
