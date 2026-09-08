#!/usr/bin/env bash
# Canonical fail-closed supply-chain entrypoint used by local and hosted proof.
set -euo pipefail
set +o xtrace
set +o verbose

for name in actionlint awk bash cargo cargo-audit cargo-deny cat chmod cp \
    dirname find git gitleaks grep jq mkdir mktemp mv realpath rm sha256sum \
    sort stat syft; do
  if declare -F -- "${name}" >/dev/null 2>&1; then
    printf 'security wrapper rejects caller-defined shell function: %s\n' "${name}" >&2
    exit 1
  fi
done

script_path="${BASH_SOURCE[0]}"
case "${script_path}" in
  /*) ;;
  *) script_path="${PWD}/${script_path}" ;;
esac
script_dir="${script_path%/*}"
root="$(cd -- "${script_dir}/.." && pwd -P)"
implementation="${root}/ops/ci/security.sh"
system_bash='/usr/bin/bash'

[[ -f "${system_bash}" && ! -L "${system_bash}" && -x "${system_bash}" ]] || {
  printf 'canonical Bash is unavailable: %s\n' "${system_bash}" >&2
  exit 1
}
[[ -f "${implementation}" && ! -L "${implementation}" &&
   "$(/usr/bin/stat -c '%h' -- "${implementation}")" == 1 ]] || {
  printf 'security implementation lacks regular single-link custody: %s\n' \
    "${implementation}" >&2
  exit 1
}

cd "${root}"
env_args=(-i PATH=/usr/bin:/bin LANG=C LC_ALL=C TZ=UTC)
for name in HOME CARGO_HOME CARGO_TARGET_DIR RUSTUP_HOME JAIN_RELEASE_CI \
    JAIN_NATIVE_BUILD_TOOLS_ROOT JAIN_RUSTSEC_ADVISORY_SOURCE; do
  if [[ -v "${name}" ]]; then
    env_args+=("${name}=${!name}")
  fi
done
exec /usr/bin/env "${env_args[@]}" "${system_bash}" ops/ci/security.sh "$@"
