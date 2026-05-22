use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{info, warn};

pub(crate) async fn print_cmd(label: &str, cmd: &mut Command) -> Result<()> {
    println!("\n== {} ==", label);
    let output = cmd.output().await?;
    if output.status.success() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    } else {
        warn!(
            "{} failed: {}",
            label,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) async fn run_docker_prune(args: &[&str]) -> Result<()> {
    info!("Running: docker {}", args.join(" "));
    let output = Command::new("docker").args(args).output().await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("Total reclaimed space") || line.contains("Deleted") {
                println!("{}", line);
            }
        }
    } else {
        warn!(
            "Failed to run docker {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) async fn truncate_gitlab_logs() -> Result<()> {
    let logs_dir = crate::config::gitlab_logs_dir();
    let script = format!(
        r#"
set -eu
find "{logs}" -type f \( -name '@*' -o -name '*.gz' \) -exec rm -f {{}} + || true
find "{logs}" -type f -name current -exec sh -c ': > "$1"' _ {{}} \; || true
find "{logs}/gitlab-rails" -type f \( -name '*_json.log' -o -name '*_client.log' \) -exec sh -c ': > "$1"' _ {{}} \; || true
"#,
        logs = logs_dir.display()
    );
    let output = Command::new("sh")
        .arg("-lc")
        .arg(script)
        .output()
        .await
        .context("truncating gitlab logs")?;
    if !output.status.success() {
        warn!(
            "gitlab log truncation warning: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) async fn truncate_docker_json_logs() -> Result<()> {
    let script = r#"
set -eu
for cid in $(docker ps -aq --filter name=jeryu-gitlab --filter label=jeryu.managed=true); do
  log_path=$(docker inspect --format '{{.LogPath}}' "$cid" 2>/dev/null || true)
  if [ -n "$log_path" ] && [ -f "$log_path" ]; then
    : > "$log_path" || true
  fi
done
"#;
    let output = Command::new("sh")
        .arg("-lc")
        .arg(script)
        .output()
        .await
        .context("truncating docker json logs")?;
    if !output.status.success() {
        warn!(
            "docker json log truncation warning: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
