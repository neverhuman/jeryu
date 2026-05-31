//! Deterministic verdict-id minting: a 30-char `vgv_`-prefixed id derived from
//! the issue timestamp and the head SHA.

use chrono::Utc;

pub(super) fn mint_verdict_id(now: chrono::DateTime<Utc>, head_sha: &str) -> String {
    let ts_hex = format!("{:013X}", now.timestamp_millis() as u64);
    let tail: String = head_sha
        .chars()
        .rev()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(13)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let mut s = format!("vgv_{ts_hex}{tail}");
    while s.len() < 30 {
        s.push('0');
    }
    s.truncate(30);
    s
}
