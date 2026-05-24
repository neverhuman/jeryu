use super::*;

const MIN_GITLAB_ARTIFACT_SIZE_MB: u64 = 4096;

pub(crate) async fn gitlab_redis_write_check() -> HostDoctorCheck {
    let output = tokio::process::Command::new("docker")
        .args([
            "exec",
            "jeryu-gitlab",
            "sh",
            "-lc",
            "gitlab-redis-cli set jeryu:doctor:write ok EX 60 >/dev/null && gitlab-redis-cli get jeryu:doctor:write",
        ])
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => HostDoctorCheck {
            id: "gitlab-redis-write".to_string(),
            ok: String::from_utf8_lossy(&output.stdout).trim() == "ok",
            detail: "Redis accepts writes".to_string(),
        },
        Ok(output) => HostDoctorCheck {
            id: "gitlab-redis-write".to_string(),
            ok: false,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(err) => HostDoctorCheck {
            id: "gitlab-redis-write".to_string(),
            ok: false,
            detail: err.to_string(),
        },
    }
}

pub(crate) async fn gitlab_artifact_size_check() -> HostDoctorCheck {
    let url = format!("http://localhost:{}", crate::config::GITLAB_HTTP_PORT);
    let auth = crate::gitlab_auth::resolve_or_repair(&url).await;

    match auth {
        Ok(auth) => {
            let client = crate::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));
            let parsed = client
                .api_get_json::<serde_json::Value>(client.api_url("/application/settings"))
                .await;
            match parsed {
                Ok(json) => match json
                    .get("max_artifacts_size")
                    .and_then(|value| value.as_u64())
                {
                    Some(max_mb) => HostDoctorCheck {
                        id: "gitlab-artifact-size".to_string(),
                        ok: max_mb >= MIN_GITLAB_ARTIFACT_SIZE_MB,
                        detail: format!(
                            "max_artifacts_size={}MiB (required >= {}MiB)",
                            max_mb, MIN_GITLAB_ARTIFACT_SIZE_MB
                        ),
                    },
                    None => HostDoctorCheck {
                        id: "gitlab-artifact-size".to_string(),
                        ok: false,
                        detail: "could not parse max_artifacts_size".to_string(),
                    },
                },
                Err(err) => HostDoctorCheck {
                    id: "gitlab-artifact-size".to_string(),
                    ok: false,
                    detail: err.to_string(),
                },
            }
        }
        Err(err) => HostDoctorCheck {
            id: "gitlab-artifact-size".to_string(),
            ok: false,
            detail: format!("GitLab auth unavailable: {err}"),
        },
    }
}

pub(crate) fn parse_age(raw: &str) -> Result<Duration> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CacheError::EmptyAge.into());
    }
    let (num, unit) = trimmed.split_at(trimmed.len() - 1);
    let value = num
        .parse::<u64>()
        .with_context(|| format!("invalid age value: {raw}"))?;
    match unit {
        "m" | "M" => Ok(Duration::from_secs(value * 60)),
        "h" | "H" => Ok(Duration::from_secs(value * 60 * 60)),
        "d" | "D" => Ok(Duration::from_secs(value * 24 * 60 * 60)),
        _ => Err(CacheError::UnsupportedAge(raw.to_string()).into()),
    }
}

pub(crate) fn gb_to_bytes(gb: f64) -> u64 {
    (gb * 1024.0 * 1024.0 * 1024.0) as u64
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= GIB {
        format!("{:.1}GiB", bytes as f64 / GIB)
    } else if bytes as f64 >= MIB {
        format!("{:.1}MiB", bytes as f64 / MIB)
    } else {
        format!("{}B", bytes)
    }
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl CacheManager {
    pub async fn gc_disk_cache(&self) -> Result<()> {
        self.gc_disk_cache_with_pressure(false, false, false).await
    }

    pub async fn gc_disk_cache_with_pressure(
        &self,
        is_warning: bool,
        is_critical: bool,
        is_emergency: bool,
    ) -> Result<()> {
        let (older_than, max_cache_gb, keep_active) = if is_emergency {
            ("15m".to_string(), Some(20.0_f64), true)
        } else if is_critical {
            ("2h".to_string(), Some(60.0_f64), true)
        } else if is_warning {
            ("4h".to_string(), Some(120.0_f64), true)
        } else {
            ("12h".to_string(), None, true)
        };

        SmartCache::new(crate::state::Db::open().await?)
            .gc_with_options(GcOptions {
                keep_active_managers: keep_active,
                older_than: Some(older_than),
                max_cache_gb,
                quiet: true,
                ..GcOptions::default()
            })
            .await
            .map(|_| ())
    }

    pub async fn status(&self) -> Result<()> {
        SmartCache::new(crate::state::Db::open().await?)
            .status()
            .await
    }
}

#[cfg(test)]
#[path = "cache_reports_manager_tests.rs"]
mod tests;
