#!/usr/bin/env bash
# Actual baseline-independent producers; this command does not qualify full proofs.
set -euo pipefail

aux_git() {
  env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git -C "$aux_root" "$@"
}

aux_select() {
  [[ $# -le 2 ]] || return 2
  aux_producer=${1:-full}
  aux_scope=${2:-all}
  case $aux_producer in copy-code|migration|rust-workspace|independent|full) ;; *) return 2 ;; esac
  case $aux_scope in
    all) aux_components=(jeryu-core jeryu-intelligence jeryu-release-ops jeryu-web) ;;
    jeryu-core|jeryu-intelligence|jeryu-release-ops|jeryu-web) aux_components=("$aux_scope") ;;
    *) return 2 ;;
  esac
  [[ $aux_producer != rust-workspace || $aux_scope == all ]]
}

aux_regular() {
  [[ -f $1 && ! -L $1 && -O $1 && -s $1 &&
     $(realpath -e -- "$1") == "$1" && $(stat -c %h -- "$1") == 1 ]]
}

aux_source() {
  require_jankurai || return 1
  local observed_root observed_head
  observed_root=$(aux_git rev-parse --show-toplevel) || return 1
  observed_head=$(aux_git rev-parse HEAD) || return 1
  [[ $observed_root == "$aux_root" && $observed_head == "$JERYU_MONOREPO_EXPECTED_HEAD" ]] || return 1
  jq -cen --arg commit "$JERYU_MONOREPO_EXPECTED_HEAD" \
    --arg tree "$(aux_git rev-parse 'HEAD^{tree}')" \
    --arg lock "$(sha256sum "$aux_root/Cargo.lock" | cut -d' ' -f1)" \
    --arg manifest "$(sha256sum "$aux_root/Cargo.toml" | cut -d' ' -f1)" \
    --arg toolchain "$(sha256sum "$aux_root/rust-toolchain.toml" | cut -d' ' -f1)" \
    --arg helper "$(aux_git rev-parse 'HEAD:scripts/auxiliary-proofs.sh')" \
    --arg validator "$(aux_git rev-parse 'HEAD:scripts/auxiliary-rust.jq')" \
    --arg binary "$(sha256sum "$JERYU_CANDIDATE_JANKURAI_DESCRIPTOR" | cut -d' ' -f1)" \
    --arg receipt "$JERYU_JANKURAI_RECEIPT_SHA256" \
    '{commit:$commit,tree:$tree,cargo_lock_sha256:$lock,cargo_manifest_sha256:$manifest,
      toolchain_sha256:$toolchain,implementation_blob:$helper,validator_blob:$validator,
      auditor_binary_sha256:$binary,auditor_installation_receipt_sha256:$receipt} |
      select(all([.commit,.tree,.implementation_blob,.validator_blob][]; test("^[0-9a-f]{40}$")) and
        all([.cargo_lock_sha256,.cargo_manifest_sha256,.toolchain_sha256,
          .auditor_binary_sha256,.auditor_installation_receipt_sha256][]; test("^[0-9a-f]{64}$")))'
}

aux_check_copy_code() {
  aux_regular "$1" || return 1
  jq -e --arg root "$2" '
    .schema_version == "1.1.0" and .generated_by == "jankurai copy-code" and
    .repo == $root and (.status == "pass" or .status == "review") and
    .policy.strict == true and .policy.include_tests == false and .policy.active_source_only == true and
    .policy.min_lines == 10 and .policy.min_tokens == 100 and .policy.max_findings == 50 and
    (.summary.files_scanned | type == "number" and . > 0 and . == floor) and
    (.summary.active_files | type == "number" and . > 0 and . == floor) and
    .summary.hard_classes == 0 and .summary.hard_instances == 0 and
    (.classes | type == "array") and
    all(.classes[]; .hard_fail == false and .effective_severity == "warning")
  ' "$1" >/dev/null
}

aux_check_migration() {
  aux_regular "$1" || return 1
  jq -e --arg root "$2" '
    .schema_version == "1.0.0" and .command == "jankurai migrate" and
    .source_root == $root and .status == "complete" and
    (.inventory | type == "object") and (.module_inventory | type == "array") and
    (.liability_score | type == "number" and . >= 0 and . <= 100) and
    (.required_proof_lanes | type == "array") and
    (.rollback_cutover_notes | type == "array")
  ' "$1" >/dev/null
}

