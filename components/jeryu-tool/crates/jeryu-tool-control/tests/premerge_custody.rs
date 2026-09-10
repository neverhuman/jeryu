#![cfg(target_os = "linux")]

use std::{path::Path, process::Command};

// Only empty directories, fixed sentinel bytes and the actual sourced helper
// run here. No Git checkout, installer, auditor, Cargo or container is invoked.
const FIXTURE: &str = r#"
helper="$1/ops/ci/premerge-attempt.sh"
umask 077
test_root="$(mktemp -d /tmp/jeryu-premerge-test.XXXXXXXX)"
test_identity="$(stat -c '%d:%i:%u' "$test_root")"
cleanup() {
  local status=$? root identity path mount_point links
  trap - EXIT
  if (( status != 0 )); then
    printf 'retained failing premerge fixture: %s\n' "$test_root" >&2
    exit "$status"
  fi
  # Candidate state is fixed synthetic data. Use rmdir after removing its sole
  # admitted sentinel; unknown content or identity retains the fixture.
  if [[ -f "$test_root/candidates" ]]; then
    while IFS=$'\t' read -r root identity; do
      [[ "$root" =~ ^/tmp/jeryu-tool-premerge-candidate\.[A-Za-z0-9]{8}$ &&
         -d "$root" && ! -L "$root" && -O "$root" &&
         "$(realpath -e "$root")" == "$root" &&
         "$(stat -c '%d:%i:%u' "$root")" == "$identity" ]] || exit 1
      while read -r _ _ _ _ mount_point _; do
        printf -v mount_point '%b' "$mount_point"
        [[ "$mount_point" != "$root" && "$mount_point" != "$root/"* ]] || exit 1
      done </proc/self/mountinfo
      [[ "$(find "$root" -mindepth 1 -maxdepth 1 -printf '%f\n')" == sentinel ]] || exit 1
      [[ -f "$root/sentinel" && ! -L "$root/sentinel" &&
         "$(stat -c %h "$root/sentinel")" == 1 &&
         "$(cat "$root/sentinel")" == fixture ]] || exit 1
      rm -- "$root/sentinel"
      rmdir -- "$root" || exit 1
    done <"$test_root/candidates"
  fi
  [[ -d "$test_root" && ! -L "$test_root" &&
     "$(realpath -e "$test_root")" == "$test_root" &&
     "$(stat -c '%d:%i:%u' "$test_root")" == "$test_identity" ]] || exit 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ "$mount_point" != "$test_root" && "$mount_point" != "$test_root/"* ]] || exit 1
  done </proc/self/mountinfo
  links="$(find "$test_root" -xdev -type l -print -quit)" || exit 1
  [[ -z "$links" ]] || exit 1
  # Every remaining byte belongs to this completed synthetic fixture.
  rm -rf --one-file-system --preserve-root=all -- "$test_root"
}
trap cleanup EXIT
mkdir "$test_root/repo"
printf 'outside sentinel\n' >"$test_root/sentinel"
attempt() {
  local root="$1" outcome="$2" log="$3"
  bash -euo pipefail -c '
    source "$1"
    test_root="$2"
    outcome="$4"
    mktemp() {
      printf "%s\n" "$*" >>"$test_root/allocations"
      if [[ "$outcome" == allocation-failure && "$2" == /tmp/jeryu-tool-premerge-candidate.* ]]; then return 73; fi
      command mktemp "$@"
    }
    premerge_begin "$3"
    printf "%s\t%s\n" "$candidate_root" "$(stat -c %d:%i:%u "$candidate_root")" >>"$test_root/candidates"
    printf "fixture\n" >"$candidate_root/sentinel"
    printf "%s\n%s\n" "$candidate_root" "$evidence_dir" >"$test_root/latest"
    case "$4" in
      success) exit 0 ;;
      failure) ln -s "$test_root/sentinel" "$candidate_root/fixture-link"; exit 71 ;;
      term) kill -TERM "$BASHPID"; exit 99 ;;
      *) exit 98 ;;
    esac
  ' premerge-fixture "$helper" "$test_root" "$root" "$outcome" >"$log" 2>&1
}
refused() {
  local expected="$1"
  shift
  local status=0
  "$@" || status=$?
  [[ "$status" == "$expected" ]] || { printf 'expected exit=%s actual=%s\n' "$expected" "$status" >&2; exit 1; }
}
read_attempt() {
  mapfile -t latest <"$test_root/latest"
  candidate="${latest[0]}" evidence="${latest[1]}"
  [[ -d "$candidate" && ! -L "$candidate" && -O "$candidate" &&
     -d "$evidence" && ! -L "$evidence" && -O "$evidence" &&
     "$(stat -c %a "$candidate")" == 700 && "$(stat -c %a "$evidence")" == 700 ]]
  grep -Fx "candidate_root=$candidate" "$evidence/attempt-paths.txt" >/dev/null
  grep -Fx "evidence_dir=$evidence" "$evidence/attempt-paths.txt" >/dev/null
  [[ "$(cat "$candidate/sentinel")" == fixture ]]
}
"#;

