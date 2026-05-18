//! Owner: messaging::broker — singleton EmbeddedBroker handle + thin send/consume helpers
//! Proof: `cargo nextest run -p jeryu --features jansu-broker messaging::broker::`
//! Invariants:
//!   - exactly one EmbeddedBroker per process (OnceCell)
//!   - all topics in `topics::ALL` are pre-created at init
//!   - send() returns BrokerError, never panics on missing topic
//!   - consumer poll uses a bounded `next_with_timeout` to keep tokio::select! responsive

use std::sync::OnceLock;
use std::time::Duration;

use jansu_embedded::{Consumer, EmbeddedBroker, EmbeddedRecord, Error as EmbeddedError};
use thiserror::Error;
use tracing::{debug, info, warn};

use super::topics;

/// Wrapper around `jansu_embedded::Error` so callers depend on a stable type.
#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker not initialized — call messaging::init_broker() at startup")]
    NotInitialized,
    #[error("broker init failed: {0}")]
    Init(String),
    #[error("topic create failed for {topic}: {source}")]
    CreateTopic {
        topic: String,
        source: EmbeddedError,
    },
    #[error("produce failed on {topic}: {source}")]
    Produce {
        topic: String,
        source: EmbeddedError,
    },
    #[error("consume failed on {topic}: {source}")]
    Consume {
        topic: String,
        source: EmbeddedError,
    },
}

/// Process-wide singleton. `Clone` is cheap on the underlying broker.
static BROKER: OnceLock<EmbeddedBroker> = OnceLock::new();

/// Cheap clone of the in-process broker handle.
#[derive(Clone, Debug)]
pub struct BrokerHandle {
    inner: EmbeddedBroker,
}

impl BrokerHandle {
    /// Produce a single record. `key` is used by jansu for idempotency hashing;
    /// pass `Some(delivery_uuid_bytes)` for at-least-once dedup semantics.
    pub async fn send(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        payload: &[u8],
    ) -> Result<(), BrokerError> {
        self.inner
            .send(topic, topics::PARTITION_DEFAULT, key, payload)
            .await
            .map(|_offset| ())
            .map_err(|source| BrokerError::Produce {
                topic: topic.to_string(),
                source,
            })
    }

    /// Build a consumer that reads from offset 0 (replay) or `start_offset`.
    pub fn consumer(&self, topic: &str, start_offset: i64) -> ConsumerHandle {
        ConsumerHandle {
            topic: topic.to_string(),
            inner: self
                .inner
                .consumer(topic, topics::PARTITION_DEFAULT, start_offset),
        }
    }
}

/// Wrapper around `jansu_embedded::Consumer` adding a timeout-bounded poll
/// so it can be selected against other future arms.
pub struct ConsumerHandle {
    topic: String,
    inner: Consumer,
}

impl ConsumerHandle {
    /// Current absolute offset (next read position).
    pub fn offset(&self) -> i64 {
        self.inner.offset()
    }

    /// Poll for the next record. Returns Ok(None) on broker-reported empty.
    /// The autonomy daemon wraps this in tokio::time::timeout to avoid blocking
    /// the select! arm when no records are pending.
    pub async fn next(&mut self) -> Result<Option<EmbeddedRecord>, BrokerError> {
        self.inner
            .next()
            .await
            .map_err(|source| BrokerError::Consume {
                topic: self.topic.clone(),
                source,
            })
    }

    /// Poll with a budget. Returns Ok(None) if the budget elapses with no record.
    pub async fn next_with_timeout(
        &mut self,
        budget: Duration,
    ) -> Result<Option<EmbeddedRecord>, BrokerError> {
        match tokio::time::timeout(budget, self.inner.next()).await {
            Ok(Ok(record)) => Ok(record),
            Ok(Err(source)) => Err(BrokerError::Consume {
                topic: self.topic.clone(),
                source,
            }),
            Err(_elapsed) => Ok(None),
        }
    }
}

/// Initialize the singleton broker + pre-create all topics. Idempotent: a
/// second call is a no-op (returns Ok).
pub async fn init_broker() -> Result<BrokerHandle, BrokerError> {
    if let Some(existing) = BROKER.get() {
        return Ok(BrokerHandle {
            inner: existing.clone(),
        });
    }

    let broker = EmbeddedBroker::new()
        .await
        .map_err(|e| BrokerError::Init(e.to_string()))?;

    for topic in topics::ALL {
        broker
            .create_topic(topic, 1)
            .await
            .map_err(|source| BrokerError::CreateTopic {
                topic: (*topic).to_string(),
                source,
            })?;
        debug!(topic = %topic, "created jansu topic");
    }

    let _ = BROKER.set(broker.clone());
    info!(
        topics = ?topics::ALL,
        "embedded jansu broker initialized"
    );

    Ok(BrokerHandle { inner: broker })
}

/// Returns the initialized broker. Fails if `init_broker()` was never called.
pub fn broker_handle() -> Result<BrokerHandle, BrokerError> {
    match BROKER.get() {
        Some(b) => Ok(BrokerHandle { inner: b.clone() }),
        None => {
            warn!("broker_handle() called before init_broker()");
            Err(BrokerError::NotInitialized)
        }
    }
}
