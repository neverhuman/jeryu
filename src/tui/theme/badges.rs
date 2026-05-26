use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

use crate::{
    api::freshness::{FreshnessState, SourceFreshness},
    tui::{
        action_registry::ActionRiskTier,
        runtime::stream::StreamMode,
        theme::{GlyphSet, TerminalCaps, Theme},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    pub glyph: &'static str,
    pub label: String,
    pub color: Color,
    pub requires_proof: bool,
}

impl Badge {
    pub fn span(&self) -> Span<'static> {
        Span::styled(
            format!("[{}]", self.label),
            Style::default().fg(self.color).add_modifier(Modifier::BOLD),
        )
    }

    pub fn glyph_span(&self) -> Span<'static> {
        Span::styled(self.glyph.to_string(), Style::default().fg(self.color))
    }
}

pub fn status_badge(status: &str, theme: &Theme, caps: TerminalCaps) -> Badge {
    let glyphs = GlyphSet::for_caps(caps);
    let (label, color) = match status {
        "success" | "passed" | "green" | "released" => ("PASS", theme.ok),
        "running" | "in-flight" | "canary-authorized" => ("RUN", theme.running),
        "pending"
        | "created"
        | "waiting"
        | "waiting_for_resource"
        | "preparing"
        | "ready-for-canary" => ("WAIT", theme.waiting),
        "failed" => ("FAIL", theme.fail),
        "blocked" | "blocked-by-upstream" => ("BLOCK", theme.blocked),
        "canceled" | "vti-skipped" | "skipped" | "omitted" => ("SKIP", theme.skipped),
        "manual" => ("MANUAL", theme.waiting),
        _ => ("INFO", theme.text_muted),
    };
    Badge {
        glyph: glyphs.status(status),
        label: label.into(),
        color: caps.color(color),
        requires_proof: false,
    }
}

pub fn freshness_badge(freshness: &SourceFreshness, theme: &Theme, caps: TerminalCaps) -> Badge {
    let (label, color, glyph) = match freshness.state {
        FreshnessState::Live => ("LIVE".into(), theme.ok, GlyphSet::for_caps(caps).running),
        FreshnessState::Fresh => (
            with_age("FRESH", freshness.age_ms),
            theme.ok,
            GlyphSet::for_caps(caps).success,
        ),
        FreshnessState::Aged => (
            with_age("STALE", freshness.age_ms),
            theme.warning,
            GlyphSet::for_caps(caps).blocked,
        ),
        FreshnessState::LastKnown => (
            "LAST KNOWN".into(),
            theme.warning,
            GlyphSet::for_caps(caps).manual,
        ),
        FreshnessState::Inferred => (
            "INFERRED".into(),
            theme.waiting,
            GlyphSet::for_caps(caps).info,
        ),
        FreshnessState::Partial => (
            "PARTIAL".into(),
            theme.warning,
            GlyphSet::for_caps(caps).pending,
        ),
        FreshnessState::SourceDown => (
            "SOURCE DOWN".into(),
            theme.fail,
            GlyphSet::for_caps(caps).failed,
        ),
        FreshnessState::Unknown => (
            "UNKNOWN".into(),
            theme.text_muted,
            GlyphSet::for_caps(caps).info,
        ),
    };
    Badge {
        glyph,
        label,
        color: caps.color(color),
        requires_proof: freshness.state.blocks_risky_action(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofConfidence {
    Measured,
    Structural,
    Historical,
    Heuristic,
    Missing,
    Stale,
    Unverified,
}

pub fn proof_confidence_badge(
    confidence: ProofConfidence,
    theme: &Theme,
    caps: TerminalCaps,
) -> Badge {
    let (label, color, requires_proof) = match confidence {
        ProofConfidence::Measured => ("MEAS", theme.ok, false),
        ProofConfidence::Structural => ("STRUCT", theme.running, false),
        ProofConfidence::Historical => ("HIST", theme.waiting, false),
        ProofConfidence::Heuristic => ("HEUR", theme.warning, true),
        ProofConfidence::Missing => ("NO PROOF", theme.fail, true),
        ProofConfidence::Stale => ("STALE", theme.warning, true),
        ProofConfidence::Unverified => ("UNVERIFIED", theme.text_muted, true),
    };
    Badge {
        glyph: GlyphSet::for_caps(caps).proof,
        label: label.into(),
        color: caps.color(color),
        requires_proof,
    }
}

pub fn risk_badge(risk: ActionRiskTier, theme: &Theme, caps: TerminalCaps) -> Badge {
    let (color, requires_proof) = match risk {
        ActionRiskTier::R0 => (theme.ok, false),
        ActionRiskTier::R1 => (theme.waiting, false),
        ActionRiskTier::R2 => (theme.running, false),
        ActionRiskTier::R3 => (theme.warning, false),
        ActionRiskTier::R4 => (theme.fail, true),
        ActionRiskTier::R5 => (theme.production, true),
    };
    Badge {
        glyph: GlyphSet::for_caps(caps).risk,
        label: risk.label().into(),
        color: caps.color(color),
        requires_proof,
    }
}

pub fn stream_mode_badge(mode: StreamMode, theme: &Theme, caps: TerminalCaps) -> Badge {
    let (label, color) = match mode {
        StreamMode::Live => ("LIVE", theme.ok),
        StreamMode::Polling => ("POLL", theme.waiting),
        StreamMode::LastKnown => ("LAST KNOWN", theme.warning),
        StreamMode::Fixture => ("FIXTURE", theme.agent),
    };
    Badge {
        glyph: GlyphSet::for_caps(caps).running,
        label: label.into(),
        color: caps.color(color),
        requires_proof: mode == StreamMode::LastKnown,
    }
}

pub fn cache_hit_badge(theme: &Theme, caps: TerminalCaps) -> Badge {
    static_badge("HIT", GlyphSet::for_caps(caps).success, theme.ok, caps)
}

pub fn cache_taint_badge(theme: &Theme, caps: TerminalCaps) -> Badge {
    static_badge(
        "TAINT",
        GlyphSet::for_caps(caps).blocked,
        theme.blocked,
        caps,
    )
}

pub fn flake_badge(theme: &Theme, caps: TerminalCaps) -> Badge {
    static_badge("FLK?", GlyphSet::for_caps(caps).risk, theme.warning, caps)
}

fn static_badge(label: &str, glyph: &'static str, color: Color, caps: TerminalCaps) -> Badge {
    Badge {
        glyph,
        label: label.into(),
        color: caps.color(color),
        requires_proof: false,
    }
}

fn with_age(prefix: &str, age_ms: Option<u64>) -> String {
    match age_ms {
        Some(ms) => format!("{prefix} {}", compact_age(ms)),
        None => prefix.into(),
    }
}

fn compact_age(age_ms: u64) -> String {
    if age_ms < 1_000 {
        format!("{age_ms}ms")
    } else if age_ms < 60_000 {
        format!("{}s", age_ms / 1_000)
    } else if age_ms < 3_600_000 {
        format!("{}m", age_ms / 60_000)
    } else {
        format!("{}h", age_ms / 3_600_000)
    }
}
