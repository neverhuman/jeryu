//! Owner: CI failure classification helpers
//! Proof: `cargo test -p jeryu --lib ci_failure decision`
//! Invariants: GitLab infrastructure failures are classified before agents blame repo code.

pub const SOURCE_FETCH_AUTH_FAILURE_KIND: &str = "source_fetch_auth";

pub fn is_source_fetch_auth_failure(log: &str) -> bool {
    let lower = log.to_ascii_lowercase();
    let source_fetch_context = lower.contains("getting source from git repository")
        || lower.contains("get_sources")
        || lower.contains("fetching changes")
        || lower.contains("checking out")
        || lower.contains("preparing repository");
    let auth_failure = lower.contains("http basic: access denied")
        || lower.contains("authentication failed")
        || lower.contains("could not read username")
        || lower.contains("repository not found");

    source_fetch_context && auth_failure
}

pub fn source_fetch_auth_incident_summary() -> &'static str {
    "GitLab source checkout failed before user code ran; check runner/job-token source auth and retry after infrastructure repair"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gitlab_source_fetch_http_basic_denial() {
        let log = "Getting source from Git repository\nremote: HTTP Basic: Access denied\nfatal: Authentication failed";

        assert!(is_source_fetch_auth_failure(log));
    }

    #[test]
    fn ignores_application_auth_failures_without_source_context() {
        let log = "test auth_login_rejects_bad_password ... FAILED\nHTTP Basic: Access denied";

        assert!(!is_source_fetch_auth_failure(log));
    }
}
