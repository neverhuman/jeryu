//! Owner: messaging::consumer_loop — drains topic events into the inline dispatch
//! Proof: `cargo nextest run -p jeryu --features jansu-broker --test jansu_webhook_jobs_roundtrip`
//! Invariants:
//!   - one consumer per topic (no cross-topic ordering required)
//!   - shutdown_rx cancels cleanly without losing in-flight records
//!   - decode failures are logged and skipped, never panic
//!
//! Spawned at jeryu startup once the EngineState is constructed. Each topic
//! has its own consumer task; the supervisor returns after all three exit.

use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::{broker::BrokerHandle, topics};
use crate::engine::{SharedState, dispatch_inline};

const POLL_BUDGET: Duration = Duration::from_millis(250);

/// Map a jansu topic name back to the GitLab webhook event header value the
/// inline dispatcher expects. Kept in lock-step with engine_webhook.rs.
fn event_type_for_topic(topic: &str) -> &'static str {
    match topic {
        topics::JOBS => "Job Hook",
        topics::PIPELINES => "Pipeline Hook",
        topics::PUSHES => "Push Hook",
        _ => "unknown",
    }
}

/// Drive one topic into `dispatch_inline`. Exits cleanly when `shutdown` flips
/// to true. The caller owns the JoinHandle.
async fn drain_topic(
    state: SharedState,
    broker: BrokerHandle,
    topic: &'static str,
    mut shutdown: watch::Receiver<bool>,
) {
    let event_type = event_type_for_topic(topic);
    let mut consumer = broker.consumer(topic, 0);
    info!(topic, event_type, "consumer started");

    loop {
        if *shutdown.borrow() {
            debug!(topic, "consumer shutdown signal received");
            break;
        }

        tokio::select! {
            biased;
            _ = shutdown.changed() => {
                debug!(topic, "consumer shutdown via watch");
                break;
            }
            poll = consumer.next_with_timeout(POLL_BUDGET) => {
                match poll {
                    Ok(Some(record)) => {
                        let body = match String::from_utf8(record.payload) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!(topic, error = %e, "non-UTF-8 payload — skipped");
                                continue;
                            }
                        };
                        debug!(
                            topic,
                            offset = record.offset,
                            key_len = record.key.as_ref().map(|k| k.len()).unwrap_or(0),
                            "dispatching record"
                        );
                        dispatch_inline(&state, event_type, body).await;
                    }
                    Ok(None) => {
                        // timeout — loop back to re-check shutdown
                    }
                    Err(e) => {
                        error!(topic, error = %e, "consume error — backing off");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    info!(topic, "consumer stopped");
}

/// Spawn one consumer task per webhook topic. Returns a handle that joins
/// all three when awaited.
pub fn spawn(
    state: SharedState,
    broker: BrokerHandle,
    shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(topics::ALL.len());
        for &topic in topics::ALL {
            handles.push(tokio::spawn(drain_topic(
                state.clone(),
                broker.clone(),
                topic,
                shutdown.clone(),
            )));
        }
        for h in handles {
            if let Err(e) = h.await {
                error!(error = %e, "consumer task panicked");
            }
        }
    })
}
