use super::*;

#[test]
fn defaults_round_trip() {
    let s = Settings::default();
    let json = serde_json::to_string_pretty(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(s2.gitlab.http_port, 8929);
    assert_eq!(s2.git.mode, "after_success");
    assert_eq!(s2.mirror.remote, "jeryu");
    assert_eq!(s2.release.repo_root.as_deref(), Some("/home/ubuntu/dougx"));
    assert_eq!(s2.webhook.bind, "127.0.0.1:9777");
    assert_eq!(s2.mcp.bind, "127.0.0.1:9778");
    assert_eq!(s2.pool.runner_shutdown_timeout_secs, 3600);
    assert!(s2.sccache.enabled);
    assert_eq!(s2.sccache.cache_size, "10G");
    assert_eq!(s2.tui.sync_interval_ms, 5000);
    assert!(!s2.sandbox.strict_network_isolation);
}

#[test]
fn unknown_keys_ignored() {
    let json = r#"{"gitlab": {"http_port": 9999}, "unknown_future_key": true}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(s.gitlab.http_port, 9999);
    // All other fields should be their defaults
    assert_eq!(s.webhook.bind, "127.0.0.1:9777");
    assert_eq!(s.mcp.bind, "127.0.0.1:9778");
    assert_eq!(s.pool.runner_shutdown_timeout_secs, 3600);
}

#[test]
fn partial_section_uses_defaults() {
    let json = r#"{"sccache": {"cache_size": "20G"}}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(s.sccache.cache_size, "20G");
    assert!(s.sccache.enabled); // default preserved
    assert_eq!(s.sccache.binary_version, "v0.9.1"); // default preserved
}
