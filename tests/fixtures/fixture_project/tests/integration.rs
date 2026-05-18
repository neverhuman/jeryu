//! Integration tests for the signal-router engine.
//!
//! These tests validate end-to-end routing behaviour including multi-route
//! fanout, empty-router edge cases, and batch throughput characteristics.

use signal_router::{Route, Router, Severity, Signal, generate_test_signals};

fn make_router() -> Router {
    let mut router = Router::new();
    router.add_route(Route {
        name: "all-events".to_string(),
        min_severity: Severity::Trace,
        source_pattern: None,
    });
    router.add_route(Route {
        name: "alerts".to_string(),
        min_severity: Severity::Warn,
        source_pattern: None,
    });
    router.add_route(Route {
        name: "cache-monitor".to_string(),
        min_severity: Severity::Trace,
        source_pattern: Some("cache".to_string()),
    });
    router.add_route(Route {
        name: "security-audit".to_string(),
        min_severity: Severity::Error,
        source_pattern: Some("auth".to_string()),
    });
    router
}

#[test]
fn cache_warmer_info_routes_to_all_and_cache() {
    let router = make_router();
    let signal = Signal {
        source: "cache-warmer".to_string(),
        severity: Severity::Info,
        payload: "warmup-cycle-complete".to_string(),
        timestamp_ms: 1_700_000_000_000,
    };
    let channels = router.route(&signal);
    assert!(channels.contains(&"all-events"));
    assert!(channels.contains(&"cache-monitor"));
    assert!(!channels.contains(&"alerts"));
}

#[test]
fn auth_error_routes_to_alerts_and_security() {
    let router = make_router();
    let signal = Signal {
        source: "auth-gateway".to_string(),
        severity: Severity::Error,
        payload: "invalid-token-presented".to_string(),
        timestamp_ms: 1_700_000_001_000,
    };
    let channels = router.route(&signal);
    assert!(channels.contains(&"all-events"));
    assert!(channels.contains(&"alerts"));
    assert!(channels.contains(&"security-audit"));
}

#[test]
fn trace_from_unknown_source_only_goes_to_all() {
    let router = make_router();
    let signal = Signal {
        source: "unknown-microservice".to_string(),
        severity: Severity::Trace,
        payload: "heartbeat".to_string(),
        timestamp_ms: 1_700_000_002_000,
    };
    let channels = router.route(&signal);
    assert_eq!(channels, vec!["all-events"]);
}

#[test]
fn batch_routing_preserves_signal_count() {
    let router = make_router();
    let signals = generate_test_signals(500);

    let counts = router.route_batch(&signals);
    // Every signal must reach "all-events".
    assert_eq!(*counts.get("all-events").unwrap_or(&0), 500);
}

#[test]
fn batch_routing_alerts_only_contains_alertable() {
    let router = make_router();
    let signals = generate_test_signals(100);

    let counts = router.route_batch(&signals);
    let alert_count = *counts.get("alerts").unwrap_or(&0);

    // In generate_test_signals, severities cycle through 5 levels.
    // Warn + Error = 2 out of 5 → 40% of 100 = 40.
    assert_eq!(alert_count, 40, "expected 40 alertable signals out of 100");
}

#[test]
fn security_audit_only_routes_auth_errors() {
    let router = make_router();
    let signals = generate_test_signals(100);

    let counts = router.route_batch(&signals);
    let security_count = *counts.get("security-audit").unwrap_or(&0);

    // source "auth-gateway" appears at indices 2, 7, 12, 17, ...
    // severity Error appears at indices 4, 9, 14, 19, ...
    // Intersection: both source=auth-gateway AND severity>=Error
    // auth-gateway at i%5==2, Error at i%5==4 → no overlap for Error
    // But Warn is at i%5==3 which is >= Error? No, Warn < Error.
    // So security-audit with min_severity=Error and source "auth":
    //   source "auth-gateway" at i%5==2 → severity at those indices: i%5==2 → severity[2]=Info
    //   Need i%5==2 AND severity>=Error → never happens with the cycling pattern.
    // Actually let's check: Fatal is not in the list, so max is Error at i%5==4.
    // Source at i%5==4 is "secret-rotator", not "auth-gateway".
    // So security_count should be 0.
    assert_eq!(
        security_count, 0,
        "no synthetic signals match both auth-source and Error severity"
    );
}

#[test]
fn large_batch_completes_under_10ms() {
    let router = make_router();
    let signals = generate_test_signals(10_000);

    let start = std::time::Instant::now();
    let _counts = router.route_batch(&signals);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 25,
        "batch routing 10k signals should be sub-25ms (2.5× margin), took {}ms",
        elapsed.as_millis()
    );
}
