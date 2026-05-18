//! Owner: integration test for Wave 11.C Phase 5 — consumer offset resume
//! Proof: `cargo nextest run -p jeryu --features jansu-broker --test jansu_consumer_resumes_after_restart`
//! Invariants:
//!   - rebuilding a consumer at a remembered offset replays from that point
//!   - the broker is the source of truth; consumer state is just a cursor

#![cfg(feature = "jansu-broker")]

use std::time::Duration;

use jeryu::messaging::{init_broker, topics};

#[tokio::test]
async fn jansu_consumer_resumes_from_remembered_offset() {
    let broker = init_broker().await.expect("broker init");
    let n: usize = 5;

    // Produce N pipeline events.
    for i in 0..n {
        let body = format!(r#"{{"pipeline_id":{i}}}"#);
        broker
            .send(
                topics::PIPELINES,
                Some(format!("id-{i}").as_bytes()),
                body.as_bytes(),
            )
            .await
            .expect("send");
    }

    // First consumer reads 2 records, then "crashes" — i.e. we drop it and
    // remember the offset.
    let mut c1 = broker.consumer(topics::PIPELINES, 0);
    let r0 = c1
        .next_with_timeout(Duration::from_secs(2))
        .await
        .expect("poll")
        .expect("r0");
    let r1 = c1
        .next_with_timeout(Duration::from_secs(2))
        .await
        .expect("poll")
        .expect("r1");
    let remembered = c1.offset();
    drop(c1);

    assert_eq!(r0.offset, 0);
    assert_eq!(r1.offset, 1);
    // With the upstream J-4 fix (jansu PR #11 commit 9f61c0d), Consumer::next
    // advances self.offset on every pop — including the early-return buffered
    // path — so the cursor reports next-to-read, not last-read.
    assert_eq!(remembered, 2, "consumer cursor reports next-to-read offset");

    // Second consumer resumes at the remembered cursor (already next-to-read)
    // and sees offsets [2, 3, 4] exactly — no batch-tail redelivery.
    let mut c2 = broker.consumer(topics::PIPELINES, remembered);
    let mut seen: Vec<i64> = Vec::new();
    while let Some(record) = c2
        .next_with_timeout(Duration::from_millis(300))
        .await
        .expect("poll")
    {
        seen.push(record.offset);
    }

    assert_eq!(
        seen,
        vec![2, 3, 4],
        "resumed consumer sees the remaining offsets exactly once each"
    );
}
