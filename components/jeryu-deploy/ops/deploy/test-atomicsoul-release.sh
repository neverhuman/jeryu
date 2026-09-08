#!/usr/bin/env bash
# Fast contract test for the atomicsoul env/signing deploy helpers.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-atomicsoul-release.XXXXXX")"
trap 'rm -rf "${tmp}"' EXIT
mkdir -p "${tmp}/bundle"

release="test-atomicsoul-release"
env_dir="${tmp}/env"

ops/deploy/make-production-env.sh --release "${release}" --out-dir "${env_dir}" >/dev/null

cat >"${tmp}/bundle/jeryu" <<'SH'
#!/usr/bin/env bash
printf 'jeryu test artifact\n'
SH
chmod 0755 "${tmp}/bundle/jeryu"
printf '{"schema":"jeryu.release-receipt/v2","release":"%s","commit":"1111111111111111111111111111111111111111"}\n' "${release}" \
  >"${tmp}/bundle/release-receipt.json"
sha256sum "${tmp}/bundle/jeryu" | awk '{print $1 "  jeryu"}' >"${tmp}/bundle/SHA256SUMS"

ops/deploy/sign-and-push-atomicsoul.sh \
  --env "${env_dir}/production.env" \
  --bundle "${tmp}/bundle" \
  --release "${release}" \
  --dry-run >/dev/null

test -s "${tmp}/bundle/atomicsoul-deploy/SHA256SUMS"
test -s "${tmp}/bundle/atomicsoul-deploy/SHA256SUMS.sig"
test -s "${tmp}/bundle/atomicsoul-deploy/SHA256SUMS.pub.pem"
test -s "${tmp}/bundle/atomicsoul-deploy/deploy-manifest.json"

(
  cd "${tmp}/bundle"
  openssl pkeyutl -verify -rawin -pubin \
    -inkey atomicsoul-deploy/SHA256SUMS.pub.pem \
    -sigfile atomicsoul-deploy/SHA256SUMS.sig \
    -in atomicsoul-deploy/SHA256SUMS >/dev/null
  sha256sum -c atomicsoul-deploy/SHA256SUMS >/dev/null
)

jq -e \
  --arg release "${release}" \
  '.schema == "jeryu.atomicsoul.deploy-manifest/v1" and .release == $release' \
  "${tmp}/bundle/atomicsoul-deploy/deploy-manifest.json" >/dev/null

printf 'atomicsoul deploy helper smoke ok\n'
