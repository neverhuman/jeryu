#!/usr/bin/env bash
# Fetch pinned neverhuman support trees into components/.
# No-op when the current HEAD tree already matches family.lock.toml.
# Never uses git worktree. Refuses localhost, dirty substitutions, and mismatches.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
lock="${JERYU_FAMILY_LOCK:-$root/family.lock.toml}"
plan=0
case "${1:-}" in
  --plan) plan=1 ;;
  --lock)
    shift
    lock="${1:-}"
    ;;
  --help|-h)
    printf 'usage: scripts/fetch-family.sh [--plan] [--lock PATH]\n' >&2
    exit 0
    ;;
  "")
    ;;
  *)
    printf 'usage: scripts/fetch-family.sh [--plan] [--lock PATH]\n' >&2
    exit 2
    ;;
esac

[[ -f "$lock" && ! -L "$lock" ]] || { printf 'fetch-family: lock must be a regular file\n' >&2; exit 1; }
command -v python3 >/dev/null || { printf 'fetch-family: python3 is required\n' >&2; exit 1; }

python3 - "$root" "$lock" "$plan" <<'PY'
import re, shutil, subprocess, sys, tempfile
from pathlib import Path

root = Path(sys.argv[1])
lock = Path(sys.argv[2])
plan = sys.argv[3] == "1"
text = lock.read_text()
if re.search(r"127\.0\.0\.1|localhost|git\.neverhuman\.org", text):
    sys.exit("fetch-family: lock contains a forbidden remote")

rows = []
cur = {}
for line in text.splitlines():
    if line.startswith("[[repo]]"):
        if cur:
            rows.append(cur)
        cur = {}
        continue
    m = re.match(r'^(name|path|github|tag|tree|commit)\s*=\s*"(.*)"\s*$', line)
    if m:
        cur[m.group(1)] = m.group(2)
if cur:
    rows.append(cur)
if not rows:
    sys.exit("fetch-family: lock contains no [[repo]] rows")

def git(*args, cwd=None):
    return subprocess.check_output(["git", *args], cwd=cwd, text=True).rstrip()

def refuse(msg):
    sys.exit(f"fetch-family: {msg}")

changed = 0
for row in rows:
    name, path, github, tree = row["name"], row["path"], row["github"], row["tree"]
    tag = row.get("tag", "")
    if not github.startswith("https://github.com/neverhuman/") or not github.endswith(".git"):
        refuse(f"{name}: github must be https://github.com/neverhuman/<name>.git")
    if "127.0.0.1" in github or "localhost" in github:
        refuse(f"{name}: localhost remote")
    if not re.fullmatch(r"[0-9a-f]{40}", tree):
        refuse(f"{name}: tree must be a 40-hex SHA")
    component = root / path
    if component.is_symlink():
        refuse(f"{name}: component path is a symlink")
    try:
        current = git("rev-parse", f"HEAD:{path}", cwd=root)
    except subprocess.CalledProcessError:
        current = ""
    if current == tree and component.is_dir():
        print(f"fetch-family: {name} already matches {tree[:12]}")
        continue
    porcelain = subprocess.check_output(
        ["git", "status", "--porcelain", "--", path], cwd=root, text=True
    )
    if porcelain.strip():
        refuse(f"{name}: dirty substitution refused\n{porcelain}")
    print(f"fetch-family: {name} have={current[:12] or 'missing'} want={tree[:12]}")
    if not tag or tag == "pending":
        refuse(f"{name}: tag must be a published immutable tag before fetch")
    if not re.fullmatch(r"[A-Za-z0-9._/-]+", tag):
        refuse(f"{name}: tag is not a safe git ref")
    if plan:
        changed += 1
        continue
    tmp = Path(tempfile.mkdtemp(prefix=f"jeryu-fetch-{name}-"))
    try:
        subprocess.check_call(
            ["git", "clone", "--no-local", "--branch", tag, github, str(tmp / "src")],
            stdout=subprocess.DEVNULL,
        )
        src = tmp / "src"
        cloned_tree = git("rev-parse", "HEAD^{tree}", cwd=src)
        if cloned_tree != tree:
            refuse(
                f"{name}: tag {tag} tree {cloned_tree} does not match pin {tree}; "
                "publish the monorepo-integrated tree before fetch can replace it"
            )
        if component.exists():
            shutil.rmtree(component)
        component.mkdir(parents=True)
        archive = subprocess.check_output(["git", "archive", "HEAD"], cwd=src)
        subprocess.run(["tar", "-x", "-C", str(component)], input=archive, check=True)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    changed += 1

print(f"fetch-family: {'planned' if plan else 'updated'} {changed} component(s)")
PY
