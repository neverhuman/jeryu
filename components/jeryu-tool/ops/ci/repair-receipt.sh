#!/usr/bin/env bash
set -euo pipefail

REPAIR_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly REPAIR_ROOT

repair_fail() {
  printf 'repair receipt: %s\n' "$*" >&2
  # Bash suppresses errexit inside a sourced function invoked as an `if`
  # condition. Exit the verifier/emitter subshell so rejection stays closed.
  exit 1
}

repair_usage() {
  printf '%s\n' \
    'usage:' \
    '  repair-receipt.sh emit --output target/repair-receipts/<run-id>.json \' \
    '    --lane <lane> --purpose <single-line-purpose> --exit-code <0..255> \' \
    '    --started-at <UTC-second> --finished-at <UTC-second> \' \
    '    --evidence <target/path> [--evidence <target/path> ...]' \
    '  repair-receipt.sh verify target/repair-receipts/<run-id>.json' >&2
  exit 2
}

repair_lane_argv() {
  case "${1:-}" in
    fast) printf '["just","fast"]\n' ;;
    fast-proof) printf '["just","fast-proof"]\n' ;;
    fast-test) printf '["just","fast-test"]\n' ;;
    fast-coverage) printf '["just","fast-coverage"]\n' ;;
    check) printf '["just","check"]\n' ;;
    required) printf '["just","required"]\n' ;;
    score) printf '["just","score"]\n' ;;
    security) printf '["just","security"]\n' ;;
    tool-adoption) printf '["just","tool-adoption"]\n' ;;
    proof-routing) printf '["just","proof-routing"]\n' ;;
    contract-drift) printf '["just","contract-drift"]\n' ;;
    artifact-support) printf '["just","artifact-support"]\n' ;;
    repair-proof) printf '["just","repair-proof"]\n' ;;
    repair-receipt-contract) printf '["just","repair-receipt-contract"]\n' ;;
    *) return 1 ;;
  esac
}

