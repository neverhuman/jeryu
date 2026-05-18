//! Owner: integration test for Wave 11.C Phase 5 — topic isolation
//! Proof: `cargo nextest run -p jeryu --features jansu-broker --test jansu_three_topics_no_crosstalk`
//! Invariants: producing to topic A never appears on consumer for topic B/C

#![cfg(feature = "jansu-broker")]

use std::time::Duration;

use jeryu::messaging::{init_broker, topics};

#[tokio::test]
async fn jansu_three_topics_have_no_crosstalk() {
    let broker = init_broker().await.expect("broker init");

    let jobs_payload = b"JOBS-marker";
    let pipelines_payload = b"PIPELINES-marker";
    let pushes_payload = b"PUSHES-marker";

    broker
        .send(topics::JOBS, Some(b"k-jobs"), jobs_payload)
        .await
        .expect("send jobs");
    broker
        .send(topics::PIPELINES, Some(b"k-pipelines"), pipelines_payload)
        .await
        .expect("send pipelines");
    broker
        .send(topics::PUSHES, Some(b"k-pushes"), pushes_payload)
        .await
        .expect("send pushes");

    let mut seen_jobs = collect_topic_payloads(&broker, topics::JOBS).await;
    let mut seen_pipelines = collect_topic_payloads(&broker, topics::PIPELINES).await;
    let mut seen_pushes = collect_topic_payloads(&broker, topics::PUSHES).await;

    // Each topic must contain its own marker and NOT contain the others.
    assert!(
        seen_jobs.iter().any(|p| p == jobs_payload),
        "jobs missing own marker"
    );
    assert!(
        !seen_jobs.iter().any(|p| p == pipelines_payload),
        "jobs leaked pipeline marker"
    );
    assert!(
        !seen_jobs.iter().any(|p| p == pushes_payload),
        "jobs leaked push marker"
    );

    assert!(seen_pipelines.iter().any(|p| p == pipelines_payload));
    assert!(!seen_pipelines.iter().any(|p| p == jobs_payload));
    assert!(!seen_pipelines.iter().any(|p| p == pushes_payload));

    assert!(seen_pushes.iter().any(|p| p == pushes_payload));
    assert!(!seen_pushes.iter().any(|p| p == jobs_payload));
    assert!(!seen_pushes.iter().any(|p| p == pipelines_payload));

    // Quiet unused-mut warnings: we collected then consumed via iter().
    seen_jobs.clear();
    seen_pipelines.clear();
    seen_pushes.clear();
}

async fn collect_topic_payloads(
    broker: &jeryu::messaging::BrokerHandle,
    topic: &str,
) -> Vec<Vec<u8>> {
    let mut consumer = broker.consumer(topic, 0);
    let mut out = Vec::new();
    while let Some(record) = consumer
        .next_with_timeout(Duration::from_millis(300))
        .await
        .expect("poll")
    {
        out.push(record.payload);
    }
    out
}
