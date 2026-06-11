#!/usr/bin/env bash
set -euo pipefail

manifest="repos.manifest.toml"
emit_json=0
check_paths=0

usage() {
  printf 'usage: %s [--manifest PATH] [--json] [--check-paths]\n' "$0" >&2
}

fail() {
  printf 'manifest error: %s\n' "$1" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      shift
      manifest="${1:-}"
      ;;
    --json)
      emit_json=1
      ;;
    --check-paths)
      check_paths=1
      ;;
    *)
      usage
      exit 2
      ;;
  esac
  shift
done

[[ -n "$manifest" ]] || fail "--manifest requires a path"
[[ -r "$manifest" ]] || fail "manifest not readable: $manifest"

mapfile -t rows < <(
  python3 - "$manifest" <<'PY'
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

with open(sys.argv[1], "rb") as fh:
    data = tomllib.load(fh)

for repo in data.get("repo", []):
    print("|".join([
        str(repo.get("name", "")),
        str(repo.get("path", "")),
        str(repo.get("github_slug", "")),
        str(repo.get("jeryu_slug", "")),
        str(repo.get("profile", "")),
        str(repo.get("default_branch", "")),
        str(repo.get("current_tag", "")),
        str(repo.get("required_check", "")),
        str(bool(repo.get("has_jeryu_std", False))).lower(),
        str(bool(repo.get("onboarded", False))).lower(),
    ]))
PY
)

[[ "${#rows[@]}" -gt 0 ]] || fail "manifest must contain [[repo]] entries"

declare -A seen=()
for row in "${rows[@]}"; do
  IFS='|' read -r name path github_slug jeryu_slug profile default_branch current_tag required_check has_jeryu_std onboarded <<<"$row"
  [[ -n "$name" ]] || fail "repo entry missing name"
  seen["$name"]=1
  for field in path github_slug jeryu_slug profile default_branch current_tag required_check; do
    value="${!field}"
    [[ -n "$value" ]] || fail "$name missing $field"
  done
  [[ "$default_branch" == "main" ]] || fail "$name default_branch must be main"
  [[ "$has_jeryu_std" == "true" ]] || fail "$name must set has_jeryu_std=true"
  if [[ "$check_paths" == "1" ]]; then
    [[ -d "$path" ]] || fail "$name path missing: $path"
    [[ -f "$path/AGENTS.md" ]] || fail "$name missing AGENTS.md"
    [[ -f "$path/agent/owner-map.json" ]] || fail "$name missing agent/owner-map.json"
    [[ -f "$path/agent/test-map.json" ]] || fail "$name missing agent/test-map.json"
  fi
done

mapfile -t required < <(
  python3 - "$manifest" <<'PY'
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
with open(sys.argv[1], "rb") as fh:
    data = tomllib.load(fh)
for name in data.get("required_repos", []):
    print(name)
PY
)

missing=()
for name in "${required[@]}"; do
  [[ -n "${seen[$name]:-}" ]] || missing+=("$name")
done
if [[ "${#missing[@]}" -gt 0 ]]; then
  fail "manifest missing required repos: ${missing[*]}"
fi

if [[ "$emit_json" == "1" ]]; then
  python3 - "$manifest" <<'PY'
import json
import sys
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
with open(sys.argv[1], "rb") as fh:
    data = tomllib.load(fh)
print(json.dumps({"repo": data.get("repo", [])}, indent=2, sort_keys=True))
PY
else
  for row in "${rows[@]}"; do
    IFS='|' read -r name path github_slug jeryu_slug _ <<<"$row"
    printf '%s|%s|%s|%s\n' "$name" "$path" "$github_slug" "$jeryu_slug"
  done
fi

