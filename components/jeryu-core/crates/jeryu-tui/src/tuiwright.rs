//! tuiwright UX-QA receipt emission.
//!
//! The snapshot tests in `tests/lens_snapshots.rs` already render every
//! [`ActiveTab`] at 80x24 and 120x40 across the healthy and degraded fixtures.
//! This module turns that same sweep into a machine-readable **receipt**: a JSON
//! ledger written under `target/jankurai/ux-qa/` recording, for every rendered
//! frame, the `{tab, width, height, fixture, frame_hash, ok}` row.
//!
//! ## Determinism
//! A UX-QA receipt is only useful if it is reproducible. Two things are kept
//! deterministic on purpose:
//!
//! - **The timestamp is injected, never wall-clock.** [`receipt_stamp`] derives
//!   a stable ISO-8601 label from `SOURCE_DATE_EPOCH` (the reproducible-builds
//!   convention) and falls back to a fixed label when the env var is unset or
//!   unparseable. Nothing here calls `Utc::now()`, so the receipt's identity is
//!   a pure function of its inputs.
//! - **The frame hash is a pure FNV-1a** over the flattened cell text, so the
//!   same model + size + stream always produces the same `frame_hash` with no
//!   external hashing crate.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::{ActiveTab, App};
use crate::runtime::render::render_once;
use crate::widgets::header::StreamMode;

use jeryu_readmodel::TuiReadModel;

/// The two terminal geometries the Flight Deck UX-QA sweep certifies: the
/// compact 80x24 baseline and the wide 120x40 layout.
pub const SWEEP_SIZES: &[(u16, u16)] = &[(80, 24), (120, 40)];

/// Stable label used when no `SOURCE_DATE_EPOCH` is provided. Chosen so an
/// un-stamped receipt is still obviously a fixed, non-wall-clock value.
pub const DEFAULT_STAMP: &str = "1970-01-01T00:00:00Z";

/// One certified frame in a [`TuiwrightReceipt`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRecord {
    /// The [`ActiveTab::label`] of the rendered tab.
    pub tab: String,
    /// Terminal width the frame was rendered at.
    pub width: u16,
    /// Terminal height the frame was rendered at.
    pub height: u16,
    /// Fixture family the frame was rendered from (e.g. `healthy`/`degraded`).
    pub fixture: String,
    /// Deterministic FNV-1a hash of the flattened cell text, rendered as hex.
    pub frame_hash: String,
    /// Whether the frame rendered cleanly: brand chrome present and no
    /// "not yet ported" placeholder leaked through.
    pub ok: bool,
}

/// A full tuiwright UX-QA receipt: the stamp plus every certified frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiwrightReceipt {
    /// Receipt schema tag — lets downstream tools recognize the shape.
    pub kind: String,
    /// Injected ISO-8601 stamp (see [`receipt_stamp`]).
    pub stamp: String,
    /// The per-frame ledger.
    pub frames: Vec<FrameRecord>,
}

impl TuiwrightReceipt {
    /// Total number of certified frames.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the receipt has no frames.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Number of frames that rendered cleanly.
    pub fn ok_count(&self) -> usize {
        self.frames.iter().filter(|f| f.ok).count()
    }

    /// Whether the receipt covers `tab` at `width`x`height` for `fixture`.
    pub fn covers(&self, tab: ActiveTab, width: u16, height: u16, fixture: &str) -> bool {
        let label = tab.label();
        self.frames.iter().any(|f| {
            f.tab == label && f.width == width && f.height == height && f.fixture == fixture
        })
    }
}

/// Derive the deterministic receipt stamp.
///
/// Reads `SOURCE_DATE_EPOCH` (seconds since the Unix epoch, the
/// reproducible-builds convention) and formats it as an ISO-8601 UTC instant.
/// When the var is missing or unparseable, returns [`DEFAULT_STAMP`]. Never
/// consults the wall clock, so callers can pin the receipt's identity.
pub fn receipt_stamp() -> String {
    match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(raw) => stamp_from_epoch(raw.trim()),
        Err(_) => DEFAULT_STAMP.to_string(),
    }
}

/// Format a `SOURCE_DATE_EPOCH` seconds string as an ISO-8601 UTC instant,
/// falling back to [`DEFAULT_STAMP`] on any parse failure.
fn stamp_from_epoch(raw: &str) -> String {
    let secs: i64 = match raw.parse() {
        Ok(s) => s,
        Err(_) => return DEFAULT_STAMP.to_string(),
    };
    match chrono_like_iso(secs) {
        Some(iso) => iso,
        None => DEFAULT_STAMP.to_string(),
    }
}

