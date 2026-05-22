use super::SharedState;

pub(crate) async fn record_emergency_pressure_event(state: &SharedState, root_free: u64) {
    let _ = state
        .db
        .append_event(
            "disk_pressure_emergency",
            None,
            None,
            "system_health_loop",
            &serde_json::json!({
                "root_free_bytes": root_free,
                "root_free_human": crate::cache::human_bytes(root_free),
                "warning_floor_bytes": crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
                "emergency_floor_bytes": crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
                "paused_pools": ["build", "default"],
            })
            .to_string(),
        )
        .await;
}

pub(crate) async fn record_gc_pass_event(
    state: &SharedState,
    current_free: u64,
    pass: u32,
    escalated: bool,
    is_emergency: bool,
) {
    let _ = state
        .db
        .append_event(
            "disk_pressure_gc",
            None,
            None,
            "system_health_loop",
            &serde_json::json!({
                "root_free_bytes": current_free,
                "root_free_human": crate::cache::human_bytes(current_free),
                "pass": pass,
                "critical": escalated,
                "emergency": is_emergency,
                "warning_floor_bytes": crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
                "emergency_floor_bytes": crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
            })
            .to_string(),
        )
        .await;
}

pub(crate) async fn record_gc_complete_event(
    state: &SharedState,
    usage_before: u64,
    current_free: u64,
    pass: u32,
) {
    let _ = state
        .db
        .append_event(
            "disk_pressure_gc_complete",
            None,
            None,
            "system_health_loop",
            &serde_json::json!({
                "root_free_before_bytes": usage_before,
                "root_free_after_bytes": current_free,
                "freed_bytes": current_free.saturating_sub(usage_before),
                "passes": pass,
            })
            .to_string(),
        )
        .await;
}

pub(crate) async fn record_gc_stalled_event(
    state: &SharedState,
    current_free: u64,
    consecutive_zero_freed: u32,
) {
    let _ = state
        .db
        .append_event(
            "disk_gc_stalled",
            None,
            None,
            "system_health_loop",
            &serde_json::json!({
                "root_free_bytes": current_free,
                "consecutive_stalls": consecutive_zero_freed,
            })
            .to_string(),
        )
        .await;
}
