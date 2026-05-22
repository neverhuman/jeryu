use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum HostCommands {
    /// Perform a storage audit on the host.
    StorageAudit,
    /// Check host, GitLab, Docker, and runner-cache health.
    Doctor {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run an aggressive reclaim operation.
    Reclaim {
        #[arg(long)]
        mode: String,
        #[arg(long, default_value_t = false)]
        plan: bool,
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    /// Install the jeryu-gc systemd timer from ops/ci.
    ///
    /// Use this for manual re-install only. `jeryu bootstrap` now installs
    /// the always-on `jeryu-gcd.service` plus this timer as a deep-sweep
    /// safety net; prefer bootstrap for initial setup.
    InstallGcTimer {
        #[arg(long, default_value_t = false)]
        allow_sudo: bool,
    },
    /// Install the always-on `jeryu-gcd.service` (disk-pressure daemon).
    ///
    /// Maintains df ≥ 80 GiB free via pressure-tier GC. Auto-invoked by
    /// `jeryu bootstrap`; this command is for manual re-install or
    /// recovery.
    InstallGcdService {
        #[arg(long, default_value_t = false)]
        allow_sudo: bool,
    },
}
