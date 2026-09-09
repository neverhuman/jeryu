#!/usr/bin/env bash
# Candidate build context; every source path is checked by source-authority.sh.

jeryu_candidate_cargo_config_record() {
  local path=$1 directory before after digest
  directory=$(dirname -- "$path")
  if [[ -e $directory || -L $directory ]]; then
    require_physical_dir "$directory" 'Cargo configuration directory'
  fi
  if [[ ! -e $path && ! -L $path ]]; then
    jq -cnS --arg path "$path" '{path:$path,state:"absent"}'
    return
  fi
  if [[ $path == "$cargo_home/credentials" || $path == "$cargo_home/credentials.toml" ]]; then
    die 'anonymous candidate Cargo home must not contain credential files'
  fi
  require_physical_file "$path" 'Cargo configuration'
  before=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$path") || return 1
  digest=$(sha_file "$path") || return 1
  # Includes, source replacements, environment injection and build overrides are closed.
  awk '
    function refuse() {
      print "candidate Cargo configuration is outside the supported closed subset" > "/dev/stderr"
      exit 1
    }
    {
      sub(/#.*/, "")
      gsub(/^[[:space:]]+|[[:space:]]+$/, "")
      if ($0 == "") next
      if ($0 == "[build]") {
        if (seen_build++) refuse()
        section = "build"
      } else if ($0 == "[net]") {
        if (seen_net++) refuse()
        section = "net"
      } else if (section == "build" && /^jobs[[:space:]]*=[[:space:]]*[12]$/) {
        if (seen_jobs++) refuse()
      } else if (section == "net" && /^git-fetch-with-cli[[:space:]]*=[[:space:]]*true$/) {
        if (seen_git++) refuse()
      } else refuse()
    }
  ' "$path" || return 1
  after=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$path") || return 1
  [[ $before == "$after" && ! -L $path && $(realpath -e -- "$path") == "$path" ]] ||
    die 'Cargo configuration moved while recording its bytes'
  jq -cnS --arg path "$path" --arg sha "$digest" '{path:$path,state:"file",sha256:$sha}'
}

jeryu_candidate_cargo_config_snapshot() {
  local directory path
  # Cargo checks both names at CARGO_HOME and at every cwd ancestor, up to /.
  {
    printf '%s\0' "$cargo_home/config" "$cargo_home/config.toml" \
      "$cargo_home/credentials" "$cargo_home/credentials.toml"
    directory=${repo_root:?}
    while :; do
      printf '%s\0' "${directory%/}/.cargo/config" "${directory%/}/.cargo/config.toml"
      [[ $directory != / ]] || break
      directory=$(dirname -- "$directory") || return 1
    done
  } | LC_ALL=C sort -zu |
    while IFS= read -r -d '' path; do
      jeryu_candidate_cargo_config_record "$path" || exit 1
    done | jq -csS '.'
}

jeryu_candidate_artifact_build_environment() {
  local local_home actual_cargo_real actual_rustc_real expected_cargo_real expected_rustc_real
  require_tool awk
  [[ ${build_jobs:?} -le 2 ]] || die 'candidate artifact builds require at most two Cargo jobs'
  jeryu_source_snapshot || return 1
  candidate_root=$(jeryu_candidate_source_root) || return 1
  toolchain_channel=$(awk -F'"' '/^[[:space:]]*channel[[:space:]]*=/ {print $2; exit}' \
    "$candidate_root/rust-toolchain.toml")
  [[ $toolchain_channel =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
    die 'candidate root toolchain is not an exact stable channel'
  # shellcheck disable=SC2034 # Output consumed by artifact-support.sh.
  build_authority_mode='monorepo-candidate-toolchain'
  [[ -z ${CARGO_TARGET_DIR:-} ]] || die 'candidate CARGO_TARGET_DIR overrides are forbidden'
  local_home=$(getent passwd "$(id -u)" | awk -F: 'NR == 1 {print $6}')
  require_physical_dir "$local_home" 'physical account home'
  cargo_home=${CARGO_HOME:-"$local_home/.cargo"}
  rustup_home=${RUSTUP_HOME:-"$local_home/.rustup"}
  require_physical_dir "$cargo_home" 'candidate Cargo home'
  require_physical_dir "$rustup_home" 'candidate Rustup home'
  cargo_bin="$rustup_home/toolchains/${toolchain_channel}-x86_64-unknown-linux-gnu/bin/cargo"
  rustc_bin="$rustup_home/toolchains/${toolchain_channel}-x86_64-unknown-linux-gnu/bin/rustc"
  require_physical_file "$cargo_bin" 'candidate cargo binary'
  require_physical_file "$rustc_bin" 'candidate rustc binary'
  actual_cargo_real=$(realpath -e -- "$(command -v cargo)")
  actual_rustc_real=$(realpath -e -- "$(command -v rustc)")
  expected_cargo_real=$(realpath -e -- "$local_home/.cargo/bin/cargo")
  expected_rustc_real=$(realpath -e -- "$local_home/.cargo/bin/rustc")
  [[ $actual_cargo_real == "$expected_cargo_real" && $actual_rustc_real == "$expected_rustc_real" ]] ||
    die 'candidate PATH cargo or rustc differs from the account Rustup launcher'

  candidate_cargo_configuration=$(jeryu_candidate_cargo_config_snapshot) || return 1
  private_target_parent="$repo_root/target/artifact-support-build"
  if [[ ! -e $private_target_parent && ! -L $private_target_parent ]]; then
    mkdir -- "$private_target_parent"
  fi
  require_physical_dir "$private_target_parent" 'candidate target parent'
  [[ -O $private_target_parent ]] || die 'candidate target parent is not owned by this worker'
  private_target=$(mktemp -d "$private_target_parent/.jeryu-tool-finder.XXXXXX")
  # shellcheck disable=SC2034 # Output consumed by artifact-support.sh.
  private_target_identity=$(
    jeryu_record_test_scratch "$private_target" || exit 1
    printf '%s\n' "${jeryu_test_scratch_identity:?}"
  )
  [[ "$(stat -c '%u:%a' -- "$private_target")" == "$(id -u):700" &&
     -z "$(find "$private_target" -mindepth 1 -maxdepth 1 -print -quit)" ]] ||
    die 'candidate Cargo target lacks empty private custody'
  mkdir -m 0700 -- "$private_target/home" "$private_target/tmp"
  git_global_config="$private_target/gitconfig"
  (umask 077; : >"$git_global_config")
  require_physical_file "$git_global_config" 'empty candidate Git configuration'
  [[ ! -s $git_global_config ]] || die 'candidate Git configuration is not empty'
  cargo_sha=$(sha_file "$cargo_bin")
  rustc_sha=$(sha_file "$rustc_bin")
  git_global_config_sha=$(sha_file "$git_global_config")
  # shellcheck disable=SC2034 # Output consumed by artifact-support.sh.
  cargo_home_config_sha=''
  cargo_version=$(run_clean_tool "$cargo_bin" --version)
  rustc_version=$(run_clean_tool "$rustc_bin" --version)
  [[ $cargo_version == "cargo $toolchain_channel "* && $rustc_version == "rustc $toolchain_channel "* ]] ||
    die 'candidate compiler version differs from the root toolchain pin'
}

jeryu_candidate_verify_artifact_build_authority() {
  local actual_configuration
  actual_configuration=$(jeryu_candidate_cargo_config_snapshot) ||
    die 'candidate Cargo configuration chain cannot be verified'
  require_physical_file "$cargo_bin" 'candidate cargo binary'
  require_physical_file "$rustc_bin" 'candidate rustc binary'
  require_physical_file "$git_global_config" 'empty candidate Git configuration'
  [[ $(sha_file "$cargo_bin") == "$cargo_sha" &&
     $(sha_file "$rustc_bin") == "$rustc_sha" &&
     $(sha_file "$git_global_config") == "$git_global_config_sha" &&
     ! -s $git_global_config &&
     $(run_clean_tool "$cargo_bin" --version) == "$cargo_version" &&
     $(run_clean_tool "$rustc_bin" --version) == "$rustc_version" &&
     $actual_configuration == "$candidate_cargo_configuration" ]] ||
    die 'candidate build tools or Cargo configuration chain moved'
}
