use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::{
    lenses::queue::{QueueLensInput, QueuePane, analyze_capacity, runner_delta},
    theme::{TerminalCaps, Theme, status_badge},
    widgets::{shared::Panel, truncate_label},
};

#[derive(Debug, Clone, Copy)]
pub struct QueueLens<'a> {
    input: QueueLensInput<'a>,
    theme: &'a Theme,
    caps: TerminalCaps,
    active: QueuePane,
}

impl<'a> QueueLens<'a> {
    pub fn new(input: QueueLensInput<'a>, theme: &'a Theme, caps: TerminalCaps) -> Self {
        Self {
            input,
            theme,
            caps,
            active: QueuePane::Capacity,
        }
    }

    pub fn active(mut self, active: QueuePane) -> Self {
        self.active = active;
        self
    }
}

impl Widget for QueueLens<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let capacity = analyze_capacity(self.input);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(4)])
            .split(area);
        render_header(
            self.input,
            &capacity,
            rows[0],
            buf,
            self.theme,
            self.caps,
            self.active,
        );

        let body = if area.width < 84 {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                ])
                .split(rows[1])
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                ])
                .split(rows[1])
        };

        render_lab(self.input, body[0], buf, self.theme, self.active);
        render_jobs(self.input, body[1], buf, self.theme, self.active);
        render_pools(self.input, body[2], buf, self.theme, self.active);
        render_stages(self.input, body[3], buf, self.theme);
    }
}

fn render_header(
    input: QueueLensInput<'_>,
    capacity: &crate::tui::lenses::queue::QueueCapacity,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: QueuePane,
) {
    let badge = status_badge(capacity.status_key(), theme, caps);
    Panel::new("Queue Limit Lab", theme)
        .active(active == QueuePane::Capacity, theme)
        .badge(badge)
        .line(Line::from(Span::styled(
            format!(
                "  queued {}  running {}  runners {}/{}",
                capacity.queued_jobs,
                capacity.running_jobs,
                capacity.busy_runners,
                capacity.active_parallel_limit
            ),
            theme.primary(),
        )))
        .line(Line::from(Span::styled(
            format!(
                "  theoretical limit {}  saturation {}%  attention {}",
                capacity.theoretical_limit,
                capacity.saturation_pct,
                input.model.attention.len()
            ),
            theme.secondary(),
        )))
        .render(area, buf);
}

fn render_lab(
    input: QueueLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: QueuePane,
) {
    let capacity = analyze_capacity(input);
    let delta = runner_delta(input, 1);
    Panel::new("Does Adding Runners Help?", theme)
        .active(active == QueuePane::Lab, theme)
        .line(Line::from(Span::styled(
            format!("  {}", capacity.add_runner_effect.label()),
            theme.bold(theme.status_color(capacity.status_key())),
        )))
        .line(Line::from(Span::styled(
            format!("  {}", capacity.add_runner_effect.explanation()),
            theme.secondary(),
        )))
        .line(Line::from(Span::styled(
            format!(
                "  +{} runner limit {} -> {}",
                delta.additional_runners, delta.current_limit, delta.projected_limit
            ),
            theme.primary(),
        )))
        .line(Line::from(Span::styled(
            format!("  jobs unblocked {}", delta.jobs_unblocked),
            theme.primary(),
        )))
        .render(area, buf);
}

fn render_jobs(
    input: QueueLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: QueuePane,
) {
    let mut panel = Panel::new("Waiting Jobs", theme)
        .active(active == QueuePane::Jobs, theme)
        .empty("No waiting jobs");
    for job in input
        .waiting_jobs()
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} {} {}s",
                truncate_label(&job.label, 22),
                job.status,
                job.queued_secs()
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn render_pools(
    input: QueueLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: QueuePane,
) {
    let mut panel = Panel::new("Pool Fit", theme)
        .active(active == QueuePane::Pools, theme)
        .empty("No pool data");
    for pool in input
        .pools()
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        let state = if pool.paused { "paused" } else { "active" };
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} {} idle {}/{}",
                truncate_label(&pool.name, 14),
                state,
                pool.idle_slots(),
                pool.configured_max_slots()
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn render_stages(input: QueueLensInput<'_>, area: Rect, buf: &mut Buffer, theme: &Theme) {
    let mut panel = Panel::new("Stage Queue", theme).empty("No stages");
    for stage in input
        .stage_summaries()
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} q{} r{} avg {}s",
                truncate_label(&stage.stage, 14),
                stage.queued,
                stage.running,
                stage.avg_queue_secs
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}
