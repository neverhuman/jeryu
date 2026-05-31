//! tuiwright-style snapshot tests.
//!
//! Each test renders one of the implemented lenses from a fixture
//! [`TuiReadModel`] through the full chrome (`render_once`) and asserts key
//! content of the flattened cell text. This proves the read-model → TUI rewire:
//! the lenses are pure projections of `jeryu-readmodel`, and the runtime
//! composes them with the header/status-strip chrome without any backend I/O.

use jeryu_readmodel::{
    EntityKind, EntityRef, PoolActivity, PoolRollup, RepoActivity, RepoSummary, ReposSnapshot,
    TagDemand, TuiReadModel, TuiReadModelBuilder, sample_read_model,
};
use jeryu_tui::tuiwright::{self, SWEEP_SIZES};
use jeryu_tui::{ActiveTab, App, StreamMode, render_once};

/// Render a single Flight Deck frame for `tab` from `model`.
fn snapshot(model: TuiReadModel, tab: ActiveTab) -> String {
    let mut app = App::new_render_only(model);
    app.set_tab(tab);
    render_once(&app, 120, 40, StreamMode::Fixture)
}

/// Render a frame at an explicit terminal size.
fn snapshot_sized(model: TuiReadModel, tab: ActiveTab, width: u16, height: u16) -> String {
    let mut app = App::new_render_only(model);
    app.set_tab(tab);
    render_once(&app, width, height, StreamMode::Fixture)
}

/// Render a frame at an explicit size with an explicit transport badge — used by
/// the degraded-fixture sweep (StreamMode::Degraded + stale freshness).
fn snapshot_sized_mode(
    model: TuiReadModel,
    tab: ActiveTab,
    width: u16,
    height: u16,
    stream: StreamMode,
) -> String {
    let mut app = App::new_render_only(model);
    app.set_tab(tab);
    render_once(&app, width, height, stream)
}

// ── Mission lens ─────────────────────────────────────────────────────────

#[test]
fn mission_lens_renders_posture_attention_and_next_action() {
    let ink = snapshot(sample_read_model(), ActiveTab::Mission);

    // Chrome: brand, active tab, fixture stream badge, status hints.
    assert!(ink.contains("jeryu"), "header brand missing");
    assert!(ink.contains("Mission"), "tab label missing");
    assert!(ink.contains("FIXTURE"), "stream badge missing");

    // Mission lens content projected from the read model.
    assert!(ink.contains("Mission Control"), "posture panel missing");
    assert!(ink.contains("Posture"), "posture panel title missing");
    assert!(ink.contains("Attention"), "attention panel missing");
    assert!(ink.contains("Next Action"), "next-action panel missing");

    // Fixture data surfaced: the sample model has a build-web failure + retry.
    assert!(
        ink.contains("build-web failing"),
        "attention item not projected"
    );
    assert!(ink.contains("Retry"), "next action label not projected");
    // Sample posture is Warning and not safe to merge.
    assert!(ink.contains("Warning"), "overall posture not projected");
}

#[test]
fn mission_lens_default_model_shows_empty_states() {
    let ink = snapshot(TuiReadModel::default(), ActiveTab::Mission);
    assert!(ink.contains("(no attention items)"));
    assert!(ink.contains("(no recommendation)"));
    // Default posture is safe to code.
    assert!(ink.contains("Safe to code: yes"));
}

// ── Queue lens ──────────────────────────────────────────────────────────

#[test]
fn queue_lens_renders_capacity_and_headroom_from_read_model() {
    let mut model = TuiReadModel {
        event_cursor: 77,
        ..Default::default()
    };
    model.mission.queued_jobs = 12;
    model.mission.running_jobs = 4;
    model.mission.failed_jobs = 2;
    model.mission.active_runners = 6;
    model.mission.total_runners = 8;

    // Queue maps to the Pools tab in this crate's tab routing.
    let ink = snapshot(model, ActiveTab::Pools);

    assert!(ink.contains("Queue"), "queue header missing");
    assert!(ink.contains("Capacity"), "capacity table missing");
    assert!(ink.contains("Headroom"), "headroom row missing");
    assert!(ink.contains("12 queued"), "queued count not projected");
    assert!(ink.contains("utilization"), "utilization not derived");
    // queue (12) exceeds headroom (8-6=2) → warning copy.
    assert!(
        ink.contains("exceeds free capacity"),
        "headroom-overflow warning not rendered"
    );
    assert!(ink.contains("cursor=77"), "event cursor not projected");
}

