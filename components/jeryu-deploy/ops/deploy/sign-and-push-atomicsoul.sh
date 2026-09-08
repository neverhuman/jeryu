#!/usr/bin/env bash
# Sign a release bundle checksum manifest and push/install artifacts on atomicsoul.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

env_file="${JERYU_PRODUCTION_ENV:-}"
bundle="target/release/bundle"
release="${JERYU_DEPLOY_RELEASE:-${JERYU_RELEASE_TAG:-}}"
host=""
web_dist="apps/web/dist"
split_manifest="repos.manifest.toml"
dry_run=0
force=0
restart=0

usage() {
  cat <<'USAGE'
usage: sign-and-push-atomicsoul.sh --env production.env [options]

Options:
  --bundle DIR          release bundle directory (default: target/release/bundle)
  --release RELEASE     release id, defaults to env JERYU_DEPLOY_RELEASE
  --host SSH_HOST       SSH host, defaults to env JERYU_ATOMICSOUL_HOST or atomicsoul
  --web-dist DIR        web dist to install on atomicsoul (default: apps/web/dist)
  --split-manifest TOML split manifest to install (default: repos.manifest.toml)
  --dry-run             sign and validate locally, but do not contact atomicsoul
  --force               replace an existing remote release directory
  --restart             run systemctl --user enable --now jeryu.service after install

The script expects a production.env from make-production-env.sh. It signs
bundle/atomicsoul-deploy/SHA256SUMS with the per-release Ed25519 key, pushes
the bundle/web/manifest/runtime env to atomicsoul, verifies checksums and the
signature remotely, and updates ~/.jeryu/bin and ~/.jeryu/share symlinks.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env)
      shift
      env_file="${1:-}"
      ;;
    --bundle)
      shift
      bundle="${1:-}"
      ;;
    --release)
      shift
      release="${1:-}"
      ;;
    --host)
      shift
      host="${1:-}"
      ;;
    --web-dist)
      shift
      web_dist="${1:-}"
      ;;
    --split-manifest)
      shift
      split_manifest="${1:-}"
      ;;
    --dry-run)
      dry_run=1
      ;;
    --force)
      force=1
      ;;
    --restart)
      restart=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'sign-and-push-atomicsoul: unknown arg: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

need() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'sign-and-push-atomicsoul: missing required command: %s\n' "$1" >&2
    exit 1
  }
}

need openssl
need sha256sum
need jq
need rsync
need ssh

[[ -n "${env_file}" ]] || {
  printf 'sign-and-push-atomicsoul: --env is required\n' >&2
  exit 2
}
[[ -f "${env_file}" ]] || {
  printf 'sign-and-push-atomicsoul: env file not found: %s\n' "${env_file}" >&2
  exit 1
}

set -a
# shellcheck source=/dev/null
. "${env_file}"
set +a

release="${release:-${JERYU_DEPLOY_RELEASE:-${JERYU_RELEASE_TAG:-}}}"
host="${host:-${JERYU_ATOMICSOUL_HOST:-atomicsoul}}"

[[ -n "${release}" ]] || {
  printf 'sign-and-push-atomicsoul: release is required; pass --release or set JERYU_DEPLOY_RELEASE\n' >&2
  exit 2
}
if [[ ! "${release}" =~ ^[A-Za-z0-9._-]+$ ]]; then
  printf 'sign-and-push-atomicsoul: release must contain only letters, digits, dot, underscore, and dash: %s\n' "${release}" >&2
  exit 2
fi

: "${JERYU_BOOTSTRAP_ADMIN_PASSWORD:?production env missing JERYU_BOOTSTRAP_ADMIN_PASSWORD}"
: "${JERYU_SIGNRAIL_ED25519_SEED:?production env missing JERYU_SIGNRAIL_ED25519_SEED}"
: "${JERYU_DEPLOY_SIGNING_KEY:?production env missing JERYU_DEPLOY_SIGNING_KEY}"

