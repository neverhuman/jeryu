pub(crate) fn is_live_job_status(status: &str) -> bool {
    matches!(
        status,
        "running" | "pending" | "created" | "waiting_for_resource" | "preparing"
    )
}

pub(crate) fn is_terminal_job_status(status: &str) -> bool {
    matches!(status, "success" | "failed" | "canceled" | "skipped")
}

pub(crate) fn live_job_status_rank(status: &str) -> u8 {
    match status {
        "running" => 5,
        "waiting_for_resource" | "preparing" => 4,
        "pending" => 3,
        "created" => 2,
        _ => 0,
    }
}
