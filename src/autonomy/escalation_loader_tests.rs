use super::*;
use crate::autonomy::escalation::EscalationKind;
use std::fs;
use tempfile::tempdir;

#[test]
fn load_returns_default_when_autonomy_yml_missing() {
    let dir = tempdir().unwrap();
    let cfg = load_escalation_config(dir.path()).expect("missing file -> default");
    assert!(!cfg.enabled);
    assert!(cfg.on_events.is_empty());
    assert!(cfg.webhooks.is_empty());
}

#[test]
fn load_returns_default_when_escalation_key_missing() {
    let dir = tempdir().unwrap();
    let yml = "schema: vibegate.autonomy.v1\ndefault_profile: supervised\n";
    fs::write(dir.path().join("autonomy.yml"), yml).unwrap();
    let cfg = load_escalation_config(dir.path()).expect("no escalation key -> default");
    assert!(!cfg.enabled);
    assert!(cfg.on_events.is_empty());
    assert!(cfg.webhooks.is_empty());
}

#[test]
fn load_parses_full_three_webhook_config() {
    let dir = tempdir().unwrap();
    let yml = r##"
schema: vibegate.autonomy.v1
escalation:
  enabled: true
  on_events: [require_human, kill_bell_engaged]
  webhooks:
    - kind: slack
      url_secret_name: SLACK_WEBHOOK_URL
      channel: "#jeryu-needs-you"
    - kind: pagerduty
      url_secret_name: PAGERDUTY_INTEGRATION_URL
      severity: warning
    - kind: generic_json
      url_secret_name: ESCALATION_WEBHOOK_URL
      headers:
        X-Source: jeryu
"##;
    fs::write(dir.path().join("autonomy.yml"), yml).unwrap();
    let cfg = load_escalation_config(dir.path()).expect("parses");
    assert!(cfg.enabled);
    assert_eq!(cfg.on_events, vec!["require_human", "kill_bell_engaged"]);
    assert_eq!(cfg.webhooks.len(), 3);
    assert_eq!(cfg.webhooks[0].kind, EscalationKind::Slack);
    assert_eq!(cfg.webhooks[0].url_secret_name, "SLACK_WEBHOOK_URL");
    assert_eq!(cfg.webhooks[1].kind, EscalationKind::PagerDuty);
    assert_eq!(cfg.webhooks[1].severity.as_deref(), Some("warning"));
    assert_eq!(cfg.webhooks[2].kind, EscalationKind::GenericJson);
    assert_eq!(
        cfg.webhooks[2].headers.get("X-Source"),
        Some(&"jeryu".to_string())
    );
}

#[test]
fn load_handles_unknown_keys_gracefully() {
    let dir = tempdir().unwrap();
    // `escalate_after_minutes`, `someday_field` are not in the schema.
    let yml = r##"
schema: vibegate.autonomy.v1
public_name: "Evidence Gate"
escalation:
  enabled: true
  escalate_after_minutes: 30
  someday_field: 42
  on_events: [require_human]
  webhooks:
    - kind: slack
      url_secret_name: SLACK_WEBHOOK_URL
      unknown_per_webhook_field: ignore_me
"##;
    fs::write(dir.path().join("autonomy.yml"), yml).unwrap();
    let cfg = load_escalation_config(dir.path()).expect("unknown keys ignored");
    assert!(cfg.enabled);
    assert_eq!(cfg.on_events, vec!["require_human"]);
    assert_eq!(cfg.webhooks.len(), 1);
    assert_eq!(cfg.webhooks[0].kind, EscalationKind::Slack);
}

#[test]
fn load_with_slack_only_returns_one_webhook() {
    let dir = tempdir().unwrap();
    let yml = r##"
escalation:
  enabled: true
  on_events: [require_human]
  webhooks:
    - kind: slack
      url_secret_name: SLACK_WEBHOOK_URL
"##;
    fs::write(dir.path().join("autonomy.yml"), yml).unwrap();
    let cfg = load_escalation_config(dir.path()).expect("parses");
    assert_eq!(cfg.webhooks.len(), 1);
    assert_eq!(cfg.webhooks[0].kind, EscalationKind::Slack);
    assert!(cfg.webhooks[0].channel.is_none());
    assert!(cfg.webhooks[0].severity.is_none());
    assert!(cfg.webhooks[0].headers.is_empty());
}

#[test]
fn load_returns_err_on_invalid_yaml() {
    let dir = tempdir().unwrap();
    // Unbalanced bracket + bad indentation — not valid YAML.
    let yml = "escalation:\n  enabled: true\n  webhooks: [oops\n";
    fs::write(dir.path().join("autonomy.yml"), yml).unwrap();
    let err = load_escalation_config(dir.path()).expect_err("must error on malformed YAML");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("YAML") || msg.contains("yaml") || msg.contains("parsing"),
        "expected YAML-related error, got: {msg}"
    );
}

#[test]
fn load_with_disabled_true_still_returns_webhooks() {
    // The CLI's "list webhooks" / "dry-run" path needs to see the
    // configured webhooks even when escalation is globally off.
    let dir = tempdir().unwrap();
    let yml = r##"
escalation:
  enabled: false
  on_events: [require_human]
  webhooks:
    - kind: slack
      url_secret_name: SLACK_WEBHOOK_URL
    - kind: pagerduty
      url_secret_name: PAGERDUTY_INTEGRATION_URL
"##;
    fs::write(dir.path().join("autonomy.yml"), yml).unwrap();
    let cfg = load_escalation_config(dir.path()).expect("parses");
    assert!(!cfg.enabled);
    assert_eq!(cfg.webhooks.len(), 2);
    // permits() must still be false because enabled=false (fail-closed).
    assert!(!cfg.permits("require_human"));
}

#[test]
fn load_from_repo_root_actual_autonomy_yml_round_trips() {
    let repo_autonomy_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".jeryu/autonomy");
    if !repo_autonomy_dir.join("autonomy.yml").exists() {
        // Not all consumers of this crate keep .jeryu/autonomy/ at the manifest
        // root; skip rather than fail.
        return;
    }
    let cfg = load_escalation_config(&repo_autonomy_dir)
        .expect("repo .jeryu/autonomy/autonomy.yml must parse");
    assert!(
        !cfg.webhooks.is_empty(),
        "expected the canonical config to ship with at least one webhook"
    );
}

#[test]
fn build_default_dispatcher_returns_usable_value() {
    let resolver = Arc::new(SecretResolver::default());
    let dispatcher = build_default_dispatcher(resolver);
    assert!(Arc::strong_count(&dispatcher.secret_resolver) >= 1);
}