[[ -d "${bundle}" ]] || {
  printf 'sign-and-push-atomicsoul: bundle directory not found: %s\n' "${bundle}" >&2
  exit 1
}
[[ -x "${bundle}/jeryu" ]] || {
  printf 'sign-and-push-atomicsoul: executable bundle artifact missing: %s/jeryu\n' "${bundle}" >&2
  exit 1
}
[[ -f "${web_dist}/index.html" ]] || {
  printf 'sign-and-push-atomicsoul: web dist index missing: %s/index.html\n' "${web_dist}" >&2
  exit 1
}
[[ -f "${split_manifest}" ]] || {
  printf 'sign-and-push-atomicsoul: split manifest missing: %s\n' "${split_manifest}" >&2
  exit 1
}
[[ -f "${JERYU_DEPLOY_SIGNING_KEY}" ]] || {
  printf 'sign-and-push-atomicsoul: signing key not found: %s\n' "${JERYU_DEPLOY_SIGNING_KEY}" >&2
  exit 1
}

release_root="${JERYU_ATOMICSOUL_RELEASE_ROOT:-/home/ubuntu/.jeryu/releases}"
release_dir="${JERYU_ATOMICSOUL_RELEASE_DIR:-${release_root%/}/${release}}"
serve_bind="${JERYU_SERVE_BIND:-127.0.0.1:8787}"
data_dir="${JERYU_DATA_DIR:-/home/ubuntu/.local/share/jeryu}"
spa_dir="${JERYU_SPA_DIR:-/home/ubuntu/.jeryu/share/web-dist}"
remote_manifest="${JERYU_SPLIT_MANIFEST:-/home/ubuntu/.jeryu/share/repos.manifest.toml}"

