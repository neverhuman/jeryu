//! Owner: integration test for Wave 11.C Phase 5 (Jansu webhook dispatch)
//! Proof: `cargo nextest run -p jeryu --features jansu-broker --test jansu_webhook_jobs_roundtrip`
//! Invariants:
//!   - producer → consumer round-trip preserves payload bytes exactly
//!   - delivery UUID is passed as the message key for idempotency
//!   - consumer can be polled with a budget and returns None on empty topic

#![cfg(feature = "jansu-broker")]

use std::time::Duration;

use jeryu::messaging::{init_broker, topics};

#[tokio::test]
async fn jansu_webhook_jobs_roundtrip_preserves_payload() {
    let broker = init_broker().await.expect("broker init");
    let payload = br#"{"build_id":42,"project_id":7,"build_status":"success"}"#;
    let uuid = b"00000000-0000-0000-0000-000000000001";

    broker
        .send(topics::JOBS, Some(uuid), payload)
        .await
        .expect("send");

    let mut consumer = broker.consumer(topics::JOBS, 0);
    let record = consumer
        .next_with_timeout(Duration::from_secs(2))
        .await
        .expect("poll")
        .expect("got record");

    assert_eq!(record.payload, payload, "payload bytes preserved");
    assert_eq!(
        record.key.as_deref(),
        Some(uuid.as_slice()),
        "key preserved"
    );
    assert!(record.offset >= 0);
}

#[tokio::test]
async fn jansu_empty_topic_polls_return_none_within_budget() {
    let broker = init_broker().await.expect("broker init");
    // Drain the topic so the test starts clean — earlier tests in this binary
    // may have produced records (broker is a process singleton).
    let mut consumer = broker.consumer(topics::PUSHES, i64::MAX);

    let start = std::time::Instant::now();
    let result = consumer
        .next_with_timeout(Duration::from_millis(300))
        .await
        .expect("poll");
    let elapsed = start.elapsed();

    assert!(result.is_none(), "no record available on virgin offset");
    // Confirm the timeout actually bounds the poll.
    assert!(
        elapsed < Duration::from_secs(2),
        "next_with_timeout returned in {elapsed:?}, expected < 2s"
    );
}