#[test]
fn queue_lens_default_model_is_idle() {
    let ink = snapshot(TuiReadModel::default(), ActiveTab::Pools);
    assert!(ink.contains("Queue"));
    assert!(ink.contains("0 queued"));
    assert!(ink.contains("Headroom"));
}

// ── Repos lens ──────────────────────────────────────────────────────────

#[test]
fn repos_lens_renders_fleet_families_and_detail_from_read_model() {
    let mut model = TuiReadModel {
        event_cursor: 9012,
        ..Default::default()
    };
    let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
    repo.status = "running".into();
    repo.running_count = 1;
    repo.entity = EntityRef::new(EntityKind::Repo, "neverhuman/jeryu");
    model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);

    let ink = snapshot(model, ActiveTab::Repos);

    assert!(ink.contains("Repository Fleet"), "fleet panel missing");
    assert!(ink.contains("Families"), "families panel missing");
    assert!(ink.contains("Repositories"), "repos panel missing");
    assert!(ink.contains("Repo Detail"), "detail panel missing");
    assert!(ink.contains("Scoped Attention"), "attention panel missing");
    // Projected family + repo content.
    assert!(ink.contains("neverhuman"), "family name not projected");
    assert!(ink.contains("core"), "repo alias not projected");
    assert!(
        ink.contains(".jeryu/repos.toml"),
        "registry path not projected"
    );
}

#[test]
fn repos_lens_default_model_shows_empty_fleet() {
    let ink = snapshot(TuiReadModel::default(), ActiveTab::Repos);
    assert!(ink.contains("Repository Fleet"));
    assert!(ink.contains("No repositories"));
    assert!(ink.contains("No repo families"));
}

// ── Approvals lens ─────────────────────────────────────────────────────────

#[test]
fn approvals_lens_renders_pending_prs_from_read_model() {
    let ink = snapshot(sample_read_model(), ActiveTab::Approvals);
    assert!(ink.contains("Approvals"), "approvals header missing");
    assert!(ink.contains("#101"), "PR number not projected");
    assert!(ink.contains("#102"), "PR number not projected");
    // GitHub PR shape: CI checks status colored, risk tier shown.
    assert!(ink.contains("failure"), "failing checks not projected");
    assert!(ink.contains("failing checks"), "queue alert not rendered");
}

// ── Evidence lens ──────────────────────────────────────────────────────────

#[test]
fn evidence_lens_renders_proof_receipts_from_read_model() {
    let ink = snapshot(sample_read_model(), ActiveTab::Evidence);
    assert!(ink.contains("Evidence"), "evidence header missing");
    assert!(ink.contains("cap-17"), "capsule id not projected");
    assert!(ink.contains("allow"), "allow decision not projected");
    assert!(ink.contains("deny"), "deny decision not projected");
    assert!(ink.contains("17 capsules"), "capsule count not projected");
}

// ── Agents lens ────────────────────────────────────────────────────────────

#[test]
fn agents_lens_renders_fleet_from_read_model() {
    let ink = snapshot(sample_read_model(), ActiveTab::Agents);
    assert!(ink.contains("Agents"), "agents header missing");
    assert!(ink.contains("Lifecycle"), "lifecycle panel missing");
    assert!(
        ink.contains("agent-wrath-17"),
        "agent session not projected"
    );
    assert!(ink.contains("blocked"), "blocked status not projected");
}

// ── Release lens ───────────────────────────────────────────────────────────

#[test]
fn release_lens_renders_candidates_from_read_model() {
    let ink = snapshot(sample_read_model(), ActiveTab::Release);
    assert!(ink.contains("Release"), "release header missing");
    assert!(ink.contains("GATE"), "gate column missing");
    assert!(ink.contains("SBOM"), "sbom column missing");
    assert!(ink.contains("canary"), "promotion stage not projected");
    assert!(ink.contains("verified"), "sbom status not projected");
}

// ── Workflow lens ──────────────────────────────────────────────────────────

#[test]
fn workflow_lens_renders_delivery_posture_from_read_model() {
    let ink = snapshot(sample_read_model(), ActiveTab::Workflow);
    assert!(ink.contains("Workflow Atlas"), "atlas header missing");
    assert!(ink.contains("core/web"), "repo slug not projected");
    // GitHub PR shape: pipelines keyed by PR number.
    assert!(ink.contains("#101"), "driving PR number not projected");
    assert!(ink.contains("BLOCKED"), "blocked posture not projected");
}

// ── Pools/Health (Runners) lens ──────────────────────────────────────────────

