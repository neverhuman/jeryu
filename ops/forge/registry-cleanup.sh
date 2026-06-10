#!/usr/bin/env bash
#
# registry-cleanup.sh - user-approved ONE-TIME forge registry cleanup.
#
# Deletes the stale registry entries left over from the early import era: every
# repo under the legacy `local/` owner plus three junk `jeryu/` entries, leaving
# exactly the 24 canonical jeryu/<name> repos. REGISTRY-ONLY: every delete is
# issued with delete_storage=false — this script NEVER touches disk.
#
# Usage:
#   ops/forge/registry-cleanup.sh           # dry-run (default): print the plan
#   ops/forge/registry-cleanup.sh --apply   # execute the deletes
#
# Manual pre-step (take a registry backup BEFORE --apply):
#   sqlite3 ~/.local/share/jeryu/forge.sqlite \
#     ".backup ~/.jeryu/backups/forge-pre-cleanup-<ts>.sqlite"
set -euo pipefail

API="${JERYU_API:-http://127.0.0.1:8787}"
APPLY=0

for arg in "$@"; do
  case "${arg}" in
    --apply) APPLY=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: ${arg}" >&2; exit 2 ;;
  esac
done

note() { printf '\033[1;36m▶ %s\033[0m\n' "$*"; }
ok()   { printf '\033[32m✓ %s\033[0m\n' "$*"; }
warn() { printf '\033[33m! %s\033[0m\n' "$*"; }

# The 24 canonical jeryu/<name> repos that MUST survive the cleanup, verbatim.
CANONICAL=(
  echoforge jailgun jankurai jansu jekko jeryu
  jmcp jmcp-core jmcp-deploy jmcp-talk jmcp-web jnoccio
  openQG redline-testing redlineDB
  veox-deploy veox-docs-meta veox-enclave veox-neverhuman-data veox-nht
  veox-proofs veox-shared veox-stage-catalog veox-warp
)
# Junk jeryu/ entries to delete alongside every legacy local/ repo, verbatim.
JUNK=(jeryu/lastcommit jeryu/lockdown-test jeryu/tmp-check-xyz)

workdir="$(mktemp -d -t forge-registry-cleanup-XXXXXX)"
trap 'rm -rf "${workdir}"' EXIT

# Build the delete set from the LIVE listing: every owner=="local" repo plus JUNK.
# SAFETY ASSERT first: the delete set plus jeryu/<CANONICAL> must EXACTLY tile the
# live listing — a stray repo in either direction prints a diff and aborts before
# a single DELETE is issued.
curl -fsS "${API}/api/v1/repos" -o "${workdir}/repos.json"
python3 - "${workdir}/repos.json" "${CANONICAL[*]}" "${JUNK[*]}" > "${workdir}/deletes.txt" <<'PY'
import json
import sys

listing = json.load(open(sys.argv[1]))
canonical = {f"jeryu/{name}" for name in sys.argv[2].split()}
junk = set(sys.argv[3].split())

live = {f'{r["id"]["owner"]}/{r["id"]["name"]}' for r in listing["repositories"]}
delete = {full for full in live if full.startswith("local/")} | junk

planned = delete | canonical
extra = sorted(planned - live)   # planned for delete/keep but NOT in the live listing
stray = sorted(live - planned)   # live but neither canonical nor in the delete set
if extra or stray:
    print("registry-cleanup: SAFETY ASSERT FAILED — plan does not tile the live listing", file=sys.stderr)
    for full in extra:
        print(f"  planned but not live: {full}", file=sys.stderr)
    for full in stray:
        print(f"  live but unplanned:   {full}", file=sys.stderr)
    sys.exit(1)

for full in sorted(delete):
    print(full)
PY
mapfile -t DELETES < "${workdir}/deletes.txt"
ok "safety assert passed: delete set (${#DELETES[@]}) + canonical (${#CANONICAL[@]}) exactly tile the live listing"

print_disk_followup() {
  cat <<'EOF'

manual follow-up — disk candidates (NEVER deleted here; registry-only script):
    ~/.local/share/jeryu/git/jeryu/lastcommit.git
    ~/.local/share/jeryu/git/jeryu/tmp-check-xyz.git
    ~/.local/share/jeryu/git/jeryu/jbaby.git      (on disk, unregistered)
note: jeryu/lockdown-test has no bare dir — it was a registry-only ghost.
EOF
}

if [ "${APPLY}" -ne 1 ]; then
  warn "DRY RUN — would delete ${#DELETES[@]} registry entries (re-run with --apply):"
  for full in "${DELETES[@]}"; do
    echo "DELETE ${full}"
  done
  warn "DRY RUN — ${#DELETES[@]} registry entries would be deleted; nothing was changed."
  print_disk_followup
  exit 0
fi

note "deleting ${#DELETES[@]} registry entries (delete_storage=false; fail-fast)"
for full in "${DELETES[@]}"; do
  owner="${full%%/*}"
  name="${full#*/}"
  echo "DELETE ${full}"
  curl -fsS -X DELETE "${API}/api/v1/repos/${owner}%2F${name}" \
    -H 'content-type: application/json' \
    --data "{\"confirm_full_name\":\"${full}\",\"delete_storage\":false}"
done

# Post-check: exactly the 24 canonical jeryu/<name> repos remain — nothing else.
curl -fsS "${API}/api/v1/repos" -o "${workdir}/repos-after.json"
if python3 - "${workdir}/repos-after.json" "${CANONICAL[*]}" <<'PY'
import json
import sys

listing = json.load(open(sys.argv[1]))
canonical = sorted(sys.argv[2].split())
remaining = sorted(f'{r["id"]["owner"]}/{r["id"]["name"]}' for r in listing["repositories"])
expected = [f"jeryu/{name}" for name in canonical]
if remaining == expected:
    sys.exit(0)
print(f"registry-cleanup: POST-CHECK FAILED — {len(remaining)} repos remain, expected {len(expected)}", file=sys.stderr)
for full in sorted(set(remaining) - set(expected)):
    print(f"  unexpected survivor: {full}", file=sys.stderr)
for full in sorted(set(expected) - set(remaining)):
    print(f"  canonical missing:   {full}", file=sys.stderr)
sys.exit(1)
PY
then
  ok "post-check passed: exactly ${#CANONICAL[@]} canonical jeryu/<name> repos remain"
else
  exit 1
fi
print_disk_followup
