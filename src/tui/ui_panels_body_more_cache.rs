use super::*;

// ---------------------------------------------------------------------------
// Tab 7 — Cache (existing dashboard, preserved)
// ---------------------------------------------------------------------------

pub(crate) fn draw_cache_dashboard(f: &mut Frame, app: &mut App, area: Rect) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(8)])
        .split(area);

    draw_disk_pressure_panel(f, app, outer[0]);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(outer[1]);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    focus::register_pane(app, PaneId::CacheDisk, outer[0]);
    focus::register_pane(app, PaneId::CacheStorage, top_chunks[0]);
    focus::register_pane(app, PaneId::CacheGateway, top_chunks[1]);
    focus::register_pane(app, PaneId::CacheSingleflight, bottom_chunks[0]);
    focus::register_pane(app, PaneId::CacheTaint, bottom_chunks[1]);

    let objects_str = format!(
        "\n  Total Cached Objects: {}\n  Hot Cache Bandwidth:  {} MB\n  Exact Hits:  {} / {} ({:.1}%)\n  Misses:      {}\n\n  CAS Disk:    {} MiB\n  Crate Cache: {} MiB",
        app.state.cache_objects_count,
        app.state.hot_cache_usage_bytes / 1024 / 1024,
        app.state.cache_hits,
        app.state.total_requests,
        app.state.hit_ratio,
        app.state.miss_count,
        app.state.cas_disk_bytes / 1024 / 1024,
        app.state.crate_cache_disk_bytes / 1024 / 1024
    );
    f.render_widget(
        Paragraph::new(objects_str).block(
            Block::default()
                .title(" [ Storage Overview ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::CacheStorage)),
        ),
        top_chunks[0],
    );

    let proxy_str = if app.state.proxy_healthy {
        "ONLINE"
    } else {
        "OFFLINE"
    };
    let reg_str = if app.state.registry_healthy {
        "ONLINE"
    } else {
        "OFFLINE"
    };
    let services_str = format!(
        "\n  Singleflight Gateway: {}\n  OCI Mirror:           {}\n  CA Certs Injected:    {}",
        proxy_str, reg_str, app.state.ca_mounted
    );
    f.render_widget(
        Paragraph::new(services_str).block(
            Block::default()
                .title(" [ Gateway Health ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::CacheGateway)),
        ),
        top_chunks[1],
    );

    let sf_str = format!(
        "\n  Coalesced Fetches: {}\n  Est. Bandwidth Saved: ~{} MB\n\n  Eliminating redundant crate downloads\n  across parallel runners automatically.",
        app.state.singleflight_requests,
        app.state.singleflight_requests * 5
    );
    f.render_widget(
        Paragraph::new(sf_str).block(
            Block::default()
                .title(" [ Singleflight Analytics ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::CacheSingleflight)),
        ),
        bottom_chunks[0],
    );

    let taint_str = format!(
        "\n  Active Taint Rules:        {}\n  Detonation Lane Breaches:  {}\n  Cold Execution Downgrades: {}\n\n  {}",
        app.state.active_taint_count,
        app.state.detonation_breaches,
        app.state.cold_execution_downgrades,
        if app.state.active_taint_count == 0 && app.state.detonation_breaches == 0 {
            "System executing hermetically."
        } else {
            "[RISK] Taint rules active — cache quarantined."
        }
    );
    f.render_widget(
        Paragraph::new(taint_str).block(
            Block::default()
                .title(" [ Trust & Taint Boundaries ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::CacheTaint)),
        ),
        bottom_chunks[1],
    );
}

/// Disk pressure strip — surfaces what `jeryu-gcd` is doing.
fn draw_disk_pressure_panel(f: &mut Frame, app: &App, area: Rect) {
    let free = app.state.storage_breakdown.disk_available_bytes;
    let total = app.state.storage_breakdown.total_disk_bytes;
    let free_gib = free / (1024 * 1024 * 1024);
    let total_gib = total / (1024 * 1024 * 1024);

    // Mirrors the constants in `src/cache/types.rs`. Kept inline to avoid a
    // dependency on i64↔u64 conversion noise.
    const WARN_GIB: u64 = 80;
    const CRIT_GIB: u64 = 60;
    const EMERG_GIB: u64 = 40;

    let label = if free_gib < EMERG_GIB {
        "EMERGENCY"
    } else if free_gib < CRIT_GIB {
        "CRITICAL"
    } else if free_gib < WARN_GIB {
        "WARNING"
    } else {
        "NOMINAL"
    };

    let body = format!(
        "\n  Free: {} GiB / {} GiB     Pressure: {}     Daemon: jeryu-gcd",
        free_gib, total_gib, label,
    );

    f.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .title(" [ Disk Pressure ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::CacheDisk)),
        ),
        area,
    );
}
