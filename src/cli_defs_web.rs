//! CLI definitions for `jeryu web` — JeRyu Web Forge BFF.
//!
//! Phase-0 surface only: the subcommands here parse and dispatch, but the
//! real Axum + WebSocket + SPA static-asset binding lands in W-B-01..04
//! per `WEB_WORK_CLAUDE.md` §7.0 and §35.1.5. The legacy engine routes
//! (`/health`, `/hooks`, `/cache/summary`) are preserved by W-B-02 when
//! the real server materializes.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum WebCommand {
    /// Start the JeRyu Web Forge BFF server (Phase-0 stub).
    ///
    /// The real server binds an Axum router that merges the new
    /// `/api/v1/*` and `/api/ws` paths with the engine's legacy
    /// `/health`, `/hooks`, and `/cache/summary` routes. Until W-B-01..04
    /// land, this command prints the configuration it would use and
    /// exits 0.
    Serve {
        /// Bind address (host:port).
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: String,
        /// Open the SPA in the default browser after the server is ready.
        #[arg(long, default_value_t = false)]
        open: bool,
        /// In dev mode, reverse-proxy unmatched paths to a Vite dev server.
        #[arg(long)]
        dev_assets: Option<String>,
        /// Path to the built SPA `dist/` directory (prod mode).
        #[arg(long, default_value = "apps/web/dist")]
        spa_dir: String,
    },
    /// Print the running web server URL (Phase-0 stub).
    Open,
    /// Build the SPA assets — delegates to `npm run build` in `apps/web/`
    /// (Phase-0 stub).
    BuildAssets,
}
