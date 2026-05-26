//! `jeryu web serve` command entry point.
//!
//! Phase-0 stub. The real BFF binds Axum + WebSocket + SPA static assets
//! and lands incrementally in W-B-01..04 per `WEB_WORK_CLAUDE.md` §7.0.
//! Until then this stub prints what would happen and exits 0, so
//! downstream work-packages can wire dispatch immediately.
//!
//! Per `WEB_WORK_CLAUDE.md` §35.1.5, the eventual real server preserves
//! the legacy engine routes (`/health`, `/hooks`, `/cache/summary`) by
//! merging them into the new `/api/v1/*` + `/api/ws` router. W-B-02 owns
//! that router merge.
//!
//! This file deliberately depends only on `anyhow` plus the typed CLI
//! enum — the heavy server crates (axum, tower, tower-http) land in
//! W-B-01..04 behind `#[cfg(feature = "web")]` blocks.

use anyhow::Result;

use crate::cli::WebCommand;

pub(crate) async fn run(cmd: WebCommand) -> Result<()> {
    match cmd {
        WebCommand::Serve {
            bind,
            open,
            dev_assets,
            spa_dir,
        } => {
            println!("jeryu web serve (stub)");
            println!("  bind:        {bind}");
            println!("  spa-dir:     {spa_dir}");
            if let Some(url) = dev_assets {
                println!("  dev-assets:  {url}  (reverse-proxy mode)");
            }
            if open {
                println!("  --open: would launch the default browser when ready");
            }
            println!();
            println!(
                "The full BFF binding lands in W-B-01..04 per WEB_WORK_CLAUDE.md"
            );
            println!("(phase 1). This stub confirms CLI dispatch is wired.");
            println!(
                "Legacy routes /health, /hooks, /cache/summary are preserved by"
            );
            println!("W-B-02 per WEB_WORK_CLAUDE.md §35.1.5.");
            Ok(())
        }
        WebCommand::Open => {
            println!("jeryu web open (stub): would print the bound URL");
            Ok(())
        }
        WebCommand::BuildAssets => {
            println!(
                "jeryu web build-assets (stub): delegates to `npm --workspace @jeryu/web run build`"
            );
            Ok(())
        }
    }
}
