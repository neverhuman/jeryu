//! Adapters for `jeryu ci {run,status,explain}`.

use std::{
    collections::HashSet,
    io::Write,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::cli::CiCommands;
use crate::client::{ClientError, ClientResult, ForgeClient};
use crate::commands::{api::ApiClient, render};

pub(crate) fn run(
    client: &dyn ForgeClient,
    api_url: Option<&str>,
    owner: &str,
    json: bool,
    cmd: CiCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match cmd {
        CiCommands::Run {
            repo,
            git_ref,
            kind,
        } => {
            let run = client.ci_run(&repo, &git_ref, kind.into())?;
            render(
                out,
                json,
                &run,
                &format!(
                    "scheduled ci run {} for {}@{} ({} jobs)",
                    run.id, run.repo, run.git_ref, run.jobs
                ),
            )
        }
        CiCommands::Status { repo } => {
            if let Some(api_url) = api_url {
                return live_status(api_url, owner, &repo, json, out);
            }
            let runs = client.ci_status(&repo)?;
            let human = runs
                .iter()
                .map(|r| format!("{} {}@{} [{:?}]", r.id, r.repo, r.git_ref, r.status))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &runs, &human)
        }
        CiCommands::Explain { run_id } => {
            let explanation = client.ci_explain(&run_id)?;
            let human = format!(
                "run {} blocked={}: {}",
                explanation.run_id,
                explanation.blocked,
                explanation.reasons.join("; ")
            );
            render(out, json, &explanation, &human)
        }
    }
}

// Bound one complete read, including every page and its response body. Reject
// inconsistent counts, duplicate IDs and incomplete pages. This is not an atomic
// snapshot: concurrent changes that keep those checks consistent are undetectable.
const PAGE_SIZE: usize = 100;
const MAX_RUNS: usize = 10_000;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const STATUS_TIMEOUT: Duration = Duration::from_secs(30);

fn live_status(
    api_url: &str,
    owner: &str,
    repo: &str,
    json_output: bool,
    out: &mut dyn Write,
) -> ClientResult<()> {
    for segment in [owner, repo] {
        if segment.is_empty()
            || matches!(segment, "." | "..")
            || !segment
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(ClientError::Invalid(
                "repository owner and name must be nonempty URL segments containing only letters, digits, '.', '_' or '-'".into(),
            ));
        }
    }
    let deadline = Instant::now() + STATUS_TIMEOUT;
    let api = ApiClient::new(api_url)?;
    let mut budget = MAX_RESPONSE_BYTES;
    let mut total = None;
    let mut runs = Vec::new();
    let mut ids = HashSet::new();
    loop {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                ClientError::NotWired("CI status exceeded its 30-second read deadline".into())
            })?;
        let page_number = runs.len() / PAGE_SIZE + 1;
        let path =
            format!("/repos/{owner}/{repo}/check-runs?per_page={PAGE_SIZE}&page={page_number}");
        let (page, bytes) = api.get_bounded(&path, timeout, budget)?;
        budget -= bytes;
        let count = page["total_count"]
            .as_u64()
            .filter(|n| *n <= MAX_RUNS as u64)
            .ok_or_else(|| invalid_report("total_count must be an integer at most 10000"))?
            as usize;
        if total.is_some_and(|previous| previous != count) {
            return Err(invalid_report(
                "total_count changed during pagination; retry the read",
            ));
        }
        total = Some(count);
        let batch = page["check_runs"]
            .as_array()
            .ok_or_else(|| invalid_report("check_runs must be an array"))?;
        let expected = count.saturating_sub(runs.len()).min(PAGE_SIZE);
        if batch.len() != expected {
            return Err(invalid_report("incomplete or inconsistent check-run page"));
        }
        for run in batch {
            let id = check_string(run, "id")?;
            if id.len() != 36
                || !id.bytes().enumerate().all(|(i, c)| {
                    if matches!(i, 8 | 13 | 18 | 23) {
                        c == b'-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                })
                || !ids.insert(id.to_ascii_lowercase())
            {
                return Err(invalid_report("check-run IDs must be distinct UUIDs"));
            }
            check_string(run, "name")?;
            check_string(run, "head_sha")?;
            if !matches!(
                check_string(run, "status")?,
                "queued" | "in_progress" | "completed"
            ) {
                return Err(invalid_report("unrecognized check-run status"));
            }
            match run.get("conclusion") {
                Some(Value::Null) => {}
                Some(Value::String(conclusion))
                    if matches!(
                        conclusion.as_str(),
                        "success"
                            | "failure"
                            | "neutral"
                            | "cancelled"
                            | "skipped"
                            | "action_required"
                            | "stale"
                            | "timed_out"
                    ) => {}
                _ => {
                    return Err(invalid_report(
                        "missing or unrecognized check-run conclusion",
                    ));
                }
            }
            runs.push(run.clone());
        }
        if runs.len() == count {
            break;
        }
    }
    if Instant::now() >= deadline {
        return Err(ClientError::NotWired(
            "CI status exceeded its 30-second read deadline".into(),
        ));
    }
    let human = if runs.is_empty() {
        format!("no check runs for {owner}/{repo}")
    } else {
        runs.iter()
            .map(|run| {
                format!(
                    "{} {owner}/{repo}@{} [{}; conclusion={}] {}",
                    run["id"].as_str().unwrap_or_default(),
                    run["head_sha"].as_str().unwrap_or_default().escape_debug(),
                    run["status"].as_str().unwrap_or_default(),
                    run["conclusion"].as_str().unwrap_or("none"),
                    run["name"].as_str().unwrap_or_default().escape_debug(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    render(
        out,
        json_output,
        &json!({"total_count": runs.len(), "check_runs": runs}),
        &human,
    )
}

fn invalid_report(reason: &str) -> ClientError {
    ClientError::Invalid(format!("invalid CI check-run report: {reason}"))
}

fn check_string<'a>(run: &'a Value, field: &str) -> ClientResult<&'a str> {
    run.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_report(&format!("{field} must be a nonempty string")))
}
