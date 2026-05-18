//! Owner: jeryu-gcd (always-on disk daemon)
//! Proof: `cargo nextest run -p jeryu-gcd`
//! Invariants: never deletes data at Nominal except scheduled sweep; sd_notify on every tick.
//!
//! Binary entry. The pure decision logic lives in `lib.rs::tick_action`.
//! This file wires the tick loop to systemd, df, the SmartCache GC machinery,
//! and structured logging.

use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use jeryu_gcd::{
    DEFAULT_INTERVAL_SECS, DEFAULT_NOMINAL_SWEEP_INTERVAL, DEFAULT_TARGET_FREE_BYTES, TickAction,
    tick_action,
};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "jeryu-gcd",
    version,
    about = "Always-on disk-pressure daemon for JeRyu"
)]
struct Cli {
    /// Loop interval in seconds. 0 means run one tick and exit (smoke mode).
    #[arg(long, env = "JERYU_GCD_INTERVAL_SECS", default_value_t = DEFAULT_INTERVAL_SECS)]
    interval_secs: u64,
    /// Target free bytes (slightly above the 80 GiB headroom floor).
    #[arg(long, env = "JERYU_GCD_TARGET_FREE_BYTES", default_value_t = DEFAULT_TARGET_FREE_BYTES)]
    target_free_bytes: u64,
    /// Run a single tick and exit. Used by `jeryu-gcd --oneshot` smoke tests.
    #[arg(long, default_value_t = false)]
    oneshot: bool,
    /// Path passed to df. Defaults to root.
    #[arg(long, default_value = "/")]
    fs_path: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let log_filter = match tracing_subscriber::EnvFilter::try_from_default_env() {
        Ok(f) => f,
        Err(_) => tracing_subscriber::EnvFilter::new("info"),
    };
    tracing_subscriber::fmt().with_env_filter(log_filter).init();

    let cli = Cli::parse();
    info!(
        interval_secs = cli.interval_secs,
        target_free_bytes = cli.target_free_bytes,
        fs_path = %cli.fs_path,
        oneshot = cli.oneshot,
        "jeryu-gcd starting"
    );

    sd_notify_ready();

    let mut last_nominal_sweep = match Instant::now().checked_sub(DEFAULT_NOMINAL_SWEEP_INTERVAL) {
        Some(t) => t,
        None => Instant::now(),
    };

    loop {
        if let Err(e) = run_tick(&cli, &mut last_nominal_sweep).await {
            error!(error = %e, "tick failed; will retry next interval");
        }
        sd_notify_watchdog();

        if cli.oneshot || cli.interval_secs == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_secs(cli.interval_secs)).await;
    }
    Ok(())
}

async fn run_tick(cli: &Cli, last_nominal_sweep: &mut Instant) -> Result<()> {
    let usage = jeryu::cache::df_usage(&cli.fs_path).await?;
    let nominal_sweep_due = last_nominal_sweep.elapsed() >= DEFAULT_NOMINAL_SWEEP_INTERVAL;
    let action = tick_action(usage.available_bytes, nominal_sweep_due);

    info!(
        action = action.label(),
        available_gib = usage.available_bytes / (1024 * 1024 * 1024),
        total_gib = usage.total_bytes / (1024 * 1024 * 1024),
        used_pct = usage.used_percent,
        "tick"
    );

    let manager = jeryu::cache::CacheManager;

    match action {
        TickAction::Idle => {}
        TickAction::NominalSweep => {
            manager
                .gc_disk_cache_with_pressure(false, false, false)
                .await?;
            *last_nominal_sweep = Instant::now();
        }
        TickAction::WarningGc => {
            manager
                .gc_disk_cache_with_pressure(true, false, false)
                .await?;
            // At Warning we already sweep aged incremental caches — the
            // 80 GiB headroom is what unblocks runner fanout, so we must
            // actively defend it.
            if let Err(e) = jeryu::cache::sweep_incremental_caches(
                jeryu::cache::root_disk_pressure_level(usage.available_bytes),
            )
            .await
            {
                warn!(error = %e, "sweep_incremental_caches (Warning) failed");
            }
        }
        TickAction::CriticalGc => {
            manager
                .gc_disk_cache_with_pressure(false, true, false)
                .await?;
            run_aux("orphan_workers", jeryu::reclaim::gc_orphaned_workers()).await;
            if let Err(e) = jeryu::cache::sweep_incremental_caches(
                jeryu::cache::root_disk_pressure_level(usage.available_bytes),
            )
            .await
            {
                warn!(error = %e, "sweep_incremental_caches (Critical) failed");
            }
        }
        TickAction::EmergencyGc => {
            manager
                .gc_disk_cache_with_pressure(false, false, true)
                .await?;
            if let Err(e) = jeryu::cache::sweep_incremental_caches(
                jeryu::cache::root_disk_pressure_level(usage.available_bytes),
            )
            .await
            {
                warn!(error = %e, "sweep_incremental_caches failed");
            }
        }
    }

    Ok(())
}

async fn run_aux<F: std::future::Future<Output = u64>>(name: &str, fut: F) {
    let killed = fut.await;
    if killed > 0 {
        info!(name, killed, "aux cleanup");
    }
}

/// Notify systemd we're ready. No-op when not running under systemd
/// (`$NOTIFY_SOCKET` is unset).
fn sd_notify_ready() {
    let _ = sd_notify("READY=1\nSTATUS=jeryu-gcd ready\n");
}

/// Keep the systemd watchdog happy. No-op without `$NOTIFY_SOCKET`.
fn sd_notify_watchdog() {
    let _ = sd_notify("WATCHDOG=1\n");
}

fn sd_notify(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::net::UnixDatagram;
    let socket = match std::env::var("NOTIFY_SOCKET") {
        Ok(s) if !s.is_empty() => s,
        _ => return Ok(()),
    };
    let sock = UnixDatagram::unbound()?;
    let bytes = msg.as_bytes();
    // Abstract namespace sockets start with `@` and must be sent as `\0`-prefixed.
    match sock.send_to(bytes, &socket) {
        Ok(_) => {}
        Err(_) => {
            if let Some(stripped) = socket.strip_prefix('@') {
                let mut path = vec![0u8];
                path.extend(stripped.as_bytes());
                let addr = std::str::from_utf8(&path).unwrap_or("/dev/null");
                let _ = sock.send_to(bytes, addr);
            }
        }
    }
    std::io::stderr().flush().ok();
    Ok(())
}
