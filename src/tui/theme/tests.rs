use chrono::Utc;
use ratatui::style::Color;

use crate::{
    api::freshness::{FreshnessState, SourceFreshness, SourceKind},
    tui::{
        action_registry::ActionRiskTier,
        runtime::stream::StreamMode,
        theme::{
            ProofConfidence, TerminalCaps, Theme, freshness_badge, proof_confidence_badge,
            risk_badge, status_badge, stream_mode_badge,
        },
    },
};

#[test]
fn dark_theme_maps_statuses() {
    let theme = Theme::dark();
    assert_eq!(theme.status_color("success"), theme.ok);
    assert_eq!(theme.status_color("failed"), theme.fail);
    assert_eq!(theme.status_color("running"), theme.running);
    assert_eq!(theme.status_color("blocked"), theme.blocked);
}

#[test]
fn status_glyphs_are_distinct() {
    let theme = Theme::dark();
    assert_ne!(theme.status_glyph("success"), theme.status_glyph("failed"));
    assert_ne!(theme.status_glyph("running"), theme.status_glyph("pending"));
}

#[test]
fn high_contrast_uses_basic_colors() {
    let theme = Theme::high_contrast();
    assert_eq!(theme.ok, Color::Green);
    assert_eq!(theme.fail, Color::Red);
}

#[test]
fn freshness_badges_render_plan_language() {
    let theme = Theme::dark();
    let mut fresh = SourceFreshness::live(SourceKind::InspectionHttp, Utc::now(), "10");
    fresh.state = FreshnessState::Fresh;
    fresh.age_ms = Some(1_500);
    assert_eq!(
        freshness_badge(&fresh, &theme, TerminalCaps::unicode()).label,
        "FRESH 1s"
    );

    fresh.state = FreshnessState::Aged;
    fresh.age_ms = Some(125_000);
    assert_eq!(
        freshness_badge(&fresh, &theme, TerminalCaps::unicode()).label,
        "STALE 2m"
    );

    fresh.state = FreshnessState::SourceDown;
    assert_eq!(
        freshness_badge(&fresh, &theme, TerminalCaps::unicode()).label,
        "SOURCE DOWN"
    );
}

#[test]
fn proof_confidence_badges_distinguish_missing_and_heuristic() {
    let theme = Theme::dark();
    let heuristic =
        proof_confidence_badge(ProofConfidence::Heuristic, &theme, TerminalCaps::ascii());
    let missing = proof_confidence_badge(ProofConfidence::Missing, &theme, TerminalCaps::ascii());

    assert_eq!(heuristic.label, "HEUR");
    assert_eq!(missing.label, "NO PROOF");
    assert!(heuristic.requires_proof);
    assert!(missing.requires_proof);
}

#[test]
fn risk_badges_mark_release_and_production_as_proof_gated() {
    let theme = Theme::dark();
    assert!(!risk_badge(ActionRiskTier::R3, &theme, TerminalCaps::unicode()).requires_proof);
    assert!(risk_badge(ActionRiskTier::R4, &theme, TerminalCaps::unicode()).requires_proof);
    assert!(risk_badge(ActionRiskTier::R5, &theme, TerminalCaps::unicode()).requires_proof);
}

#[test]
fn ascii_mode_uses_only_ascii_glyphs() {
    let theme = Theme::dark();
    let badges = [
        status_badge("success", &theme, TerminalCaps::ascii()),
        proof_confidence_badge(ProofConfidence::Measured, &theme, TerminalCaps::ascii()),
        risk_badge(ActionRiskTier::R5, &theme, TerminalCaps::ascii()),
        stream_mode_badge(StreamMode::Polling, &theme, TerminalCaps::ascii()),
    ];

    for badge in badges {
        assert!(badge.glyph.is_ascii());
        assert!(badge.label.is_ascii());
    }
}

#[test]
fn no_color_mode_resets_semantic_colors() {
    let theme = Theme::dark();
    let badge = status_badge("failed", &theme, TerminalCaps::no_color());
    assert_eq!(badge.color, Color::Reset);
}
