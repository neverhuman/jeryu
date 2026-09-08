#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

receipt_tool=ops/ci/repair-receipt.sh
if [[ -L target || ( -e target && ! -d target ) ]]; then
  printf 'repair receipt test: target is not a physical directory\n' >&2
  exit 1
fi
[[ -d target ]] || mkdir -m 0750 -- target
[[ "$(realpath -e -- target)" == "$ROOT/target" ]] || {
  printf 'repair receipt test: target path is not canonical\n' >&2
  exit 1
}
test_root="$(mktemp -d -p target repair-receipt-test.XXXXXXXX)"
test_id="repair-contract-$BASHPID-$(date -u +%s%N)"
valid="target/repair-receipts/$test_id.json"
symlinked="target/repair-receipts/$test_id-symlink.json"
hardlink="target/repair-receipts/$test_id-hardlink.json"
wrong_head="target/repair-receipts/$test_id-wrong-head.json"
wrong_tree="target/repair-receipts/$test_id-wrong-tree.json"
malformed="target/repair-receipts/$test_id-malformed.json"
noncanonical="target/repair-receipts/$test_id-noncanonical.json"
replacement="$test_root/replacement.json"
replacement_error="$test_root/replacement.err"
evidence="${test_root#"$ROOT"/}/evidence.log"
valid_queue="${test_root#"$ROOT"/}/repair-queue.jsonl"
invalid_queue="${test_root#"$ROOT"/}/repair-queue-invalid.jsonl"

cleanup() {
  local path
  for path in "$valid" "$symlinked" "$hardlink" "$wrong_head" "$wrong_tree" \
    "$malformed" "$noncanonical" "${replacement#"$ROOT"/}" \
    "${replacement_error#"$ROOT"/}" "$evidence" "$valid_queue" \
    "$invalid_queue"; do
    [[ ! -e "$path" && ! -L "$path" ]] || chmod u+w -- "$path" 2>/dev/null || true
    rm -f -- "$path"
  done
  rmdir -- "$test_root" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

expect_fail() {
  local label="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    printf 'repair receipt hostile unexpectedly passed: %s\n' "$label" >&2
    return 1
  fi
}

mutate_receipt() {
  local filter="$1" destination="$2"
  jq -cS "$filter" "$valid" >"$destination"
  chmod 0444 -- "$destination"
}

validate_repair_queue() {
  local candidate="$1" line
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -n "$line" ]] || continue
    jq -e '
      type == "object" and
      ((keys - ["lane", "owner", "path", "priority", "rule_id", "task", "tlr", "why"])
        | length) == 0 and
      (.path | type == "string" and length > 0) and
      (.priority | type == "string" and length > 0) and
      (.task | type == "string" and length > 0) and
      (.why | type == "string" and length > 0) and
      ((has("rule_id") | not) or (.rule_id | type) == "string") and
      ((has("tlr") | not) or (.tlr | type) == "string") and
      ((has("lane") | not) or (.lane | type) == "string") and
      ((has("owner") | not) or (.owner | type) == "string")
    ' <<<"$line" >/dev/null || return 1
  done <"$candidate"
}

jq -e '
  .additionalProperties == false and
  .required == ["path", "priority", "task", "why"] and
  (.properties | keys) == ["lane", "owner", "path", "priority", "rule_id", "task", "tlr", "why"]
' schemas/repair-queue.schema.json >/dev/null
jq -e '
  .additionalProperties == false and
  .properties.schema_version.const == "jeryu.tool.repair-receipt/v1" and
  .properties.repo.const == "jeryu/jeryu-tool" and
  (.properties.lane.enum | index("repair-receipt-contract")) != null
' schemas/repair-receipt.schema.json >/dev/null
printf '%s\n' \
  '{"lane":"fast","owner":"workspace","path":"Justfile","priority":"medium","rule_id":"HLT-018-PERF-CONCURRENCY-DRIFT","task":"rerun the narrow proof","tlr":"Verification","why":"the cached lane failed"}' \
  >"$valid_queue"
