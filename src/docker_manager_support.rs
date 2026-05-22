use super::*;

pub(super) fn runner_bootstrap_cmd_docker() -> String {
    let ver = &crate::settings::get().sccache.binary_version;
    format!(
        r#"
set -eu
if ! command -v sccache >/dev/null 2>&1; then
  curl -fsSL https://github.com/mozilla/sccache/releases/download/{ver}/sccache-{ver}-x86_64-unknown-linux-musl.tar.gz \
    | tar -xz --strip-components=1 -C /usr/local/bin sccache-{ver}-x86_64-unknown-linux-musl/sccache 2>/dev/null || true
fi
exec gitlab-runner run
"#
    )
}

pub(super) fn runner_bootstrap_cmd_custom() -> String {
    let ver = &crate::settings::get().sccache.binary_version;
    format!(
        r#"
set -eu
cat >/usr/sbin/policy-rc.d <<'EOF'
#!/bin/sh
exit 101
EOF
chmod +x /usr/sbin/policy-rc.d
if ! command -v docker >/dev/null 2>&1; then
  if command -v apk >/dev/null 2>&1; then
    apk add --no-cache docker-cli >/dev/null
  elif command -v apt-get >/dev/null 2>&1; then
    apt-get -qq update>/dev/null
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends docker.io >/dev/null
  fi
fi
if command -v docker >/dev/null 2>&1; then
  ln -sf "$(command -v docker)" /usr/local/bin/docker || true
fi
for _ in 1 2 3 4 5; do
  [ -S /var/run/docker.sock ] && break
  sleep 1
done
[ -S /var/run/docker.sock ] || {{
  echo "jeryu custom executor bootstrap: docker socket is missing" >&2
  rm -f /usr/sbin/policy-rc.d
  exit 1
}}
for _ in 1 2 3 4 5; do
  docker info >/dev/null 2>&1 && break
  sleep 1
done
docker info >/dev/null 2>&1 || {{
  echo "jeryu custom executor bootstrap: docker info failed against mounted socket" >&2
  rm -f /usr/sbin/policy-rc.d
  exit 1
}}
rm -f /usr/sbin/policy-rc.d
if ! command -v sccache >/dev/null 2>&1; then
  curl -fsSL https://github.com/mozilla/sccache/releases/download/{ver}/sccache-{ver}-x86_64-unknown-linux-musl.tar.gz \
    | tar -xz --strip-components=1 -C /usr/local/bin sccache-{ver}-x86_64-unknown-linux-musl/sccache 2>/dev/null || true
fi
exec gitlab-runner run
"#
    )
}

pub(super) fn current_exe_mount_source(result: std::io::Result<PathBuf>) -> PathBuf {
    match result {
        Ok(path) => path,
        Err(_) => PathBuf::from("/usr/local/bin/jeryu"),
    }
}

pub(super) async fn compose_up(_docker: &DockerCtl) -> Result<()> {
    let data_dir = config::data_dir();
    let output = tokio::process::Command::new("docker")
        .args(["compose", "up", "-d", "--no-deps"])
        .args(compose_up_targets())
        .current_dir(&data_dir)
        .output()
        .await
        .context("running docker compose up")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose up failed: {}", stderr);
    }
    info!("docker compose up completed");
    Ok(())
}

pub(super) async fn compose_up_service(_docker: &DockerCtl, service: &str) -> Result<()> {
    let data_dir = config::data_dir();
    let output = tokio::process::Command::new("docker")
        .args(["compose", "up", "-d", "--no-deps", service])
        .current_dir(&data_dir)
        .output()
        .await
        .with_context(|| format!("running docker compose up {service}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose up {service} failed: {}", stderr);
    }
    info!(service, "docker compose up service completed");
    Ok(())
}

pub(super) async fn compose_down(_docker: &DockerCtl) -> Result<()> {
    let data_dir = config::data_dir();
    let output = tokio::process::Command::new("docker")
        .args(["compose", "down"])
        .current_dir(&data_dir)
        .output()
        .await
        .context("running docker compose down")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("docker compose down warning: {}", stderr);
    }
    info!("docker compose down completed");
    Ok(())
}
