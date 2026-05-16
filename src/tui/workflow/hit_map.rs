//! Owner: Interactive TUI subsystem — Delivery hit-map
//! Proof: tests in pr_rail / minimap modules
//! Invariants: Populated by renderer each frame; read by mouse handler.

use ratatui::layout::Rect;

/// Region rectangles + per-card hit boxes captured during rendering.
/// Read by the mouse handler to translate (col, row) into actions.
#[derive(Debug, Clone, Default)]
pub struct DeliveryHitMap {
    pub mission: Option<Rect>,
    pub pr_rail: Option<Rect>,
    pub phase_rail: Option<Rect>,
    pub canvas: Option<Rect>,
    pub minimap: Option<Rect>,
    pub inspector: Option<Rect>,
    /// Per-node hit boxes laid out on the visible canvas; rect + (phase_idx, node_idx).
    pub cards: Vec<(Rect, usize, usize)>,
}

impl DeliveryHitMap {
    pub fn contains(rect: Option<Rect>, x: u16, y: u16) -> bool {
        match rect {
            Some(r) => x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height,
            None => false,
        }
    }

    pub fn card_at(&self, x: u16, y: u16) -> Option<(usize, usize)> {
        for (r, pi, ni) in &self.cards {
            if x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height {
                return Some((*pi, *ni));
            }
        }
        None
    }
}