printf '%s\n' \
  '{"path":"Justfile","priority":"medium","why":"missing task"}' \
  >"$invalid_queue"
validate_repair_queue "$valid_queue"
if validate_repair_queue "$invalid_queue"; then
  printf 'repair queue hostile unexpectedly passed: missing-task\n' >&2
  exit 1
fi

printf 'repair receipt contract evidence\n' >"$evidence"
"$receipt_tool" emit --output "$valid" --lane repair-receipt-contract \
  --purpose "prove create-once repair evidence custody" --exit-code 0 \
  --started-at 2026-09-04T00:00:00Z --finished-at 2026-09-04T00:00:01Z \
  --evidence "$evidence" >/dev/null
"$receipt_tool" verify "$valid" >/dev/null

valid_sha="$(sha256sum -- "$valid")"
valid_sha="${valid_sha%% *}"
expect_fail create-once-replay "$receipt_tool" emit --output "$valid" \
  --lane repair-receipt-contract --purpose replay --exit-code 0 \
  --started-at 2026-09-04T00:00:00Z --finished-at 2026-09-04T00:00:01Z \
  --evidence "$evidence"
[[ "$(sha256sum -- "$valid")" == "$valid_sha  $valid" ]]

expect_fail traversal "$receipt_tool" emit \
  --output "target/repair-receipts/../$test_id-escape.json" \
  --lane repair-receipt-contract --purpose traversal --exit-code 0 \
  --started-at 2026-09-04T00:00:00Z --finished-at 2026-09-04T00:00:01Z \
  --evidence "$evidence"

ln -s -- "$valid" "$symlinked"
expect_fail symlink "$receipt_tool" verify "$symlinked"
rm -f -- "$symlinked"

ln -- "$valid" "$hardlink"
expect_fail hardlink "$receipt_tool" verify "$valid"
rm -f -- "$hardlink"
"$receipt_tool" verify "$valid" >/dev/null

printf 'mutated repair evidence\n' >>"$evidence"
expect_fail evidence-mutation "$receipt_tool" verify "$valid"
printf 'repair receipt contract evidence\n' >"$evidence"
"$receipt_tool" verify "$valid" >/dev/null

mutate_receipt '.git_head="0000000000000000000000000000000000000000"' "$wrong_head"
expect_fail wrong-head "$receipt_tool" verify "$wrong_head"
mutate_receipt '.git_tree="0000000000000000000000000000000000000000"' "$wrong_tree"
expect_fail wrong-tree "$receipt_tool" verify "$wrong_tree"
mutate_receipt '.unexpected=true' "$malformed"
expect_fail malformed-schema "$receipt_tool" verify "$malformed"
jq . "$valid" >"$noncanonical"
chmod 0444 -- "$noncanonical"
expect_fail noncanonical-custody "$receipt_tool" verify "$noncanonical"

# Source the verifier and replace the lexical pathname after it opens the
# original inode. Descriptor/path revalidation must catch the race.
# shellcheck source=ops/ci/repair-receipt.sh
source ops/ci/repair-receipt.sh
replacement_enabled=1
repair_receipt_after_open() {
  local selected="$1"
  [[ "$replacement_enabled" == 1 ]] || return 0
  replacement_enabled=0
  jq -cS '.purpose="replacement raced verifier"' "$selected" >"$replacement"
  chmod 0444 -- "$replacement"
  mv -fT -- "$replacement" "$selected"
}
if repair_verify_receipt "$valid" >/dev/null 2>"$replacement_error"; then
  printf 'repair receipt hostile unexpectedly passed: pathname-replacement\n' >&2
  exit 1
fi
grep -F 'receipt path changed after selection' "$replacement_error" >/dev/null

printf 'repair receipt contract ok: queue replay traversal symlink hardlink evidence head tree schema canonical replacement\n'
