use super::*;

pub async fn release_preflight(ssh_host: Option<&str>) -> PreflightReport {
    let mut blockers = Vec::new();
    let mut checks = std::collections::HashMap::new();
    let target = ssh_host.unwrap_or("atomicsoul");

    // SSH check
    let ssh_ok = tokio::process::Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=no",
            target,
            "echo",
            "ci-preflight-ok",
        ])
        .output()
        .await
        .map(|o| {
            o.status.success() && String::from_utf8_lossy(&o.stdout).contains("ci-preflight-ok")
        })
        .unwrap_or(false);
    checks.insert(
        "ssh".to_string(),
        if ssh_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !ssh_ok {
        blockers.push(PreflightBlocker {
            code: "SSH_UNREACHABLE".to_string(),
            component: "canary-target".to_string(),
            detail: format!("SSH to {target} failed (ConnectTimeout=5)"),
            recommended_action: format!(
                "verify {target} is powered on and reachable from this host"
            ),
        });
    }

    // Vault check
    let vault_port = crate::config::VAULT_HTTP_PORT;
    let vault_url = format!("http://127.0.0.1:{vault_port}/v1/sys/health");
    let vault_ok = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(client) => client
            .get(&vault_url)
            .send()
            .await
            .map(|r| r.status().as_u16() < 500)
            .unwrap_or(false),
        Err(_) => false,
    };
    checks.insert(
        "vault".to_string(),
        if vault_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !vault_ok {
        blockers.push(PreflightBlocker {
            code: "VAULT_UNREACHABLE".to_string(),
            component: "vault".to_string(),
            detail: format!("Vault health check failed at {vault_url}"),
            recommended_action: "run: jeryu cache doctor; check vault container is running"
                .to_string(),
        });
    }

    // Registry check (TCP connect to local registry mirror)
    let registry_port = crate::settings::get().cache.registry_port;
    let registry_ok = tokio::net::TcpStream::connect(format!("127.0.0.1:{registry_port}"))
        .await
        .is_ok();
    checks.insert(
        "registry".to_string(),
        if registry_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !registry_ok {
        blockers.push(PreflightBlocker {
            code: "REGISTRY_UNREACHABLE".to_string(),
            component: "registry-mirror".to_string(),
            detail: format!("registry mirror TCP connect to 127.0.0.1:{registry_port} failed"),
            recommended_action: "run: jeryu serve (starts registry mirror)".to_string(),
        });
    }

    // Disk check
    const DISK_EMERGENCY_FREE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
    const DISK_CRITICAL_FREE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
    const DISK_WARNING_FREE_BYTES: u64 = 75 * 1024 * 1024 * 1024;
    let disk_status = match crate::cache::df_usage("/").await {
        Ok(usage) => {
            if usage.available_bytes < DISK_EMERGENCY_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "emergency ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                blockers.push(PreflightBlocker {
                    code: "DISK_EMERGENCY".to_string(),
                    component: "host-disk".to_string(),
                    detail: format!(
                        "root disk only has {} free",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                    recommended_action: "run: jeryu cache status --json; then jeryu cache gc --json --keep-active-managers=false --max-cache-gb 20".to_string(),
                });
                false
            } else if usage.available_bytes < DISK_CRITICAL_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "critical ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                blockers.push(PreflightBlocker {
                    code: "DISK_CRITICAL".to_string(),
                    component: "host-disk".to_string(),
                    detail: format!(
                        "root disk only has {} free",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                    recommended_action: "run: jeryu cache status --json; then jeryu cache gc --dry-run --json --older-than 12h --max-cache-gb 20".to_string(),
                });
                false
            } else if usage.available_bytes < DISK_WARNING_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "warning ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                true
            } else {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "ok ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                true
            }
        }
        Err(_) => {
            checks.insert("disk".to_string(), "unknown".to_string());
            true
        }
    };
    let _ = disk_status; // disk warning doesn't block

    PreflightReport {
        ok: blockers.is_empty(),
        blockers,
        checks,
        generated_at: chrono::Utc::now().to_rfc3339(),
    }
}