/// A degraded pool fabric: a saturated `trusted` pool with a stuck runner plus
/// tag-starved (`gpu`) queued work that no pool serves.
fn degraded_pool_model() -> TuiReadModel {
    let mut saturated = PoolRollup::new("trusted");
    saturated.tags = vec!["oci".into()];
    saturated.active_slots = 2;
    saturated.running_jobs = 2;
    saturated.queued_jobs = 5;
    saturated.failed_jobs = 1;
    saturated.online_runners = 2;
    saturated.stuck_runners = 1;

    TuiReadModel {
        event_cursor: 91,
        pool_activity: PoolActivity {
            repos: vec![RepoActivity {
                repo: "neverhuman/jeryu".into(),
                queued_jobs: 5,
                running_jobs: 2,
                ..RepoActivity::default()
            }],
            pools: vec![saturated],
            unplaceable: vec![TagDemand {
                tags: vec!["gpu".into()],
                count: 3,
            }],
            freshness: None,
        },
        ..Default::default()
    }
}

/// A healthy pool fabric: one clean `trusted` pool with idle headroom.
fn healthy_pool_model() -> TuiReadModel {
    let mut clean = PoolRollup::new("trusted");
    clean.active_slots = 4;
    clean.running_jobs = 1;
    clean.online_runners = 4;

    TuiReadModel {
        pool_activity: PoolActivity {
            pools: vec![clean],
            repos: vec![RepoActivity {
                repo: "neverhuman/jeryu".into(),
                running_jobs: 1,
                ..RepoActivity::default()
            }],
            ..PoolActivity::default()
        },
        ..Default::default()
    }
}

#[test]
fn pools_health_lens_renders_degraded_fabric_at_80x24() {
    let ink = snapshot_sized(degraded_pool_model(), ActiveTab::Runners, 80, 24);
    // Chrome present; the Runners tab routes to the Pools/Health lens (its body
    // title proves the routing — the header tab strip itself clips at 80 cols).
    assert!(ink.contains("jeryu"), "header brand missing");
    // Fleet totals header + per-pool grid.
    assert!(ink.contains("Pools/Health"), "fleet header missing");
    assert!(ink.contains("trusted"), "pool row not projected");
    // Health banner: tag-starvation is Critical and ranks first.
    assert!(ink.contains("CRITICAL"), "critical health not projected");
}

#[test]
fn pools_health_lens_renders_degraded_fabric_at_120x40() {
    let ink = snapshot_sized(degraded_pool_model(), ActiveTab::Runners, 120, 40);
    assert!(ink.contains("Pools/Health"), "fleet header missing");
    assert!(ink.contains("trusted"), "pool row not projected");
    assert!(ink.contains("SATURATED"), "saturated state not projected");
    assert!(ink.contains("CRITICAL"), "critical health not projected");
    // Ranked bottleneck describe(): tag-starvation banner.
    assert!(
        ink.contains("no pool serves it"),
        "tag-starvation banner not projected"
    );
    // Stuck runner surfaced in totals/footer.
    assert!(ink.contains("STUCK"), "stuck-runner alert not projected");
    assert!(ink.contains("cursor=91"), "event cursor not projected");
}

#[test]
fn pools_health_lens_renders_healthy_fabric_at_80x24() {
    let ink = snapshot_sized(healthy_pool_model(), ActiveTab::Runners, 80, 24);
    assert!(ink.contains("Pools/Health"), "fleet header missing");
    assert!(ink.contains("trusted"), "pool row not projected");
    assert!(ink.contains("HEALTHY"), "healthy health not projected");
    assert!(
        ink.contains("Fleet healthy"),
        "healthy banner not projected"
    );
}

#[test]
fn pools_health_lens_renders_healthy_fabric_at_120x40() {
    let ink = snapshot_sized(healthy_pool_model(), ActiveTab::Runners, 120, 40);
    assert!(ink.contains("Pools/Health"), "fleet header missing");
    assert!(ink.contains("HEALTHY"), "healthy health not projected");
    assert!(ink.contains("active"), "active pool state not projected");
    assert!(
        ink.contains("fleet healthy"),
        "healthy footer not projected"
    );
    // No bottleneck alerts on a clean fabric.
    assert!(
        !ink.contains("CRITICAL"),
        "false critical on healthy fabric"
    );
    assert!(
        !ink.contains("SATURATED"),
        "false saturation on healthy fabric"
    );
}

// ── Chrome / routing invariants ───────────────────────────────────────────

