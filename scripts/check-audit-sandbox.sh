#!/usr/bin/env bash
# Read-only capability check; explicit provisioning is for disposable hosted CI.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
mode=${1:---check}
[[ $# -le 1 && ( $mode == --check || $mode == --prepare-disposable-ci ) ]] || {
  printf 'usage: check-audit-sandbox.sh [--check|--prepare-disposable-ci]\n' >&2
  exit 2
}
[[ $EUID -ne 0 ]] || {
  printf 'audit sandbox capability must be checked as an unprivileged user\n' >&2
  exit 1
}
probe() {
  /usr/bin/timeout --signal=TERM --kill-after=2s 10s /usr/bin/bwrap \
    --unshare-user --unshare-net --unshare-pid --die-with-parent --new-session \
    --ro-bind / / -- /usr/bin/true
}
if probe; then exit 0; fi
[[ $mode == --prepare-disposable-ci ]] || exit 1
# These are accidental-run guards, never publication or release authority.
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_ENVIRONMENT:-} == github-hosted ]] || {
  printf 'profile provisioning requires the explicit disposable hosted CI mode\n' >&2
  exit 1
}
[[ -f /proc/sys/kernel/apparmor_restrict_unprivileged_userns &&
   $(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns) == 1 ]] || {
  printf 'namespace failure is not the expected AppArmor userns restriction\n' >&2
  exit 1
}
# Refuse to replace an existing policy, including one deliberately disabled.
policy_result=0
sudo -n /usr/bin/rg --quiet --fixed-strings bwrap /etc/apparmor.d || policy_result=$?
[[ $policy_result == 1 ]] || {
  printf 'existing Bubblewrap policy or unreadable policy custody requires maintainer reconciliation\n' >&2
  exit 1
}
# Add only this named profile. A name/attachment conflict is a failure, and no
# system-wide sysctl, existing profile or installed executable is changed.
profile=$root/ci/apparmor.d/jeryu-audit-bwrap
sha256sum "$profile"
sudo -n /sbin/apparmor_parser --add --skip-cache "$profile"
probe
