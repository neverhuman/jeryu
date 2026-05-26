# Tuiwright Split Migration Map

Claim: `TUI-RESET-20260526-004`

Current source: `tests/tui_tuiwright.rs` has 23 black-box `#[test]` cases. Do not delete or move a monolith test until its listed assertion is preserved in the target suite.

Current monolith proof command:

```bash
TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1
```

Current split proof command:

```bash
TERM=xterm-256color cargo test --test tuiwright -- --test-threads=1
```

## Shared Harness

Move reusable setup and assertions into a shared module before splitting tests:

| Current helper | Preserve |
|---|---|
| `tuiwright_lock`, `jeryu_bin` | Serial PTY/capture execution and binary discovery. |
| `capture_tui`, `capture_tui_size`, `read_png` | PNG capture through `jeryu tui --capture --tab ... --width ... --height ...`. |
| `assert_png_shape_and_ink`, `assert_png_shape_and_ink_size` | Cell-scaled PNG dimensions and non-empty ink threshold. |
| `assert_cell_region_has_ink`, `assert_main_layout_regions` | Header, content, activity/log, and footer region ink checks. |
| `spawn_interactive_tui`, `spawn_interactive_tui_size`, `screen_text` | Demo PTY sessions with deterministic env and in-memory SQLite. |
| `find_text_cell_region`, `title_row_yellow_cell_count`, `assert_focused_title_row`, `wait_for_focused_title` | Focus assertion by yellow title-row cells. |
| `assert_text_absent`, `wait_for_text_absent`, `assert_text_order` | Negative text waits and visible ordering checks. |

## Suite Map

| Target suite | Current tests to preserve | Assertions that must survive |
|---|---|---|
| `capture.rs` | `capture_path_renders_all_primary_tabs`; capture half of `bugs_capture_has_populated_demo_data_and_narrow_layout`; capture half of `jankurai_tab_renders_with_real_score_data` | All 16 primary tabs render PNGs with expected dimensions, >1000 non-background pixels, and ink in header/content/activity-footer regions; bugs and jankurai captures keep the same layout checks. |
| `navigation.rs` | `bugs_global_shortcut_focus_navigation_and_inspector_drilldown_work`; `tab_always_cycles_main_tabs_from_workflow`; `workflow_macro_micro_focus_and_drilldown_work`; `keyboard_macro_focuses_activity_log_and_drills_down`; `activity_log_enter_expands_and_esc_restores`; `esc_badge_click_exits_entered_pane`; `fleet_bar_focus_enter_opens_detail_and_arrows_cycle_repos`; `fleet_bar_esc_resets_to_all_from_selected_repo`; `workflow_inspector_tab_strip_reflects_node_kind`; `workflow_repo_filter_changes_pr_rail_title`; `repo_detail_overlay_esc_click_closes_it` | Global shortcuts, arrows, Tab, Enter, Esc, and mouse clicks keep focus movement, drilldown, fullscreen pane restore, repo overlay cycling, repo filter title changes, inspector tab strip, and overlay close behavior. |
| `responsive.rs` | responsive half of `bugs_capture_has_populated_demo_data_and_narrow_layout`; wide-session portions of `workflow_macro_micro_focus_and_drilldown_work`, `fleet_bar_focus_enter_opens_detail_and_arrows_cycle_repos`, `workflow_inspector_tab_strip_reflects_node_kind`, `workflow_r_key_posts_action_message`, `workflow_repo_filter_changes_pr_rail_title`, `repo_detail_overlay_has_esc_badge`, `repo_detail_overlay_esc_click_closes_it` | The 96x34 bugs capture has correct dimensions and ink in content/footer; 220x44 workflow interactions remain usable with visible focus, overlays, and detail panes. |
| `streams.rs` | No direct stream-resume assertion today; retain activity/log visibility checks from `keyboard_macro_focuses_activity_log_and_drills_down` only if the split treats log panes as stream smoke. | Future suite should add live events, disconnect, stale marker, reconnect, cursor resume, and gap fetch coverage. |
| `actions.rs` | `bugs_sort_keys_change_indicator_and_visible_order`; `workflow_r_key_posts_action_message`; action-like parts of `workflow_inspector_tab_strip_reflects_node_kind` | Sort keys update indicators and visible bug order; workflow `r` posts a rollback/roll action message; critical-path jump `c` keeps the inspector strip visible and agent details are checked when present. |
| `redaction.rs` | No current black-box redaction assertion. | Add screenshots, text dumps, bundles, panic output, and copied-path secret/token checks before deleting this placeholder. |
| `source_doctor.rs` | No current Source Doctor assertion. | Add API-down, MCP-drift, schema-mismatch, stale-docs, DB-profile-mismatch, and source-down critical-data cases. |
| `accessibility.rs` | `repo_detail_overlay_has_esc_badge`; `help_overlay_has_esc_badge`; `command_palette_has_esc_badge`; focus-order assertions from navigation tests | `[esc]` badges remain visible in repo detail, help, and command palette overlays; keyboard-only focus order remains stable and visible through focused title-row color checks. |
| `performance.rs` | No hard performance assertion today. | Current timeouts are harness safety only; future suite should add large fixtures, event bursts, huge tables, trace subscriptions, and long-session bounds. |
| `flicker.rs` | No explicit anti-flicker assertion today. | Add empty-refresh preservation, selection-by-id, sticky log tail, stale-dim-not-blank, and event-burst-no-jump coverage. |
| `replay.rs` | No current replay assertion. | Add recorded event stream scenarios for job fail/retry, OOM, cache miss storm, VTI miss, agent race, and canary rollback. |
| `incident.rs` | No current incident assertion. | Add pinned emergency view, high-contrast incident mode, decision ledger, and action proof-link assertions. |

## Domain Assertions

| Current test | Target suite | Assertion inventory |
|---|---|---|
| `bugs_tab_exposes_semantic_bug_details` | `navigation.rs` or later `bugs` fixture group | Bugs tab shows project list, sort label, `redlinedb`, severity/priority/status counts, repo relation, current/expected behavior, reproduction, evidence, and acceptance text. |
| `fleet_bar_shows_repo_names_on_initial_render` | `navigation.rs` | Initial fleet bar shows demo aliases `nht`, `shared`, `warp`, and `All run:1`. |
| `jankurai_tab_renders_with_real_score_data` | `capture.rs` plus domain smoke | Jankurai tab capture has layout ink and interactive text contains score `89` or `Jankurai`. |
| `fleet_bar_discovers_repos_via_workspace_root_env` | `navigation.rs` or future fixture/client suite | Non-demo launch honors `JERYU_WORKSPACE_ROOT` and renders synthetic aliases `myalpha` and `mybeta`. |
| `fleet_bar_discovers_repos_via_serve_daemon_strategy4` | `navigation.rs` or future fixture/client suite | Conditional daemon strategy finds aliases from a running `jeryu serve` workspace and renders at least the first alias. |

## Split Order

1. Create the shared harness module and copy helpers without behavior changes.
2. Move capture and responsive tests first; they are the least stateful.
3. Move navigation/accessibility/actions tests while keeping the monolith proof green.
4. Add placeholder files for empty future suites only after their first real assertion exists.
5. Delete `tests/tui_tuiwright.rs` only when all rows above have landed in split suites and the replacement proof command is approved.
