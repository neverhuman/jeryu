//! REST endpoints for the JeRyu Web Forge BFF.
//!
//! Phase 1 shipped `/api/v1/bootstrap`. Phase 2 (W-B-06/07/09/10) adds:
//!   - `repos.rs`        list / preview / create / get / patch repo
//!   - `settings.rs`     get / preview / patch repo settings
//!   - `repo_browser.rs` refs / tree / blob / raw / readme / compare / commits / blame
//!   - `markdown.rs`     `POST /api/v1/markdown/render` (§35.1.8)
//!
//! Phase 4/5 (W-B-14..17 + W-B-30/31) adds:
//!   - `ci.rs`           pipelines / jobs / checks / log / retry / cancel
//!   - `actions.rs`      generic action preview/execute
//!   - `search.rs`       global search across repo/file/commit/mr/issue/user
//!   - `activity.rs`     activity feed (rolling 500-event buffer)
//!   - `issues.rs`       v1.5 stubs returning 501
//!   - `agents.rs`       agent sessions / evidence (read-only)
//!
//! Each submodule houses its route handlers; the router lives in
//! [`super::router`](super::router::build_web_router).

pub mod activity;
pub mod actions;
pub mod agents;
pub mod bootstrap;
pub mod ci;
pub mod issues;
pub mod markdown;
pub mod merge_requests;
pub mod repo_browser;
pub mod repos;
pub mod reviews;
pub mod search;
pub mod settings;
