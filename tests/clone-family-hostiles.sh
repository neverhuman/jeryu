#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
clone_script="${repo_root}/scripts/clone-family.sh"
tmp_root="$(realpath -e -- "${TMPDIR:-/tmp}")"
sandbox="$(mktemp -d "${tmp_root}/jeryu-clone-family-hostiles.XXXXXX")"

cleanup() {
  case "$sandbox" in
    "${tmp_root}"/jeryu-clone-family-hostiles.*) ;;
    *) return 1 ;;
  esac
  [[ -d "$sandbox" && ! -L "$sandbox" && -O "$sandbox" ]] || return 1
  rm -rf -- "$sandbox"
}
trap cleanup EXIT INT TERM HUP

fail() {
  printf 'clone-family hostile test failed: %s\n' "$1" >&2
  exit 1
}

write_repo() {
  local file="$1"
  local name="$2"
  local slug="$3"
  cat >>"$file" <<EOF
[[repo]]
name = "${name}"
path = "/unused/${name}"
github_slug = "neverhuman/${name}"
jeryu_slug = "${slug}"
profile = "public-portal"
default_branch = "main"
current_tag = "${name}-v5.0.0-split.0"
required_check = "${name}/required"
has_jeryu_std = true
onboarded = true

EOF
}

manifest="${sandbox}/family.toml"
printf 'required_repos = ["jeryu", "jeryu-tool"]\n\n' >"$manifest"
write_repo "$manifest" jeryu jeryu/jeryu
write_repo "$manifest" jeryu-tool jeryu/jeryu-tool

plan="$(bash "$clone_script" --manifest "$manifest" --plan "${sandbox}/dest")"
[[ "$plan" == "jeryu-tool|https://git.neverhuman.org/git/jeryu/jeryu-tool.git|${sandbox}/dest/jeryu-tool" ]] ||
  fail "default plan did not select exactly the hosted non-portal repository"
[[ "$plan" != *github.com* && "$plan" != *127.0.0.1* ]] ||
  fail "plan leaked a retired source authority"

portal_plan="$(JERYU_CLONE_PORTAL=1 bash "$clone_script" --manifest "$manifest" --plan "${sandbox}/dest")"
[[ "$portal_plan" == *"jeryu|https://git.neverhuman.org/git/jeryu/jeryu.git|${sandbox}/dest/jeryu"* ]] ||
  fail "explicit portal plan did not use hosted authority"

bad_manifest="${sandbox}/bad.toml"
printf 'required_repos = ["member"]\n\n' >"$bad_manifest"
write_repo "$bad_manifest" member '../escape'
if bash "$clone_script" --manifest "$bad_manifest" --plan "${sandbox}/dest" >"${sandbox}/bad.out" 2>&1; then
  fail "path-traversing hosted slug was accepted"
fi

member_manifest="${sandbox}/member.toml"
printf 'required_repos = ["member"]\n\n' >"$member_manifest"
write_repo "$member_manifest" member jeryu/member

wrong_dest="${sandbox}/wrong-origin"
mkdir -p "${wrong_dest}/member"
git -C "${wrong_dest}/member" init -q -b main
git -C "${wrong_dest}/member" remote add origin 'https://secret-token@example.invalid/jeryu/member.git'
if bash "$clone_script" --manifest "$member_manifest" "$wrong_dest" >"${sandbox}/wrong.out" 2>&1; then
  fail "wrong existing origin was accepted"
fi
if grep -q 'secret-token' "${sandbox}/wrong.out"; then
  fail "wrong-origin diagnostic disclosed embedded credential text"
fi

dirty_dest="${sandbox}/dirty"
mkdir -p "${dirty_dest}/member"
git -C "${dirty_dest}/member" init -q -b main
git -C "${dirty_dest}/member" remote add origin 'https://git.neverhuman.org/git/jeryu/member.git'
printf 'uncommitted\n' >"${dirty_dest}/member/local.txt"
if bash "$clone_script" --manifest "$member_manifest" "$dirty_dest" >"${sandbox}/dirty.out" 2>&1; then
  fail "dirty existing checkout reached network mutation"
fi
grep -q 'checkout is dirty' "${sandbox}/dirty.out" || fail "dirty refusal was not explicit"

symlink_dest="${sandbox}/symlink"
mkdir -p "$symlink_dest" "${sandbox}/elsewhere"
ln -s "${sandbox}/elsewhere" "${symlink_dest}/member"
if bash "$clone_script" --manifest "$member_manifest" "$symlink_dest" >"${sandbox}/symlink.out" 2>&1; then
  fail "symlink repository path was accepted"
fi

printf 'clone-family hostiles ok\n'
