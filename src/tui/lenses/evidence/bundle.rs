use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::tui::{lenses::evidence::EvidenceLensInput, widgets::truncate_label};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedBundlePreview {
    pub bundle_id: String,
    pub proof_count: usize,
    pub receipt_count: usize,
    pub redacted_fields: Vec<String>,
    pub line_items: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

pub fn build_redacted_bundle_preview(input: EvidenceLensInput<'_>) -> RedactedBundlePreview {
    let proofs = input.proof_hits();
    let receipts = input.receipt_hits();
    let mut redacted_fields = BTreeSet::new();
    let mut line_items = Vec::new();

    for proof in proofs.iter().take(4) {
        let summary = redact_sensitive_text(&proof.summary, &mut redacted_fields);
        line_items.push(format!(
            "proof {} {}",
            proof.proof_id,
            truncate_label(&summary, 56)
        ));
    }
    for receipt in receipts.iter().take(4) {
        let summary = redact_sensitive_text(&receipt.summary, &mut redacted_fields);
        line_items.push(format!(
            "receipt {} {}",
            receipt.receipt_id,
            truncate_label(&summary, 56)
        ));
    }

    RedactedBundlePreview {
        bundle_id: format!(
            "bundle-{}-{}-{}",
            input.event_page.cursor,
            proofs.len(),
            receipts.len()
        ),
        proof_count: proofs.len(),
        receipt_count: receipts.len(),
        redacted_fields: redacted_fields.into_iter().collect(),
        line_items,
        generated_at: input.generated_at,
    }
}

pub fn redact_bundle_text(text: &str) -> String {
    let mut redacted_fields = BTreeSet::new();
    redact_sensitive_text(text, &mut redacted_fields)
}

fn redact_sensitive_text(text: &str, redacted_fields: &mut BTreeSet<String>) -> String {
    text.split_whitespace()
        .map(|word| {
            if sensitive_word(word) {
                redacted_fields.insert(redaction_label(word).to_string());
                "[REDACTED]".to_string()
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn sensitive_word(word: &str) -> bool {
    let lowered = word.to_ascii_lowercase();
    lowered.contains("token")
        || lowered.contains("secret")
        || lowered.contains("password")
        || lowered.contains("credential")
        || lowered.starts_with("ghp_")
        || lowered.starts_with("glpat-")
}

fn redaction_label(word: &str) -> &'static str {
    let lowered = word.to_ascii_lowercase();
    if lowered.contains("password") {
        "password"
    } else if lowered.contains("token")
        || lowered.starts_with("ghp_")
        || lowered.starts_with("glpat-")
    {
        "token"
    } else if lowered.contains("credential") {
        "credential"
    } else {
        "secret"
    }
}