fn run(script: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("physical Tool root");
    let output = Command::new("bash")
        .args(["-euo", "pipefail", "-c"])
        .arg(format!("{FIXTURE}\n{script}"))
        .arg("premerge-custody-test")
        .arg(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run premerge custody fixture");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn successful_attempts_are_private_unique_and_preserve_prior_evidence() {
    run(r#"
mkdir -p "$test_root/repo/target/jankurai/premerge-candidate"
prior="$test_root/repo/target/jankurai/premerge-candidate/sentinel"
printf 'prior evidence\n' >"$prior"
prior_identity="$(stat -c '%d:%i:%u:%a:%h:%s' "$prior")"
attempt "$test_root/repo" success "$test_root/first.log"
read_attempt
first_candidate="$candidate" first_evidence="$evidence"
attempt "$test_root/repo" success "$test_root/second.log"
read_attempt
[[ "$candidate" != "$first_candidate" && "$evidence" != "$first_evidence" ]]
[[ -d "$first_candidate" && -d "$first_evidence" ]]
[[ "$(stat -c '%d:%i:%u:%a:%h:%s' "$prior")" == "$prior_identity" ]]
[[ "$(cat "$prior")" == 'prior evidence' ]]
grep -F 'reserved for separate verified retirement:' "$test_root/first.log" >/dev/null
grep -F 'exit=0' "$test_root/second.log" >/dev/null
"#);
}

#[test]
fn failed_and_interrupted_attempts_keep_candidate_and_diagnostics() {
    run(r#"
refused 71 attempt "$test_root/repo" failure "$test_root/failure.log"
read_attempt
[[ "$(readlink "$candidate/fixture-link")" == "$test_root/sentinel" ]]
[[ "$(cat "$test_root/sentinel")" == 'outside sentinel' ]]
grep -F 'exit=71' "$test_root/failure.log" >/dev/null
# Remove only the exact fixture link after proving it survived the failed gate.
rm -- "$candidate/fixture-link"
refused 143 attempt "$test_root/repo" term "$test_root/term.log"
read_attempt
grep -F 'exit=143' "$test_root/term.log" >/dev/null
"#);
}

#[test]
fn unsafe_ancestors_and_path_aliases_fail_before_candidate_allocation() {
    run(r#"
ln -s "$test_root/repo" "$test_root/alias"
refused 1 attempt "$test_root/alias" success "$test_root/alias.log"
refused 1 attempt "$test_root/repo/../repo" success "$test_root/traversal.log"
rm -- "$test_root/alias"
mkdir "$test_root/outside"
ln -s "$test_root/outside" "$test_root/repo/target"
refused 1 attempt "$test_root/repo" success "$test_root/target-link.log"
[[ -z "$(find "$test_root/outside" -mindepth 1 -print -quit)" ]]
rm -- "$test_root/repo/target"
mkdir "$test_root/repo/target"
ln -s "$test_root/outside" "$test_root/repo/target/jankurai"
refused 1 attempt "$test_root/repo" success "$test_root/evidence-link.log"
rm -- "$test_root/repo/target/jankurai"
chmod 777 "$test_root/repo/target"
refused 1 attempt "$test_root/repo" success "$test_root/writable.log"
chmod 700 "$test_root/repo/target"
[[ ! -e "$test_root/candidates" && ! -e "$test_root/latest" && ! -e "$test_root/allocations" ]]
[[ "$(cat "$test_root/sentinel")" == 'outside sentinel' ]]
"#);
}

#[test]
fn candidate_allocation_failure_preserves_new_attempt_and_old_symlink() {
    run(r#"
mkdir -p "$test_root/repo/target/jankurai"
prior="$test_root/repo/target/jankurai/premerge-candidate"
ln -s "$test_root/sentinel" "$prior"
refused 73 attempt "$test_root/repo" allocation-failure "$test_root/allocation.log"
mapfile -t kept < <(find "$test_root/repo/target/jankurai" -mindepth 1 -maxdepth 1 -type d)
[[ "${#kept[@]}" == 1 && "$(stat -c %a "${kept[0]}")" == 700 ]]
grep -F 'candidate=not-allocated' "$test_root/allocation.log" >/dev/null
grep -F "evidence=${kept[0]} exit=73" "$test_root/allocation.log" >/dev/null
[[ "$(readlink "$prior")" == "$test_root/sentinel" ]]
[[ "$(cat "$test_root/sentinel")" == 'outside sentinel' ]]
[[ ! -e "$test_root/candidates" ]]
rm -- "$prior"
"#);
}
