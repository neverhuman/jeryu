#!/usr/bin/env bash
# Both Rust owners generate their own contracts and the browser's combined set.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
mode=${1:---check}
[[ $# -le 1 && ( $mode == --check || $mode == --write ) ]] || {
  printf 'usage: scripts/contracts.sh [--check|--write]\n' >&2; exit 2;
}
temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT
mkdir -p "$temporary/core" "$temporary/work" "$temporary/web"
generate() {
  TS_RS_EXPORT_DIR="$2" cargo run --locked -p "$1" --bin export_contracts
}
generate jeryu-readmodel "$temporary/core"
generate jeryu-jira "$temporary/work"
generate jeryu-readmodel "$temporary/web"
generate jeryu-jira "$temporary/web"
compare() {
  # README files describe generation; the generated surface consists of *.ts.
  diff -ru --exclude='README.md' "$1" "$2"
}
if [[ $mode == --write ]]; then
  generate jeryu-readmodel "$root/components/jeryu-core/contracts/generated"
  generate jeryu-jira "$root/components/jeryu-jira/contracts/generated"
  generate jeryu-readmodel "$root/components/jeryu-web/contracts/generated"
  generate jeryu-jira "$root/components/jeryu-web/contracts/generated"
fi
compare "$temporary/core" components/jeryu-core/contracts/generated
compare "$temporary/work" components/jeryu-jira/contracts/generated
compare "$temporary/web" components/jeryu-web/contracts/generated
printf 'Rust owners and browser contracts match generated output\n'
