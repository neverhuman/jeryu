use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::{
    lenses::evidence::{EvidenceLensInput, EvidencePane, build_entity_proof_graph},
    theme::{ProofConfidence, TerminalCaps, Theme, proof_confidence_badge},
    widgets::{shared::Panel, truncate_label},
};

#[derive(Debug, Clone, Copy)]
pub struct EvidenceLens<'a> {
    input: EvidenceLensInput<'a>,
    theme: &'a Theme,
    caps: TerminalCaps,
    active: EvidencePane,
}

impl<'a> EvidenceLens<'a> {
    pub fn new(input: EvidenceLensInput<'a>, theme: &'a Theme, caps: TerminalCaps) -> Self {
        Self {
            input,
            theme,
            caps,
            active: EvidencePane::Search,
        }
    }

    pub fn active(mut self, active: EvidencePane) -> Self {
        self.active = active;
        self
    }
}

impl Widget for EvidenceLens<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(4)])
            .split(area);
        render_search(self.input, rows[0], buf, self.theme, self.caps, self.active);

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
                    Constraint::Percentage(28),
                    Constraint::Percentage(26),
                    Constraint::Percentage(24),
                    Constraint::Percentage(22),
                ])
                .split(rows[1])
        };

        render_timeline(self.input, body[0], buf, self.theme, self.caps, self.active);
        render_graph(self.input, body[1], buf, self.theme, self.active);
        render_receipts(self.input, body[2], buf, self.theme, self.active);
        render_bundle(self.input, body[3], buf, self.theme, self.active);
    }
}

fn render_search(
    input: EvidenceLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: EvidencePane,
) {
    let proofs = input.proof_hits();
    let receipts = input.receipt_hits();
    let badge = proof_confidence_badge(overall_confidence(&proofs), theme, caps);
    let query = if input.search.query.trim().is_empty() {
        "all"
    } else {
        input.search.query.trim()
    };
    let entity = input
        .search
        .selected_entity
        .as_ref()
        .map_or_else(|| "fleet".to_string(), |entity| entity.display());

    Panel::new("Evidence Flight Recorder / Proof Search", theme)
        .active(active == EvidencePane::Search, theme)
        .badge(badge)
        .line(Line::from(Span::styled(
            format!("  query {query}  entity {entity}"),
            theme.primary(),
        )))
        .line(Line::from(Span::styled(
            format!(
                "  proofs {}  receipts {}  cursor {}",
                proofs.len(),
                receipts.len(),
                input.event_page.next_cursor
            ),
            theme.secondary(),
        )))
        .render(area, buf);
}

fn render_timeline(
    input: EvidenceLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: EvidencePane,
) {
    let proofs = input.proof_hits();
    let badge = proof_confidence_badge(overall_confidence(&proofs), theme, caps);
    let mut panel = Panel::new("Proof Timeline", theme)
        .active(active == EvidencePane::Timeline, theme)
        .badge(badge)
        .empty("NO PROOF");
    for proof in proofs.iter().take(area.height.saturating_sub(2) as usize) {
        let label = if proof.confidence == ProofConfidence::Missing {
            "NO PROOF"
        } else {
            proof.status.as_str()
        };
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} {} {}",
                truncate_label(&proof.proof_id, 22),
                label,
                proof.source.label()
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn render_graph(
    input: EvidenceLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: EvidencePane,
) {
    let graph = build_entity_proof_graph(input);
    let mut panel = Panel::new("Entity Proof Graph", theme)
        .active(active == EvidencePane::Graph, theme)
        .empty("No graph links")
        .line(Line::from(Span::styled(
            format!("  nodes {}  edges {}", graph.nodes.len(), graph.edges.len()),
            theme.secondary(),
        )));
    for edge in graph
        .edges
        .iter()
        .take(area.height.saturating_sub(3) as usize)
    {
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} -> {} {}",
                truncate_label(&edge.from, 18),
                truncate_label(&edge.to, 18),
                edge.label
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn render_receipts(
    input: EvidenceLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: EvidencePane,
) {
    let receipts = input.receipt_hits();
    let mut panel = Panel::new("Receipts", theme)
        .active(active == EvidencePane::Receipts, theme)
        .empty("No receipts");
    for receipt in receipts.iter().take(area.height.saturating_sub(2) as usize) {
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} {} {}",
                truncate_label(&receipt.receipt_id, 20),
                receipt.status_label(),
                truncate_label(&receipt.action_id, 18)
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn render_bundle(
    input: EvidenceLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: EvidencePane,
) {
    let bundle = input.bundle_preview();
    let mut panel = Panel::new("Redacted Bundle", theme)
        .active(active == EvidencePane::Bundle, theme)
        .line(Line::from(Span::styled(
            format!("  {}", bundle.bundle_id),
            theme.secondary(),
        )))
        .line(Line::from(Span::styled(
            format!(
                "  redacted {}  items {}",
                bundle.redacted_fields.len(),
                bundle.line_items.len()
            ),
            theme.primary(),
        )));
    for item in bundle
        .line_items
        .iter()
        .take(area.height.saturating_sub(4) as usize)
    {
        panel = panel.line(Line::from(Span::styled(
            format!("  {}", truncate_label(item, 48)),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn overall_confidence(proofs: &[crate::tui::lenses::evidence::ProofHit]) -> ProofConfidence {
    if proofs.is_empty()
        || proofs
            .iter()
            .any(|proof| proof.confidence == ProofConfidence::Missing)
    {
        ProofConfidence::Missing
    } else if proofs
        .iter()
        .any(|proof| proof.confidence == ProofConfidence::Stale)
    {
        ProofConfidence::Stale
    } else if proofs
        .iter()
        .any(|proof| proof.confidence == ProofConfidence::Heuristic)
    {
        ProofConfidence::Heuristic
    } else {
        ProofConfidence::Measured
    }
}
