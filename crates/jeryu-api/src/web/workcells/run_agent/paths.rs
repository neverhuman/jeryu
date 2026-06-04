use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use axum::response::Response as AxumResponse;
use jeryu_runnerd::WorkcellLease;

use crate::web::workcells_support::{TypedError, typed_error};

pub(super) fn selected_run_root(
    lease: &WorkcellLease,
    requested: Option<&Path>,
) -> Result<PathBuf, Box<AxumResponse>> {
    let selected = match requested {
        Some(path) => path.to_path_buf(),
        None => match lease.repo_roots.first() {
            Some(path) => path.clone(),
            None => {
                return Err(run_path_denied(
                    "the workcell has no claimed repo roots to run inside",
                ));
            }
        },
    };
    let selected = canonical_existing(&selected, "the selected repo root does not exist")?;
    let allowed = lease
        .repo_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| selected == root);
    if !allowed {
        return Err(run_path_denied(
            "the selected repo root is outside the claimed workcell slice",
        ));
    }
    Ok(selected)
}

pub(super) fn resolve_program_in_run_root(
    run_root: &Path,
    program: &str,
) -> Result<PathBuf, Box<AxumResponse>> {
    let candidate = PathBuf::from(program);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        run_root.join(candidate)
    };
    let candidate = canonical_existing(&candidate, "the requested program does not exist")?;
    if !candidate.starts_with(run_root) {
        return Err(run_path_denied(
            "the requested program is outside the selected repo root",
        ));
    }
    Ok(candidate)
}

fn canonical_existing(path: &Path, reason: &'static str) -> Result<PathBuf, Box<AxumResponse>> {
    path.canonicalize().map_err(|_| run_path_denied(reason))
}

fn run_path_denied(reason: &'static str) -> Box<AxumResponse> {
    Box::new(typed_error(TypedError {
        status: StatusCode::FORBIDDEN,
        code: "workcell_run_path_denied",
        purpose: "run an agent inside a workcell repo slice",
        reason,
        common_fixes: &[
            "claim the workcell with the repo root that contains the program",
            "stage the agent command under the selected repo root before running it",
        ],
        docs_url: "docs/testing.md#workcells",
        repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
        message: reason,
    }))
}