aux_copy_code() {
  local out=$2 source="$aux_root/components/$1"
  jankurai copy-code "$source" --strict --json "$out/copy-code.json" \
    --md "$out/copy-code.md" || return $?
  aux_regular "$out/copy-code.md" || return 1
  aux_check_copy_code "$out/copy-code.json" "$source"
}

aux_migration() {
  local component=$1 out=$2 source="$aux_root/components/$1"
  jankurai migrate "$source" --analyze --out "$out/migration-report.json" \
    --md "$out/migration-report.md" || return $?
  aux_regular "$out/migration-report.md" || return 1
  aux_check_migration "$out/migration-report.json" "$source"
}

aux_rust_workspace() {
  local out=$2 path blob
  # Pinned Jankurai metadata has no --locked flag. Use the same default feature
  # graph, offline resolution and exact before/after lock/source admission.
  cargo --version > "$out/cargo-version.txt" || return $?
  rustc --version --verbose > "$out/rustc-version.txt" || return $?
  (cd "$aux_root" && cargo metadata --locked --offline --format-version 1) \
    > "$out/cargo-metadata.json" || return $?
  aux_regular "$out/cargo-metadata.json" || return 1
  jq -e --arg root "$aux_root" -f "$aux_root/scripts/auxiliary-rust.jq" \
    "$out/cargo-metadata.json" > "$out/workspace-ownership.json" || return 1
  while IFS= read -r path; do
    aux_regular "$aux_root/$path" || return 1
    blob=$(aux_git rev-parse "HEAD:$path") || return 1
    printf '%s %s\n' "$blob" "$path"
  done < <(jq -r '.members[].manifest_path' "$out/workspace-ownership.json") \
    > "$out/owning-manifest-blobs.txt" || return 1
  CARGO_NET_OFFLINE=true jankurai rust map "$aux_root" --out-dir "$out" || return $?
  CARGO_NET_OFFLINE=true jankurai rust witness build "$aux_root" \
    --out "$out/witness-graph.json" || return $?
  aux_check_rust_workspace "$out"
}

aux_check_rust_workspace() {
  local out=$1 file
  for file in workspace-ownership.json agent-map.json test-map.json witness-graph.json; do
    aux_regular "$out/$file" || return 1
  done
  jq -e -s --arg root "$aux_root" '
    .[0] as $o | .[1] as $map | .[2] as $tests | .[3] as $w |
    ($o.members | map(.name) | sort) as $names |
    $o.workspace_root == $root and $map.workspace_root == $root and
    $tests.workspace_root == $root and $w.workspace_root == $root and
    ($map.members | map(.name) | sort) == $names and
    ($tests.entries | map(.arc) | sort) == $names and
    ($w.crates | map(.name) | sort) == $names and
    all($o.members[]; . as $m |
      any($map.members[]; .name == $m.name and
        .manifest_path == $m.manifest_path and
        (.direct_dependencies | sort) == $m.direct_dependencies and
        (.reverse_dependencies | sort) == $m.reverse_dependencies) and
      any($w.crates[]; .name == $m.name and
        (.direct_deps | sort) == $m.direct_dependencies and
        (.reverse_deps | sort) == $m.reverse_dependencies and
        (.interface_hash | test("^[0-9a-f]{64}$")) and
        (.implementation_hash | test("^[0-9a-f]{64}$")) and
        (.file_count | type == "number" and . > 0 and . == floor)))
  ' "$out/workspace-ownership.json" "$out/agent-map.json" \
    "$out/test-map.json" "$out/witness-graph.json" >/dev/null
}