/// Format a Unix timestamp (seconds, UTC) as `YYYY-MM-DDTHH:MM:SSZ` without
/// pulling in `chrono` here — a small, total civil-time conversion that keeps
/// the receipt module dependency-light and obviously deterministic.
fn chrono_like_iso(secs: i64) -> Option<String> {
    if secs < 0 {
        return None;
    }
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Civil-from-days (Howard Hinnant's algorithm), epoch = 1970-01-01.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    Some(format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z"))
}

/// Deterministic FNV-1a hash of the flattened cell text, hex-encoded.
fn frame_hash(ink: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = FNV_OFFSET;
    for b in ink.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    format!("{h:016x}")
}

/// Render `tab` at `width`x`height` from `model` under `stream`, certifying one
/// [`FrameRecord`] for the `fixture` family.
pub fn certify_frame(
    model: TuiReadModel,
    tab: ActiveTab,
    width: u16,
    height: u16,
    fixture: &str,
    stream: StreamMode,
) -> FrameRecord {
    let mut app = App::new_render_only(model);
    app.set_tab(tab);
    let ink = render_once(&app, width, height, stream);
    let ok = ink.contains("jeryu") && !ink.contains("not yet ported");
    FrameRecord {
        tab: tab.label().to_string(),
        width,
        height,
        fixture: fixture.to_string(),
        frame_hash: frame_hash(&ink),
        ok,
    }
}

/// Build a receipt that sweeps **every** [`ActiveTab`] across both
/// [`SWEEP_SIZES`] for one fixture family.
///
/// `model_for` is invoked per frame so callers hand back a fresh snapshot
/// (the read model is consumed by rendering).
pub fn sweep_fixture<F>(
    stamp: impl Into<String>,
    fixture: &str,
    stream: StreamMode,
    mut model_for: F,
) -> TuiwrightReceipt
where
    F: FnMut() -> TuiReadModel,
{
    let mut frames = Vec::with_capacity(ActiveTab::ALL.len() * SWEEP_SIZES.len());
    for tab in ActiveTab::ALL {
        for &(width, height) in SWEEP_SIZES {
            frames.push(certify_frame(
                model_for(),
                *tab,
                width,
                height,
                fixture,
                stream,
            ));
        }
    }
    TuiwrightReceipt {
        kind: "tuiwright.ux-qa".to_string(),
        stamp: stamp.into(),
        frames,
    }
}

/// The UX-QA receipt directory under a workspace `target/` dir.
pub fn receipt_dir(target_dir: &Path) -> PathBuf {
    target_dir.join("jankurai").join("ux-qa")
}

/// Write `receipt` as pretty JSON to `target/jankurai/ux-qa/tuiwright-<stamp>.json`.
///
/// The stamp is sanitized for filesystem safety (`:` is illegal on some hosts).
/// Returns the path written.
pub fn write_receipt(target_dir: &Path, receipt: &TuiwrightReceipt) -> std::io::Result<PathBuf> {
    let dir = receipt_dir(target_dir);
    std::fs::create_dir_all(&dir)?;
    let safe_stamp: String = receipt
        .stamp
        .chars()
        .map(|c| if c == ':' { '-' } else { c })
        .collect();
    let path = dir.join(format!("tuiwright-{safe_stamp}.json"));
    let json = serde_json::to_string_pretty(receipt)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_defaults_to_stable_label_when_unset() {
        // The default must never be a wall-clock value.
        assert_eq!(stamp_from_epoch("not-a-number"), DEFAULT_STAMP);
        assert_eq!(stamp_from_epoch("-5"), DEFAULT_STAMP);
    }

    #[test]
    fn stamp_from_epoch_is_iso_utc() {
        // 1_700_000_000 == 2023-11-14T22:13:20Z.
        assert_eq!(stamp_from_epoch("1700000000"), "2023-11-14T22:13:20Z");
        // Epoch zero round-trips to the default label's instant.
        assert_eq!(stamp_from_epoch("0"), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn frame_hash_is_deterministic_and_distinct() {
        assert_eq!(frame_hash("abc"), frame_hash("abc"));
        assert_ne!(frame_hash("abc"), frame_hash("abd"));
    }
}
