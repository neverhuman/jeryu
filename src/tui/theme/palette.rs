use ratatui::style::{Color, Modifier, Style};

use crate::api::entity::Severity;

#[derive(Debug, Clone)]
pub struct Theme {
    pub ok: Color,
    pub running: Color,
    pub waiting: Color,
    pub warning: Color,
    pub fail: Color,
    pub blocked: Color,
    pub skipped: Color,
    pub security: Color,
    pub production: Color,
    pub agent: Color,
    pub vti_fire: Color,
    pub selection: Color,
    pub border_subtle: Color,
    pub border_active: Color,
    pub border_accent: Color,
    pub inactive: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_inverse: Color,
    pub bg_primary: Color,
    pub bg_surface: Color,
    pub bg_highlight: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            ok: Color::Rgb(102, 204, 153),
            running: Color::Rgb(102, 178, 255),
            waiting: Color::Rgb(255, 204, 102),
            warning: Color::Rgb(255, 178, 102),
            fail: Color::Rgb(255, 102, 102),
            blocked: Color::Rgb(204, 102, 255),
            skipped: Color::Rgb(128, 128, 128),
            security: Color::Rgb(255, 102, 178),
            production: Color::Rgb(255, 80, 80),
            agent: Color::Rgb(102, 255, 255),
            vti_fire: Color::Rgb(255, 165, 0),
            selection: Color::Rgb(0, 150, 200),
            border_subtle: Color::Rgb(60, 60, 70),
            border_active: Color::Rgb(102, 178, 255),
            border_accent: Color::Rgb(102, 255, 255),
            inactive: Color::Rgb(100, 100, 80),
            text_primary: Color::Rgb(230, 230, 230),
            text_secondary: Color::Rgb(170, 170, 180),
            text_muted: Color::Rgb(100, 100, 110),
            text_inverse: Color::Rgb(20, 20, 25),
            bg_primary: Color::Reset,
            bg_surface: Color::Rgb(30, 30, 38),
            bg_highlight: Color::Rgb(45, 45, 55),
        }
    }

    pub fn high_contrast() -> Self {
        Self {
            ok: Color::Green,
            running: Color::Cyan,
            waiting: Color::Yellow,
            warning: Color::Yellow,
            fail: Color::Red,
            blocked: Color::Magenta,
            skipped: Color::DarkGray,
            security: Color::Red,
            production: Color::LightRed,
            agent: Color::LightCyan,
            vti_fire: Color::LightYellow,
            selection: Color::White,
            border_subtle: Color::Gray,
            border_active: Color::White,
            border_accent: Color::LightCyan,
            inactive: Color::DarkGray,
            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_muted: Color::DarkGray,
            text_inverse: Color::Black,
            bg_primary: Color::Reset,
            bg_surface: Color::Reset,
            bg_highlight: Color::DarkGray,
        }
    }

    pub fn status_color(&self, status: &str) -> Color {
        match status {
            "success" | "passed" | "green" | "released" | "omitted" => self.ok,
            "running" | "in-flight" | "canary-authorized" => self.running,
            "pending"
            | "created"
            | "waiting"
            | "waiting_for_resource"
            | "preparing"
            | "ready-for-canary" => self.waiting,
            "failed" => self.fail,
            "blocked" | "blocked-by-upstream" => self.blocked,
            "canceled" | "vti-skipped" | "skipped" => self.skipped,
            _ => self.text_muted,
        }
    }

    pub fn status_glyph(&self, status: &str) -> &'static str {
        crate::tui::theme::GlyphSet::unicode().status(status)
    }

    pub fn bold(&self, color: Color) -> Style {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    pub fn secondary(&self) -> Style {
        Style::default().fg(self.text_secondary)
    }

    pub fn primary(&self) -> Style {
        Style::default().fg(self.text_primary)
    }

    pub fn border_style_for(&self, active: bool) -> Style {
        if active {
            Style::default().fg(self.border_active)
        } else {
            Style::default().fg(self.border_subtle)
        }
    }

    pub fn severity_color(&self, severity: Severity) -> Color {
        match severity {
            Severity::Critical => self.fail,
            Severity::Error => self.warning,
            Severity::Warning => self.waiting,
            Severity::Info => self.text_muted,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