case "${release_dir}" in
  /home/ubuntu/.jeryu/releases/*) ;;
  *)
    printf 'sign-and-push-atomicsoul: refusing unsafe remote release dir: %s\n' "${release_dir}" >&2
    exit 1
    ;;
esac

evidence_dir="${bundle}/atomicsoul-deploy"
rm -rf "${evidence_dir}"
install -d -m 0755 "${evidence_dir}"

(
  cd "${bundle}"
  find . -type f \
    ! -path './atomicsoul-deploy/*' \
    -printf '%P\0' \
    | LC_ALL=C sort -z \
    | xargs -0 -r sha256sum
) >"${evidence_dir}/SHA256SUMS"

openssl pkey -in "${JERYU_DEPLOY_SIGNING_KEY}" -pubout \
  -out "${evidence_dir}/SHA256SUMS.pub.pem" >/dev/null 2>&1
openssl pkeyutl -sign -rawin \
  -inkey "${JERYU_DEPLOY_SIGNING_KEY}" \
  -in "${evidence_dir}/SHA256SUMS" \
  -out "${evidence_dir}/SHA256SUMS.sig"
openssl pkeyutl -verify -rawin -pubin \
  -inkey "${evidence_dir}/SHA256SUMS.pub.pem" \
  -sigfile "${evidence_dir}/SHA256SUMS.sig" \
  -in "${evidence_dir}/SHA256SUMS" >/dev/null

(
  cd "${bundle}"
  sha256sum -c atomicsoul-deploy/SHA256SUMS >/dev/null
)

commit=""
if [[ -f "${bundle}/release-receipt.json" ]]; then
  commit="$(jq -r '.commit // ""' "${bundle}/release-receipt.json")"
fi
if [[ -z "${commit}" ]]; then
  commit="$(git rev-parse --verify HEAD 2>/dev/null || printf '')"
fi

manifest_sha="$(sha256sum "${evidence_dir}/SHA256SUMS" | awk '{print $1}')"
signature_sha="$(sha256sum "${evidence_dir}/SHA256SUMS.sig" | awk '{print $1}')"
pubkey_sha="$(sha256sum "${evidence_dir}/SHA256SUMS.pub.pem" | awk '{print $1}')"
binary_sha="$(awk '$2 == "jeryu" {print $1; found=1} END {if (!found) exit 1}' "${evidence_dir}/SHA256SUMS")"
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

jq -n \
  --arg schema "jeryu.atomicsoul.deploy-manifest/v1" \
  --arg release "${release}" \
  --arg commit "${commit}" \
  --arg generated_at "${generated_at}" \
  --arg host "${host}" \
  --arg release_dir "${release_dir}" \
  --arg binary_sha "${binary_sha}" \
  --arg manifest_sha "${manifest_sha}" \
  --arg signature_sha "${signature_sha}" \
  --arg pubkey_sha "${pubkey_sha}" \
  '{
    schema: $schema,
    release: $release,
    commit: $commit,
    generated_at: $generated_at,
    target: {host: $host, release_dir: $release_dir},
    artifacts: {
      jeryu: {sha256: $binary_sha},
      "atomicsoul-deploy/SHA256SUMS": {sha256: $manifest_sha},
      "atomicsoul-deploy/SHA256SUMS.sig": {sha256: $signature_sha},
      "atomicsoul-deploy/SHA256SUMS.pub.pem": {sha256: $pubkey_sha}
    },
    verifier: {
      command: "openssl pkeyutl -verify -rawin -pubin -inkey atomicsoul-deploy/SHA256SUMS.pub.pem -sigfile atomicsoul-deploy/SHA256SUMS.sig -in atomicsoul-deploy/SHA256SUMS"
    }
  }' >"${evidence_dir}/deploy-manifest.json"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
runtime_env="${tmp}/production.env"
bootstrap_password="${tmp}/bootstrap-admin-password"
unit_file="${tmp}/jeryu.service"

write_systemd_env() {
  local key="$1" value="$2"
  printf '%s=%s\n' "${key}" "${value}"
}

umask 077
{
  printf '# Runtime env installed by ops/deploy/sign-and-push-atomicsoul.sh. Do not commit.\n'
  write_systemd_env JERYU_DEPLOY_RELEASE "${release}"
  write_systemd_env JERYU_PRODUCTION_DOMAIN "${JERYU_PRODUCTION_DOMAIN:-git.neverhuman.org}"
  write_systemd_env JERYU_PRODUCTION_ORIGIN "${JERYU_PRODUCTION_ORIGIN:-https://git.neverhuman.org}"
  write_systemd_env JERYU_BOOTSTRAP_ADMIN_PASSWORD "${JERYU_BOOTSTRAP_ADMIN_PASSWORD}"
  write_systemd_env JERYU_SERVE_BIND "${serve_bind}"
  write_systemd_env JERYU_DATA_DIR "${data_dir}"
  write_systemd_env JERYU_SPA_DIR "${spa_dir}"
  write_systemd_env JERYU_SPLIT_MANIFEST "${remote_manifest}"
} >"${runtime_env}"
printf '%s\n' "${JERYU_BOOTSTRAP_ADMIN_PASSWORD}" >"${bootstrap_password}"
chmod 0600 "${runtime_env}" "${bootstrap_password}"

cat >"${unit_file}" <<'EOF'
[Unit]
Description=Jeryu web and Git forge
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=%h/.jeryu/secrets/production.env
ExecStart=%h/.jeryu/bin/jeryu serve --bind ${JERYU_SERVE_BIND} --data-dir ${JERYU_DATA_DIR} --spa-dir ${JERYU_SPA_DIR} --split-manifest ${JERYU_SPLIT_MANIFEST}
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=default.target
EOF
chmod 0644 "${unit_file}"

if [[ "${dry_run}" == "1" ]]; then
  printf '[sign-and-push-atomicsoul] dry-run: signed %s\n' "${evidence_dir}/SHA256SUMS"
  printf '[sign-and-push-atomicsoul] dry-run: bundle checksums and Ed25519 signature verified locally\n'
  printf '[sign-and-push-atomicsoul] dry-run: would push to %s:%s\n' "${host}" "${release_dir}"
  exit 0
fi

staging="/home/ubuntu/.jeryu/incoming/${release}.$$"
case "${staging}" in
  /home/ubuntu/.jeryu/incoming/*) ;;
  *)
    printf 'sign-and-push-atomicsoul: refusing unsafe staging dir: %s\n' "${staging}" >&2
    exit 1
    ;;
esac

ssh "${host}" 'bash -s' -- "${staging}" "${release_dir}" "${force}" <<'REMOTE_PREP'
set -euo pipefail
staging="$1"
release_dir="$2"
force="$3"
case "$staging" in /home/ubuntu/.jeryu/incoming/*) ;; *) echo "unsafe staging dir: $staging" >&2; exit 1 ;; esac
case "$release_dir" in /home/ubuntu/.jeryu/releases/*) ;; *) echo "unsafe release dir: $release_dir" >&2; exit 1 ;; esac
mkdir -p "$(dirname "$release_dir")"
if [ -e "$release_dir" ] && [ "$force" != "1" ]; then
  echo "remote release directory already exists: $release_dir" >&2
  exit 1
fi
rm -rf "$staging"
mkdir -p "$staging/bundle" "$staging/web-dist" "$staging/secrets" "$staging/systemd"
REMOTE_PREP

rsync -a --delete "${bundle}/" "${host}:${staging}/bundle/"
rsync -a --delete "${web_dist}/" "${host}:${staging}/web-dist/"
rsync -a "${split_manifest}" "${host}:${staging}/repos.manifest.toml"
rsync -a --chmod=F600 "${runtime_env}" "${host}:${staging}/secrets/production.env"
rsync -a --chmod=F600 "${bootstrap_password}" "${host}:${staging}/secrets/bootstrap-admin-password"
rsync -a "${unit_file}" "${host}:${staging}/systemd/jeryu.service"

ssh "${host}" 'bash -s' -- "${staging}" "${release_dir}" "${release}" "${force}" "${restart}" <<'REMOTE_INSTALL'
set -euo pipefail
staging="$1"
release_dir="$2"
release="$3"
force="$4"
restart="$5"
home="${HOME:-/home/ubuntu}"

case "$staging" in /home/ubuntu/.jeryu/incoming/*) ;; *) echo "unsafe staging dir: $staging" >&2; exit 1 ;; esac
case "$release_dir" in /home/ubuntu/.jeryu/releases/*) ;; *) echo "unsafe release dir: $release_dir" >&2; exit 1 ;; esac

if [ -e "$release_dir" ]; then
  if [ "$force" = "1" ]; then
    rm -rf "$release_dir"
  else
    echo "remote release directory already exists: $release_dir" >&2
    exit 1
  fi
fi
mkdir -p "$(dirname "$release_dir")"
mv "$staging" "$release_dir"

cd "$release_dir/bundle"
openssl pkeyutl -verify -rawin -pubin \
  -inkey atomicsoul-deploy/SHA256SUMS.pub.pem \
  -sigfile atomicsoul-deploy/SHA256SUMS.sig \
  -in atomicsoul-deploy/SHA256SUMS >/dev/null
sha256sum -c atomicsoul-deploy/SHA256SUMS >/dev/null

mkdir -p "$home/.jeryu/bin" "$home/.jeryu/share" "$home/.jeryu/secrets" "$home/.config/systemd/user"
install -m 0755 "$release_dir/bundle/jeryu" "$home/.jeryu/bin/jeryu-$release"
ln -sfn "jeryu-$release" "$home/.jeryu/bin/jeryu"

rm -rf "$home/.jeryu/share/web-dist-$release"
cp -a "$release_dir/web-dist" "$home/.jeryu/share/web-dist-$release"
ln -sfn "web-dist-$release" "$home/.jeryu/share/web-dist"

install -m 0644 "$release_dir/repos.manifest.toml" "$home/.jeryu/share/repos.manifest-$release.toml"
ln -sfn "repos.manifest-$release.toml" "$home/.jeryu/share/repos.manifest.toml"

install -m 0600 "$release_dir/secrets/production.env" "$home/.jeryu/secrets/production.env"
install -m 0600 "$release_dir/secrets/bootstrap-admin-password" "$home/.jeryu/secrets/bootstrap-admin-password"
install -m 0644 "$release_dir/systemd/jeryu.service" "$home/.config/systemd/user/jeryu.service"

if command -v systemctl >/dev/null 2>&1; then
  systemctl --user daemon-reload >/dev/null 2>&1 || true
  if [ "$restart" = "1" ]; then
    systemctl --user enable --now jeryu.service
  fi
elif [ "$restart" = "1" ]; then
  echo "systemctl not available on remote host" >&2
  exit 1
fi

printf '[atomicsoul] installed release %s at %s\n' "$release" "$release_dir"
printf '[atomicsoul] active binary symlink: %s\n' "$(readlink "$home/.jeryu/bin/jeryu")"
REMOTE_INSTALL

printf '[sign-and-push-atomicsoul] pushed and verified %s on %s\n' "${release}" "${host}"
