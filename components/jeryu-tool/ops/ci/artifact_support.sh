#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
cd "$REPO_ROOT"
require_tool jq

for path in target target/artifact-support; do
  if [[ -L "$path" || ( -e "$path" && ! -d "$path" ) ]]; then
    printf 'artifact output is not a physical directory: %s\n' "$path" >&2
    exit 1
  fi
  [[ -d "$path" ]] || mkdir -m 0750 -- "$path"
  [[ "$(realpath -e -- "$path")" == "$REPO_ROOT/$path" ]]
done
head_sha="$(git rev-parse --verify 'HEAD^{commit}')"
tree_sha="$(git rev-parse --verify 'HEAD^{tree}')"
archive_sha256="$(git archive --format=tar "$head_sha" | sha256sum | cut -d ' ' -f1)"
for output in target/artifact-support/jeryu-tool.json \
  target/artifact-support/jeryu-tool.json.sha256; do
  if [[ -L "$output" || ( -e "$output" && ! -f "$output" ) ]]; then
    printf 'artifact output is not a regular file: %s\n' "$output" >&2
    exit 1
  fi
  if [[ -e "$output" && "$(stat -Lc '%u:%h' -- "$output")" != "$EUID:1" ]]; then
    printf 'artifact output has unsafe custody: %s\n' "$output" >&2
    exit 1
  fi
  rm -f -- "$output"
done
tmp="$(mktemp -p target/artifact-support .jeryu-tool.XXXXXXXX)"
trap 'rm -f -- "$tmp"' EXIT HUP INT TERM
jq -cnS --arg head "$head_sha" --arg tree "$tree_sha" --arg archive "$archive_sha256" '
  {schema_version:"jeryu.tool.artifact-support/v1",repository:"jeryu/jeryu-tool",
   head_sha:$head,tree_sha:$tree,source_archive_sha256:$archive,
   publication_performed:false,installation_performed:false}
' >"$tmp"
chmod 0444 "$tmp"
mv -fT -- "$tmp" target/artifact-support/jeryu-tool.json
trap - EXIT HUP INT TERM
jq -e --arg head "$head_sha" --arg tree "$tree_sha" --arg archive "$archive_sha256" '
  .schema_version == "jeryu.tool.artifact-support/v1" and
  .repository == "jeryu/jeryu-tool" and .head_sha == $head and .tree_sha == $tree and
  .source_archive_sha256 == $archive and .publication_performed == false and
  .installation_performed == false
' target/artifact-support/jeryu-tool.json >/dev/null
sha256sum target/artifact-support/jeryu-tool.json \
  >target/artifact-support/jeryu-tool.json.sha256
printf 'artifact support ok: archive_sha256=%s\n' "$archive_sha256"
