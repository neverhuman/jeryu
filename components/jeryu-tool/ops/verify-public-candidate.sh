#!/usr/bin/env bash
# Sourced only by explicit monorepo candidate CI. This never grants release authority.

jeryu_candidate_die() {
  printf 'public-candidate-jankurai: %s\n' "$*" >&2
  exit 1
}

jeryu_candidate_parent_custody() {
  local parent="$1" owner mode
  while :; do
    [[ -d "$parent" && ! -L "$parent" && $(realpath -e -- "$parent") == "$parent" ]] ||
      jeryu_candidate_die "nonphysical input parent"
    owner=$(stat -c %u -- "$parent") || jeryu_candidate_die "unreadable input parent"
    mode=$(stat -c %a -- "$parent") || jeryu_candidate_die "unreadable input parent mode"
    [[ $owner == 0 || $owner == "$(id -u)" ]] || jeryu_candidate_die "foreign input parent"
    if (( (8#$mode & 0022) != 0 )); then
      [[ $owner == 0 ]] && (( (8#$mode & 01000) != 0 )) ||
        jeryu_candidate_die "writable input parent"
    fi
    [[ $parent != / ]] || break
    parent=$(dirname -- "$parent")
  done
}

jeryu_candidate_open_file() {
  local path="$1" identity fd
  [[ $path == /* && -f $path && ! -L $path && $(realpath -e -- "$path") == "$path" ]] ||
    jeryu_candidate_die "input must be a physical absolute regular file"
  jeryu_candidate_parent_custody "$(dirname -- "$path")"
  [[ $(stat -c '%u:%h' -- "$path") == "$(id -u):1" ]] ||
    jeryu_candidate_die "input must be owner-held and single-link"
  (( (8#$(stat -c %a -- "$path") & 0022) == 0 )) ||
    jeryu_candidate_die "input is writable by another identity"
  identity=$(stat -c '%d:%i:%u:%g:%a:%h' -- "$path")
  exec {fd}<"$path" || jeryu_candidate_die "cannot retain input descriptor"
  [[ $(stat -Lc '%d:%i:%u:%g:%a:%h' -- "/proc/$BASHPID/fd/$fd") == "$identity" &&
     $(realpath -e -- "/proc/$BASHPID/fd/$fd") == "$path" ]] ||
    jeryu_candidate_die "input changed during descriptor selection"
  printf -v "$2" '%s' "$fd"
  JERYU_CANDIDATE_HELD_FDS+=("$fd")
  candidate_paths+=("$path")
  candidate_identities+=("$identity")
}

jeryu_candidate_recheck_files() {
  local index fd path process=$BASHPID
  for index in "${!JERYU_CANDIDATE_HELD_FDS[@]}"; do
    fd=${JERYU_CANDIDATE_HELD_FDS[$index]}; path=${candidate_paths[$index]}
    [[ ! -L $path && $(realpath -e -- "$path") == "$path" &&
       $(stat -c '%d:%i:%u:%g:%a:%h' -- "$path") == "${candidate_identities[$index]}" &&
       $(stat -Lc '%d:%i:%u:%g:%a:%h' -- "/proc/$process/fd/$fd") == "${candidate_identities[$index]}" ]] ||
      jeryu_candidate_die "input custody changed during verification"
    jeryu_candidate_parent_custody "$(dirname -- "$path")"
  done
}

require_public_candidate_jankurai() {
  [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 && ${JAIN_RELEASE_CI:-0} != 1 &&
     ${JERYU_JANKURAI_ALLOW_TEST_RECEIPT:-0} == 0 && ${JERYU_INSTALL_TEST_MODE:-0} == 0 ]] ||
    jeryu_candidate_die "candidate mode cannot satisfy release-broker or test authority"
  [[ $(id -u) != 0 ]] || jeryu_candidate_die "candidate CI must run as an unprivileged user"
  local fd
  for fd in "${JERYU_CANDIDATE_HELD_FDS[@]:-}"; do
    [[ -z $fd ]] || exec {fd}<&-
  done
  JERYU_CANDIDATE_HELD_FDS=()
  local -a candidate_paths=() candidate_identities=()
  local here monorepo_root tool_root expected metadata metadata_sha binary receipt install_root
  local monorepo_prefix pin_input builder_input
  here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
  monorepo_root=$(git -C "$here" rev-parse --show-toplevel 2>/dev/null) || \
    jeryu_candidate_die "verifier requires a git checkout"
  monorepo_root=$(cd -- "$monorepo_root" && pwd -P)
  if [[ $here == "$monorepo_root/components/jeryu-tool/ops" ]]; then
    tool_root="$monorepo_root/components/jeryu-tool"
    monorepo_prefix="components/jeryu-tool/"
  elif [[ $here == "$monorepo_root/ops" ]]; then
    tool_root="$monorepo_root"
    monorepo_prefix=""
  else
    jeryu_candidate_die "verifier requires the Tool component"
  fi
  expected=${JERYU_MONOREPO_EXPECTED_HEAD:-}
  [[ $expected =~ ^[0-9a-f]{40}$ ]] || jeryu_candidate_die "exact candidate source commit is required"
  binary=${JERYU_GOVERNED_JANKURAI_BIN:-}
  receipt=${JERYU_JANKURAI_RECEIPT:-}
  [[ $binary == */bin/jankurai ]] || jeryu_candidate_die "explicit candidate installation is required"
  install_root=${binary%/bin/jankurai}
  [[ $install_root != /home/ubuntu/.jeryu && $install_root != /home/ubuntu/.jeryu/* &&
     $install_root != /opt/jain-ci && $install_root != /opt/jain-ci/* ]] ||
    jeryu_candidate_die "candidate receipt cannot qualify a governed installation root"
  [[ $receipt == "$install_root/receipts/jankurai/sha256/"* ]] ||
    jeryu_candidate_die "receipt must belong to this installation"
  local receipt_sha=${receipt##*/}
  receipt_sha=${receipt_sha%.json}
  [[ $receipt_sha =~ ^[0-9a-f]{64}$ && $receipt == "$install_root/receipts/jankurai/sha256/$receipt_sha.json" ]] ||
    jeryu_candidate_die "receipt must have its exact content address"

  metadata=$(bash "$here/render-monorepo-candidate.sh" --monorepo-root "$monorepo_root" \
    --check --expected-head "$expected") || jeryu_candidate_die "current candidate renderer failed"
  metadata_sha=$(printf '%s\n' "$metadata" | sha256sum | cut -d' ' -f1)
  jq -e --arg head "$expected" '
    .schema == "jeryu.jankurai-candidate-render/v1" and .mode == "check" and .drift == false and
    .source.commit == $head and .source.clean_at_start == true and
    .source.repository == "https://github.com/neverhuman/jeryu.git" and
    .distribution.repository == "https://github.com/neverhuman/jankurai.git" and
    .scope == {complete:true,repositories:["jeryu","jeryu-cache","jeryu-ci-runner","jeryu-core",
      "jeryu-deploy","jeryu-intelligence","jeryu-jira","jeryu-release-ops","jeryu-tool",
      "jeryu-tool-finder","jeryu-web"]} and
    .governance == {protected_main:false,handover:"pending",predecessor_authentication:"not-performed"} and
    .verification == {build:"not-performed",installation:"not-performed",public_readback:"not-performed"} and
    (.consumers | length > 0) and all(.consumers[]; .changed == false and .before_sha256 == .expected_sha256)
  ' <<< "$metadata" >/dev/null || jeryu_candidate_die "candidate metadata is not a clean drift check"

  local lock_fd binary_fd receipt_fd pin_fd builder_fd
  jeryu_candidate_open_file "$install_root/.jankurai-install.lock" lock_fd
  [[ $(stat -Lc %a -- "/proc/$BASHPID/fd/$lock_fd") == 600 ]] || jeryu_candidate_die "invalid lock mode"
  flock -s -w 30 "$lock_fd" || jeryu_candidate_die "installation transaction is busy"
  jeryu_candidate_open_file "$binary" binary_fd
  jeryu_candidate_open_file "$receipt" receipt_fd
  pin_input="$tool_root/generated/jankurai-pin.env"
  builder_input="$tool_root/ops/build-jankurai-hermetic.sh"
  jeryu_candidate_open_file "$pin_input" pin_fd
  jeryu_candidate_open_file "$builder_input" builder_fd
  local process=$BASHPID
  local binary_held="/proc/$process/fd/$binary_fd" receipt_held="/proc/$process/fd/$receipt_fd"
  local pin_held="/proc/$process/fd/$pin_fd" builder_held="/proc/$process/fd/$builder_fd"
  [[ $(stat -Lc %a -- "$receipt_held") == 600 ]] || jeryu_candidate_die "invalid receipt mode"
  [[ $(sha256sum "$receipt_held" | cut -d' ' -f1) == "$receipt_sha" ]] || jeryu_candidate_die "receipt content changed"
  cmp -s <(jq -S . "$receipt_held") "$receipt_held" || jeryu_candidate_die "receipt JSON is not canonical"
  local pin_sha builder_sha pin_blob builder_blob lock_identity
  pin_sha=$(sha256sum "$pin_held" | cut -d' ' -f1)
  builder_sha=$(sha256sum "$builder_held" | cut -d' ' -f1)
  [[ $pin_sha == "$(jq -r .generated_pin_sha256 <<< "$metadata")" ]] || jeryu_candidate_die "generated pin changed"
  pin_blob=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    git -C "$monorepo_root" rev-parse "$expected:${monorepo_prefix}generated/jankurai-pin.env")
  builder_blob=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    git -C "$monorepo_root" rev-parse "$expected:${monorepo_prefix}ops/build-jankurai-hermetic.sh")
  lock_identity=$(stat -Lc '%d:%i:%u:%g:%a:%h' -- "/proc/$process/fd/$lock_fd")

  # Parse the generated assignments as data; never execute a pin file.
  local line name value pin_json
  local -A pin_values=()
  local -a pin_args=()
  while IFS= read -r line; do
    [[ -z $line || $line == '#'* ]] && continue
    [[ $line =~ ^(JANKURAI_[A-Z0-9_]+)=\"([^\"]*)\"$ ]] || jeryu_candidate_die "noncanonical generated pin assignment"
    name=${BASH_REMATCH[1]}; value=${BASH_REMATCH[2]}
    [[ ! -v pin_values[$name] ]] || jeryu_candidate_die "duplicate generated pin assignment"
    pin_values[$name]=$value
    pin_args+=(--arg "$name" "$value")
  done < "$pin_held"
  [[ ${#pin_values[@]} == 26 ]] || jeryu_candidate_die "incomplete generated pin"
  pin_json=$(jq -cn "${pin_args[@]}" '$ARGS.named')
  jq -e --argjson renderer "$metadata" --arg renderer_sha "$metadata_sha" \
    --argjson pin "$pin_json" --arg pin_blob "$pin_blob" --arg pin_sha "$pin_sha" \
    --arg builder_blob "$builder_blob" --arg builder_sha "$builder_sha" \
    --arg binary_path "$binary" --arg install_root "$install_root" --arg lock_identity "$lock_identity" \
    -f "$here/public-candidate-receipt.jq" "$receipt_held" >/dev/null ||
    jeryu_candidate_die "candidate receipt does not match current source, build and installation"
  [[ $(sha256sum "$binary_held" | cut -d' ' -f1) == "${pin_values[JANKURAI_BINARY_SHA256]}" ]] ||
    jeryu_candidate_die "candidate binary digest mismatch"
  [[ $(env -i PATH=/usr/bin:/bin JANKURAI_NO_UPDATE_CHECK=1 "$binary_held" --version) == "${pin_values[JANKURAI_VERSION]}" ]] ||
    jeryu_candidate_die "candidate binary version mismatch"

  local final_metadata
  final_metadata=$(bash "$here/render-monorepo-candidate.sh" --monorepo-root "$monorepo_root" \
    --check --expected-head "$expected") || jeryu_candidate_die "candidate source changed during verification"
  [[ $final_metadata == "$metadata" ]] || jeryu_candidate_die "candidate metadata changed during verification"
  jeryu_candidate_recheck_files
  [[ $(sha256sum "$binary_held" | cut -d' ' -f1) == "${pin_values[JANKURAI_BINARY_SHA256]}" &&
     $(sha256sum "$receipt_held" | cut -d' ' -f1) == "$receipt_sha" &&
     $(sha256sum "$pin_held" | cut -d' ' -f1) == "$pin_sha" &&
     $(sha256sum "$builder_held" | cut -d' ' -f1) == "$builder_sha" ]] ||
    jeryu_candidate_die "input content changed during verification"
  export JERYU_CANDIDATE_JANKURAI_DESCRIPTOR="$binary_held"
  export JERYU_JANKURAI_RECEIPT_SHA256="$receipt_sha" JANKURAI_NO_UPDATE_CHECK=1 GIT_TERMINAL_PROMPT=0
}