#[test]
fn builder_fixture_round_trips_into_a_renderable_frame() {
    let model = TuiReadModelBuilder::new().event_cursor(5).build();
    let ink = snapshot(model, ActiveTab::Mission);
    assert!(ink.contains("Cursor: 5"));
}

#[test]
fn cache_tab_renders_live_summary_not_a_placeholder() {
    let ink = snapshot(sample_read_model(), ActiveTab::Cache);
    assert!(ink.contains("Hit ratio"));
    // Chrome still present.
    assert!(ink.contains("jeryu"));
    assert!(ink.contains("Cache"));
}

#[test]
fn stream_mode_badge_reflects_transport() {
    let mut app = App::new_render_only(TuiReadModel::default());
    app.set_tab(ActiveTab::Mission);
    assert!(render_once(&app, 120, 40, StreamMode::Live).contains("LIVE"));
    assert!(render_once(&app, 120, 40, StreamMode::Poll).contains("[poll]"));
    assert!(render_once(&app, 120, 40, StreamMode::Degraded).contains("DEGRADED"));
}

#[test]
fn every_tab_renders_at_compact_and_wide_sizes() {
    for tab in ActiveTab::ALL {
        for (width, height) in [(80, 24), (120, 40)] {
            let mut app = App::new_render_only(sample_read_model());
            app.set_tab(*tab);
            let ink = render_once(&app, width, height, StreamMode::Fixture);
            assert!(
                ink.contains("jeryu"),
                "brand missing for {tab:?} {width}x{height}"
            );
            assert!(
                !ink.contains("not yet ported"),
                "placeholder rendered for {tab:?} {width}x{height}"
            );
        }
    }
}

// ── Degraded-fixture sweep ────────────────────────────────────────────────

/// A fully degraded fixture: the sample model with the freshness watermark
/// flipped stale and the runner fabric swapped for the saturated/tag-starved/
/// stuck rollup. Rendering this with [`StreamMode::Degraded`] exercises the
/// degraded path of the chrome (EXPIRED freshness chip + DEGRADED transport
/// badge) and every lens against a non-healthy snapshot.
fn degraded_fixture_model() -> TuiReadModel {
    let mut model = sample_read_model();
    model.freshness.overall_stale = true;
    model.pool_activity = degraded_pool_model().pool_activity;
    model
}

#[test]
fn degraded_fixture_chrome_shows_expired_and_degraded_badges() {
    // The degraded chrome path: stale freshness → EXPIRED chip, and the
    // DEGRADED transport badge. Asserted once on Mission so the per-tab sweep
    // below can focus on "renders without placeholder".
    for (width, height) in [(80, 24), (120, 40)] {
        let ink = snapshot_sized_mode(
            degraded_fixture_model(),
            ActiveTab::Mission,
            width,
            height,
            StreamMode::Degraded,
        );
        assert!(
            ink.contains("DEGRADED"),
            "degraded transport badge missing at {width}x{height}"
        );
        assert!(
            ink.contains("EXPIRED"),
            "stale freshness chip missing at {width}x{height}"
        );
    }
}

#[test]
fn every_tab_renders_in_degraded_fixture_at_compact_and_wide_sizes() {
    for tab in ActiveTab::ALL {
        for (width, height) in [(80, 24), (120, 40)] {
            let ink = snapshot_sized_mode(
                degraded_fixture_model(),
                *tab,
                width,
                height,
                StreamMode::Degraded,
            );
            assert!(
                ink.contains("jeryu"),
                "brand missing for degraded {tab:?} {width}x{height}"
            );
            assert!(
                !ink.contains("not yet ported"),
                "placeholder rendered for degraded {tab:?} {width}x{height}"
            );
            // The degraded transport badge is part of the chrome on every tab.
            assert!(
                ink.contains("DEGRADED"),
                "degraded badge missing for {tab:?} {width}x{height}"
            );
        }
    }
}

#[test]
fn degraded_fixture_runners_tab_surfaces_saturation_at_both_sizes() {
    // The Runners/Pools tab over the degraded fabric: the saturated pool, the
    // critical tag-starvation, and the stuck-runner alert all surface, at both
    // the compact and wide sizes, under the degraded transport.
    for (width, height) in [(80, 24), (120, 40)] {
        let ink = snapshot_sized_mode(
            degraded_fixture_model(),
            ActiveTab::Runners,
            width,
            height,
            StreamMode::Degraded,
        );
        assert!(
            ink.contains("Pools/Health"),
            "fleet header missing at {width}x{height}"
        );
        assert!(
            ink.contains("trusted"),
            "saturated pool row missing at {width}x{height}"
        );
        assert!(
            ink.contains("CRITICAL"),
            "tag-starvation criticality missing at {width}x{height}"
        );
        assert!(
            ink.contains("STUCK"),
            "stuck-runner alert missing at {width}x{height}"
        );
    }
}

