use super::*;

// ---------------------------------------------------------------------------
// Footer / key hints
// ---------------------------------------------------------------------------

pub(crate) fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let help = if app.maximize_logs {
        " Tab/Shift+Tab:tabs  Arrows:scroll  PgUp/PgDn/Home/End:logs  Enter:fullscreen  Esc:back  ?:help  ^K:palette  q:quit"
    } else {
        " Tab/Shift+Tab:tabs  Arrows:move  Enter:drill  Esc:back  ?:help  ^K:palette  q:quit"
    };
    let p = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}