repair_valid_utc_second() {
  local value="${1:-}" normalized
  [[ "$value" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] \
    || return 1
  normalized="$(date -u -d "$value" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null)" \
    || return 1
  [[ "$normalized" == "$value" ]]
}

repair_valid_output() {
  [[ "${1:-}" =~ ^target/repair-receipts/[a-z0-9][a-z0-9._-]{0,95}\.json$ ]]
}

repair_valid_evidence() {
  local relative="${1:-}" component
  local -a parts
  [[ "$relative" != /* && "$relative" == target/* && "$relative" != */ ]] \
    || return 1
  IFS=/ read -r -a parts <<<"$relative"
  ((${#parts[@]} >= 2)) || return 1
  for component in "${parts[@]}"; do
    [[ "$component" =~ ^[A-Za-z0-9._-]+$ && "$component" != . && "$component" != .. ]] \
      || return 1
  done
}

repair_ensure_directory() {
  local path="$1" denied_write_mask="${2:-022}" mode
  if [[ -L "$path" || ( -e "$path" && ! -d "$path" ) ]]; then
    repair_fail "unsafe directory: ${path#"$REPAIR_ROOT"/}"
  fi
  if [[ ! -e "$path" ]]; then
    mkdir -m 0750 -- "$path" || return 1
  fi
  [[ ! -L "$path" && -d "$path" && "$(realpath -e -- "$path")" == "$path" ]] \
    || return 1
  mode="$(stat -c '%a' -- "$path")" || return 1
  [[ "$(stat -c '%u' -- "$path")" == "$EUID" ]] || return 1
  (( (8#$mode & 8#$denied_write_mask) == 0 )) \
    || repair_fail "writable directory: ${path#"$REPAIR_ROOT"/}"
}

repair_require_clean_head() {
  [[ "$(realpath -e -- "$REPAIR_ROOT")" == "$REPAIR_ROOT" && ! -L "$REPAIR_ROOT" ]] \
    || repair_fail "repository root is not physical"
  [[ -z "$(git -C "$REPAIR_ROOT" status --porcelain=v1)" ]] \
    || repair_fail "repository worktree is dirty"
}

repair_file_measurement() (
  local relative="$1" absolute fd fd_path before after sha bytes
  repair_valid_evidence "$relative" || repair_fail "unsafe evidence path: $relative"
  absolute="$REPAIR_ROOT/$relative"
  [[ -f "$absolute" && ! -L "$absolute" && "$(realpath -e -- "$absolute")" == "$absolute" ]] \
    || repair_fail "evidence is not a physical regular file: $relative"
  exec {fd}<"$absolute"
  fd_path="/proc/$BASHPID/fd/$fd"
  before="$(stat -Lc '%d:%i:%u:%h:%s' -- "$fd_path")" || return 1
  after="$(stat -Lc '%d:%i:%u:%h:%s' -- "$absolute")" || return 1
  [[ "$before" == "$after" && "$before" =~ ^[^:]+:[^:]+:$EUID:1: ]] \
    || repair_fail "evidence identity or custody is unsafe: $relative"
  sha="$(sha256sum -- "$fd_path")"
  sha="${sha%% *}"
  bytes="${before##*:}"
  [[ "$(stat -Lc '%d:%i:%u:%h:%s' -- "$absolute")" == "$before" ]] \
    || repair_fail "evidence changed while hashing: $relative"
  jq -cn --arg path "$relative" --arg sha "$sha" --argjson bytes "$bytes" \
    '{path:$path,sha256:$sha,bytes:$bytes}'
)

# The hostile contract overrides this seam to replace a selected pathname
# after the verifier has acquired its read descriptor.
repair_receipt_after_open() {
  :
}

repair_validate_payload() {
  local input="$1" head="$2" tree="$3" lane="$4" expected_argv="$5"
  jq -s -e --arg head "$head" --arg tree "$tree" --arg lane "$lane" \
    --argjson argv "$expected_argv" '
      length == 1 and .[0] as $r |
      ($r | keys) == [
        "correlation_id", "dirty_worktree", "evidence", "exit_code", "finished_at",
        "git_head", "git_tree", "lane", "purpose", "repo", "rerun", "root",
        "run_id", "schema_version", "started_at", "status"
      ] and
      $r.schema_version == "jeryu.tool.repair-receipt/v1" and
      $r.repo == "jeryu/jeryu-tool" and $r.root == "." and
      $r.git_head == $head and $r.git_tree == $tree and
      $r.dirty_worktree == false and $r.lane == $lane and
      ($r.run_id | type == "string" and test("^[a-z0-9][a-z0-9._-]{0,95}$")) and
      $r.correlation_id == $r.run_id and
      ($r.purpose | type == "string" and length >= 1 and length <= 256 and
        (test("[\\r\\n]") | not)) and
      ($r.exit_code | type == "number" and floor == . and . >= 0 and . <= 255) and
      $r.status == (if $r.exit_code == 0 then "pass" else "fail" end) and
      ($r.rerun | keys) == ["argv", "cwd", "docs_url"] and
      $r.rerun.cwd == "." and $r.rerun.argv == $argv and
      $r.rerun.docs_url == "docs/testing.md#repair-receipts" and
      ($r.evidence | type == "array" and length >= 1 and sort_by(.path) == .) and
      ([ $r.evidence[].path ] | unique | length) == ($r.evidence | length) and
      all($r.evidence[];
        (keys == ["bytes", "path", "sha256"]) and
        (.path | type == "string") and
        (.sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
        (.bytes | type == "number" and floor == . and . >= 0))
    ' "$input" >/dev/null
}

repair_verify_receipt() (
  local relative="$1" absolute fd fd_path before after head tree lane expected_argv
  local started finished started_epoch finished_epoch path sha bytes measured receipt_bytes
  repair_valid_output "$relative" || repair_fail "unsafe receipt path: $relative"
  repair_require_clean_head
  absolute="$REPAIR_ROOT/$relative"
  [[ -f "$absolute" && ! -L "$absolute" && "$(realpath -e -- "$absolute")" == "$absolute" ]] \
    || repair_fail "receipt is not a physical regular file: $relative"
  exec {fd}<"$absolute"
  fd_path="/proc/$BASHPID/fd/$fd"
  before="$(stat -Lc '%d:%i:%u:%a:%h' -- "$fd_path")" || return 1
  after="$(stat -Lc '%d:%i:%u:%a:%h' -- "$absolute")" || return 1
  [[ "$before" == "$after" && "$before" =~ ^[^:]+:[^:]+:$EUID:444:1$ ]] \
    || repair_fail "receipt identity, mode, owner, or link count is unsafe: $relative"
  receipt_bytes="$(stat -Lc '%s' -- "$fd_path")" || return 1
  ((receipt_bytes >= 1 && receipt_bytes <= 262144)) \
    || repair_fail "receipt exceeds its 256-KiB parse bound: $relative"
  repair_receipt_after_open "$absolute"
  [[ ! -L "$absolute" && -f "$absolute" ]] || return 1
  [[ "$(stat -Lc '%d:%i:%u:%a:%h' -- "$absolute")" == "$before" ]] \
    || repair_fail "receipt path changed after selection: $relative"
  cmp -s -- "$fd_path" <(jq -cS . "$fd_path") \
    || repair_fail "receipt is malformed or not canonical JSON: $relative"
  head="$(git -C "$REPAIR_ROOT" rev-parse HEAD)"
  tree="$(git -C "$REPAIR_ROOT" rev-parse "HEAD^{tree}")"
  lane="$(jq -r '.lane' "$fd_path")"
  expected_argv="$(repair_lane_argv "$lane")" \
    || repair_fail "receipt lane is not admitted: $lane"
  repair_validate_payload "$fd_path" "$head" "$tree" "$lane" "$expected_argv" \
    || repair_fail "receipt schema or source binding is invalid: $relative"
  read -r started finished < <(jq -r '[.started_at,.finished_at]|@tsv' "$fd_path")
  if ! repair_valid_utc_second "$started" \
    || ! repair_valid_utc_second "$finished"; then
    repair_fail "receipt timestamps are invalid"
  fi
  started_epoch="$(date -u -d "$started" +%s)"
  finished_epoch="$(date -u -d "$finished" +%s)"
  ((finished_epoch >= started_epoch)) || repair_fail "receipt time runs backwards"
  while IFS=$'\t' read -r path sha bytes; do
    measured="$(repair_file_measurement "$path")" || return 1
    jq -e --arg path "$path" --arg sha "$sha" --argjson bytes "$bytes" \
      '.path==$path and .sha256==$sha and .bytes==$bytes' <<<"$measured" >/dev/null \
      || repair_fail "evidence bytes changed: $path"
  done < <(jq -r '.evidence[]|[.path,.sha256,(.bytes|tostring)]|@tsv' "$fd_path")
  [[ "$(stat -Lc '%d:%i:%u:%a:%h' -- "$absolute")" == "$before" ]] \
    || repair_fail "receipt changed during verification: $relative"
  printf 'repair receipt ok: %s\n' "$relative"
)

repair_emit_receipt() (
  local output="" lane="" purpose="" exit_code="" started="" finished=""
  local tmp="" parent head tree run_id status expected_argv evidence_json
  local -a evidence=() objects=()
  umask 077
  while (($#)); do
    case "$1" in
      --output) output="${2:-}"; shift 2 ;;
      --lane) lane="${2:-}"; shift 2 ;;
      --purpose) purpose="${2:-}"; shift 2 ;;
      --exit-code) exit_code="${2:-}"; shift 2 ;;
      --started-at) started="${2:-}"; shift 2 ;;
      --finished-at) finished="${2:-}"; shift 2 ;;
      --evidence) evidence+=("${2:-}"); shift 2 ;;
      *) repair_usage ;;
    esac
  done
  repair_valid_output "$output" || repair_fail "unsafe receipt path: $output"
  expected_argv="$(repair_lane_argv "$lane")" || repair_fail "lane is not admitted: $lane"
  [[ -n "$purpose" && ${#purpose} -le 256 && "$purpose" != *$'\r'* \
    && "$purpose" != *$'\n'* ]] || repair_fail "purpose must be one bounded line"
  if [[ ! "$exit_code" =~ ^[0-9]+$ ]] || ((exit_code > 255)); then
    repair_fail "exit code must be in 0..255"
  fi
  if ! repair_valid_utc_second "$started" \
    || ! repair_valid_utc_second "$finished"; then
    repair_fail "timestamps must be exact UTC seconds"
  fi
  (($(date -u -d "$finished" +%s) >= $(date -u -d "$started" +%s))) \
    || repair_fail "finished-at precedes started-at"
  ((${#evidence[@]} >= 1)) || repair_fail "at least one evidence file is required"
  repair_require_clean_head
  # The shared Cargo target may be group-writable. The private receipt
  # directory remains owner-controlled and denies group/world writes.
  repair_ensure_directory "$REPAIR_ROOT/target" 002
  repair_ensure_directory "$REPAIR_ROOT/target/repair-receipts"
  for path in "${evidence[@]}"; do
    objects+=("$(repair_file_measurement "$path")") || return 1
  done
  evidence_json="$(printf '%s\n' "${objects[@]}" | jq -cs 'sort_by(.path)')"
  jq -e '([.[].path]|unique|length)==length' <<<"$evidence_json" >/dev/null \
    || repair_fail "evidence paths are duplicated"
  head="$(git -C "$REPAIR_ROOT" rev-parse HEAD)"
  tree="$(git -C "$REPAIR_ROOT" rev-parse "HEAD^{tree}")"
  run_id="${output##*/}"
  run_id="${run_id%.json}"
  status=fail
  ((exit_code == 0)) && status=pass
  parent="$REPAIR_ROOT/target/repair-receipts"
  tmp="$(mktemp -p "$parent" ".repair-receipt.$BASHPID.XXXXXXXX")"
  # shellcheck disable=SC2317
  repair_emit_cleanup() {
    [[ -z "$tmp" ]] || rm -f -- "$tmp"
  }
  trap repair_emit_cleanup EXIT
  trap 'repair_emit_cleanup; exit 129' HUP
  trap 'repair_emit_cleanup; exit 130' INT
  trap 'repair_emit_cleanup; exit 143' TERM
  jq -cnS --arg run_id "$run_id" --arg head "$head" --arg tree "$tree" \
    --arg lane "$lane" --arg purpose "$purpose" --arg status "$status" \
    --arg started "$started" --arg finished "$finished" \
    --argjson exit_code "$exit_code" --argjson argv "$expected_argv" \
    --argjson evidence "$evidence_json" \
    '{schema_version:"jeryu.tool.repair-receipt/v1",repo:"jeryu/jeryu-tool",root:".",
      run_id:$run_id,correlation_id:$run_id,git_head:$head,git_tree:$tree,
      dirty_worktree:false,lane:$lane,purpose:$purpose,status:$status,
      exit_code:$exit_code,started_at:$started,finished_at:$finished,
      rerun:{cwd:".",argv:$argv,docs_url:"docs/testing.md#repair-receipts"},
      evidence:$evidence}' >"$tmp"
  chmod 0444 -- "$tmp"
  [[ ! -e "$REPAIR_ROOT/$output" && ! -L "$REPAIR_ROOT/$output" ]] \
    || repair_fail "receipt already exists: $output"
  ln -T -- "$tmp" "$REPAIR_ROOT/$output" \
    || repair_fail "create-once publication lost a race: $output"
  rm -f -- "$tmp"
  tmp=""
  sync -f "$REPAIR_ROOT/$output"
  sync -f "$parent"
  repair_verify_receipt "$output" >/dev/null
  printf '%s\n' "$output"
)

repair_main() {
  case "${1:-}" in
    emit) shift; repair_emit_receipt "$@" ;;
    verify) (($# == 2)) || repair_usage; repair_verify_receipt "$2" ;;
    *) repair_usage ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  repair_main "$@"
fi
