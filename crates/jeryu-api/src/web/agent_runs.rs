mod errors;
mod export;
mod git;
mod lifecycle;
mod preflight;
mod source;
mod state;
mod types;

pub(in crate::web) use export::export_pr;
pub(in crate::web) use lifecycle::{control, start, status};
pub(crate) use state::AgentRunManager;

use axum::response::Response as AxumResponse;
use std::time::{SystemTime, UNIX_EPOCH};

const EMPTY_TREE_SHA: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

type AgentRunResult<T> = Result<T, Box<AxumResponse>>;

fn boxed_response(response: AxumResponse) -> Box<AxumResponse> {
    Box::new(response)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::source::normalize_pr_base;

    #[test]
    fn normalize_pr_base_strips_heads_and_origin() {
        assert_eq!(normalize_pr_base("refs/heads/main".into()), "main");
        assert_eq!(normalize_pr_base("origin/main".into()), "main");
        assert_eq!(normalize_pr_base("feature/x".into()), "feature/x");
    }
}
