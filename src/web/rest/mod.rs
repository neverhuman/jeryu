//! REST endpoints for the JeRyu Web Forge BFF.
//!
//! Phase 1 shipped `/api/v1/bootstrap`. Phase 2 (W-B-06/07/09/10) adds:
//!   - `repos.rs`        list / preview / create / get / patch repo
//!   - `settings.rs`     get / preview / patch repo settings
//!   - `repo_browser.rs` refs / tree / blob / raw / readme / compare / commits / blame
//!   - `markdown.rs`     `POST /api/v1/markdown/render` (§35.1.8)
//!
//! Each submodule houses its route handlers; the router lives in
//! [`super::router`](super::router::build_web_router).

pub mod bootstrap;
pub mod markdown;
pub mod merge_requests;
pub mod repo_browser;
pub mod repos;
pub mod reviews;
pub mod settings;
