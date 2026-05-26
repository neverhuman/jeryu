use ratatui::{Frame, layout::Rect};

use crate::tui::{
    app::App,
    focus::{self, PaneId},
    lenses::repos::{ReposLens, ReposPane, ReposSelection, select_repos_lens_input},
    theme::{TerminalCaps, Theme},
};

pub fn draw_app_repos_lens(f: &mut Frame, app: &mut App, area: Rect) {
    focus::register_pane(app, PaneId::ReposLens, area);
    focus::register_drill_esc_hotspot(app, PaneId::ReposLens, area);

    let mut model = crate::api::read_model::TuiReadModel::default();
    model.repos = crate::api::read_model::ReposSnapshot::from_fleet_snapshot(&app.state.fleet);
    let selection = ReposSelection {
        family: None,
        repo: app.selected_repo().map(|repo| repo.slug.clone()),
    };
    let input = select_repos_lens_input(&model, &selection);
    let theme = Theme::dark();

    f.render_widget(
        ReposLens::new(input, &theme, TerminalCaps::unicode()).active(ReposPane::Fleet),
        area,
    );
}