#[test]
fn healthy_and_degraded_fixtures_differ_per_tab() {
    // Guard against a degenerate fixture: for the lenses that project the
    // freshness/pool fabric, the healthy and degraded snapshots must not be
    // byte-identical, proving the degraded inputs actually flow through.
    for tab in [ActiveTab::Mission, ActiveTab::Runners] {
        let healthy = snapshot_sized_mode(sample_read_model(), tab, 120, 40, StreamMode::Fixture);
        let degraded =
            snapshot_sized_mode(degraded_fixture_model(), tab, 120, 40, StreamMode::Degraded);
        assert_ne!(
            healthy, degraded,
            "{tab:?}: degraded snapshot is identical to healthy"
        );
    }
}

// ── tuiwright receipt emission ────────────────────────────────────────────

/// Resolve the workspace `target/` directory from `CARGO_TARGET_TMPDIR` by
/// walking up to the nearest ancestor literally named `target`. Falls back to a
/// `target` dir beside the tmpdir if none is found.
fn workspace_target_dir() -> std::path::PathBuf {
    let tmp = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let mut cur: &std::path::Path = &tmp;
    loop {
        if cur.file_name().and_then(|n| n.to_str()) == Some("target") {
            return cur.to_path_buf();
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return tmp.join("target"),
        }
    }
}

/// Render every tab at both sweep sizes across the healthy and degraded
/// fixtures, write the resulting tuiwright receipt under `target/jankurai/ux-qa/`,
/// and assert it covers the full matrix.
///
/// The stamp is injected (never wall-clock): `SOURCE_DATE_EPOCH` if set, else a
/// stable label. This keeps the receipt's identity reproducible.
#[test]
fn tuiwright_receipt_covers_every_tab_at_both_sizes() {
    let stamp = tuiwright::receipt_stamp();

    // Two fixture families: the healthy sample model under FIXTURE transport,
    // and the fully-degraded model under DEGRADED transport.
    let healthy =
        tuiwright::sweep_fixture(&stamp, "healthy", StreamMode::Fixture, sample_read_model);
    let degraded = tuiwright::sweep_fixture(
        &stamp,
        "degraded",
        StreamMode::Degraded,
        degraded_fixture_model,
    );

    // Merge both family sweeps into one receipt.
    let mut receipt = healthy;
    receipt.frames.extend(degraded.frames);

    let expected = ActiveTab::ALL.len() * SWEEP_SIZES.len() * 2;
    assert_eq!(
        receipt.len(),
        expected,
        "receipt must record every tab x both sizes x both fixtures"
    );
    assert_eq!(
        receipt.ok_count(),
        expected,
        "every certified frame must render cleanly"
    );

    // Every tab x size x fixture is present.
    for tab in ActiveTab::ALL {
        for &(width, height) in SWEEP_SIZES {
            for fixture in ["healthy", "degraded"] {
                assert!(
                    receipt.covers(*tab, width, height, fixture),
                    "receipt missing {tab:?} {width}x{height} ({fixture})"
                );
            }
        }
    }

    // Stamp is the injected, deterministic value — not a wall-clock instant.
    assert_eq!(receipt.stamp, stamp);
    assert!(!receipt.stamp.is_empty(), "receipt stamp must be set");

    // Write into the workspace target dir and assert the JSON file exists.
    // `CARGO_TARGET_TMPDIR` lives under the workspace `target/` (e.g.
    // `target/tmp/<pkg>`); walk up to the nearest ancestor named `target`.
    let target_dir = workspace_target_dir();
    let path = tuiwright::write_receipt(&target_dir, &receipt).expect("write tuiwright receipt");
    assert!(
        path.exists(),
        "receipt file not written: {}",
        path.display()
    );
    assert!(
        path.to_string_lossy().contains("jankurai/ux-qa/"),
        "receipt not under target/jankurai/ux-qa/: {}",
        path.display()
    );

    // Round-trip: the on-disk JSON parses back into an identical receipt.
    let raw = std::fs::read_to_string(&path).expect("read back receipt");
    let parsed: tuiwright::TuiwrightReceipt =
        serde_json::from_str(&raw).expect("receipt JSON parses");
    assert_eq!(parsed, receipt, "round-tripped receipt diverged");
}
