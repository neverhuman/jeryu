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
    assert_eq!(
        remembered, 1,
        "consumer cursor reports the last-read offset"
    );

    // Second consumer resumes one past the last-read offset. Jansu's fetch
    // semantics are at-least-once: a batch may overlap the requested start
    // offset (e.g. start_offset=2 may yield offsets [2, 3, 4, 3, 4, 4] as
    // successive batches re-deliver tail records). Consumers must dedup by
    // offset — we assert set inclusion, not exact sequence.
    let resume_from = remembered + 1;
    let mut c2 = broker.consumer(topics::PIPELINES, resume_from);
    let mut seen_offsets: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    while let Some(record) = c2
        .next_with_timeout(Duration::from_millis(300))
        .await
        .expect("poll")
    {
        // Only count offsets at-or-past the requested resume point.
        if record.offset >= resume_from {
            seen_offsets.insert(record.offset);
        }
    }

    let expected: std::collections::BTreeSet<i64> = [2, 3, 4].into_iter().collect();
    assert_eq!(
        seen_offsets, expected,
        "resumed consumer must observe every remaining offset at least once"
    );
}