aux_step() {
  local producer=$1 component=$2 function=$3 out result current file
  out="$aux_attempt/$producer-$component"
  mkdir -m 700 -- "$out" || return 1
  if "$function" "$component" "$out" > "$out/producer.log" 2>&1; then
    result=0
  else
    result=$?
    aux_failed=1
  fi
  current=$(aux_source) || return 1
  [[ $current == "$aux_initial" ]] || {
    printf 'source or governed tool changed during %s/%s\n' "$producer" "$component" >&2
    return 1
  }
  # Record only actual outputs from this new attempt. No report is synthesized.
  for file in "$out"/*; do
    [[ -f $file && ! -L $file && -O $file && $(stat -c %h -- "$file") == 1 ]] || return 1
  done
  (cd "$out" && sha256sum -- ./*) > "$aux_attempt/$producer-$component.sha256" || return 1
  jq -cn --arg producer "$producer" --arg component "$component" --argjson exit "$result" \
    '{producer:$producer,scope:$component,exit_code:$exit,passed:($exit == 0)}' \
    >> "$aux_attempt/steps.jsonl" || return 1
  printf '%s/%s exit=%s\n' "$producer" "$component" "$result"
}

aux_dispatch() {
  local component
  if [[ $aux_producer == rust-workspace ||
        ( $aux_scope == all && ( $aux_producer == independent || $aux_producer == full ) ) ]]; then
    aux_step rust-workspace jeryu aux_rust_workspace || return 1
  fi
  for component in "${aux_components[@]}"; do
    case $aux_producer in
      copy-code|independent|full) aux_step copy-code "$component" aux_copy_code || return 1 ;;
    esac
    case $aux_producer in
      migration|independent|full) aux_step migration "$component" aux_migration || return 1 ;;
    esac
  done
}

aux_missing_full() {
  printf '%s\n' \
    'Full proof admission unavailable: authenticated protected predecessor and policy;' \
    'complete rename/deletion/binary changed paths and source-bound hunk admission;' \
    'proofbind/proofmark execution with authenticated negative proofs;' \
    'required auxiliary UX/vibe/coverage configuration and exact producer conformance.' \
    'Ordinary audits, security, source coverage, product UX and release gates remain separate requirements.' \
    'Standalone split projection and an export-bound auditor receipt adapter remain required.'
}

aux_conclude() {
  if [[ $aux_producer == full ]]; then
    aux_missing_full | tee "$aux_attempt/unavailable-full-gates.txt" >&2
    return 1
  fi
  [[ $aux_failed == 0 ]]
}

aux_main() {
  aux_select "$@" || {
    printf 'usage: auxiliary-proofs.sh {copy-code|migration|rust-workspace|independent|full} [all|jeryu-core|jeryu-intelligence|jeryu-release-ops|jeryu-web]\n' >&2
    return 2
  }
  aux_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P) || return 1
  [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 && ${JERYU_MONOREPO_EXPECTED_HEAD:-} =~ ^[0-9a-f]{40}$ &&
     -d $aux_root/components/jeryu-tool ]] || {
    printf 'Auxiliary producers require explicit monorepo candidate verification; the standalone export adapter is pending.\n' >&2
    return 1
  }
  # shellcheck source=/dev/null
  source "$aux_root/ops/ci/lib.sh"
  aux_initial=$(aux_source) || return 1
  local directory="$aux_root" part current
  umask 077
  for part in target ci auxiliary; do
    directory="$directory/$part"
    [[ -e $directory || -L $directory ]] || mkdir -m 700 -- "$directory" || return 1
    [[ -d $directory && ! -L $directory && -O $directory &&
       $(realpath -e -- "$directory") == "$directory" ]] || return 1
  done
  aux_attempt=$(mktemp -d "$directory/attempt.XXXXXXXX") || return 1
  aux_attempt_identity=$(stat -c '%d:%i:%u:%g:%a' -- "$aux_attempt") || return 1
  printf '%s\n' "$aux_initial" > "$aux_attempt/source.json"
  printf 'Auxiliary attempt retained at %s\n' "$aux_attempt"
  aux_failed=0
  aux_dispatch || return 1
  current=$(aux_source) || return 1
  [[ $current == "$aux_initial" && ! -L $aux_attempt &&
     $(stat -c '%d:%i:%u:%g:%a' -- "$aux_attempt") == "$aux_attempt_identity" ]] || return 1
  jq -s --argjson source "$aux_initial" --arg selection "$aux_producer" --arg scope "$aux_scope" \
    '{schema:"jeryu.auxiliary-producers/v1",source:$source,selection:$selection,scope:$scope,
      steps:.,full_proof:false,protected_main:false,handover:"pending",
      selected_producers_passed:all(.[]; .passed == true),
      qualification:"selected-producers-only"}' "$aux_attempt/steps.jsonl" > "$aux_attempt/result.json" || return 1
  aux_conclude
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  aux_main "$@"
fi
